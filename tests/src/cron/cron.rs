use rs_pkg::cron::{Cron, CronConfig};
use rs_pkg::log;
use tracing::debug;

#[tokio::test]
#[cfg(test)]
async fn test_cron() {
    use std::time::Duration;

    use tokio::time::sleep;
    use tracing::{Instrument, debug_span};

    log::init_default();

    let mut cfg = CronConfig::default();
    cfg.interval = "1s".to_string();
    cfg.run_after_start = "10ms".to_string();
    cfg.interval_after_finish = false;

    let cron = Cron::new("TestCron", &cfg);
    _ = cron
        .run(|| async { debug!("TEST CRON RUNNING!") }.instrument(debug_span!("test")))
        .instrument(debug_span!("cron"))
        .await;
    _ = sleep(Duration::from_secs(10)).await;
}
