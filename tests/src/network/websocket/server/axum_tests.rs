use axum::Router;
use axum::extract::{State, WebSocketUpgrade, ws};
use axum::response::Response;
use axum::routing::get;
use rs_pkg::log;
use rs_pkg::network::websocket::*;
use std::sync::Arc;
use tracing::debug;

async fn ws_handler(
    state: State<Arc<WebSocketServer<ws::Message>>>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |ws| {
        let state = state.clone();
        async move {
            state.handle_stream(ws).await;
        }
    })
}

#[cfg(test)]
#[tokio::test]
async fn test_server_as_axum_handler() {
    log::init_default();
    debug!("START");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .unwrap();

    let mut server_cfg = WebSocketServerConfig::default();
    server_cfg.is_router = true;

    let ws_server =
        WebSocketServer::new(&server_cfg).with_message_handler(|msg: ws::Message| async {
            tracing::debug!("receive msg: {}", msg.to_text().unwrap());
            Some(msg)
        });

    let state = Arc::new(ws_server);

    let router = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state);

    _ = axum::serve(listener, router).await.unwrap();

    debug!("EXIT");
}
