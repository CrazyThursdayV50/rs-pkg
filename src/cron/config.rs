pub struct Config {
    // 程序启动后多久运行
    pub run_after_start: String,
    // 两次任务间隔多久
    pub interval: String,
    // 是否等待任务完成后才计时
    pub interval_after_finish: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            run_after_start: "0s".to_string(),
            interval: "1m".to_string(),
            interval_after_finish: true,
        }
    }
}
