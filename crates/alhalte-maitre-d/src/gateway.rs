//! WebSocket room demultiplexer and UNIX socket communication bridge with `alhalte-banquette`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use alhalte_banquette::{
    BanquetteToMaitreD, ClientFrame, LogChunk, MaitreDToBanquette, ProtocolCodec, RoomData,
    ServerFrame,
};
use futures_util::{SinkExt, StreamExt};
use tin::TinId;
use tokio::net::UnixStream;
use tokio::sync::{RwLock, mpsc, oneshot};
use tokio_util::codec::Framed;

use crate::db::Database;
use crate::error::{MaitreDError, Result};
use crate::spawner::BanquetteSpawner;

type ExportResponseSender =
    oneshot::Sender<std::result::Result<(RoomData, Vec<(TinId, LogChunk)>), String>>;

/// Current UNIX timestamp in seconds, used for `room_heads.updated_at`.
fn unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

/// Active bidirectional bridge connecting WebSocket clients in a room with a `banquette` process.
pub struct RoomBridge {
    /// 64-bit room identifier.
    pub room_id: u64,
    /// Connected client senders (client_id -> channel to client WebSocket).
    clients: Arc<RwLock<HashMap<u64, mpsc::Sender<ServerFrame>>>>,
    /// Current scene association for each client.
    client_scenes: Arc<RwLock<HashMap<u64, String>>>,
    /// Unbounded sender queue into the banquette UNIX socket.
    to_banquette_tx: mpsc::UnboundedSender<MaitreDToBanquette>,
    /// Pending export snapshot response senders.
    pending_exports: Arc<RwLock<HashMap<u64, ExportResponseSender>>>,
    /// Monotonic counter for export request IDs.
    export_counter: Arc<AtomicU64>,
    /// Done flag indicating whether room is fully public.
    is_done: Arc<AtomicBool>,
}

impl RoomBridge {
    /// Spawns a banquette for `room_id`, connects to its UNIX socket, and returns the bridge.
    ///
    /// `room_head` is the persisted `RoomData` tin pointer. The spawner restores
    /// the room from it, so a bridge is never silently created around an empty
    /// room when a head exists.
    ///
    /// # Errors
    ///
    /// Returns an error when the spawner fails (including a persisted head that
    /// cannot be downloaded, decoded, or validated) or the UNIX socket does not
    /// become connectable in time.
    pub async fn new(
        room_id: u64,
        admin_account: u64,
        room_head: Option<TinId>,
        spawner: Arc<dyn BanquetteSpawner>,
        db: Database,
        on_terminated: impl Fn(u64) + Send + Sync + 'static,
    ) -> Result<Self> {
        let clients = Arc::new(RwLock::new(HashMap::new()));
        let client_scenes = Arc::new(RwLock::new(HashMap::new()));
        let pending_exports = Arc::new(RwLock::new(HashMap::new()));
        let export_counter = Arc::new(AtomicU64::new(1));
        let is_done = Arc::new(AtomicBool::new(false));
        let (to_banquette_tx, mut to_banquette_rx) =
            mpsc::unbounded_channel::<MaitreDToBanquette>();

        let bridge_clients = Arc::clone(&clients);
        let bridge_scenes = Arc::clone(&client_scenes);
        let bridge_exports = Arc::clone(&pending_exports);

        tracing::info!("Launching banquette for room {room_id}...");
        let socket_path = spawner
            .spawn_banquette(room_id, admin_account, room_head)
            .await?;

        // Connect to UNIX domain socket with retry loop
        let stream = Self::connect_with_retry(&socket_path, 60, Duration::from_millis(50)).await?;

        tracing::info!(
            "Connected to banquette for room {room_id} at {:?}",
            socket_path
        );

        let (read_half, write_half) = stream.into_split();
        let mut writer = Framed::new(write_half, ProtocolCodec::<MaitreDToBanquette>::new());
        let mut reader = Framed::new(read_half, ProtocolCodec::<BanquetteToMaitreD>::new());

        // Task to flush and stream outgoing frames to banquette
        let write_task = tokio::spawn(async move {
            while let Some(msg) = to_banquette_rx.recv().await {
                if let Err(e) = writer.send(msg).await {
                    tracing::error!("Error writing frame to banquette: {e:?}");
                    break;
                }
            }
        });

        tokio::spawn(async move {
            // Read incoming responses and broadcasts from banquette
            while let Some(item) = reader.next().await {
                match item {
                    Ok(frame) => {
                        Self::dispatch_banquette_frame(
                            frame,
                            room_id,
                            &db,
                            &bridge_clients,
                            &bridge_scenes,
                            &bridge_exports,
                        )
                        .await;
                    }
                    Err(e) => {
                        tracing::error!(
                            "Error reading frame from banquette for room {room_id}: {e:?}"
                        );
                        break;
                    }
                }
            }

            write_task.abort();
            tracing::info!("Banquette bridge for room {room_id} terminated");
            {
                let mut pending = bridge_exports.write().await;
                pending.clear();
            }
            on_terminated(room_id);
        });

        Ok(Self {
            room_id,
            clients,
            client_scenes,
            to_banquette_tx,
            pending_exports,
            export_counter,
            is_done,
        })
    }

    /// Retries connecting to the UNIX domain socket until ready or timeout.
    async fn connect_with_retry(
        path: &PathBuf,
        max_attempts: usize,
        delay: Duration,
    ) -> Result<UnixStream> {
        for _attempt in 0..max_attempts {
            if path.exists()
                && let Ok(stream) = UnixStream::connect(path).await
            {
                return Ok(stream);
            }
            tokio::time::sleep(delay).await;
        }
        Err(MaitreDError::Banquette(format!(
            "Timed out waiting for banquette UNIX socket at {:?}",
            path
        )))
    }

    /// Dispatches frames received from `banquette` to connected WebSocket clients and pending queries.
    async fn dispatch_banquette_frame(
        frame: BanquetteToMaitreD,
        room_id: u64,
        db: &Database,
        clients: &Arc<RwLock<HashMap<u64, mpsc::Sender<ServerFrame>>>>,
        client_scenes: &Arc<RwLock<HashMap<u64, String>>>,
        pending_exports: &Arc<RwLock<HashMap<u64, ExportResponseSender>>>,
    ) {
        match frame {
            BanquetteToMaitreD::SendToClient { client_id, frame } => {
                let cl = clients.read().await;
                if let Some(tx) = cl.get(&client_id) {
                    let _ = tx.send(frame).await;
                }
            }
            BanquetteToMaitreD::BroadcastToRoom { frame } => {
                let cl = clients.read().await;
                for tx in cl.values() {
                    let _ = tx.send(frame.clone()).await;
                }
            }
            BanquetteToMaitreD::BroadcastToScene { scene_id, frame } => {
                let cl = clients.read().await;
                let scenes = client_scenes.read().await;
                for (cid, tx) in cl.iter() {
                    let matches = match scenes.get(cid) {
                        Some(curr_scene) => curr_scene == &scene_id,
                        None => scene_id == "/",
                    };
                    if matches {
                        let _ = tx.send(frame.clone()).await;
                    }
                }
            }
            BanquetteToMaitreD::Pong => {
                tracing::trace!("Received Pong from banquette");
            }
            BanquetteToMaitreD::RoomTerminated { reason } => {
                tracing::info!("Banquette process reported room termination: {reason}");
                let cl = clients.read().await;
                for tx in cl.values() {
                    let _ = tx
                        .send(ServerFrame::Error {
                            code: "ROOM_TERMINATED".to_string(),
                            message: reason.clone(),
                        })
                        .await;
                }
            }
            BanquetteToMaitreD::ExportRoomResponse { request_id, result } => {
                let mut pending = pending_exports.write().await;
                if let Some(tx) = pending.remove(&request_id) {
                    let _ = tx.send(result);
                }
            }
            // spec: alhalte-maitre-d/room-head 0001 persist RoomData head pointer
            // spec: alhalte-maitre-d/room-done 0001 persist the public flag with the head
            BanquetteToMaitreD::RoomPersisted { tin_id, done } => {
                let room_id_hex = format!("{room_id:016x}");
                // `done` lives on the room row (not in `RoomData`) so the
                // pre-spawn gate survives a maitre-d restart. A successful
                // persist (including a restored room's first persist) updates it.
                match db.set_room_done(&room_id_hex, done) {
                    Ok(true) => {}
                    Ok(false) => tracing::warn!(
                        "room {room_id_hex} reported done={done} but is not registered"
                    ),
                    Err(e) => {
                        tracing::error!("Failed to record done for {room_id_hex}: {e}");
                    }
                }
                match db.set_room_head(&room_id_hex, &tin_id, unix_timestamp()) {
                    Ok(previous) => {
                        if let Some(previous) = previous
                            && previous != tin_id
                        {
                            // banquette unregisters its own superseded head; this
                            // only happens when another instance persisted in the
                            // meantime, so retention is left to the owner of that
                            // head and only recorded here.
                            tracing::debug!(
                                "room {room_id_hex} head replaced {} -> {}",
                                hex::encode(previous),
                                hex::encode(tin_id)
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to record room head for {room_id_hex}: {e}");
                    }
                }
            }
        }
    }

    /// Registers a client connection to this room, returning a receiver for incoming server frames.
    pub async fn register_client(
        &self,
        client_id: u64,
        account_id: u64,
        session_id: u64,
    ) -> mpsc::Receiver<ServerFrame> {
        let (tx, rx) = mpsc::channel(128);
        {
            let mut cl = self.clients.write().await;
            cl.insert(client_id, tx);
        }
        {
            let mut scenes = self.client_scenes.write().await;
            scenes.insert(client_id, "/".to_string());
        }

        let _ = self
            .to_banquette_tx
            .send(MaitreDToBanquette::ClientConnected {
                client_id,
                account_id,
                session_id,
            });

        rx
    }

    /// Unregisters a client from this room.
    pub async fn unregister_client(&self, client_id: u64) {
        {
            let mut cl = self.clients.write().await;
            cl.remove(&client_id);
        }
        {
            let mut scenes = self.client_scenes.write().await;
            scenes.remove(&client_id);
        }

        let _ = self
            .to_banquette_tx
            .send(MaitreDToBanquette::ClientDisconnected { client_id });
    }

    /// Returns whether this room is marked as done (fully public).
    pub fn is_done(&self) -> bool {
        self.is_done.load(Ordering::Relaxed)
    }

    /// Sets the done flag for this room.
    pub fn set_done(&self, done: bool) {
        self.is_done.store(done, Ordering::Relaxed);
    }

    /// Sends a ServerFrame to a specific connected client.
    pub async fn send_to_client(&self, client_id: u64, frame: ServerFrame) {
        let cl = self.clients.read().await;
        if let Some(tx) = cl.get(&client_id) {
            let _ = tx.send(frame).await;
        }
    }

    /// Forwards a client frame to the room's `banquette` process.
    pub fn send_client_frame(&self, client_id: u64, frame: ClientFrame) {
        // Track scene movements locally
        match &frame {
            ClientFrame::EnterScene { scene_id } => {
                let scenes = Arc::clone(&self.client_scenes);
                let sid = scene_id.clone();
                tokio::spawn(async move {
                    scenes.write().await.insert(client_id, sid);
                });
            }
            ClientFrame::LeaveScene { .. } => {
                let scenes = Arc::clone(&self.client_scenes);
                tokio::spawn(async move {
                    scenes.write().await.remove(&client_id);
                });
            }
            ClientFrame::SetDone { done } => {
                self.is_done.store(*done, Ordering::Relaxed);
            }
            _ => {}
        }

        let _ = self
            .to_banquette_tx
            .send(MaitreDToBanquette::ClientMessage { client_id, frame });
    }

    /// Requests an export snapshot from the room's `banquette` process.
    pub async fn export_room(
        &self,
        caller_account: Option<u64>,
    ) -> Result<(RoomData, Vec<(TinId, LogChunk)>)> {
        let req_id = self.export_counter.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_exports.write().await;
            pending.insert(req_id, tx);
        }

        if let Err(e) = self.to_banquette_tx.send(MaitreDToBanquette::ExportRoom {
            request_id: req_id,
            caller_account,
        }) {
            let mut pending = self.pending_exports.write().await;
            pending.remove(&req_id);
            return Err(MaitreDError::Banquette(format!(
                "Failed to send ExportRoom to banquette: {e}"
            )));
        }

        match tokio::time::timeout(Duration::from_secs(30), rx).await {
            Ok(Ok(Ok(data))) => Ok(data),
            Ok(Ok(Err(err_msg))) => Err(MaitreDError::Banquette(err_msg)),
            Ok(Err(_)) => Err(MaitreDError::Banquette(
                "Export response channel dropped".to_string(),
            )),
            Err(_) => {
                let mut pending = self.pending_exports.write().await;
                pending.remove(&req_id);
                Err(MaitreDError::Banquette(
                    "Export request timed out".to_string(),
                ))
            }
        }
    }
}

/// Global registry managing active [`RoomBridge`] instances.
#[derive(Clone)]
pub struct RoomBridgeManager {
    rooms: Arc<RwLock<HashMap<u64, Arc<RoomBridge>>>>,
    spawner: Arc<dyn BanquetteSpawner>,
    db: Database,
}

impl RoomBridgeManager {
    /// Creates a new [`RoomBridgeManager`].
    ///
    /// The database is used to read the persisted room head before spawning
    /// banquette and to record the head reported by `RoomPersisted`.
    pub fn new(spawner: Arc<dyn BanquetteSpawner>, db: Database) -> Self {
        Self {
            rooms: Arc::new(RwLock::new(HashMap::new())),
            spawner,
            db,
        }
    }

    /// Obtains an existing [`RoomBridge`] or launches a new one from the persisted head.
    ///
    /// # Errors
    ///
    /// Returns an error when the room head cannot be read from the database or
    /// when banquette cannot be spawned / connected. A head that exists but
    /// cannot be restored surfaces here instead of yielding an empty room.
    pub async fn get_or_create_room(
        &self,
        room_id: u64,
        admin_account: u64,
    ) -> Result<Arc<RoomBridge>> {
        // Held across the awaited spawn so two concurrent callers cannot start
        // two banquette processes for the same room.
        let mut rooms = self.rooms.write().await;
        if let Some(bridge) = rooms.get(&room_id) {
            return Ok(Arc::clone(bridge));
        }

        let room_id_hex = format!("{room_id:016x}");
        let room_head = self.db.get_room_head(&room_id_hex)?;

        // The `rooms.done` row is authoritative across restarts; a restored
        // `RoomData.done` is written back through `RoomPersisted`.
        let initial_done = self.db.get_room_done(&room_id_hex)?.unwrap_or(false);

        let rooms_ref = Arc::clone(&self.rooms);
        let on_term = move |rid: u64| {
            let rooms_inner = Arc::clone(&rooms_ref);
            tokio::spawn(async move {
                rooms_inner.write().await.remove(&rid);
            });
        };

        let bridge = Arc::new(
            RoomBridge::new(
                room_id,
                admin_account,
                room_head,
                Arc::clone(&self.spawner),
                self.db.clone(),
                on_term,
            )
            .await?,
        );
        bridge.set_done(initial_done);

        rooms.insert(room_id, Arc::clone(&bridge));
        Ok(bridge)
    }

    /// Explicitly removes a room from management.
    pub async fn remove_room(&self, room_id: u64) {
        let mut rooms = self.rooms.write().await;
        rooms.remove(&room_id);
    }

    /// Checks whether a room is marked as done (fully public).
    ///
    /// The persisted `rooms.done` row is authoritative so a `maitre-d` restart
    /// still allows non-members through the pre-spawn gate before any banquette
    /// process exists. The in-memory bridge is only a fallback if the row is
    /// missing or cannot be read.
    pub async fn is_room_done(&self, room_id: u64) -> bool {
        let room_id_hex = format!("{room_id:016x}");
        match self.db.get_room_done(&room_id_hex) {
            Ok(Some(done)) => return done,
            Ok(None) => {}
            Err(e) => tracing::error!("Failed to read done for room {room_id_hex}: {e}"),
        }

        let rooms = self.rooms.read().await;
        rooms
            .get(&room_id)
            .map(|bridge| bridge.is_done())
            .unwrap_or(false)
    }

    /// Sets the done flag for a room and persists it to the database.
    ///
    /// Persisting happens even when no bridge is live (for example before the
    /// first WebSocket connection) so the flag is visible after a restart.
    pub async fn set_room_done(&self, room_id: u64, done: bool) {
        let room_id_hex = format!("{room_id:016x}");
        match self.db.set_room_done(&room_id_hex, done) {
            Ok(true) => {}
            Ok(false) => {
                tracing::warn!("Cannot persist done={done}: room {room_id_hex} is not registered")
            }
            Err(e) => tracing::error!("Failed to persist done for room {room_id_hex}: {e}"),
        }

        let rooms = self.rooms.read().await;
        if let Some(bridge) = rooms.get(&room_id) {
            bridge.set_done(done);
        }
    }
}
