//! UNIX domain socket server hosting the room process and communicating with `alhalte-maitre-d`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{RwLock, broadcast, mpsc, watch};
use tokio_util::codec::Framed;

use crate::cellar_sink::CellarSink;
use crate::error::{BanquetteError, Result};
use crate::log::{ChangeBgmEvent, CharacterCameEvent, CharacterGoneEvent, LogOperationEvent};
use crate::protocol::{
    BanquetteToMaitreD, ClientFrame, MaitreDToBanquette, ProtocolCodec, ServerFrame,
};
use crate::room::{HeartbeatOutcome, Room};
use tin::TinId;

/// Connected client metadata registered with the server.
#[derive(Debug, Clone)]
pub struct ConnectedClient {
    /// Client ID assigned by gateway.
    pub client_id: u64,
    /// Authenticated account ID.
    pub account_id: u64,
    /// Active session ID.
    pub session_id: u64,
    /// Current scene ID the client has entered, if any.
    pub current_scene: Option<String>,
}

/// Server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// UNIX domain socket path.
    pub socket_path: PathBuf,
    /// Heartbeat interval (default 4 minutes / 240 seconds).
    pub heartbeat_interval: Duration,
    /// Room admin account ID, also used as the cellar owner of internal room writes.
    pub admin_account: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from("/tmp/alhalte-banquette.sock"),
            heartbeat_interval: Duration::from_secs(240),
            admin_account: 1,
        }
    }
}

/// Resolves when the optional external shutdown signal fires.
///
/// `None` means "no external shutdown" and waits forever. A dropped sender is
/// treated as a shutdown request so an in-process server can never outlive the
/// component that owns it.
async fn wait_for_shutdown(shutdown: Option<watch::Receiver<bool>>) {
    match shutdown {
        Some(mut rx) => {
            let _ = rx.changed().await;
        }
        None => std::future::pending::<()>().await,
    }
}

/// Sends a [`ServerFrame`] to a single client through the connection's outgoing channel.
///
/// Delivery failures mean that the maitre-d connection is closing, so they are
/// intentionally ignored (the connection task terminates on its own).
async fn send_to_client(
    out_tx: &mpsc::Sender<BanquetteToMaitreD>,
    client_id: u64,
    frame: ServerFrame,
) {
    let _ = out_tx
        .send(BanquetteToMaitreD::SendToClient { client_id, frame })
        .await;
}

/// Broadcasts a [`ServerFrame`] to all clients currently in a scene.
async fn broadcast_to_scene(
    out_tx: &mpsc::Sender<BanquetteToMaitreD>,
    scene_id: &str,
    frame: ServerFrame,
) {
    let _ = out_tx
        .send(BanquetteToMaitreD::BroadcastToScene {
            scene_id: scene_id.to_string(),
            frame,
        })
        .await;
}

/// Broadcasts a [`ServerFrame`] to all clients in the room.
async fn broadcast_to_room(out_tx: &mpsc::Sender<BanquetteToMaitreD>, frame: ServerFrame) {
    let _ = out_tx
        .send(BanquetteToMaitreD::BroadcastToRoom { frame })
        .await;
}

/// Decodes a 64 hexadecimal character tin ID into a [`TinId`].
fn decode_tin_id_hex(value: &str) -> Result<TinId> {
    let bytes = hex::decode(value)
        .map_err(|e| BanquetteError::Serialization(format!("invalid tin id hex '{value}': {e}")))?;
    let len = bytes.len();
    bytes.as_slice().try_into().map_err(|_| {
        BanquetteError::Serialization(format!(
            "tin id must be 32 bytes (64 hex characters), got {len} bytes"
        ))
    })
}

/// The Banquette UNIX domain socket server instance.
pub struct BanquetteServer {
    config: ServerConfig,
    room: Arc<RwLock<Room>>,
    cellar: Arc<dyn CellarSink>,
    clients: Arc<RwLock<HashMap<u64, ConnectedClient>>>,
    /// Broadcasts successfully stored `RoomData` tin IDs to connected maitre-d links.
    persisted_tx: broadcast::Sender<TinId>,
}

impl BanquetteServer {
    /// Creates a new [`BanquetteServer`].
    pub fn new(config: ServerConfig, room: Room, cellar: Arc<dyn CellarSink>) -> Self {
        let (persisted_tx, _) = broadcast::channel(16);
        Self {
            config,
            room: Arc::new(RwLock::new(room)),
            cellar,
            clients: Arc::new(RwLock::new(HashMap::new())),
            persisted_tx,
        }
    }

    /// Retrieves an atomic reference to the internal [`Room`].
    pub fn room(&self) -> Arc<RwLock<Room>> {
        Arc::clone(&self.room)
    }

    /// Runs the server until shutdown.
    ///
    /// On shutdown (idle timeout or socket error) the room is persisted once more
    /// and the resulting head tin is reported to `maitre-d` before the socket is
    /// closed.
    pub async fn run(&self) -> Result<()> {
        self.run_inner(None).await
    }

    /// Runs the server until `shutdown` is notified, then persists the room once more.
    ///
    /// Sending `true` (or dropping the sender) requests a graceful shutdown.
    /// Used by in-process spawners and tests to stop a room deterministically.
    pub async fn run_until_shutdown(&self, shutdown: watch::Receiver<bool>) -> Result<()> {
        self.run_inner(Some(shutdown)).await
    }

    async fn run_inner(&self, shutdown: Option<watch::Receiver<bool>>) -> Result<()> {
        let socket_path = &self.config.socket_path;
        if socket_path.exists() {
            let _ = std::fs::remove_file(socket_path);
        }

        let listener = UnixListener::bind(socket_path)?;
        tracing::info!(
            "Banquette server listening on unix socket: {:?}",
            socket_path
        );

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        // Spawn periodic heartbeat task
        let heartbeat_room = Arc::clone(&self.room);
        let heartbeat_cellar = Arc::clone(&self.cellar);
        let heartbeat_interval = self.config.heartbeat_interval;
        let admin_account = self.config.admin_account;
        let heartbeat_persisted_tx = self.persisted_tx.clone();
        let heartbeat_shutdown = shutdown.clone();

        let heartbeat_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(heartbeat_interval);
            // First tick completes immediately, skip it
            interval.tick().await;

            loop {
                tokio::select! {
                    _ = wait_for_shutdown(heartbeat_shutdown.clone()) => break,
                    _ = interval.tick() => {
                        let mut room = heartbeat_room.write().await;
                        match room.step_heartbeat(heartbeat_cellar.as_ref(), admin_account).await {
                            Ok(result) => {
                                if let Some(tin_id) = result.persisted_tin {
                                    // Only a successful store advances the head pointer.
                                    let _ = heartbeat_persisted_tx.send(tin_id);
                                }
                                if result.outcome == HeartbeatOutcome::ShouldExit {
                                    tracing::warn!("3 consecutive missed heartbeats; initiating room shutdown");
                                    let _ = shutdown_tx.send(()).await;
                                    break;
                                }
                            }
                            Err(e) => {
                                tracing::error!("Error during periodic heartbeat: {:?}", e);
                            }
                        }
                    }
                }
            }
        });

        loop {
            tokio::select! {
                _ = wait_for_shutdown(shutdown.clone()) => {
                    tracing::info!("Shutdown signal received, closing server");
                    break;
                }
                _ = shutdown_rx.recv() => {
                    tracing::info!("Shutdown signal received, closing server");
                    break;
                }
                accept_res = listener.accept() => {
                    match accept_res {
                        Ok((stream, _)) => {
                            let room = Arc::clone(&self.room);
                            let cellar = Arc::clone(&self.cellar);
                            let clients = Arc::clone(&self.clients);
                            let persisted_tx = self.persisted_tx.clone();
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_connection(stream, room, cellar, clients, persisted_tx).await {
                                    tracing::error!("Connection handler error: {:?}", e);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("Accept error: {:?}", e);
                        }
                    }
                }
            }
        }

        // Stop the heartbeat loop before the final persist so the two cannot interleave.
        heartbeat_task.abort();
        let _ = heartbeat_task.await;

        self.persist_and_report_final_head().await;

        if socket_path.exists() {
            let _ = std::fs::remove_file(socket_path);
        }

        Ok(())
    }

    /// Persists the room one last time and reports the head tin to `maitre-d`.
    ///
    /// Best effort: a failure is logged and the server still exits. The short
    /// delay gives the outgoing writer task a chance to flush `RoomPersisted`
    /// before the socket is torn down.
    async fn persist_and_report_final_head(&self) {
        let persisted = {
            let mut room = self.room.write().await;
            room.persist(self.cellar.as_ref(), self.config.admin_account)
                .await
        };

        match persisted {
            Ok(tin_id) => {
                let _ = self.persisted_tx.send(tin_id);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(e) => tracing::error!("Final room persist on shutdown failed: {e:?}"),
        }
    }

    /// Handles a single incoming connection from `maitre-d`.
    ///
    /// `persisted_tx` receives the tin ID of every successfully stored room head;
    /// each connection forwards those to `maitre-d` as
    /// [`BanquetteToMaitreD::RoomPersisted`].
    pub async fn handle_connection(
        stream: UnixStream,
        room: Arc<RwLock<Room>>,
        cellar: Arc<dyn CellarSink>,
        clients: Arc<RwLock<HashMap<u64, ConnectedClient>>>,
        persisted_tx: broadcast::Sender<TinId>,
    ) -> Result<()> {
        let (read_half, write_half) = stream.into_split();
        let mut reader = Framed::new(read_half, ProtocolCodec::<MaitreDToBanquette>::new());
        let (out_tx, mut out_rx) = mpsc::channel::<BanquetteToMaitreD>(128);

        // Task to write outgoing frames back to maitre-d
        let write_task = tokio::spawn(async move {
            let mut writer = Framed::new(write_half, ProtocolCodec::<BanquetteToMaitreD>::new());
            while let Some(msg) = out_rx.recv().await {
                if let Err(e) = writer.send(msg).await {
                    tracing::error!("Error sending frame to maitre-d: {:?}", e);
                    break;
                }
            }
        });

        let mut persisted_rx = persisted_tx.subscribe();
        let persist_out_tx = out_tx.clone();
        let persist_task = tokio::spawn(async move {
            while let Ok(tin_id) = persisted_rx.recv().await {
                if persist_out_tx
                    .send(BanquetteToMaitreD::RoomPersisted { tin_id })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        while let Some(item) = reader.next().await {
            let frame = item?;
            match frame {
                MaitreDToBanquette::Ping => {
                    let _ = out_tx.send(BanquetteToMaitreD::Pong).await;
                }
                MaitreDToBanquette::ClientConnected {
                    client_id,
                    account_id,
                    session_id,
                } => {
                    let mut cl = clients.write().await;
                    cl.insert(
                        client_id,
                        ConnectedClient {
                            client_id,
                            account_id,
                            session_id,
                            current_scene: Some("/".to_string()),
                        },
                    );
                    let mut r = room.write().await;
                    r.record_client_activity();
                    let _ = r
                        .client_enter_scene(client_id, account_id, "/", cellar.as_ref())
                        .await;
                }
                MaitreDToBanquette::ClientDisconnected { client_id } => {
                    let client_opt = {
                        let mut cl = clients.write().await;
                        cl.remove(&client_id)
                    };
                    if let Some(c) = client_opt {
                        let mut r = room.write().await;
                        let _ = r
                            .client_disconnect(
                                client_id,
                                c.account_id,
                                c.session_id,
                                cellar.as_ref(),
                            )
                            .await;
                    }
                }
                MaitreDToBanquette::ClientMessage { client_id, frame } => {
                    Self::process_client_message(
                        client_id, frame, &room, &cellar, &clients, &out_tx,
                    )
                    .await?;
                }
                MaitreDToBanquette::ExportRoom {
                    request_id,
                    caller_account,
                } => {
                    let room_guard = room.read().await;
                    let res = room_guard
                        .export_snapshot(cellar.as_ref(), caller_account)
                        .await
                        .map_err(|e| e.to_string());
                    let _ = out_tx
                        .send(BanquetteToMaitreD::ExportRoomResponse {
                            request_id,
                            result: res,
                        })
                        .await;
                }
            }
        }

        write_task.abort();
        persist_task.abort();
        Ok(())
    }

    async fn process_client_message(
        client_id: u64,
        frame: ClientFrame,
        room: &Arc<RwLock<Room>>,
        cellar: &Arc<dyn CellarSink>,
        clients: &Arc<RwLock<HashMap<u64, ConnectedClient>>>,
        out_tx: &mpsc::Sender<BanquetteToMaitreD>,
    ) -> Result<()> {
        let (account_id, session_id) = {
            let cl = clients.read().await;
            match cl.get(&client_id) {
                Some(c) => (c.account_id, c.session_id),
                None => (0, 0),
            }
        };

        match frame {
            ClientFrame::ListScenes => {
                let r = room.read().await;
                let scenes = r.list_scene_ids();
                let _ = out_tx
                    .send(BanquetteToMaitreD::SendToClient {
                        client_id,
                        frame: ServerFrame::SceneList { scenes },
                    })
                    .await;
            }
            ClientFrame::QueryScene { scene_id } => {
                let r = room.read().await;
                // If caller cannot access the scene, do NOT send any response!
                if let Some(summary) = r.query_scene(account_id, &scene_id) {
                    let _ = out_tx
                        .send(BanquetteToMaitreD::SendToClient {
                            client_id,
                            frame: ServerFrame::SceneInfo(summary),
                        })
                        .await;
                }
            }
            ClientFrame::CreateScene {
                scene_id,
                inherit_juices,
            } => {
                let mut r = room.write().await;
                let source = if inherit_juices { Some("/") } else { None };
                match r.create_scene(account_id, &scene_id, source) {
                    Ok(()) => {
                        let scenes = r.list_scene_ids();
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::SceneCreated {
                                scene_id: scene_id.clone(),
                            },
                        )
                        .await;
                        broadcast_to_room(out_tx, ServerFrame::SceneList { scenes }).await;
                    }
                    Err(e) => {
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::Error {
                                code: "CreateSceneFailed".to_string(),
                                message: e.to_string(),
                            },
                        )
                        .await;
                    }
                }
            }
            ClientFrame::DeleteScene { scene_id } => {
                let mut r = room.write().await;
                match r.delete_scene(account_id, &scene_id) {
                    Ok(()) => {
                        let scenes = r.list_scene_ids();
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::SceneDeleted {
                                scene_id: scene_id.clone(),
                            },
                        )
                        .await;
                        broadcast_to_room(out_tx, ServerFrame::SceneList { scenes }).await;
                    }
                    Err(e) => {
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::Error {
                                code: "DeleteSceneFailed".to_string(),
                                message: e.to_string(),
                            },
                        )
                        .await;
                    }
                }
            }
            ClientFrame::EnterScene { scene_id } => {
                let mut r = room.write().await;
                if let Ok(()) = r
                    .client_enter_scene(client_id, account_id, &scene_id, cellar.as_ref())
                    .await
                {
                    let mut cl = clients.write().await;
                    if let Some(c) = cl.get_mut(&client_id) {
                        c.current_scene = Some(scene_id);
                    }
                }
            }
            ClientFrame::LeaveScene { scene_id } => {
                let mut r = room.write().await;
                let _ = r
                    .client_leave_scene(
                        client_id,
                        account_id,
                        session_id,
                        &scene_id,
                        cellar.as_ref(),
                    )
                    .await;
                let mut cl = clients.write().await;
                if let Some(c) = cl.get_mut(&client_id)
                    && c.current_scene.as_deref() == Some(&scene_id)
                {
                    c.current_scene = None;
                }
            }
            ClientFrame::SetDone { done } => {
                let mut r = room.write().await;
                if let Err(e) = r.set_done(account_id, done) {
                    let _ = out_tx
                        .send(BanquetteToMaitreD::SendToClient {
                            client_id,
                            frame: ServerFrame::Error {
                                code: "SetDoneFailed".to_string(),
                                message: e.to_string(),
                            },
                        })
                        .await;
                }
            }
            ClientFrame::QueryState { scene_id } => {
                let r = room.read().await;
                // If caller cannot access the scene, do NOT send any response!
                if let Some(map) = r.query_state(account_id, &scene_id) {
                    let _ = out_tx
                        .send(BanquetteToMaitreD::SendToClient {
                            client_id,
                            frame: ServerFrame::StateSnapshot {
                                scene_id,
                                states: map.clone(),
                            },
                        })
                        .await;
                }
            }
            ClientFrame::UpdateState {
                scene_id,
                key,
                value,
            } => {
                let mut r = room.write().await;
                match r.update_scene_state(account_id, &scene_id, key.clone(), value.clone()) {
                    Ok(_) => {
                        let frame = ServerFrame::StateUpdated {
                            scene_id: scene_id.clone(),
                            key,
                            value,
                        };
                        // The requester may not be in the target scene (root-scene state is
                        // edited from other scenes, e.g. room assets and extensions), so the
                        // success is acknowledged directly in addition to the scene broadcast.
                        // A sender that is in the scene receives both frames; state updates
                        // are idempotent, so applying them twice is harmless.
                        send_to_client(out_tx, client_id, frame.clone()).await;
                        broadcast_to_scene(out_tx, &scene_id, frame).await;
                    }
                    Err(e) => {
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::Error {
                                code: "UpdateStateFailed".to_string(),
                                message: e.to_string(),
                            },
                        )
                        .await;
                    }
                }
            }
            ClientFrame::DeleteState { scene_id, key } => {
                let mut r = room.write().await;
                match r.delete_scene_state(account_id, &scene_id, &key) {
                    Ok(_) => {
                        let frame = ServerFrame::StateDeleted {
                            scene_id: scene_id.clone(),
                            key,
                        };
                        // Direct ack for requesters outside the target scene, plus the
                        // scene broadcast for everyone else. Deletion is idempotent.
                        send_to_client(out_tx, client_id, frame.clone()).await;
                        broadcast_to_scene(out_tx, &scene_id, frame).await;
                    }
                    Err(e) => {
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::Error {
                                code: "DeleteStateFailed".to_string(),
                                message: e.to_string(),
                            },
                        )
                        .await;
                    }
                }
            }
            ClientFrame::AppendLog { scene_id, entry } => {
                let mut r = room.write().await;
                match r
                    .append_log(account_id, session_id, &scene_id, entry, cellar.as_ref())
                    .await
                {
                    Ok(LogOperationEvent::Appended { position, entry }) => {
                        // Broadcast the appended entry to every client of the scene,
                        // including the requester, so that it can observe its own write.
                        broadcast_to_scene(
                            out_tx,
                            &scene_id,
                            ServerFrame::LogEventAppended {
                                scene_id: scene_id.clone(),
                                chunk_index: position.0,
                                position: position.1,
                                entry,
                            },
                        )
                        .await;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::Error {
                                code: "AppendLogFailed".to_string(),
                                message: e.to_string(),
                            },
                        )
                        .await;
                    }
                }
            }
            ClientFrame::EditLog {
                scene_id,
                chunk_index,
                position,
                entry,
            } => {
                let mut r = room.write().await;
                if let Some(scene) = r.scene_biblio.get_mut(&scene_id) {
                    match scene
                        .log
                        .edit(
                            chunk_index,
                            position,
                            entry.clone(),
                            cellar.as_ref(),
                            account_id,
                            session_id,
                        )
                        .await
                    {
                        Ok(_) => {
                            let _ = out_tx
                                .send(BanquetteToMaitreD::BroadcastToScene {
                                    scene_id: scene_id.clone(),
                                    frame: ServerFrame::LogEventEdited {
                                        scene_id: scene_id.clone(),
                                        chunk_index,
                                        position,
                                        entry,
                                    },
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = out_tx
                                .send(BanquetteToMaitreD::SendToClient {
                                    client_id,
                                    frame: ServerFrame::Error {
                                        code: "EditLogFailed".to_string(),
                                        message: e.to_string(),
                                    },
                                })
                                .await;
                        }
                    }
                }
            }
            ClientFrame::DeleteLog {
                scene_id,
                chunk_index,
                position,
            } => {
                let mut r = room.write().await;
                if let Some(scene) = r.scene_biblio.get_mut(&scene_id) {
                    match scene
                        .log
                        .delete(
                            chunk_index,
                            position,
                            cellar.as_ref(),
                            account_id,
                            session_id,
                        )
                        .await
                    {
                        Ok(_) => {
                            let _ = out_tx
                                .send(BanquetteToMaitreD::BroadcastToScene {
                                    scene_id: scene_id.clone(),
                                    frame: ServerFrame::LogEventDeleted {
                                        scene_id: scene_id.clone(),
                                        chunk_index,
                                        position,
                                    },
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = out_tx
                                .send(BanquetteToMaitreD::SendToClient {
                                    client_id,
                                    frame: ServerFrame::Error {
                                        code: "DeleteLogFailed".to_string(),
                                        message: e.to_string(),
                                    },
                                })
                                .await;
                        }
                    }
                }
            }
            ClientFrame::RewindLog {
                scene_id,
                chunk_index,
                position,
            } => {
                let mut r = room.write().await;
                if let Some(scene) = r.scene_biblio.get_mut(&scene_id) {
                    match scene.log.rewind(chunk_index, position) {
                        Ok(_) => {
                            let _ = out_tx
                                .send(BanquetteToMaitreD::BroadcastToScene {
                                    scene_id: scene_id.clone(),
                                    frame: ServerFrame::LogEventRewound {
                                        scene_id: scene_id.clone(),
                                        chunk_index,
                                        position,
                                    },
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = out_tx
                                .send(BanquetteToMaitreD::SendToClient {
                                    client_id,
                                    frame: ServerFrame::Error {
                                        code: "RewindLogFailed".to_string(),
                                        message: e.to_string(),
                                    },
                                })
                                .await;
                        }
                    }
                }
            }
            ClientFrame::QueryLogBiblio { scene_id } => {
                let r = room.read().await;
                // If caller cannot access the scene, do NOT send any response!
                if let Some(biblio) = r.query_log_biblio(account_id, &scene_id) {
                    let _ = out_tx
                        .send(BanquetteToMaitreD::SendToClient {
                            client_id,
                            frame: ServerFrame::LogBiblioSnapshot {
                                scene_id,
                                biblio: biblio.clone(),
                            },
                        })
                        .await;
                }
            }
            ClientFrame::QueryLogChunk {
                scene_id,
                chunk_index,
            } => {
                let r = room.read().await;
                // If caller cannot access the scene, do NOT send any response!
                if let Some(entries) = r
                    .query_log_chunk(account_id, &scene_id, chunk_index, cellar.as_ref())
                    .await
                {
                    let _ = out_tx
                        .send(BanquetteToMaitreD::SendToClient {
                            client_id,
                            frame: ServerFrame::LogChunkSnapshot {
                                scene_id,
                                chunk_index,
                                entries,
                            },
                        })
                        .await;
                }
            }
            ClientFrame::ListCharacters => {
                let r = room.read().await;
                let characters = r.character_biblio.all().clone();
                let _ = out_tx
                    .send(BanquetteToMaitreD::SendToClient {
                        client_id,
                        frame: ServerFrame::CharacterList { characters },
                    })
                    .await;
            }
            ClientFrame::QueryCharacter { character_id } => {
                let r = room.read().await;
                if let Some(ch) = r.character_biblio.get(character_id) {
                    let _ = out_tx
                        .send(BanquetteToMaitreD::SendToClient {
                            client_id,
                            frame: ServerFrame::CharacterInfo {
                                character_id,
                                data: ch.clone(),
                            },
                        })
                        .await;
                }
            }
            ClientFrame::CreateCharacter { data } => {
                let mut r = room.write().await;
                let char_id = r.character_biblio.create_character(account_id, data);
                let _ = out_tx
                    .send(BanquetteToMaitreD::SendToClient {
                        client_id,
                        frame: ServerFrame::CharacterCreated {
                            character_id: char_id,
                        },
                    })
                    .await;
            }
            ClientFrame::UpdateCharacter { character_id, data } => {
                let mut r = room.write().await;
                let is_admin = r.is_admin(account_id);
                match r
                    .character_biblio
                    .update(character_id, account_id, is_admin, data)
                {
                    Ok(()) => {
                        let _ = out_tx
                            .send(BanquetteToMaitreD::SendToClient {
                                client_id,
                                frame: ServerFrame::CharacterUpdated { character_id },
                            })
                            .await;
                    }
                    Err(e) => {
                        let _ = out_tx
                            .send(BanquetteToMaitreD::SendToClient {
                                client_id,
                                frame: ServerFrame::Error {
                                    code: "UpdateCharacterFailed".to_string(),
                                    message: e.to_string(),
                                },
                            })
                            .await;
                    }
                }
            }
            ClientFrame::DeleteCharacter { character_id } => {
                let mut r = room.write().await;
                let is_admin = r.is_admin(account_id);
                match r
                    .character_biblio
                    .delete(character_id, account_id, is_admin)
                {
                    Ok(_) => {
                        let _ = out_tx
                            .send(BanquetteToMaitreD::SendToClient {
                                client_id,
                                frame: ServerFrame::CharacterDeleted { character_id },
                            })
                            .await;
                    }
                    Err(e) => {
                        let _ = out_tx
                            .send(BanquetteToMaitreD::SendToClient {
                                client_id,
                                frame: ServerFrame::Error {
                                    code: "DeleteCharacterFailed".to_string(),
                                    message: e.to_string(),
                                },
                            })
                            .await;
                    }
                }
            }
            ClientFrame::MoveCharacter {
                character_id,
                to_scene_id,
            } => {
                let mut r = room.write().await;
                // Find former scene if any
                let former_scene = r
                    .scene_biblio
                    .iter()
                    .find(|(_, s)| s.present_characters.contains(&character_id))
                    .map(|(id, _)| id.clone());
                // `move_character` implicitly creates the destination scene when missing,
                // so the room's scene list must be re-broadcast in that case.
                let destination_created = !r.scene_biblio.contains_key(&to_scene_id);

                match r.move_character(account_id, character_id, &to_scene_id) {
                    Ok(()) => {
                        // Append gone log in former scene
                        if let Some(fs) = former_scene {
                            let gone_event = CharacterGoneEvent { character_id };
                            if let Ok(tin) = gone_event.to_tin() {
                                let _ = r
                                    .append_log(
                                        account_id,
                                        session_id,
                                        &fs,
                                        Some(tin),
                                        cellar.as_ref(),
                                    )
                                    .await;
                            }
                        }
                        // Append came log in destination scene
                        let came_event = CharacterCameEvent { character_id };
                        if let Ok(tin) = came_event.to_tin() {
                            let _ = r
                                .append_log(
                                    account_id,
                                    session_id,
                                    &to_scene_id,
                                    Some(tin),
                                    cellar.as_ref(),
                                )
                                .await;
                        }

                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::CharacterMoved {
                                character_id,
                                to_scene_id: to_scene_id.clone(),
                            },
                        )
                        .await;

                        if destination_created {
                            let scenes = r.list_scene_ids();
                            broadcast_to_room(out_tx, ServerFrame::SceneList { scenes }).await;
                        }
                    }
                    Err(e) => {
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::Error {
                                code: "MoveCharacterFailed".to_string(),
                                message: e.to_string(),
                            },
                        )
                        .await;
                    }
                }
            }
            ClientFrame::ChangeBgm {
                scene_id,
                slot,
                tin_id,
            } => {
                // Decode the optional 64-hex tin ID and append a `ChangeBgmEvent` log entry.
                let bgm_tin_id = match tin_id.as_deref().map(decode_tin_id_hex).transpose() {
                    Ok(id) => id,
                    Err(e) => {
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::Error {
                                code: "ChangeBgmFailed".to_string(),
                                message: e.to_string(),
                            },
                        )
                        .await;
                        return Ok(());
                    }
                };

                let event = ChangeBgmEvent {
                    bgm_tin_id,
                    volume: None,
                    slot,
                    track_index: None,
                };
                let entry = match event.to_tin() {
                    Ok(tin) => tin,
                    Err(e) => {
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::Error {
                                code: "ChangeBgmFailed".to_string(),
                                message: e.to_string(),
                            },
                        )
                        .await;
                        return Ok(());
                    }
                };

                let mut r = room.write().await;
                match r
                    .append_log(
                        account_id,
                        session_id,
                        &scene_id,
                        Some(entry),
                        cellar.as_ref(),
                    )
                    .await
                {
                    Ok(LogOperationEvent::Appended { position, entry }) => {
                        // Success is observed by the requester as this broadcast.
                        broadcast_to_scene(
                            out_tx,
                            &scene_id,
                            ServerFrame::LogEventAppended {
                                scene_id: scene_id.clone(),
                                chunk_index: position.0,
                                position: position.1,
                                entry,
                            },
                        )
                        .await;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::Error {
                                code: "ChangeBgmFailed".to_string(),
                                message: e.to_string(),
                            },
                        )
                        .await;
                    }
                }
            }
            ClientFrame::WebRtcSignal {
                to_client_id,
                payload,
            } => {
                // Relay signaling message directly to peer
                let _ = out_tx
                    .send(BanquetteToMaitreD::SendToClient {
                        client_id: to_client_id,
                        frame: ServerFrame::WebRtcSignal {
                            from_client_id: client_id,
                            payload,
                        },
                    })
                    .await;
            }
            ClientFrame::HeartbeatResponse => {
                let mut r = room.write().await;
                r.record_client_activity();
            }
        }

        Ok(())
    }
}
