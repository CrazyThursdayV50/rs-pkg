use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct Config {
    pub is_router: bool,
    pub host: String,
    pub port: u16,
    pub msg_buf: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            is_router: false,
            host: "127.0.0.1".to_string(),
            port: 8080,
            msg_buf: 100,
        }
    }
}
