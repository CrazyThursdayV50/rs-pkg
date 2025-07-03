use crate::{cron::Config, worker::Worker};
use duration_str;
use std::{future::Future, sync::Arc, time::Duration};
use tokio::{
    sync::{
        Mutex,
        broadcast::{self, error::TryRecvError},
        mpsc::{self, Receiver, Sender, error::SendError},
    },
    time::sleep,
};
use tracing::debug;
use tracing::{Instrument, debug_span};

pub struct Cron {
    name: String,
    run_after_start: Duration,
    interval: Duration,
    interval_after_finish: bool,
    worker: Arc<Worker<()>>,

    close: Arc<Sender<()>>,
    done: Arc<Mutex<Receiver<()>>>,
}

impl Cron {
    pub fn new(name: &str, cfg: &Config) -> Self {
        let worker = Worker::new(name, 1);
        let run_after_start = duration_str::parse(&cfg.run_after_start).unwrap();
        let interval = duration_str::parse(&cfg.interval).unwrap();
        let (close, done) = mpsc::channel(1);
        Self {
            name: name.to_string(),
            worker: Arc::new(worker),
            run_after_start: run_after_start,
            interval: interval,
            interval_after_finish: cfg.interval_after_finish,
            close: Arc::new(close),
            done: Arc::new(Mutex::new(done)),
        }
    }

    pub async fn stop(&self) -> Result<(), SendError<()>> {
        self.close.send(()).await
    }

    pub async fn run<F, Fut>(&self, how: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + Sync + 'static,
    {
        let run_after_start = self.run_after_start;
        let interval = self.interval;
        let ticker = crossbeam_channel::tick(self.interval);
        let wait = self.interval_after_finish;
        let done = self.done.clone();
        let worker = self.worker.clone();
        let how = Arc::new(how);
        let name = self.name.clone();
        let (tx, mut rx) = broadcast::channel(1);

        tokio::spawn(
            async move {
                let mut guard = done.lock().await;
                _ = guard.recv().await;
                guard.close();
                _ = tx.send(());

                // 关闭 worker
                _ = worker.stop().await;
                debug!("CRON STOP - {}", name)
            }
            .instrument(debug_span!("exit")),
        );

        // 启动 worker
        let how = move |_: ()| {
            let how = how.clone();
            async move { how().instrument(debug_span!("run")).await }
        };
        _ = self.worker.run(how).instrument(debug_span!("worker")).await;

        let sender = self.worker.get_sender();
        let name = self.name.clone();
        tokio::spawn(
            async move {
                debug!("CRON START - {}", name);
                sleep(run_after_start).await;
                _ = sender.send(()).await;
                match wait {
                    false => loop {
                        match rx.try_recv() {
                            Ok(_) | Err(TryRecvError::Closed) => {
                                break;
                            }

                            _ => {
                                if let Ok(_) = ticker.recv() {
                                    _ = sender.send(()).await;
                                };
                            }
                        }
                    },

                    true => loop {
                        match rx.try_recv() {
                            Ok(_) | Err(TryRecvError::Closed) => {
                                break;
                            }

                            _ => {
                                sleep(interval).await;
                                _ = sender.send(()).await;
                            }
                        }
                    },
                }
            }
            .instrument(debug_span!("run")),
        );
    }
}
