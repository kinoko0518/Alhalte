//! UNIX domain socket server hosting the room process and communicating with `alhalte-maitre-d`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{RwLock, mpsc, watch};
use tokio_util::codec::Framed;

use crate::cellar_sink::CellarSink;
use crate::error::Result;
use crate::labels::KEY_STD_NAME;
use crate::log::{CharacterCameEvent, CharacterGoneEvent, LogOperationEvent};
use crate::protocol::{
    BanquetteToMaitreD, ClientFrame, MaitreDToBanquette, ProtocolCodec, ServerFrame,
};
use crate::room::{HeartbeatOutcome, Room};

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
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from("/tmp/alhalte-banquette.sock"),
            heartbeat_interval: Duration::from_secs(240),
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

/// The Banquette UNIX domain socket server instance.
pub struct BanquetteServer {
    config: ServerConfig,
    room: Arc<RwLock<Room>>,
    cellar: Arc<dyn CellarSink>,
    clients: Arc<RwLock<HashMap<u64, ConnectedClient>>>,
}

impl BanquetteServer {
    /// Creates a new [`BanquetteServer`].
    pub fn new(config: ServerConfig, room: Room, cellar: Arc<dyn CellarSink>) -> Self {
        Self {
            config,
            room: Arc::new(RwLock::new(room)),
            cellar,
            clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Retrieves an atomic reference to the internal [`Room`].
    pub fn room(&self) -> Arc<RwLock<Room>> {
        Arc::clone(&self.room)
    }

    /// Runs the server until shutdown.
    ///
    /// On shutdown (idle timeout or socket error) the room is persisted once more,
    /// which moves the `room/<room_id>/head` cellar alias, before the socket is
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
                        match room.step_heartbeat(heartbeat_cellar.as_ref()).await {
                            Ok(result) => {
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
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_connection(stream, room, cellar, clients).await {
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

        self.persist_final().await;

        if socket_path.exists() {
            let _ = std::fs::remove_file(socket_path);
        }

        Ok(())
    }

    /// Persists the room one last time on graceful shutdown.
    ///
    /// Best effort: a failure is logged and the server still exits. A successful
    /// store moves the `room/<room_id>/head` alias, which is the only head
    /// pointer now, so the next start restores the final state (Issue #192).
    async fn persist_final(&self) {
        let persisted = {
            let mut room = self.room.write().await;
            room.persist(self.cellar.as_ref()).await
        };

        if let Err(e) = persisted {
            tracing::error!("Final room persist on shutdown failed: {e:?}");
        }
    }

    /// Handles a single incoming connection from `maitre-d`.
    pub async fn handle_connection(
        stream: UnixStream,
        room: Arc<RwLock<Room>>,
        cellar: Arc<dyn CellarSink>,
        clients: Arc<RwLock<HashMap<u64, ConnectedClient>>>,
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
                    include_state,
                } => {
                    let room_guard = room.read().await;
                    let res = match room_guard
                        .export_snapshot(cellar.as_ref(), caller_account)
                        .await
                    {
                        Ok((room_data, chunks)) => {
                            crate::export::export_room_to_zip(&room_data, &chunks, include_state)
                                .map_err(|e| e.to_string())
                        }
                        Err(e) => Err(e.to_string()),
                    };
                    let _ = out_tx
                        .send(BanquetteToMaitreD::ExportRoomResponse {
                            request_id,
                            result: res,
                        })
                        .await;
                }
                MaitreDToBanquette::QueryAccountPermission {
                    request_id,
                    account_id,
                } => {
                    let room_guard = room.read().await;
                    let permission = room_guard.get_account_permission(account_id);
                    let _ = out_tx
                        .send(BanquetteToMaitreD::AccountPermissionResponse {
                            request_id,
                            account_id,
                            permission,
                        })
                        .await;
                }
            }
        }

        write_task.abort();
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
        let (account_id, session_id, current_scene) = {
            let cl = clients.read().await;
            match cl.get(&client_id) {
                Some(c) => (c.account_id, c.session_id, c.current_scene.clone()),
                None => (0, 0, None),
            }
        };

        match frame {
            ClientFrame::ListScenes => {
                let r = room.read().await;
                let scenes = r.list_scene_summaries();
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
                name,
            } => {
                let mut r = room.write().await;
                let source = if inherit_juices { Some("/") } else { None };
                match r.create_scene_named(account_id, &scene_id, source, name.as_deref()) {
                    Ok(()) => {
                        let scenes = r.list_scene_summaries();
                        let created_summary = r.scene_summary(&scene_id);
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::SceneCreated {
                                scene_id: scene_id.clone(),
                            },
                        )
                        .await;
                        broadcast_to_room(out_tx, ServerFrame::SceneList { scenes }).await;
                        // Announce the new scene summary (including its
                        // server-authoritative name) to every client so nobody
                        // needs a reload to see it.
                        if let Some(summary) = created_summary {
                            broadcast_to_room(out_tx, ServerFrame::SceneInfo(summary)).await;
                        }
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
                        let scenes = r.list_scene_summaries();
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
                // Presence is authorized too: a client that may not read a scene
                // must not join it, otherwise the scene broadcasts would leak
                // writes to it. Like the unpermitted query frames this is silent
                // (no `StateSnapshot`, no error frame) so a rejected navigation
                // never surfaces as a spurious client error; `maitre-d` still
                // tracks the client's scene and is the entry gate until #200.
                if !r.can_access_scene(account_id, &scene_id) {
                    return Ok(());
                }
                if r.client_enter_scene(client_id, account_id, &scene_id, cellar.as_ref())
                    .await
                    .is_ok()
                {
                    let mut cl = clients.write().await;
                    if let Some(c) = cl.get_mut(&client_id) {
                        c.current_scene = Some(scene_id.clone());
                    }
                    drop(cl);
                    // Hydrate the entered scene immediately: an accessible scene
                    // is answered with a StateSnapshot so the client does not
                    // need a separate QueryState after every entry / reconnect.
                    if let Some(states) = r.query_state(account_id, &scene_id) {
                        let states = states.clone();
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::StateSnapshot { scene_id, states },
                        )
                        .await;
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
            ClientFrame::SetMemberPermission {
                account_id: target_account,
                permission,
            } => {
                let mut r = room.write().await;
                match r.set_member_permission(account_id, target_account, permission) {
                    Ok(()) => {
                        let (default_permission, permissions) = r.permission_snapshot();
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::PermissionsUpdated {
                                default_permission,
                                permissions,
                            },
                        )
                        .await;
                    }
                    Err(e) => {
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::Error {
                                code: "SetMemberPermissionFailed".to_string(),
                                message: e.to_string(),
                            },
                        )
                        .await;
                    }
                }
            }
            ClientFrame::SetDefaultPermission { permission } => {
                let mut r = room.write().await;
                match r.set_default_permission(account_id, permission) {
                    Ok(()) => {
                        let (default_permission, permissions) = r.permission_snapshot();
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::PermissionsUpdated {
                                default_permission,
                                permissions,
                            },
                        )
                        .await;
                    }
                    Err(e) => {
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::Error {
                                code: "SetDefaultPermissionFailed".to_string(),
                                message: e.to_string(),
                            },
                        )
                        .await;
                    }
                }
            }
            ClientFrame::QueryPermissions => {
                let r = room.read().await;
                // An account without access to the room gets no response, like
                // the other query frames.
                if r.can_access_scene(account_id, "/") {
                    let (default_permission, permissions) = r.permission_snapshot();
                    let _ = out_tx
                        .send(BanquetteToMaitreD::SendToClient {
                            client_id,
                            frame: ServerFrame::PermissionsSnapshot {
                                default_permission,
                                permissions,
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
                            key: key.clone(),
                            value,
                        };
                        // The requester may not be in the target scene (root-scene state is
                        // edited from other scenes, e.g. room assets and extensions), so the
                        // success is acknowledged directly in addition to the scene broadcast.
                        // A sender that is in the scene receives both frames; state updates
                        // are idempotent, so applying them twice is harmless.
                        send_to_client(out_tx, client_id, frame.clone()).await;
                        broadcast_to_scene(out_tx, &scene_id, frame).await;
                        // A rename is a summary change as well: broadcast the new
                        // `SceneInfo` so other clients update their scene list
                        // without a reload or a re-entry.
                        if key == KEY_STD_NAME
                            && let Some(summary) = r.scene_summary(&scene_id)
                        {
                            broadcast_to_room(out_tx, ServerFrame::SceneInfo(summary)).await;
                        }
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
                match r
                    .edit_log(
                        account_id,
                        session_id,
                        &scene_id,
                        chunk_index,
                        position,
                        entry.clone(),
                        cellar.as_ref(),
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
            ClientFrame::DeleteLog {
                scene_id,
                chunk_index,
                position,
            } => {
                let mut r = room.write().await;
                match r
                    .delete_log(
                        account_id,
                        session_id,
                        &scene_id,
                        chunk_index,
                        position,
                        cellar.as_ref(),
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
            ClientFrame::RewindLog {
                scene_id,
                chunk_index,
                position,
            } => {
                let mut r = room.write().await;
                match r.rewind_log(account_id, &scene_id, chunk_index, position) {
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
                let char_id = match r.create_character(account_id, data) {
                    Ok(char_id) => char_id,
                    Err(e) => {
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::Error {
                                code: "CreateCharacterFailed".to_string(),
                                message: e.to_string(),
                            },
                        )
                        .await;
                        return Ok(());
                    }
                };
                // A new character joins the scene the requester is currently in
                // (falling back to the root scene). Membership lives on the
                // scene itself so it survives reload / reconnect.
                let scene_id = current_scene
                    .filter(|sid| r.scene_biblio.contains_key(sid))
                    .unwrap_or_else(|| "/".to_string());
                let _ = r.assign_character_to_scene(char_id, &scene_id);
                let character = r.character_biblio.get(char_id).cloned();

                // The requester still receives the `CharacterCreated` ack (which
                // carries the server-allocated id); every client, including the
                // requester, also receives the full attribute map as
                // `CharacterInfo`. Applying both is idempotent.
                send_to_client(
                    out_tx,
                    client_id,
                    ServerFrame::CharacterCreated {
                        character_id: char_id,
                    },
                )
                .await;
                if let Some(data) = character {
                    broadcast_to_room(
                        out_tx,
                        ServerFrame::CharacterInfo {
                            character_id: char_id,
                            data,
                        },
                    )
                    .await;
                }
                if let Some(summary) = r.scene_summary(&scene_id) {
                    broadcast_to_room(out_tx, ServerFrame::SceneInfo(summary)).await;
                }
            }
            ClientFrame::UpdateCharacter { character_id, data } => {
                let mut r = room.write().await;
                match r.update_character(account_id, character_id, data) {
                    Ok(()) => {
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::CharacterUpdated { character_id },
                        )
                        .await;
                        // Broadcast the stored attributes (the biblio may have
                        // preserved the owner) so all clients and tabs agree.
                        if let Some(data) = r.character_biblio.get(character_id).cloned() {
                            broadcast_to_room(
                                out_tx,
                                ServerFrame::CharacterInfo { character_id, data },
                            )
                            .await;
                        }
                    }
                    Err(e) => {
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::Error {
                                code: "UpdateCharacterFailed".to_string(),
                                message: e.to_string(),
                            },
                        )
                        .await;
                    }
                }
            }
            ClientFrame::DeleteCharacter { character_id } => {
                let mut r = room.write().await;
                match r.delete_character(account_id, character_id) {
                    Ok(_) => {
                        // Removal is room-wide: drop the scene memberships and
                        // any piece state so no client keeps a stale member or an
                        // interactive ghost piece.
                        let affected = r.remove_character_from_scenes(character_id);
                        let removed_piece_state = r.clear_character_piece_state(character_id);

                        let deleted = ServerFrame::CharacterDeleted { character_id };
                        send_to_client(out_tx, client_id, deleted.clone()).await;
                        broadcast_to_room(out_tx, deleted).await;

                        for (scene_id, key) in removed_piece_state {
                            let frame = ServerFrame::StateDeleted {
                                scene_id: scene_id.clone(),
                                key,
                            };
                            // Direct ack for a requester outside the scene plus
                            // the scene broadcast for everyone in it.
                            send_to_client(out_tx, client_id, frame.clone()).await;
                            broadcast_to_scene(out_tx, &scene_id, frame).await;
                        }

                        for scene_id in affected {
                            if let Some(summary) = r.scene_summary(&scene_id) {
                                broadcast_to_room(out_tx, ServerFrame::SceneInfo(summary)).await;
                            }
                        }
                    }
                    Err(e) => {
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::Error {
                                code: "DeleteCharacterFailed".to_string(),
                                message: e.to_string(),
                            },
                        )
                        .await;
                    }
                }
            }
            ClientFrame::MoveCharacter {
                character_id,
                to_scene_id,
            } => {
                let mut r = room.write().await;
                // `move_character` implicitly creates the destination scene when missing,
                // so the room's scene list must be re-broadcast in that case.
                let destination_created = !r.scene_biblio.contains_key(&to_scene_id);

                // Membership and log writes only touch scene memory, while a later
                // `reload_if_needed` rebuilds that memory from the persisted tin. A
                // write into a scene that is not open (and therefore unloaded) would
                // be discarded on the next `EnterScene`, which is exactly how a
                // character moved from the character list became unaffiliated
                // (Issue #240). Load every scene this move touches before mutating
                // anything. The destination only needs loading when it already
                // exists: a missing one is created by `move_character` with empty,
                // loaded state, so there is nothing to restore.
                let mut scenes_to_load: Vec<String> = r
                    .scene_biblio
                    .iter()
                    .filter(|(_, s)| s.present_characters.contains(&character_id))
                    .map(|(id, _)| id.clone())
                    .collect();
                if !destination_created {
                    scenes_to_load.push(to_scene_id.clone());
                }
                scenes_to_load.sort();
                scenes_to_load.dedup();

                for scene_id in &scenes_to_load {
                    if let Err(e) = r
                        .ensure_scene_loaded(scene_id, account_id, cellar.as_ref())
                        .await
                    {
                        // No membership or log entry has been written yet, so the
                        // character stays where it was instead of being moved into
                        // a scene whose persisted state could not be read.
                        send_to_client(
                            out_tx,
                            client_id,
                            ServerFrame::Error {
                                code: "MoveCharacterFailed".to_string(),
                                message: e.to_string(),
                            },
                        )
                        .await;
                        return Ok(());
                    }
                }

                // Recompute the sources from the now-loaded scenes: the persisted
                // tin is authoritative, so a stale in-memory list must not produce a
                // bogus `character-gone` log or a `SceneInfo` for a scene that does
                // not actually hold the character (normally one source, but a
                // legacy room may list it in more than one).
                let former_scenes: Vec<String> = r
                    .scene_biblio
                    .iter()
                    .filter(|(_, s)| s.present_characters.contains(&character_id))
                    .map(|(id, _)| id.clone())
                    .collect();

                match r.move_character(account_id, character_id, &to_scene_id) {
                    Ok(()) => {
                        // Append gone log in each former scene. The event is
                        // built once so every former scene of one move shares
                        // the same append timestamp.
                        let gone_event = CharacterGoneEvent::now(character_id);
                        for fs in &former_scenes {
                            if let Ok(tin) = gone_event.to_tin() {
                                let _ = r
                                    .append_log(
                                        account_id,
                                        session_id,
                                        fs,
                                        Some(tin),
                                        cellar.as_ref(),
                                    )
                                    .await;
                            }
                        }
                        // Append came log in destination scene
                        let came_event = CharacterCameEvent::now(character_id);
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

                        let moved = ServerFrame::CharacterMoved {
                            character_id,
                            to_scene_id: to_scene_id.clone(),
                        };
                        // The requester's ack and the room-wide broadcast are the
                        // same frame; applying it twice is idempotent.
                        send_to_client(out_tx, client_id, moved.clone()).await;
                        broadcast_to_room(out_tx, moved).await;

                        if destination_created {
                            let scenes = r.list_scene_summaries();
                            broadcast_to_room(out_tx, ServerFrame::SceneList { scenes }).await;
                        }

                        // Re-announce the source and destination summaries so all
                        // clients rebuild `scene.members` from the server state.
                        let mut affected = former_scenes;
                        affected.push(to_scene_id.clone());
                        affected.sort();
                        affected.dedup();
                        for scene_id in affected {
                            if let Some(summary) = r.scene_summary(&scene_id) {
                                broadcast_to_room(out_tx, ServerFrame::SceneInfo(summary)).await;
                            }
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
