use rs_pkg::{log, worker::Worker};
use tracing::debug;

#[cfg(test)]
#[tokio::test]
async fn test_worker() {
    use std::time::Duration;
    use tokio::time::sleep;
    use tracing::{Instrument, debug_span};
    log::init_default();

    {
        let mut worker = Worker::new("TestWorker", 10);
        worker = worker.with_graceful(true);
        worker
            .run(|i: usize| async move { debug!("i: {}", i) })
            .instrument(debug_span!("worker"))
            .await;

        let sender = worker.get_sender();

        tokio::spawn(async move {
            for i in 0..10 {
                use std::time::Duration;
                use tokio::time::sleep;
                _ = sender.send(i).await;
                _ = sleep(Duration::from_millis(100)).await;
            }
        });

        sleep(Duration::from_secs(3)).await;
        // _ = worker.stop().await;
    }

    debug!("EXIT");
    sleep(Duration::from_secs(1)).await;
}
