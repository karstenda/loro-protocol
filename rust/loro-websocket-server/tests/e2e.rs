use loro as loro_crdt;
use loro_websocket_client::LoroWebsocketClient;
use loro_websocket_server as server;
use std::sync::Arc;

// helper removed: not used

#[tokio::test(flavor = "current_thread")]
async fn e2e_sync_two_clients_docupdate_roundtrip() {
    // Bind on an ephemeral port and serve
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_task = tokio::spawn(async move {
        let cfg = server::ServerConfig {
            handshake_auth: Some(Arc::new(|_ws, token| token == Some("secret"))),
            ..Default::default()
        };
        server::serve_incoming_with_config(listener, cfg).await.unwrap();
    });

    let url = format!("ws://{}/ws1?token=secret", addr);
    let c1 = LoroWebsocketClient::connect(&url).await.unwrap();
    let c2 = LoroWebsocketClient::connect(&url).await.unwrap();

    // Create two loro docs and prepare a change on doc1
    let doc1 = Arc::new(tokio::sync::Mutex::new(loro_crdt::LoroDoc::new()));
    let doc2 = Arc::new(tokio::sync::Mutex::new(loro_crdt::LoroDoc::new()));

    // Join room for both clients
    let room_id = "room1";
    let _room1 = c1.join_loro(room_id, doc1.clone()).await.unwrap();
    let _room2 = c2.join_loro(room_id, doc2.clone()).await.unwrap();

    // Create a text object and insert some content in doc1
    {
        let d1 = doc1.lock().await;
        let text_id = d1.get_text("text");
        text_id.insert(0, "hello").unwrap();
    }
    // Commit the local changes to trigger subscribe_local_update and auto-send
    {
        doc1.lock().await.commit();
    }

    // Client 2 receives the update and applies to its local doc2
    // Wait a moment for async delivery and verify doc2 state contains the inserted text
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let t2 = { doc2.lock().await.get_text("text").to_string() };
    assert_eq!(t2, "hello");

    // Clean up
    drop(c1);
    drop(c2);
    // server_task keeps running; cancel it.
    server_task.abort();
}

#[tokio::test(flavor = "current_thread")]
async fn workspaces_are_isolated() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_task = tokio::spawn(async move {
        let cfg = server::ServerConfig {
            handshake_auth: Some(Arc::new(|_ws, token| token == Some("secret"))),
            ..Default::default()
        };
        server::serve_incoming_with_config(listener, cfg).await.unwrap();
    });

    let url1 = format!("ws://{}/workspaceA?token=secret", addr);
    let url2 = format!("ws://{}/workspaceB?token=secret", addr);
    let c1 = LoroWebsocketClient::connect(&url1).await.unwrap();
    let c2 = LoroWebsocketClient::connect(&url2).await.unwrap();

    let doc1 = Arc::new(tokio::sync::Mutex::new(loro_crdt::LoroDoc::new()));
    let doc2 = Arc::new(tokio::sync::Mutex::new(loro_crdt::LoroDoc::new()));
    let room_id = "room-iso";
    let _r1 = c1.join_loro(room_id, doc1.clone()).await.unwrap();
    let _r2 = c2.join_loro(room_id, doc2.clone()).await.unwrap();

    {
        let d1 = doc1.lock().await;
        let t = d1.get_text("text");
        t.insert(0, "hello").unwrap();
        d1.commit();
    }
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let t2 = { doc2.lock().await.get_text("text").to_string() };
    assert_eq!(t2, "", "workspaces should be isolated");

    server_task.abort();
}
