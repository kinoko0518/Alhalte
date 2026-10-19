//! WebSocket room demultiplexer and UNIX socket communication bridge with `alhalte-banquette`.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use alhalte_banquette::{
    BanquetteToMaitreD, ClientFrame, LogChunk, MaitreDToBanquette, ProtocolCodec, RoomData,
    RoomPermissionLevel, ServerFrame,
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

/// How long a one-shot permission request waits for an answer from banquette.
///
/// A permission query always produces one `AccountPermissionResponse`, and a
/// permission mutation produces exactly one `PermissionsUpdated` or `Error`.
/// The timeout only guards against a wedged room process.
const PERMISSION_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

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
    /// Pending `QueryAccountPermission` response senders.
    pending_permission_queries: Arc<RwLock<HashMap<u64, oneshot::Sender<RoomPermissionLevel>>>>,
    /// Monotonic counter for export request IDs.
    export_counter: Arc<AtomicU64>,
    /// Monotonic counter for `QueryAccountPermission` request IDs.
    permission_counter: Arc<AtomicU64>,
    /// Client ID namespace for the transient clients used by permission requests.
    ///
    /// Real WebSocket clients are allocated upward from 1, so this counter walks
    /// down from `u64::MAX`; the two namespaces cannot collide.
    proxy_client_counter: Arc<AtomicU64>,
}

impl RoomBridge {
    /// Spawns a banquette for `room_id`, connects to its UNIX socket, and returns the bridge.
    ///
    /// `admin_account` is the room creator, which banquette makes its initial
    /// `Admin` when the room starts from scratch. `room_head` is the persisted
    /// `RoomData` tin pointer. The spawner restores the room from it, so a
    /// bridge is never silently created around an empty room when a head exists.
    ///
    /// `persist_head` controls `room_heads`: when false (debug login) a reported
    /// `RoomPersisted` is not recorded, because the room process keeps its state
    /// in memory and the tin would be gone after a restart.
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
        persist_head: bool,
        on_terminated: impl Fn(u64) + Send + Sync + 'static,
    ) -> Result<Self> {
        let clients = Arc::new(RwLock::new(HashMap::new()));
        let client_scenes = Arc::new(RwLock::new(HashMap::new()));
        let pending_exports = Arc::new(RwLock::new(HashMap::new()));
        let pending_permission_queries = Arc::new(RwLock::new(HashMap::new()));
        let export_counter = Arc::new(AtomicU64::new(1));
        let permission_counter = Arc::new(AtomicU64::new(1));
        let proxy_client_counter = Arc::new(AtomicU64::new(u64::MAX));
        let (to_banquette_tx, mut to_banquette_rx) =
            mpsc::unbounded_channel::<MaitreDToBanquette>();

        let bridge_clients = Arc::clone(&clients);
        let bridge_scenes = Arc::clone(&client_scenes);
        let bridge_exports = Arc::clone(&pending_exports);
        let bridge_permission_queries = Arc::clone(&pending_permission_queries);

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
                            persist_head,
                            &db,
                            &bridge_clients,
                            &bridge_scenes,
                            &bridge_exports,
                            &bridge_permission_queries,
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
            {
                let mut pending = bridge_permission_queries.write().await;
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
            pending_permission_queries,
            export_counter,
            permission_counter,
            proxy_client_counter,
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
    ///
    /// `persist_head` is forwarded to the `RoomPersisted` arm: a debug-login
    /// instance runs banquette with `--no-persist`, so the reported tin does not
    /// survive the process and must not become a `room_heads` row.
    async fn dispatch_banquette_frame(
        frame: BanquetteToMaitreD,
        room_id: u64,
        persist_head: bool,
        db: &Database,
        clients: &Arc<RwLock<HashMap<u64, mpsc::Sender<ServerFrame>>>>,
        client_scenes: &Arc<RwLock<HashMap<u64, String>>>,
        pending_exports: &Arc<RwLock<HashMap<u64, ExportResponseSender>>>,
        pending_permission_queries: &Arc<
            RwLock<HashMap<u64, oneshot::Sender<RoomPermissionLevel>>>,
        >,
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
            //
            // `RoomPersisted.default_permission` is not stored: the room's
            // permission table lives in the persisted `RoomData` and `maitre-d`
            // keeps no copy of it (Issue #200). Only the head pointer is
            // recorded, and only after banquette reports a successful store.
            BanquetteToMaitreD::RoomPersisted { tin_id, .. } => {
                if !persist_head {
                    // Debug login: banquette runs with `--no-persist`, so the
                    // reported tin only lives in that process. Recording it would
                    // make the next spawn fail to restore an in-memory room.
                    tracing::debug!(
                        "room {room_id:016x} persisted tin {} in memory only (no-persist)",
                        hex::encode(tin_id)
                    );
                    return;
                }
                let room_id_hex = format!("{room_id:016x}");
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
            // answer to `RoomBridge::query_account_permission`.
            BanquetteToMaitreD::AccountPermissionResponse {
                request_id,
                account_id,
                permission,
            } => {
                let mut pending = pending_permission_queries.write().await;
                if let Some(tx) = pending.remove(&request_id) {
                    let _ = tx.send(permission);
                } else {
                    tracing::debug!(
                        "room {} account {account_id} permission {permission:?} (request {request_id}) had no waiter",
                        format!("{room_id:016x}")
                    );
                }
            }
        }
    }

    /// Asks banquette for the effective permission of `account_id`.
    ///
    /// `0` asks for the room default, which is what an anonymous client gets.
    /// The answer is banquette's `RoomData.permissions` /
    /// `RoomData.default_permission`; `maitre-d` has no permission table of its
    /// own (Issue #200).
    ///
    /// # Errors
    ///
    /// Returns an error when the request cannot be delivered or banquette does
    /// not answer before [`PERMISSION_REQUEST_TIMEOUT`].
    pub async fn query_account_permission(&self, account_id: u64) -> Result<RoomPermissionLevel> {
        let request_id = self.permission_counter.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_permission_queries.write().await;
            pending.insert(request_id, tx);
        }

        if let Err(e) = self
            .to_banquette_tx
            .send(MaitreDToBanquette::QueryAccountPermission {
                request_id,
                account_id,
            })
        {
            let mut pending = self.pending_permission_queries.write().await;
            pending.remove(&request_id);
            return Err(MaitreDError::Banquette(format!(
                "Failed to send QueryAccountPermission to banquette: {e}"
            )));
        }

        match tokio::time::timeout(PERMISSION_REQUEST_TIMEOUT, rx).await {
            Ok(Ok(permission)) => Ok(permission),
            Ok(Err(_)) => Err(MaitreDError::Banquette(
                "Permission response channel dropped".to_string(),
            )),
            Err(_) => {
                let mut pending = self.pending_permission_queries.write().await;
                pending.remove(&request_id);
                Err(MaitreDError::Banquette(
                    "Account permission query timed out".to_string(),
                ))
            }
        }
    }

    /// Returns the room's permission table as banquette sees it.
    ///
    /// The query is sent as `account_id` through a transient client, so
    /// banquette itself decides whether that account may read the table. A
    /// `None` answer means banquette returned nothing (an account without
    /// access to the root scene).
    ///
    /// # Errors
    ///
    /// Returns an error when the request cannot be delivered or banquette does
    /// not answer before [`PERMISSION_REQUEST_TIMEOUT`].
    pub async fn permissions_snapshot(
        &self,
        account_id: u64,
    ) -> Result<Option<(RoomPermissionLevel, BTreeMap<u64, RoomPermissionLevel>)>> {
        let frame = self
            .send_permission_frame(account_id, ClientFrame::QueryPermissions, |frame| {
                matches!(
                    frame,
                    ServerFrame::PermissionsSnapshot { .. } | ServerFrame::Error { .. }
                )
            })
            .await?;

        match frame {
            ServerFrame::PermissionsSnapshot {
                default_permission,
                permissions,
            } => Ok(Some((default_permission, permissions))),
            ServerFrame::Error { code, message } => Err(MaitreDError::RoomPermissionRefused(
                format!("banquette refused the permission request ({code}): {message}"),
            )),
            other => Err(MaitreDError::Banquette(format!(
                "unexpected answer to QueryPermissions: {other:?}"
            ))),
        }
    }

    /// Applies [`ClientFrame::SetMemberPermission`] as `caller_account`.
    ///
    /// banquette authorizes the request: a caller that is not an administrator,
    /// or a demotion of the last administrator, comes back as an `Error` frame
    /// and is surfaced as [`MaitreDError::Banquette`].
    ///
    /// # Errors
    ///
    /// See [`RoomBridge::query_account_permission`]; a refused permission change
    /// is also an error.
    pub async fn set_member_permission(
        &self,
        caller_account: u64,
        target_account: u64,
        permission: RoomPermissionLevel,
    ) -> Result<()> {
        self.send_permission_mutation(
            caller_account,
            ClientFrame::SetMemberPermission {
                account_id: target_account,
                permission,
            },
        )
        .await
    }

    /// Applies [`ClientFrame::SetDefaultPermission`] as `caller_account`.
    ///
    /// # Errors
    ///
    /// See [`RoomBridge::set_member_permission`].
    pub async fn set_default_permission(
        &self,
        caller_account: u64,
        permission: RoomPermissionLevel,
    ) -> Result<()> {
        self.send_permission_mutation(
            caller_account,
            ClientFrame::SetDefaultPermission { permission },
        )
        .await
    }

    /// Sends a permission mutation and waits for banquette's acknowledgement.
    async fn send_permission_mutation(
        &self,
        caller_account: u64,
        frame: ClientFrame,
    ) -> Result<()> {
        let frame = self
            .send_permission_frame(caller_account, frame, |frame| {
                matches!(
                    frame,
                    ServerFrame::PermissionsUpdated { .. } | ServerFrame::Error { .. }
                )
            })
            .await?;

        match frame {
            ServerFrame::PermissionsUpdated { .. } => Ok(()),
            ServerFrame::Error { code, message } => Err(MaitreDError::RoomPermissionRefused(
                format!("banquette refused the permission request ({code}): {message}"),
            )),
            other => Err(MaitreDError::Banquette(format!(
                "unexpected answer to a permission change: {other:?}"
            ))),
        }
    }

    /// Sends one client frame through a transient client and returns the first
    /// frame banquette addresses to it that `accept` matches.
    ///
    /// Room / scene broadcasts reach the transient client as well, so frames
    /// that are not the answer are skipped until the deadline.
    async fn send_permission_frame(
        &self,
        account_id: u64,
        frame: ClientFrame,
        mut accept: impl FnMut(&ServerFrame) -> bool,
    ) -> Result<ServerFrame> {
        let client_id = self.proxy_client_counter.fetch_sub(1, Ordering::Relaxed);
        let mut rx = self.register_client(client_id, account_id, 0).await;

        if let Err(e) = self
            .to_banquette_tx
            .send(MaitreDToBanquette::ClientMessage { client_id, frame })
        {
            self.unregister_client(client_id).await;
            return Err(MaitreDError::Banquette(format!(
                "Failed to send a permission request to banquette: {e}"
            )));
        }

        let deadline = tokio::time::Instant::now() + PERMISSION_REQUEST_TIMEOUT;
        let mut answer = None;
        loop {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(frame)) => {
                    if accept(&frame) {
                        answer = Some(frame);
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }

        self.unregister_client(client_id).await;

        match answer {
            Some(frame) => Ok(frame),
            None => Err(MaitreDError::Banquette(
                "banquette did not answer the permission request".to_string(),
            )),
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

    /// Sends a ServerFrame to a specific connected client.
    pub async fn send_to_client(&self, client_id: u64, frame: ServerFrame) {
        let cl = self.clients.read().await;
        if let Some(tx) = cl.get(&client_id) {
            let _ = tx.send(frame).await;
        }
    }

    /// Forwards a client frame to the room's `banquette` process.
    ///
    /// Every `ClientFrame` is forwarded unchanged: authorization (including the
    /// viewer/full-access split) is enforced by banquette's room permission
    /// table, never by `maitre-d` (Issue #200).
    pub fn send_client_frame(&self, client_id: u64, frame: ClientFrame) {
        // Track scene movements locally so scene broadcasts are demultiplexed.
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
    persist_head: bool,
}

impl RoomBridgeManager {
    /// Creates a new [`RoomBridgeManager`].
    ///
    /// The database is used to read the persisted room head before spawning
    /// banquette and to record the head reported by `RoomPersisted`. Use
    /// [`RoomBridgeManager::without_persistence`] to disable both, as
    /// `maitre-d --debug-login` does.
    pub fn new(spawner: Arc<dyn BanquetteSpawner>, db: Database) -> Self {
        Self {
            rooms: Arc::new(RwLock::new(HashMap::new())),
            spawner,
            db,
            persist_head: true,
        }
    }

    /// Stops reading and recording `room_heads` (Issue #150).
    ///
    /// With `--debug-login` the spawned banquette runs with `--no-persist`: room
    /// state lives in that process and disappears with it. Restoring from a
    /// recorded head would fail, so the debug instance starts every room fresh
    /// and leaves the (debug) `room_heads` table untouched.
    #[must_use]
    pub fn without_persistence(mut self) -> Self {
        self.persist_head = false;
        self
    }

    /// Returns whether `room_heads` is read and written.
    #[must_use]
    pub fn persists_room_heads(&self) -> bool {
        self.persist_head
    }

    /// Obtains an existing [`RoomBridge`] or launches a new one from the persisted head.
    ///
    /// `admin_account` is the room's `rooms.creator_account`. It is the only
    /// source of the initial administrator: neither the connecting account nor a
    /// database role decides it (Issue #200).
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
        let room_head = if self.persist_head {
            self.db.get_room_head(&room_id_hex)?
        } else {
            None
        };

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
                self.persist_head,
                on_term,
            )
            .await?,
        );

        rooms.insert(room_id, Arc::clone(&bridge));
        Ok(bridge)
    }

    /// Explicitly removes a room from management.
    pub async fn remove_room(&self, room_id: u64) {
        let mut rooms = self.rooms.write().await;
        rooms.remove(&room_id);
    }
}
