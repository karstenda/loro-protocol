use loro_websocket_client::Client;
use loro_websocket_server as server;
use server::protocol::{CrdtType, ProtocolMessage, UpdateStatusCode};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

type Cfg = server::ServerConfig<()>;

#[derive(Clone, Debug)]
struct UpdateRecord {
    workspace: String,
    room: String,
    crdt: CrdtType,
    conn_id: u64,
    updates_len: usize,
}

#[tokio::test(flavor = "current_thread")]
async fn on_update_hook_called() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind tcp listener");
    let addr = listener.local_addr().expect("local addr");

    let update_calls: Arc<Mutex<Vec<UpdateRecord>>> = Arc::new(Mutex::new(Vec::new()));
    let notify = Arc::new(Notify::new());

    let update_calls_cfg = update_calls.clone();
    let notify_cfg = notify.clone();

    let server_task = tokio::spawn(async move {
        let cfg: Cfg = server::ServerConfig {
            on_update: Some(Arc::new(move |args: server::UpdateArgs<()>| {
                let update_calls = update_calls_cfg.clone();
                let notify = notify_cfg.clone();
                Box::pin(async move {
                    let server::UpdateArgs {
                        workspace,
                        room,
                        crdt,
                        conn_id,
                        updates,
                        doc: _,
                        ctx: _,
                    } = args;
                    update_calls.lock().await.push(UpdateRecord {
                        workspace,
                        room,
                        crdt,
                        conn_id,
                        updates_len: updates.len(),
                    });
                    notify.notify_waiters();
                    UpdateStatusCode::Ok
                })
            })),
            // Use handshake auth to ensure workspace_id is captured
            handshake_auth: Some(Arc::new(|_| true)),
            ..Default::default()
        };
        server::serve_incoming_with_config(listener, cfg)
            .await
            .unwrap();
    });

    // Connect client
    let url = format!("ws://{}/my-workspace", addr);
    let mut client = Client::connect(&url).await.expect("connect");

    // Join room
    client
        .send(&ProtocolMessage::JoinRequest {
            crdt: CrdtType::Loro,
            room_id: "room1".to_string(),
            auth: vec![],
            version: vec![],
        })
        .await
        .expect("send join");

    // Wait for join response
    match client.next().await.expect("recv") {
        Some(ProtocolMessage::JoinResponseOk { .. }) => {}
        msg => panic!("unexpected msg: {:?}", msg),
    }

    // Send update
    let update_payload = vec![1, 2, 3, 4];
    client
        .send(&ProtocolMessage::DocUpdate {
            crdt: CrdtType::Loro,
            room_id: "room1".to_string(),
            updates: vec![update_payload.clone()],
            batch_id: server::protocol::BatchId([0; 8]), // dummy batch id
        })
        .await
        .expect("send update");

    // Wait for hook to be called
    notify.notified().await;

    let calls = update_calls.lock().await;
    assert_eq!(calls.len(), 1);
    let record = &calls[0];
    assert_eq!(record.workspace, "my-workspace");
    assert_eq!(record.room, "room1");
    assert_eq!(record.crdt, CrdtType::Loro);
    assert_eq!(record.updates_len, 1);
    // conn_id is dynamic, just check it's non-zero
    assert!(record.conn_id > 0);

    server_task.abort();
}

#[tokio::test(flavor = "current_thread")]
async fn on_update_hook_can_reject() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind tcp listener");
    let addr = listener.local_addr().expect("local addr");

    let server_task = tokio::spawn(async move {
        let cfg: Cfg = server::ServerConfig {
            on_update: Some(Arc::new(move |args: server::UpdateArgs<()>| {
                Box::pin(async move {
                    if args.room == "rejected" {
                        UpdateStatusCode::PermissionDenied
                    } else {
                        UpdateStatusCode::Ok
                    }
                })
            })),
            ..Default::default()
        };
        server::serve_incoming_with_config(listener, cfg)
            .await
            .unwrap();
    });

    let url = format!("ws://{}/workspace", addr);
    let mut c1 = Client::connect(&url).await.expect("c1 connect");
    let mut c2 = Client::connect(&url).await.expect("c2 connect");

    // 1. Both join "rejected"
    for c in [&mut c1, &mut c2] {
        c.send(&ProtocolMessage::JoinRequest {
            crdt: CrdtType::Loro,
            room_id: "rejected".to_string(),
            auth: vec![],
            version: vec![],
        }).await.expect("join rejected");
        match c.next().await.expect("recv") {
            Some(ProtocolMessage::JoinResponseOk { .. }) => {},
            m => panic!("unexpected join response: {:?}", m),
        }
        // Consume snapshot
        match c.next().await.expect("recv") {
             Some(ProtocolMessage::DocUpdate { .. }) => {},
             m => panic!("expected initial snapshot, got: {:?}", m),
        }
    }

    // 2. C1 sends update to "rejected" -> Should be rejected
    let batch_id_1 = server::protocol::BatchId([1; 8]);
    c1.send(&ProtocolMessage::DocUpdate {
        crdt: CrdtType::Loro,
        room_id: "rejected".to_string(),
        updates: vec![vec![1, 2, 3]],
        batch_id: batch_id_1,
    }).await.expect("send update 1");

    // C1 gets Ack(PermissionDenied)
    match c1.next().await.expect("recv") {
        Some(ProtocolMessage::Ack { ref_id, status, .. }) => {
            assert_eq!(ref_id, batch_id_1);
            assert_eq!(status, UpdateStatusCode::PermissionDenied);
        }
        m => panic!("unexpected msg c1: {:?}", m),
    }

    // 3. Both join "accepted"
    for c in [&mut c1, &mut c2] {
        c.send(&ProtocolMessage::JoinRequest {
            crdt: CrdtType::Loro,
            room_id: "accepted".to_string(),
            auth: vec![],
            version: vec![],
        }).await.expect("join accepted");
        match c.next().await.expect("recv") {
            Some(ProtocolMessage::JoinResponseOk { .. }) => {},
            m => panic!("unexpected join response: {:?}", m),
        }
        // Consume snapshot
        match c.next().await.expect("recv") {
             Some(ProtocolMessage::DocUpdate { .. }) => {},
             m => panic!("expected initial snapshot, got: {:?}", m),
        }
    }

    // 4. C1 sends update to "accepted" -> Should be accepted
    let batch_id_2 = server::protocol::BatchId([2; 8]);
    c1.send(&ProtocolMessage::DocUpdate {
        crdt: CrdtType::Loro,
        room_id: "accepted".to_string(),
        updates: vec![vec![4, 5, 6]],
        batch_id: batch_id_2,
    }).await.expect("send update 2");

    // C1 gets Ack(Ok)
    match c1.next().await.expect("recv") {
        Some(ProtocolMessage::Ack { ref_id, status, .. }) => {
            assert_eq!(ref_id, batch_id_2);
            assert_eq!(status, UpdateStatusCode::Ok);
        }
        m => panic!("unexpected msg c1: {:?}", m),
    }

    // 5. C2 should receive the update for "accepted".
    // Crucially, it should NOT have received the update for "rejected" before this.
    match c2.next().await.expect("recv") {
        Some(ProtocolMessage::DocUpdate { room_id, batch_id, .. }) => {
            assert_eq!(room_id, "accepted");
            assert_eq!(batch_id, batch_id_2);
        }
        m => panic!("unexpected msg c2: {:?}", m),
    }

    server_task.abort();
}
