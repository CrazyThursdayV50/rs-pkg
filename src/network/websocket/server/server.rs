use futures_util::{SinkExt, StreamExt};
use std::{net::SocketAddr, ops::Deref, sync::Arc};

use tokio::{
    net::TcpStream,
    sync::{Mutex, RwLock},
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::{network::websocket::server::handlers, worker::Worker};

pub struct Server<F> {
    new_conn_worker: Arc<Worker<TcpStream>>,
    message_handler: Option<Arc<F>>,
}

// async fn accept_connection(peer: SocketAddr, stream: TcpStream) {
//     if let Err(e) = handle_connection(peer, stream).await {
//         match e {
//             Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8(_) => (),
//             err => error!("Error processing connection: {}", err),
//         }
//     }
// }

impl<F> Server<F> {
    pub fn new(buf: usize) -> Self {
        let w = Worker::new("NewConn", buf)
            .with_on_start(|| async { tracing::debug!("new websocket connection") })
            .with_on_exit(|| async { tracing::debug!("websocket connection closed") });
        Server {
            new_conn_worker: Arc::new(w),
            message_handler: None,
        }
    }

    pub async fn with_err_handler(mut self, h: E) -> Self {}

    pub async fn with_message_handler<Fut>(mut self, h: F) -> Self
    where
        F: Fn(Message) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<Message>> + Send + Sync + 'static,
    {
        self.message_handler = Some(Arc::new(h));
        self
    }

    async fn new_message_worker<Fut>(&self, stream: TcpStream) -> Option<Worker<Message>>
    where
        F: Fn(Message) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<Message>> + Send + Sync + 'static,
    {
        let handler = self.message_handler.clone();
        let peer = Arc::new(
            stream
                .peer_addr()
                .expect("connected streams should have a peer address"),
        );

        match handler {
            Some(m) => {
                let ws_stream = accept_async(stream).await.expect("Failed to accept");
                let (sender, recv) = ws_stream.split();
                let w = Worker::new(format!("Stream({})", peer.to_string()).as_str(), 100);
                let sender = Arc::new(Mutex::new(sender));
                let receiver = Arc::new(Mutex::new(recv));
                w.run(move |msg: Message| {
                    let m = m.clone();
                    let sender = sender.clone();
                    async move {
                        if let Some(msg) = m(msg).await {
                            let mut guard = sender.lock().await;
                            _ = guard.send(msg).await;
                        };
                    }
                });

                tokio::spawn(async move {
                    let receiver = receiver.clone();
                    let trigger = w.get_sender();
                    loop {
                        let mut guard = receiver.lock().await;
                        let t = guard.next().await;
                        drop(guard);
                        let y = match t {
                            Some(Ok(msg)) => {
                                let x = trigger.send(msg).await;
                                x
                            }
                            Some(Err(e)) => {}
                            None => {}
                        };
                    }
                });

                Some(w)
            }

            None => None,
        }
    }

    pub async fn run<S, Fut>(&self, stream_handler: S)
    where
        S: Fn(TcpStream) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = TcpStream> + Send + Sync + 'static,
    {
        self.new_conn_worker.run(handlers::stream_handler).await;
        // let x = move |msg: Message| {}
        todo!()
        // self.worker.run(message_handler).await;
    }
}
