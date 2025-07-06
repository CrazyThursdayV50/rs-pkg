use std::sync::Arc;

use super::client::client::run_client;
use super::server::server_tests::run_websocket_server;
use bytes::Bytes;
use rs_pkg::network::websocket::{WebSocketServer, WebSocketServerConfig};
use tokio::{
    sync::mpsc,
    time::{Duration, Instant, sleep},
};
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};
use tracing::{debug, info};

#[cfg(test)]
#[tokio::test]
async fn test_websocket() {
    use rs_pkg::log;
    log::init_default();

    let server = run_websocket_server(Some("127.0.0.1".to_string()), Some(18080)).await;
    _ = sleep(Duration::from_secs(1)).await;

    let client = run_client("ws://localhost:18080").await;
    debug!("client send msg");
    client
        .send_message(Message::Text(Utf8Bytes::from_static("hi!")))
        .await;
    client
        .send_message(Message::Text(Utf8Bytes::from_static("hi!")))
        .await;
    client
        .send_message(Message::Text(Utf8Bytes::from_static("hi!")))
        .await;
    _ = sleep(Duration::from_secs(10)).await;
}

struct BenchmarkConfig {
    // the amount of the clients that connect to a same websocket server
    clients_count: usize,
    // messages per client
    // the number of messages send from a client to a server
    messages_per_client: usize,
    message_len_bytes: usize,
}

async fn benchmark_websocket(cfg: &BenchmarkConfig) {
    let mut start = Instant::now();
    let mut count = 0;
    let total = cfg.clients_count * cfg.messages_per_client;

    let (tx, mut rx) = mpsc::channel(1000000);
    let tx = Arc::new(tx);

    let server_cfg = WebSocketServerConfig::default();
    let server = Arc::new(
        WebSocketServer::new(&server_cfg)
            .with_message_handler(move |msg: Message| {
                let tx = tx.clone();
                async move {
                    _ = tx.send(()).await;
                    match msg {
                        Message::Binary(m) => {}
                        _ => {}
                    }

                    None
                }
            })
            .with_error_handler(|err| async move { tracing::error!("err: {}", err) }),
    );

    _ = server.run().await;

    info!("prepare clients ...");
    let mut clients = Vec::new();
    for i in 0..cfg.clients_count {
        let client = Arc::new(run_client("ws://127.0.0.1:8080").await);
        clients.push(client);
        if i % 10 == 0 {
            info!("client {} prepared", i)
        }
    }
    info!("clients prepared");

    // init messages
    let mut message_vec = Vec::new();
    for n in 0..cfg.messages_per_client {
        let mut v: Vec<u8> = Vec::with_capacity(cfg.message_len_bytes);
        v.resize(cfg.message_len_bytes, n as u8);
        let msg = Message::Binary(Bytes::from(v));
        message_vec.push(msg.clone());
    }

    let mut client_messages = Vec::new();
    for _ in &clients {
        client_messages.push(message_vec.clone());
    }

    for client in clients {
        let mut v = client_messages.pop().unwrap();
        let count = cfg.messages_per_client;
        tokio::spawn(async move {
            for _ in 0..count {
                let msg = v.pop().unwrap();
                client.send_message(msg).await;
            }
        });
    }

    loop {
        let mut v = Vec::new();
        rx.recv_many(&mut v, 100).await;
        for _ in v {
            if count == 0 {
                start = Instant::now();
            }

            count += 1;
            if count % 10000 == 0 {
                info!("COUNT: {}/{}", count, total);
            }

            if count == total {
                let cost = start.elapsed();
                let c = cost.as_nanos();
                let ability = count as u128 * 1000000000 / c;
                info!(
                    "START AT: {:?}\ntotal conns: {}\ntotal msgs: {}\ntotal cost: {:?}\nability: {} msgs/s",
                    start, cfg.clients_count, total, cost, ability
                );
                return;
            }
        }
    }
}

#[tokio::test]
#[cfg(test)]
async fn test_benchmark() {
    use rs_pkg::log;
    let mut cfg = log::Config::default();
    cfg.level = "info".to_string();
    log::init(&cfg);

    let cfg = BenchmarkConfig {
        clients_count: 10000,
        message_len_bytes: 1024,
        messages_per_client: 100,
    };
    benchmark_websocket(&cfg).await;
    sleep(Duration::from_secs(3600)).await;
}
