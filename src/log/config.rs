use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Config {
    pub level: String,
    // 是否 log 代码所在行
    pub with_line: bool,
    pub console: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            level: "DEBUG".to_string(),
            with_line: true,
            console: true,
        }
    }
}
