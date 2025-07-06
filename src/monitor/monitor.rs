use futures_util::future::BoxFuture;
use std::{future::Future, sync::Arc};
use tokio::sync::{
    Mutex,
    mpsc::{Receiver, Sender, error::SendError},
};
use tracing::debug;
use tracing::{Instrument, debug_span};

type WrappedFn = Arc<Mutex<Option<Box<dyn FnOnce() -> BoxFuture<'static, ()> + Send + Sync>>>>;

#[derive(Clone)]
pub struct Monitor {
    name: String,
    on_start: WrappedFn,
    on_exit: WrappedFn,
    done: Arc<Mutex<Receiver<()>>>,
    close: Arc<Sender<()>>,
}

fn wrap<F, Fut>(f: F) -> WrappedFn
where
    F: FnOnce() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + Sync + 'static,
{
    Arc::new(Mutex::new(Some(Box::new(|| Box::pin(f())))))
}

fn debug_task(msg: String) -> WrappedFn {
    wrap(|| async move {
        debug!("{}", msg);
    })
}

impl Monitor {
    pub fn new(name: &str) -> Self {
        // 创建 monitor 开关
        let (close, done) = tokio::sync::mpsc::channel(1);
        let start = debug_task(format!("MONITOR START - {}", name));
        let stop = debug_task(format!("MONITOR STOP - {}", name));
        Self {
            name: name.to_string(),
            on_start: start,
            on_exit: stop,
            done: Arc::new(Mutex::new(done)),
            close: Arc::new(close),
        }
    }

    pub fn with_trigger(mut self, trigger: (Arc<Sender<()>>, Arc<Mutex<Receiver<()>>>)) -> Self {
        (self.close, self.done) = trigger;
        self
    }

    pub fn with_on_start<F, Fut>(mut self, task: F) -> Self
    where
        F: FnOnce() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + Sync + 'static,
    {
        self.on_start = wrap(task);
        self
    }

    pub fn with_on_exit<F, Fut>(mut self, task: F) -> Self
    where
        F: FnOnce() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + Sync + 'static,
    {
        self.on_exit = wrap(task);
        self
    }

    pub async fn run<F, Fut>(&self, task: F)
    where
        F: FnOnce(Receiver<()>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + Sync + 'static,
    {
        // 获取并执行 on_start
        if let Some(on_start) = self.on_start.lock().await.take() {
            on_start().instrument(debug_span!("start.call")).await;
        };

        // 为 task 创建一个接受关闭信号的管道
        let (task_close, task_done) = tokio::sync::mpsc::channel(1);

        let on_exit = self.on_exit.clone();
        let done = self.done.clone();
        _ = tokio::spawn(
            async move {
                // 如果 monitor 收到了关闭信号
                let mut guard = done.lock().await;
                guard.recv().await;
                guard.close();

                // 关闭 task
                _ = task_close.send(()).await;

                // 运行 on_exit
                if let Some(on_exit_fn) = on_exit.lock().await.take() {
                    on_exit_fn().await;
                };
            }
            .instrument(debug_span!("exit")),
        );

        // 运行 task
        _ = tokio::spawn(
            async move {
                task(task_done).await;
            }
            .instrument(debug_span!("call")),
        );
    }

    pub async fn stop(&self) -> Result<(), SendError<()>> {
        self.close.send(()).await
    }

    pub fn name(&self) -> String {
        self.name.to_string()
    }
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new("default")
    }
}
