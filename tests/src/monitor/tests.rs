use rs_pkg::{log, monitor::Monitor};
use std::time::Duration;
use tokio::{
    sync::mpsc::{
        Receiver,
        error::TryRecvError::{Disconnected, Empty},
    },
    time::sleep,
};
use tracing::debug;

async fn default_async_task_control(mut done: Receiver<()>) {
    debug!("TEST - RUN TASK STARTED");
    let mut n = 0;
    loop {
        // 非阻塞检查信号
        match done.try_recv() {
            Ok(_) | Err(Disconnected) => {
                done.close();
                debug!("TEST - 收到退出信号，退出循环");
                return;
            }

            Err(Empty) => {
                debug!("n: {}", n);
                n += 1;
                sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

#[cfg(test)]
#[tokio::test]
async fn test_monitor() {
    use tracing::{Instrument, debug_span};

    log::init_default();
    let monitor = Monitor::new("RustPkgTest");
    monitor
        .run(default_async_task_control)
        .instrument(debug_span!("monitor"))
        .await;
    sleep(Duration::from_secs(10)).await;
    _ = monitor.stop().await;
    sleep(Duration::from_secs(3)).await;
}
