use super::super::{ErrorHandlerType, MessageHandlerType};
use super::config::Config;
use crate::async_fn::wrap_fn;
use futures_util::{SinkExt, StreamExt};
use std::fmt::Debug;
use std::{
    error::Error,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    str::FromStr,
    sync::Arc,
};
use tokio::sync::broadcast;
use tokio::{
    net::TcpListener,
    select,
    sync::{
        Mutex, RwLock,
        mpsc::{self, Sender},
    },
};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{Instrument, debug_span, error, info, warn};

#[derive(Clone)]
enum ServerType {
    Independent(SocketAddr),
    AxumHandler,
}

#[derive(Clone)]
pub struct Server<M> {
    typ: ServerType,
    pub(crate) message_handler: Arc<MessageHandlerType<M>>,
    pub(crate) error_handler: Arc<ErrorHandlerType>,
    pub(crate) id: Arc<RwLock<isize>>,

    close: Arc<Sender<()>>,
    inner_close: Arc<RwLock<broadcast::Sender<()>>>,

    broadcast: Arc<RwLock<broadcast::Sender<M>>>,
}

impl<M> Server<M>
where
    M: Clone,
{
    pub fn new(cfg: &Config) -> Self {
        let mut server_type = ServerType::AxumHandler;
        if !cfg.is_router {
            server_type = ServerType::Independent(SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::from_str(&cfg.host).expect("invalid ws server host"),
                cfg.port,
            )));
        }

        let (send, mut recv) = mpsc::channel(1);
        let (bs, _) = broadcast::channel(1);
        let bs = Arc::new(RwLock::new(bs));
        let bs_monitor = bs.clone();
        tokio::spawn(async move {
            let guard = bs_monitor.read().await;
            recv.recv().await;
            _ = guard.send(());
        });

        let (broadcast_sender, _) = broadcast::channel(1000);

        Server {
            typ: server_type,
            message_handler: wrap_fn(|_| async { None }),
            error_handler: wrap_fn(|_| async {}),
            id: Arc::new(RwLock::new(Default::default())),
            close: Arc::new(send),
            inner_close: bs.clone(),
            broadcast: Arc::new(RwLock::new(broadcast_sender)),
        }
    }

    pub async fn stop(&self) -> Result<(), mpsc::error::SendError<()>> {
        self.close.send(()).await
    }

    pub fn with_message_handler<F, Fut>(mut self, h: F) -> Self
    where
        F: Fn(M) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<M>> + Send + Sync + 'static,
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

    pub async fn broadcast(&self, msg: M) -> Result<usize, broadcast::error::SendError<M>> {
        let guard = self.broadcast.read().await;
        guard.send(msg)
    }

    pub async fn handle_stream<S, E>(&self, s: S)
    where
        M: Debug + Send + Sync + 'static,
        S: StreamExt<Item = Result<M, E>> + SinkExt<M, Error = E> + Sized + Send + 'static,
        E: Error + Send + Sync + 'static,
    {
        let mut id = self.id.write().await;
        let current_id = *id;
        *id += 1;

        let close_guard = self.inner_close.read().await;
        let mut done = close_guard.subscribe();
        let (sink, stream) = s.split();
        let sink = Arc::new(Mutex::new(sink));
        let stream = Arc::new(Mutex::new(stream));
        let msg_handler = self.message_handler.clone();
        let err_handler = self.error_handler.clone();
        let broadcast_sender = self.broadcast.read().await;
        let mut broadcast_receiver = broadcast_sender.subscribe();

        tokio::spawn(async move {
            let mut sink = sink.lock().await;
            let mut guard = stream.lock().await;
            loop {
                select! {
                    _ = done.recv() => {
                        warn!("Conn {} Exit with done", current_id);
                        break
                    },

                    m = broadcast_receiver.recv() => {
                            match m {
                                Ok(msg) => {
                                    if let Err(e) = sink.send(msg).await {
                                        err_handler(Box::new(e)).await;
                                        return
                                    };
                                }

                                Err(e) => {
                                    err_handler(Box::new(e)).await;
                                    return
                                }
                            }
                    }

                    t = guard.next() => {
                        match t {
                            Some(Ok(msg)) => {
                                if let Some(msg) = msg_handler(msg).await {
                                    if let Err(e) = sink.send(msg).await {
                                        err_handler(Box::new(e)).await;
                                    };
                                }
                            },
                            Some(Err(err)) => {
                                err_handler(Box::new(err)).await;
                            },
                            None => return,
                        }
                    }
                }
            }
        });
    }
}

impl Server<Message> {
    pub async fn run(&self) {
        match self.typ {
            ServerType::Independent(addr) => {
                let l = TcpListener::bind(addr)
                    .await
                    .inspect_err(|e| error!("bind listener failed: {}", e))
                    .unwrap();

                info!("Websocket Server host on: {}", addr.to_string());

                let guard = self.inner_close.read().await;
                let mut server_done = guard.subscribe();

                let s = self.clone();
                tokio::spawn(
                    async move {
                    loop {
                        select! {
			                      Ok((tcp_stream,_)) = l.accept() => {
																if let Ok(ws_stream) = accept_async(tcp_stream) .await
		                                .inspect_err(|e| error!("new stream failed: {}", e))
		                            {
																		s.handle_stream(ws_stream).await;
		                            }
														}

														_ = server_done.recv() => {
															return
														}
                        }
                    }
                  }
                    .instrument(debug_span!("new_conn")),
                );
            }

            ServerType::AxumHandler => panic!("unexpected failed"),
        }
    }
}
