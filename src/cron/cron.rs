use crate::{cron::CronConfig, worker::Worker};
use duration_str;
use std::{future::Future, sync::Arc, time::Duration};
use tokio::{
    sync::{
        Mutex,
        mpsc::{self, Receiver, Sender, error::SendError},
    },
    time::{Instant, sleep, sleep_until},
};
use tracing::debug;
use tracing::{Instrument, debug_span};

#[derive(Clone)]
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
    pub fn new(name: &str, cfg: &CronConfig) -> Self {
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
        let wait = self.interval_after_finish;
        let done = self.done.clone();
        let handler_worker = self.worker.clone();
        let tick_worker = Arc::new(Worker::new("Ticker", 1));
        // let (tx, mut rx) = broadcast::channel(1);
        let how = Arc::new(how);

        let sender = self.worker.get_sender();
        let tick_sender = sender.clone();
        tick_worker
            .run(move |t: Instant| {
                let sender = tick_sender.clone();
                async move {
                    sleep_until(t).await;
                    _ = sender.send(()).await;
                }
            })
            .await;

        let how_tick_worker = tick_worker.clone();
        // 启动 worker
        let how = move |_: ()| {
            let how = how.clone();
            let tick_worker = how_tick_worker.clone();
            async move {
                how()
                    // .instrument(debug_span!("run"))
                    .await;
                let t = Instant::now().checked_add(interval);
                if wait {
                    if let Some(t) = t {
                        _ = tick_worker.send(t).await;
                    } else {
                        sleep(interval).await;
                        _ = tick_worker.send(Instant::now()).await;
                    }
                }
            }
        };

        let name = self.name.clone();
        let cron_ticker_worker = tick_worker.clone();
        tokio::spawn(
            async move {
                let tick_worker = cron_ticker_worker.clone();
                let sender = sender.clone();
                debug!("CRON START - {}", name);
                sleep(run_after_start).await;
                _ = sender.send(()).await;

                let mut t = Instant::now();
                while !wait {
                    if let Some(tt) = t.checked_add(interval) {
                        t = tt;
                        _ = tick_worker.send(t).await;
                    } else {
                        sleep(interval).await;
                        _ = tick_worker.send(Instant::now()).await;
                    }
                }
            }
            .instrument(debug_span!("start")),
        );

        _ = self.worker.run(how).instrument(debug_span!("worker")).await;

        let name = self.name.clone();
        tokio::spawn(
            async move {
                let mut guard = done.lock().await;
                _ = guard.recv().await;
                guard.close();
                // _ = tx.send(());

                // 关闭 worker
                _ = handler_worker.stop().await;
                _ = tick_worker.stop().await;
                debug!("CRON STOP - {}", name)
            }
            .instrument(debug_span!("exit")),
        );
    }
}
