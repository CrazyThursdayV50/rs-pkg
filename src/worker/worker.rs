use crate::monitor::Monitor;
use std::{future::Future, sync::Arc};
use tokio::sync::{
    Mutex,
    mpsc::{
        Receiver, Sender, channel,
        error::{
            SendError,
            TryRecvError::{Disconnected, Empty},
        },
    },
};
use tracing::{Instrument, debug, debug_span};

#[derive(Clone)]
pub struct Worker<J> {
    name: String,
    work_count: Arc<Mutex<usize>>,
    monitor: Monitor,
    recv: Arc<Mutex<Receiver<J>>>,
    send: Arc<Sender<J>>,
    graceful: bool,
}

async fn handle<F, Fut, J>(
    trigger: Arc<Mutex<Receiver<J>>>,
    done: Arc<Mutex<Receiver<()>>>,
    how: Arc<F>,
    graceful: bool,
) where
    F: Fn(J) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
    J: Send + Sync + 'static,
{
    match graceful {
        false => {
            tokio::spawn(
                async move {
                    let mut done = done.lock().await;
                    loop {
                        // 非阻塞检查信号
                        match done.try_recv() {
                            Ok(_) | Err(Disconnected) => {
                                done.close();
                                return;
                            }

                            Err(Empty) => {
                                // 检查是否有新的工作项
                                let mut guard = trigger.lock().await;
                                if let Ok(item) = guard.try_recv() {
                                    drop(guard); // 释放锁，避免在异步调用时持有锁
                                    how(item).await;
                                }
                            }
                        }
                    }
                }
                .instrument(debug_span!("handle")),
            );
        }

        true => {
            tokio::spawn(
                async move {
                    loop {
                        // 检查是否有新的工作项
                        let mut guard = trigger.lock().await;
                        match guard.recv().await {
                            Some(item) => {
                                drop(guard); // 释放锁，避免在异步调用时持有锁
                                how(item).await;
                            }

                            None => return,
                        }
                    }
                }
                .instrument(debug_span!("grace")),
            );
        }
    }
}

impl<J> Worker<J> {
    pub fn new(name: &str, buf: usize) -> Self {
        let (tx, rx) = channel(buf);
        let work_count = Arc::new(Mutex::new(0));
        Self {
            name: name.to_string(),
            work_count,
            monitor: Monitor::new(name),
            recv: Arc::new(Mutex::new(rx)),
            graceful: false,
            send: Arc::new(tx),
        }
    }

    pub fn with_on_start<F, Fut>(mut self, task: F) -> Self
    where
        F: FnOnce() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + Sync + 'static,
    {
        self.monitor = self.monitor.with_on_start(task);
        self
    }

    pub fn with_on_exit<F, Fut>(mut self, task: F) -> Self
    where
        F: FnOnce() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + Sync + 'static,
    {
        self.monitor = self.monitor.with_on_exit(task);
        self
    }

    pub fn with_graceful(mut self, graceful: bool) -> Self {
        self.graceful = graceful;
        self
    }

    pub fn with_trigger(mut self, trigger: (Arc<Sender<J>>, Arc<Mutex<Receiver<J>>>)) -> Self {
        let (send, recv) = trigger;
        self.send = send;
        self.recv = recv;
        self
    }

    pub fn get_sender(&self) -> Arc<Sender<J>> {
        self.send.clone()
    }

    pub async fn send(&self, job: J) -> Result<(), SendError<J>> {
        self.send.send(job).await
    }

    pub fn name(&self) -> String {
        self.name.to_string()
    }

    pub async fn count(&self) -> usize {
        let guard = self.work_count.lock().await;
        *guard
    }

    pub async fn stop(&self) -> Result<(), SendError<()>> {
        self.monitor.stop().await
    }

    pub async fn run<F, Fut>(&self, how: F)
    where
        F: Fn(J) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
        J: Send + Sync + 'static,
    {
        debug!("WORKER START - {}", self.name);
        let trigger = self.recv.clone();
        let graceful = self.graceful;
        let how = Arc::new(how);
        let task = move |done: Receiver<()>| async move {
            let done = Arc::new(Mutex::new(done));
            handle(trigger, done, how, graceful).await;
        };

        _ = self
            .monitor
            .run(task)
            .instrument(debug_span!("monitor"))
            .await;
    }
}
