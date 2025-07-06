use rs_pkg::log;
use rs_pkg::network::websocket::*;
use std::time::Duration;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::{self, Message};
use tracing::{Instrument, debug_span};

pub(crate) async fn run_websocket_server(
    host: Option<String>,
    port: Option<u16>,
) -> WebSocketServer<Message> {
    let mut cfg = WebSocketServerConfig::default();
    if let (Some(host), Some(port)) = (host, port) {
        cfg.host = host.to_string();
        cfg.port = port;
    }

    let server = WebSocketServer::new(&cfg)
        .with_message_handler(|msg: tungstenite::Message| async {
            tracing::debug!("receive msg from client: {}", msg);
            Some(msg)
        })
        .with_error_handler(|err| async move { tracing::error!("err: {}", err) });
    _ = server.run().instrument(debug_span!("test_run")).await;

    server
}

#[cfg(test)]
#[tokio::test]
async fn test_server() {
    log::init_default();

    let server = run_websocket_server(None, None).await;
    _ = sleep(Duration::from_secs(3600)).await;
}
