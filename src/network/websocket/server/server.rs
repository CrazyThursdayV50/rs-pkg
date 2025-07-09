use std::{net::SocketAddr, ops::Deref, sync::Arc};

use tokio::{
    net::TcpStream,
    sync::{Mutex, RwLock},
};
use tokio_tungstenite::tungstenite::Message;

use crate::{network::websocket::server::handlers, worker::Worker};

pub struct Server<M> {
    new_conn_worker: Arc<Worker<TcpStream>>,
    message_handler: Option<Arc<M>>,
}

// async fn accept_connection(peer: SocketAddr, stream: TcpStream) {
//     if let Err(e) = handle_connection(peer, stream).await {
//         match e {
//             Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8(_) => (),
//             err => error!("Error processing connection: {}", err),
//         }
//     }
// }

impl<M> Server<M> {
    pub fn new(buf: usize) -> Self {
        let w = Worker::new("NewConn", buf)
            .with_on_start(|| async { tracing::debug!("new websocket connection") })
            .with_on_exit(|| async { tracing::debug!("websocket connection closed") });
        Server {
            new_conn_worker: Arc::new(w),
            message_handler: None,
        }
    }

    pub async fn with_message_handler<Fut>(mut self, h: M) -> Self
    where
        M: Fn(Message) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + Sync + 'static,
    {
        self.message_handler = Some(Arc::new(h));
        self
    }

    async fn new_message_worker<Fut>(&self, peer: SocketAddr) -> Option<Worker<Message>>
    where
        M: Fn(Message) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + Sync + 'static,
    {
        match self.message_handler {
            Some(m) => {
                let w = Worker::new(format!("SocketWorker({})", peer.to_string()).as_str(), 100);
                w.run(m);
                Some(w)
            }

            None => None,
        }
    }

    pub async fn run<F, Fut>(&self, stream_handler: F)
    where
        F: Fn(TcpStream) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = TcpStream> + Send + Sync + 'static,
    {
        self.new_conn_worker.run(handlers::stream_handler).await;
        // let x = move |msg: Message| {}
        todo!()
        // self.worker.run(message_handler).await;
    }
}
