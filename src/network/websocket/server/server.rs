use crate::async_fn::{AsyncFn, wrap};
use futures_util::{SinkExt, StreamExt, future::BoxFuture};
use std::{
    collections::HashMap,
    error::Error,
    net::SocketAddr,
    ops::{Add, Deref},
    pin::Pin,
    sync::Arc,
};

use tokio::{
    net::TcpStream,
    select,
    sync::{
        Mutex, RwLock,
        broadcast::{Receiver, Sender, error::SendError},
        mpsc,
    },
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::worker::Worker;

type MessageHandlerType = AsyncFn<Message, Option<Message>>;
type ErrorHandlerType = AsyncFn<Box<dyn Error + Send + Sync + 'static>, ()>;

pub struct Server {
    new_conn_worker: Arc<Worker<TcpStream>>,
    message_handler: Arc<MessageHandlerType>,
    error_handler: Arc<ErrorHandlerType>,
    id: RwLock<isize>,
    workers: HashMap<isize, Arc<Worker<Message>>>,

    done: Arc<Mutex<Receiver<()>>>,
    close: Arc<Sender<()>>,
}

// async fn accept_connection(peer: SocketAddr, stream: TcpStream) {
//     if let Err(e) = handle_connection(peer, stream).await {
//         match e {
//             Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8(_) => (),
//             err => error!("Error processing connection: {}", err),
//         }
//     }
// }

impl Server {
    pub fn new(buf: usize) -> Self {
        let w = Worker::new("NewConn", buf)
            .with_on_start(|| async { tracing::debug!("new websocket connection") })
            .with_on_exit(|| async { tracing::debug!("websocket connection closed") });

        let (close, done) = tokio::sync::broadcast::channel(1);

        Server {
            new_conn_worker: Arc::new(w),
            message_handler: wrap(|_| async { None }),
            error_handler: wrap(|_| async {}),
            id: RwLock::new(0),
            workers: HashMap::new(),
            close: Arc::new(close),
            done: Arc::new(Mutex::new(done)),
        }
    }

    async fn add_worker(&mut self, worker: Arc<Worker<Message>>) {
        let id = self.next_id().await;
        self.workers.insert(id, worker);
    }

    async fn next_id(&mut self) -> isize {
        let mut id = self.id.write().await.abs();
        id = id + 1;
        self.id = RwLock::new(id);
        id
    }

    pub fn stop(&self) -> Result<usize, SendError<()>> {
        self.close.send(())
    }

    pub fn with_message_handler<F, Fut>(mut self, h: F) -> Self
    where
        F: Fn(Message) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<Message>> + Send + Sync + 'static,
    {
        self.message_handler = wrap(h);
        self
    }

    pub fn with_error_handler<F, Fut>(mut self, h: F) -> Self
    where
        F: Fn(Box<dyn Error + Send + Sync + 'static>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + Sync + 'static,
    {
        self.error_handler = wrap(h);
        self
    }

    async fn new_conn(&mut self, stream: TcpStream, close: Arc<Sender<()>>) {
        let m = self.message_handler.clone();
        let e = self.error_handler.clone();

        let peer = Arc::new(
            stream
                .peer_addr()
                .expect("connected streams should have a peer address"),
        );

        let ws_stream = accept_async(stream).await.expect("Failed to accept");
        let (sender, recv) = ws_stream.split();
        let w = Arc::new(
            Worker::new(format!("Stream({})", peer.to_string()).as_str(), 100).with_graceful(false),
        );

        let sender = Arc::new(Mutex::new(sender));
        let receiver = Arc::new(Mutex::new(recv));
        let done = Arc::new(Mutex::new(close.subscribe()));

        w.run(move |msg: Message| {
            let m = m.clone();
            let sender = sender.clone();
            async move {
                if let Some(msg) = m(msg).await {
                    let mut guard = sender.lock().await;
                    _ = guard.send(msg).await;
                };
            }
        })
        .await;

        let w1 = w.clone();
        tokio::spawn(async move {
            let receiver = receiver.clone();
            let trigger = w1.get_sender();
            let mut g = done.lock().await;
            loop {
                let mut guard = receiver.lock().await;

                select! {
                    x = g.recv() => {
                        tracing::debug!("Received message: {:?}", x);
                        return
                    },

                    t = guard.next() => {
                        drop(guard);
                        if let Err(y) = match t {
                            Some(Ok(msg)) => trigger.send(msg).await.map_err(Into::into),
                            Some(Err(err)) => Err(err.into()),
                            None => Ok(()),
                        } {
                            e(y).await;
                        };
                    }
                }
            }
        });

        self.add_worker(w).await;
    }

    pub async fn run<S, Fut>(&self, stream_handler: S)
    where
        S: Fn(TcpStream) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = TcpStream> + Send + Sync + 'static,
    {
        tokio::spawn(async {});
        // self.new_conn_worker.run(handlers::stream_handler).await;
        // let x = move |msg: Message| {}
        // todo!()
        // self.worker.run(message_handler).await;
    }
}
