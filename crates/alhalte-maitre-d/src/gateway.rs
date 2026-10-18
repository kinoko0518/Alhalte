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

use crate::error::{MaitreDError, Result};
use crate::spawner::BanquetteSpawner;

type ExportResponseSender =
    oneshot::Sender<std::result::Result<(RoomData, Vec<(TinId, LogChunk)>), String>>;

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
    /// Creates and spawns a new [`RoomBridge`].
    ///
    /// Messages queued before the UNIX socket is connected are preserved and flushed once connected.
    pub fn new(
        room_id: u64,
        admin_account: u64,
        spawner: Arc<dyn BanquetteSpawner>,
        on_terminated: impl Fn(u64) + Send + Sync + 'static,
    ) -> Self {
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

        tokio::spawn(async move {
            tracing::info!("Launching banquette for room {room_id}...");
            let socket_path = match spawner.spawn_banquette(room_id, admin_account).await {
                Ok(path) => path,
                Err(e) => {
                    tracing::error!("Failed to spawn banquette for room {room_id}: {e:?}");
                    on_terminated(room_id);
                    return;
                }
            };

            // Connect to UNIX domain socket with retry loop
            let stream = match Self::connect_with_retry(&socket_path, 60, Duration::from_millis(50))
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(
                        "Failed to connect to banquette socket at {:?} for room {room_id}: {e:?}",
                        socket_path
                    );
                    on_terminated(room_id);
                    return;
                }
            };

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

            // Read incoming responses and broadcasts from banquette
            while let Some(item) = reader.next().await {
                match item {
                    Ok(frame) => {
                        Self::dispatch_banquette_frame(
                            frame,
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

        Self {
            room_id,
            clients,
            client_scenes,
            to_banquette_tx,
            pending_exports,
            export_counter,
            is_done,
        }
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
    room_done_status: Arc<RwLock<HashMap<u64, bool>>>,
}

impl RoomBridgeManager {
    /// Creates a new [`RoomBridgeManager`].
    pub fn new(spawner: Arc<dyn BanquetteSpawner>) -> Self {
        Self {
            rooms: Arc::new(RwLock::new(HashMap::new())),
            spawner,
            room_done_status: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Obtains an existing [`RoomBridge`] or launches a new one.
    pub async fn get_or_create_room(&self, room_id: u64, admin_account: u64) -> Arc<RoomBridge> {
        let mut rooms = self.rooms.write().await;
        if let Some(bridge) = rooms.get(&room_id) {
            return Arc::clone(bridge);
        }

        let initial_done = self
            .room_done_status
            .read()
            .await
            .get(&room_id)
            .copied()
            .unwrap_or(false);

        let rooms_ref = Arc::clone(&self.rooms);
        let on_term = move |rid: u64| {
            let rooms_inner = Arc::clone(&rooms_ref);
            tokio::spawn(async move {
                rooms_inner.write().await.remove(&rid);
            });
        };

        let bridge = Arc::new(RoomBridge::new(
            room_id,
            admin_account,
            Arc::clone(&self.spawner),
            on_term,
        ));
        bridge.set_done(initial_done);

        rooms.insert(room_id, Arc::clone(&bridge));
        bridge
    }

    /// Explicitly removes a room from management.
    pub async fn remove_room(&self, room_id: u64) {
        let mut rooms = self.rooms.write().await;
        rooms.remove(&room_id);
    }

    /// Checks whether a room is marked as done (fully public).
    pub async fn is_room_done(&self, room_id: u64) -> bool {
        let rooms = self.rooms.read().await;
        if let Some(bridge) = rooms.get(&room_id) {
            bridge.is_done()
        } else {
            let statuses = self.room_done_status.read().await;
            statuses.get(&room_id).copied().unwrap_or(false)
        }
    }

    /// Sets the done flag for a room in memory.
    pub async fn set_room_done(&self, room_id: u64, done: bool) {
        {
            let mut statuses = self.room_done_status.write().await;
            statuses.insert(room_id, done);
        }
        let rooms = self.rooms.read().await;
        if let Some(bridge) = rooms.get(&room_id) {
            bridge.set_done(done);
        }
    }
}
