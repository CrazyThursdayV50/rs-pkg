use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct Config {
    pub addr: String,
    pub reconnect: bool,
    pub ping_interval: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: "ws://localhost:8080/ws".to_string(),
            reconnect: true,
            ping_interval: "3s".to_string(),
        }
    }
}
