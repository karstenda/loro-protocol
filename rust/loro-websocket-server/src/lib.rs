//! Loro WebSocket Server (simple skeleton)
//!
//! Minimal async WebSocket server that accepts connections and echoes binary
//! protocol frames back to clients. It also responds to text "ping" with
//! text "pong" as described in protocol.md keepalive section.
//!
//! This is intentionally simple and is meant as a starting point. Application
//! logic (authorization, room routing, broadcasting, etc.) should be layered
//! on top using the `loro_protocol` crate for message encoding/decoding.
//!
//! Example (not run here because it binds a socket):
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! #   let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
//! #   rt.block_on(async move {
//! loro_websocket_server::serve("127.0.0.1:9000").await?;
//! #   Ok(())
//! # })
//! # }
//! ```

use futures_util::{SinkExt, StreamExt};
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    hash::{Hash, Hasher},
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::frame::CloseFrame;
use tokio_tungstenite::tungstenite::{self, Message};

use loro::awareness::EphemeralStore;
use loro::{ExportMode, LoroDoc};
pub use loro_protocol as protocol;
use protocol::{try_decode, CrdtType, JoinErrorCode, Permission, ProtocolMessage};
use tracing::{debug, error, info, warn};

#[derive(Clone, Debug, PartialEq, Eq)]
struct RoomKey {
    crdt: CrdtType,
    room: Vec<u8>,
}
impl Hash for RoomKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // CrdtType is repr as enum with a few variants; map to u8 for hashing
        let tag = match self.crdt {
            CrdtType::Loro => 0u8,
            CrdtType::LoroEphemeralStore => 1,
            CrdtType::Yjs => 2,
            CrdtType::YjsAwareness => 3,
        };
        tag.hash(state);
        self.room.hash(state);
    }
}

type Sender = mpsc::UnboundedSender<Message>;

// Hook types
type LoadFuture = Pin<Box<dyn Future<Output = Result<Option<Vec<u8>>, String>> + Send + 'static>>;
type SaveFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'static>>;
// workspace, room, crdt
type LoadFn = Arc<dyn Fn(String, String, CrdtType) -> LoadFuture + Send + Sync>;
// workspace, room, crdt, data
type SaveFn = Arc<dyn Fn(String, String, CrdtType, Vec<u8>) -> SaveFuture + Send + Sync>;
type AuthFuture =
    Pin<Box<dyn Future<Output = Result<Option<Permission>, String>> + Send + 'static>>;
type AuthFn = Arc<dyn Fn(String, CrdtType, Vec<u8>) -> AuthFuture + Send + Sync>;

type HandshakeAuthFn = dyn Fn(&str, Option<&str>) -> bool + Send + Sync;

#[derive(Clone)]
pub struct ServerConfig {
    pub on_load_document: Option<LoadFn>,
    pub on_save_document: Option<SaveFn>,
    pub save_interval_ms: Option<u64>,
    pub default_permission: Permission,
    pub authenticate: Option<AuthFn>,
    /// Optional handshake auth: called during WS HTTP upgrade.
    ///
    /// Parameters:
    /// - `workspace_id`: extracted from request path `/{workspace}` (empty if missing)
    /// - `token`: `token` query parameter if present
    ///
    /// Return true to accept, false to reject with 401.
    pub handshake_auth: Option<Arc<HandshakeAuthFn>>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            on_load_document: None,
            on_save_document: None,
            save_interval_ms: None,
            default_permission: Permission::Write,
            authenticate: None,
            handshake_auth: None,
        }
    }
}

enum RoomDocState {
    LoroDocRoomState {
        doc: LoroDoc,
        updates: Vec<Vec<u8>>, // accumulated updates in this process lifetime
        dirty: bool,
    },
    EphemeralStoreRoomState {
        doc: EphemeralStore,
        updates: Vec<Vec<u8>>, // accumulated updates only; never persisted
    },
}

struct Hub {
    // room -> vec of (conn_id, sender)
    subs: HashMap<RoomKey, Vec<(u64, Sender)>>,
    // room -> document state (Loro persistent, or Ephemeral in-memory)
    docs: HashMap<RoomKey, RoomDocState>,
    config: ServerConfig,
    // (conn_id, room) -> permission
    perms: HashMap<(u64, RoomKey), Permission>,
    workspace: String,
}

impl Hub {
    fn new(config: ServerConfig, workspace: String) -> Self {
        Self {
            subs: HashMap::new(),
            docs: HashMap::new(),
            config,
            perms: HashMap::new(),
            workspace,
        }
    }

    const EPHEMERAL_TIMEOUT_MS: i64 = 60_000;

    fn join(&mut self, conn_id: u64, room: RoomKey, tx: &Sender) {
        let entry = self.subs.entry(room).or_default();
        if !entry.iter().any(|(id, _)| *id == conn_id) {
            entry.push((conn_id, tx.clone()));
        }
    }

    fn leave_all(&mut self, conn_id: u64) {
        let mut emptied: Vec<RoomKey> = Vec::new();
        for (k, vec) in self.subs.iter_mut() {
            vec.retain(|(id, _)| *id != conn_id);
            if vec.is_empty() {
                emptied.push(k.clone());
            }
        }
        // Drop empty rooms from subscription map
        for k in &emptied {
            let _ = self.subs.remove(k);
        }

        // Remove permissions for this connection
        self.perms.retain(|(id, _), _| *id != conn_id);

        // Clean up ephemeral state for rooms that no longer have subscribers
        for k in emptied {
            if let Some(state) = self.docs.get(&k) {
                if matches!(state, RoomDocState::EphemeralStoreRoomState { .. }) {
                    self.docs.remove(&k);
                    debug!(room=?String::from_utf8_lossy(&k.room), "cleaned up ephemeral store after last subscriber left");
                }
            }
        }
    }

    fn broadcast(&mut self, room: &RoomKey, from: u64, msg: Message) {
        if let Some(list) = self.subs.get_mut(room) {
            // drop dead senders
            let mut dead: HashSet<u64> = HashSet::new();
            for (id, tx) in list.iter() {
                if *id == from {
                    continue;
                }
                if tx.send(msg.clone()).is_err() {
                    dead.insert(*id);
                }
            }
            if !dead.is_empty() {
                list.retain(|(id, _)| !dead.contains(id));
                debug!(room=?String::from_utf8_lossy(&room.room), removed=%dead.len(), "removed dead subscribers");
            }
        }
    }

    async fn ensure_room_loaded(&mut self, room: &RoomKey) {
        match room.crdt {
            CrdtType::Loro => {
                if self.docs.contains_key(room) {
                    return;
                }
                let mut doc = LoroDoc::new();
                // attempt load via hook
                if let Some(loader) = &self.config.on_load_document {
                    let room_str = String::from_utf8_lossy(&room.room).to_string();
                    let ws = self.workspace.clone();
                    match (loader)(ws, room_str, room.crdt).await {
                        Ok(Some(bytes)) => {
                            let _ = doc.import(&bytes);
                        }
                        Ok(None) => {}
                        Err(e) => {
                            warn!(room=?String::from_utf8_lossy(&room.room), %e, "load document failed");
                        }
                    }
                }
                self.docs.insert(
                    room.clone(),
                    RoomDocState::LoroDocRoomState {
                        doc,
                        updates: Vec::new(),
                        dirty: false,
                    },
                );
            }
            CrdtType::LoroEphemeralStore => {
                // Ephemeral documents are never loaded from persistence.
                if !self.docs.contains_key(room) {
                    let doc = EphemeralStore::new(Self::EPHEMERAL_TIMEOUT_MS);
                    self.docs.insert(
                        room.clone(),
                        RoomDocState::EphemeralStoreRoomState {
                            doc,
                            updates: Vec::new(),
                        },
                    );
                }
            }
            _ => {
                // Unsupported types in this simple server; still create a subscription bucket.
                // We don't track state for Yjs/YjsAwareness here.
            }
        }
    }

    fn current_version_bytes(&self, room: &RoomKey) -> Vec<u8> {
        match self.docs.get(room) {
            Some(RoomDocState::LoroDocRoomState { .. }) => {
                // TODO: we could encode actual version; keep empty for now
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn apply_updates(&mut self, room: &RoomKey, updates: &[Vec<u8>]) {
        if let Some(state) = self.docs.get_mut(room) {
            match state {
                RoomDocState::LoroDocRoomState {
                    doc,
                    updates: ups,
                    dirty,
                } => {
                    for u in updates {
                        let _ = doc.import(u);
                        ups.push(u.clone());
                    }
                    *dirty = true;
                }
                RoomDocState::EphemeralStoreRoomState { doc, updates: ups } => {
                    for u in updates {
                        if !u.is_empty() {
                            doc.apply(u);
                        }
                        ups.push(u.clone());
                    }
                }
            }
        }
    }

    fn snapshot_bytes(&self, room: &RoomKey) -> Option<Vec<u8>> {
        match self.docs.get(room) {
            Some(RoomDocState::LoroDocRoomState { doc, .. }) => {
                doc.export(ExportMode::Snapshot).ok()
            }
            _ => None,
        }
    }

    fn ephemeral_full_state(&self, room: &RoomKey) -> Option<Vec<u8>> {
        match self.docs.get(room) {
            Some(RoomDocState::EphemeralStoreRoomState { doc, .. }) => Some(doc.encode_all()),
            _ => None,
        }
    }
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

struct HubRegistry {
    config: ServerConfig,
    hubs: tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<Hub>>>>,
}

impl HubRegistry {
    fn new(config: ServerConfig) -> Self {
        Self {
            config,
            hubs: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    async fn get_or_create(&self, workspace: &str) -> Arc<tokio::sync::Mutex<Hub>> {
        let mut map = self.hubs.lock().await;
        if let Some(h) = map.get(workspace) {
            return h.clone();
        }
        let hub = Arc::new(tokio::sync::Mutex::new(Hub::new(
            self.config.clone(),
            workspace.to_string(),
        )));
        // Spawn saver task for this hub if configured
        if let (Some(ms), Some(saver)) = (
            self.config.save_interval_ms,
            self.config.on_save_document.clone(),
        ) {
            let hub_clone = hub.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_millis(ms));
                loop {
                    interval.tick().await;
                    let mut guard = hub_clone.lock().await;
                    let ws = guard.workspace.clone();
                    let rooms: Vec<RoomKey> = guard.docs.keys().cloned().collect();
                    for room in rooms {
                        if let Some(state) = guard.docs.get_mut(&room) {
                            match state {
                                RoomDocState::LoroDocRoomState { doc, dirty, .. } => {
                                    if *dirty {
                                        let start = std::time::Instant::now();
                                        if let Ok(snapshot) = doc.export(ExportMode::Snapshot) {
                                            let room_str =
                                                String::from_utf8_lossy(&room.room).to_string();
                                            match (saver)(
                                                ws.clone(),
                                                room_str.clone(),
                                                room.crdt,
                                                snapshot,
                                            )
                                            .await
                                            {
                                                Ok(()) => {
                                                    *dirty = false;
                                                    let elapsed = start.elapsed();
                                                    debug!(workspace=%ws, room=%room_str, ms=%elapsed.as_millis(), "snapshot saved");
                                                }
                                                Err(e) => {
                                                    warn!(workspace=%ws, room=%room_str, %e, "snapshot save failed");
                                                }
                                            }
                                        }
                                    }
                                }
                                RoomDocState::EphemeralStoreRoomState { .. } => {
                                    // never persisted
                                }
                            }
                        }
                    }
                }
            });
        }
        map.insert(workspace.to_string(), hub.clone());
        hub
    }
}

/// Start a simple broadcast server on the given socket address.
pub async fn serve(addr: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!(%addr, "binding TCP listener");
    let listener = TcpListener::bind(addr).await?;
    serve_incoming_with_config(listener, ServerConfig::default()).await
}

/// Serve a pre-bound listener. Useful for tests to bind on port 0.
pub async fn serve_incoming(
    listener: TcpListener,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    serve_incoming_with_config(listener, ServerConfig::default()).await
}

pub async fn serve_incoming_with_config(
    listener: TcpListener,
    config: ServerConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let registry = Arc::new(HubRegistry::new(config.clone()));

    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                debug!(remote=%peer, "accepted TCP connection");
                let registry = registry.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_conn(stream, registry).await {
                        warn!(%e, "connection task ended with error");
                    }
                });
            }
            Err(e) => {
                error!(%e, "accept failed; continuing");
                continue;
            }
        }
    }
}

async fn handle_conn(
    stream: TcpStream,
    registry: Arc<HubRegistry>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Capture config outside of non-async closure
    let handshake_auth = registry.config.handshake_auth.clone();
    let workspace_holder: Arc<std::sync::Mutex<Option<String>>> =
        Arc::new(std::sync::Mutex::new(None));
    let workspace_holder_c = workspace_holder.clone();

    let ws = accept_hdr_async(
        stream,
        move |req: &tungstenite::handshake::server::Request,
              resp: tungstenite::handshake::server::Response| {
            if let Some(check) = &handshake_auth {
                // Parse path: expect "/{workspace}" (workspace may be empty)
                let uri = req.uri();
                let path = uri.path();
                let mut workspace_id = "";
                if let Some(rest) = path.strip_prefix('/') {
                    if !rest.is_empty() {
                        // take first segment as workspace id
                        workspace_id = rest.split('/').next().unwrap_or("");
                    }
                }
                // Save for later
                {
                    if let Ok(mut guard) = workspace_holder_c.lock() {
                        *guard = Some(workspace_id.to_string());
                    }
                }

                // Parse query token parameter (no external deps)
                let token = uri.query().and_then(|q| {
                    for pair in q.split('&') {
                        let mut it = pair.splitn(2, '=');
                        let k = it.next().unwrap_or("");
                        let v = it.next();
                        if k == "token" {
                            return Some(v.unwrap_or(""));
                        }
                    }
                    None
                });

                let allowed = (check)(workspace_id, token);
                if !allowed {
                    warn!(workspace=%workspace_id, token=?token, "handshake auth denied");
                    // Build a 401 Unauthorized response
                    let builder = tungstenite::http::Response::builder()
                        .status(tungstenite::http::StatusCode::UNAUTHORIZED);
                    // Provide a small body for clarity
                    let response = builder
                        .body(Some("Unauthorized".to_string()))
                        .unwrap_or_else(|_| {
                            tungstenite::http::Response::builder()
                                .status(401)
                                .body(None)
                                .unwrap()
                        });
                    return Err(response);
                }
                debug!(workspace=%workspace_id, token=?token, "handshake auth accepted");
            }
            Ok(resp)
        },
    )
    .await?;

    // Determine workspace id (default to empty string)
    let workspace_id = workspace_holder
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default();
    let hub = registry.get_or_create(&workspace_id).await;

    // writer task channel
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let (mut sink, mut stream) = ws.split();
    // writer
    let sink_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sink.send(msg).await.is_err() {
                debug!("sink send error; writer task exiting");
                break;
            }
        }
    });

    let conn_id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let mut joined_rooms: HashSet<RoomKey> = HashSet::new();

    while let Some(msg) = stream.next().await {
        match msg? {
            Message::Text(txt) => {
                if txt == "ping" {
                    let _ = tx.send(Message::Text("pong".into()));
                }
            }
            Message::Binary(data) => {
                if let Some(proto) = try_decode(data.as_ref()) {
                    match proto {
                        ProtocolMessage::JoinRequest {
                            crdt,
                            room_id,
                            auth,
                            ..
                        } => {
                            let room = RoomKey {
                                crdt,
                                room: room_id.clone(),
                            };
                            let mut h = hub.lock().await;
                            // ensure doc exists / load
                            h.ensure_room_loaded(&room).await;
                            // authenticate
                            let mut permission = h.config.default_permission;
                            if let Some(auth_fn) = &h.config.authenticate {
                                let room_str = String::from_utf8_lossy(&room.room).to_string();
                                match (auth_fn)(room_str, room.crdt, auth.clone()).await {
                                    Ok(Some(p)) => {
                                        permission = p;
                                    }
                                    Ok(None) => {
                                        let err = ProtocolMessage::JoinError {
                                            crdt,
                                            room_id: room.room.clone(),
                                            code: JoinErrorCode::AuthFailed,
                                            message: "Authentication failed".into(),
                                            receiver_version: None,
                                            app_code: None,
                                        };
                                        if let Ok(bytes) = loro_protocol::encode(&err) {
                                            let _ = tx.send(Message::Binary(bytes.into()));
                                        }
                                        warn!(room=?String::from_utf8_lossy(&room.room), "join denied by authenticate() returning None");
                                        continue;
                                    }
                                    Err(e) => {
                                        let err = ProtocolMessage::JoinError {
                                            crdt,
                                            room_id: room.room.clone(),
                                            code: JoinErrorCode::Unknown,
                                            message: e,
                                            receiver_version: None,
                                            app_code: None,
                                        };
                                        if let Ok(bytes) = loro_protocol::encode(&err) {
                                            let _ = tx.send(Message::Binary(bytes.into()));
                                        }
                                        warn!(room=?String::from_utf8_lossy(&room.room), "join denied due to authenticate() error");
                                        continue;
                                    }
                                }
                            }
                            // register subscriber and record permission
                            h.join(conn_id, room.clone(), &tx);
                            h.perms.insert((conn_id, room.clone()), permission);
                            joined_rooms.insert(room.clone());
                            info!(workspace=%h.workspace, room=?String::from_utf8_lossy(&room.room), ?permission, "join ok");
                            // respond ok with current version and empty extra
                            let current_version = h.current_version_bytes(&room);
                            let ok = ProtocolMessage::JoinResponseOk {
                                crdt,
                                room_id: room.room.clone(),
                                permission,
                                version: current_version,
                                extra: Some(Vec::new()),
                            };
                            if let Ok(bytes) = loro_protocol::encode(&ok) {
                                let _ = tx.send(Message::Binary(bytes.into()));
                            }
                            // send initial state:
                            // - Loro: a snapshot
                            // - LoroEphemeralStore: current accumulated ephemeral updates
                            match crdt {
                                CrdtType::Loro => {
                                    if let Some(snap) = h.snapshot_bytes(&room) {
                                        let du = ProtocolMessage::DocUpdate {
                                            crdt,
                                            room_id: room.room.clone(),
                                            updates: vec![snap],
                                        };
                                        if let Ok(bytes) = loro_protocol::encode(&du) {
                                            let _ = tx.send(Message::Binary(bytes.into()));
                                            debug!(room=?String::from_utf8_lossy(&room.room), "sent initial snapshot after join");
                                        }
                                    }
                                }
                                CrdtType::LoroEphemeralStore => {
                                    if let Some(state) = h.ephemeral_full_state(&room) {
                                        if !state.is_empty() {
                                            let du = ProtocolMessage::DocUpdate {
                                                crdt,
                                                room_id: room.room.clone(),
                                                updates: vec![state],
                                            };
                                            if let Ok(bytes) = loro_protocol::encode(&du) {
                                                let _ = tx.send(Message::Binary(bytes.into()));
                                                debug!(room=?String::from_utf8_lossy(&room.room), "sent initial ephemeral state after join");
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        ProtocolMessage::DocUpdate {
                            crdt,
                            room_id,
                            updates,
                        } => {
                            let room = RoomKey {
                                crdt,
                                room: room_id.clone(),
                            };
                            if !joined_rooms.contains(&room) {
                                // Not joined: reject with PermissionDenied
                                let err = ProtocolMessage::UpdateError {
                                    crdt,
                                    room_id: room.room.clone(),
                                    code: protocol::UpdateErrorCode::PermissionDenied,
                                    message: "Must join room before sending updates".into(),
                                    batch_id: None,
                                    app_code: None,
                                };
                                if let Ok(bytes) = loro_protocol::encode(&err) {
                                    let _ = tx.send(Message::Binary(bytes.into()));
                                }
                                warn!(room=?String::from_utf8_lossy(&room.room), "update rejected: not joined");
                            } else {
                                // Check permission
                                let perm = hub
                                    .lock()
                                    .await
                                    .perms
                                    .get(&(conn_id, room.clone()))
                                    .copied();
                                if !matches!(perm, Some(Permission::Write)) {
                                    let err = ProtocolMessage::UpdateError {
                                        crdt,
                                        room_id: room.room.clone(),
                                        code: protocol::UpdateErrorCode::PermissionDenied,
                                        message: "Write permission required to update document"
                                            .into(),
                                        batch_id: None,
                                        app_code: None,
                                    };
                                    if let Ok(bytes) = loro_protocol::encode(&err) {
                                        let _ = tx.send(Message::Binary(bytes.into()));
                                    }
                                    continue;
                                }
                                let mut h = hub.lock().await;
                                let start = std::time::Instant::now();
                                // Apply to stored doc state (Loro only); we don't branch on crdt for now.
                                h.apply_updates(&room, &updates);
                                let elapsed_ms = start.elapsed().as_millis();
                                // Broadcast original frame to others
                                h.broadcast(&room, conn_id, Message::Binary(data));
                                debug!(room=?String::from_utf8_lossy(&room.room), updates=%updates.len(), ms=%elapsed_ms, "applied and broadcast updates");
                            }
                        }
                        _ => {
                            // For simplicity, ignore other messages in minimal server.
                        }
                    }
                } else {
                    // Invalid frame: close with Protocol error, but keep server running
                    warn!("invalid protocol frame; closing connection");
                    let _ = tx.send(Message::Close(Some(CloseFrame {
                        code: CloseCode::Protocol,
                        reason: "Protocol error".into(),
                    })));
                    break;
                }
            }
            Message::Close(frame) => {
                let _ = tx.send(Message::Close(frame.clone()));
                break;
            }
            Message::Ping(p) => {
                let _ = tx.send(Message::Pong(p));
                let _ = tx.send(Message::Text("pong".into()));
            }
            _ => {}
        }
    }

    // cleanup
    {
        let mut h = hub.lock().await;
        h.leave_all(conn_id);
    }
    // drop tx to stop writer
    drop(tx);
    let _ = sink_task.await;
    debug!(conn_id, "connection closed and cleaned up");
    Ok(())
}
