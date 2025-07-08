use std::sync::Arc;

use tokio_tungstenite::tungstenite::Message;

use crate::worker::Worker;

pub struct Server {
    worker: Arc<Worker<Message>>,
}

impl Server {
    pub fn new(buf: usize) -> Self {
        let w = Worker::new("WsServer", buf);
        Server {
            worker: Arc::new(w),
        }
    }

    pub async fn run<F, Fut>(&self, message_handler: F)
    where
        F: Fn(Message) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + Sync + 'static,
    {
        self.worker.run(message_handler).await;
    }
}
