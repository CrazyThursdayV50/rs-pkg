use rs_pkg::network::websocket::WebSocketClient;

#[cfg(test)]
#[tokio::test]
async fn test_client() {
    use rs_pkg::log;
    use std::time::Duration;
    use tokio::time::sleep;
    log::init_default();

    let client = run_client("ws://localhost:18080/ws").await;
    _ = sleep(Duration::from_secs(10)).await;
}

pub(crate) async fn run_client(addr: &str) -> WebSocketClient {
    use rs_pkg::network::websocket::{WebSocketClient, WebSocketClientConfig};
    let mut cfg = WebSocketClientConfig::default();
    cfg.addr = addr.to_string();
    cfg.ping_interval = "30s".to_string();

    let client = WebSocketClient::new("WebSocketClient", &cfg);
    client.run().await;

    client
}
