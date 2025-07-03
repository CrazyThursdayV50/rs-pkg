use super::Config;
use tracing::Level;

pub fn init(cfg: &Config) {
    let builder = tracing_subscriber::fmt()
        .with_max_level(cfg.level.parse::<Level>().unwrap())
        .with_line_number(cfg.with_line);

    if cfg.console {
        builder.init();
        return;
    }

    builder.json().init();
}

pub fn init_default() {
    let cfg = Config::default();
    init(&cfg);
}
