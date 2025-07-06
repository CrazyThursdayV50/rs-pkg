use super::{
    super::{ErrorHandlerType, MessageHandlerType},
    WebSocketClientConfig,
};
use crate::{
    async_fn::wrap_fn,
    cron::{Cron, CronConfig},
    network::websocket::BytesGenerator,
};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::{error::Error, time::Duration};
use tokio::{
    select,
    sync::{
        Mutex,
        mpsc::{Receiver, Sender, error::SendError},
    },
};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, warn};

#[derive(Clone)]
pub struct Client {
    name: String,
    addr: String,
    message_handler: Arc<MessageHandlerType<Message>>,
    error_handler: Arc<ErrorHandlerType>,
    ping_payload: Arc<BytesGenerator>,
    // worker: Arc<Mutex<Worker<()>>>,
    ping_interval: String,

    client_close: Arc<Sender<()>>,
    client_done: Arc<Mutex<Receiver<()>>>,

    reconnect: bool,
    reconnect_sender: Arc<Sender<()>>,
    reconnect_receiver: Arc<Mutex<Receiver<()>>>,

    message_sender: Arc<Sender<Message>>,
    message_receiver: Arc<Mutex<Receiver<Message>>>,
}

impl Client {
    pub fn new(name: &str, cfg: &WebSocketClientConfig) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        let (client_close, client_done) = tokio::sync::mpsc::channel(1);
        let (reconnect_sender, reconnect_receiver) = tokio::sync::mpsc::channel(1);
        Self {
            name: name.to_string(),
            addr: cfg.addr.clone(),
            message_handler: wrap_fn(|msg| async {
                match msg {
                    Message::Text(t) => debug!("Received text: {}", t),
                    Message::Binary(b) => debug!("Received binary: {:?}", b),
                    Message::Ping(p) => debug!("Received ping: {:?}", p),
                    Message::Pong(p) => debug!("Received pong: {:?}", p),
                    Message::Close(c) => debug!("Received close: {:?}", c),
                    Message::Frame(f) => debug!("Received frame: {:?}", f),
                }
                None
            }),
            error_handler: wrap_fn(|e| async move { error!("Received error: {}", e) }),
            ping_payload: wrap_fn(|_| async {
                let ts = chrono::Utc::now().timestamp().to_string();
                Bytes::from(ts)
            }),

            // worker: Arc::new(Mutex::new(Worker::new(name, 1))),
            ping_interval: cfg.ping_interval.clone(),

            reconnect: cfg.reconnect,
            reconnect_sender: Arc::new(reconnect_sender),
            reconnect_receiver: Arc::new(Mutex::new(reconnect_receiver)),

            client_close: Arc::new(client_close),
            client_done: Arc::new(Mutex::new(client_done)),

            message_sender: Arc::new(sender),
            message_receiver: Arc::new(Mutex::new(receiver)),
        }
    }

    pub async fn stop(&self) -> Result<(), SendError<()>> {
        self.client_close.send(()).await
    }

    pub fn with_message_handler<F, Fut>(mut self, h: F) -> Self
    where
        F: Fn(Message) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<Message>> + Send + Sync + 'static,
    {
        self.message_handler = wrap_fn(h);
        self
    }

    pub fn with_error_handler<F, Fut>(mut self, h: F) -> Self
    where
        F: Fn(Box<dyn Error + Send + Sync + 'static>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + Sync + 'static,
    {
        self.error_handler = wrap_fn(h);
        self
    }

    pub fn with_ping_payload<F, Fut>(mut self, h: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Bytes> + Send + Sync + 'static,
    {
        let h = Arc::new(h);
        self.ping_payload = wrap_fn(move |_| {
            let h = h.clone();
            async move { h().await }
        });
        self
    }

    async fn connect(&mut self, done: Arc<Mutex<Receiver<()>>>) {
        let reconnect_sender = self.reconnect_sender.clone();

        if let Ok((stream, _)) = connect_async(&self.addr)
            .await
            .inspect_err(|e| error!("[{}] connect to {} failed: {}", self.name, self.addr, e))
        {
            let (sink, stream) = stream.split();
            let sink = Arc::new(Mutex::new(sink));
            let stream = Arc::new(Mutex::new(stream));
            let msg_handler = self.message_handler.clone();
            let msg_receiver = self.message_receiver.clone();
            let err_handler_ping = self.error_handler.clone();
            let err_handler_main = self.error_handler.clone();

            let mut cron_cfg = CronConfig::default();
            cron_cfg.interval = self.ping_interval.clone();
            cron_cfg.run_after_start = self.ping_interval.clone();
            cron_cfg.interval_after_finish = false;

            let cron = Cron::new("PING", &cron_cfg);
            let msg_sender = self.message_sender.clone();
            cron.run(move || {
                let msg_sender = msg_sender.clone();
                let err_handler_ping = err_handler_ping.clone();
                let now = chrono::Utc::now().timestamp_millis().to_string();
                let ping = Message::Ping(Bytes::from(now));
                async move {
                    if let Err(err) = msg_sender.send(ping).await {
                        _ = err_handler_ping(Box::new(err)).await;
                    }
                }
            })
            .await;

            tokio::spawn(async move {
                let mut sink = sink.lock().await;
                let mut guard = stream.lock().await;
                let mut done = done.lock().await;
                let mut msg_receiver = msg_receiver.lock().await;
                loop {
                    select! {
                        _ = done.recv() => {
                            warn!("Conn Exit with done");
                            return
                        },

                        msg = msg_receiver.recv() => {
                            debug!("msg_receiver receive: {:?}", msg);
                            match msg {
                                Some(msg) => {
                                    if let Err(e) = sink.send(msg).await {
                                        err_handler_main(Box::new(e)).await;
                                        _ = reconnect_sender.send(()).await;
                                        return
                                    };
                                },

                                None => {
                                    _ = reconnect_sender.send(()).await;
                                    return
                                },
                            }
                        }

                        t = guard.next() => {
                            debug!("stream receive: {:?}", t);
                            match t {
                                Some(Ok(msg)) => {
                                    if let Some(msg) = msg_handler(msg).await {
                                        if let Err(e) = sink.send(msg).await {
                                            err_handler_main(Box::new(e)).await;
                                            _ = reconnect_sender.send(()).await;
                                            return
                                        };
                                    }
                                },
                                Some(Err(err)) => {
                                    err_handler_main(Box::new(err)).await;
                                    _ = reconnect_sender.send(()).await;
                                    return
                                },
                                None => {
                                    _ = reconnect_sender.send(()).await;
                                    return
                                }
                            }
                        }
                    }
                }
            });
            return;
        }

        _ = reconnect_sender.send(()).await;
    }

    pub async fn send_message(&self, msg: Message) {
        _ = self.message_sender.send(msg).await;
    }

    pub async fn run(&self) {
        let s = self.clone();
        s.clone().connect(self.client_done.clone()).await;

        if self.reconnect {
            tokio::spawn(async move {
                let s = s.clone();
                let mut reconnect_guard = s.reconnect_receiver.lock().await;
                let done = s.client_done.clone();
                loop {
                    _ = reconnect_guard.recv().await;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    s.clone().connect(done.clone()).await;
                }
            });
        }
    }
}
