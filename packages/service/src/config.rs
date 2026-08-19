use std::{fs, path::Path};

use garde::Validate;
use serde::Deserialize;

#[derive(Debug, Deserialize, Validate)]
pub struct Config {
    #[garde(dive)]
    pub server: Server,
    #[garde(dive)]
    pub providers: Providers,
}

#[derive(Debug, Deserialize, Validate)]
pub struct Server {
    #[garde(range(min = 1, max = 65535))]
    pub port: u32,
    #[garde(range(min = 1))]
    pub http_timeout: u64,
    #[garde(range(min = 1))]
    pub download_read_timeout: u64,
    #[garde(range(min = 1))]
    pub request_timeout: u64,
    #[garde(length(min = 6))]
    pub mcp_token: String,
    #[garde(range(min = 1))]
    pub max_bytes: u64,
    #[garde(range(min = 1))]
    pub max_http_concurrent: usize,
    #[garde(range(min = 1))]
    pub max_download_concurrent: usize,
}

#[derive(Debug, Deserialize, Validate)]
pub struct Providers {
    #[garde(dive)]
    pub kuwo: Kuwo,
    #[garde(dive)]
    pub netease: Netease,
}

#[derive(Debug, Deserialize, Validate)]
pub struct Kuwo {
    #[garde(url)]
    pub search_url: String,
    #[garde(url)]
    pub resolve_url: String,
    #[garde(url)]
    pub lyric_url: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct Netease {
    #[garde(url)]
    pub base_url: String,
    #[garde(url)]
    pub extended_resolve_url: String,
}

pub fn load_config(path: impl AsRef<Path>) -> Result<Config, String> {
    let config: Config = match fs::read_to_string(path) {
        Ok(content) => {
            toml::from_str(&content).map_err(|error| format!("读取配置失败：{error}"))?
        }
        Err(error) => return Err(format!("读取配置文件失败：{error}")),
    };

    config
        .validate()
        .map_err(|error| format!("配置文件无效: {error}"))?;
    Ok(config)
}
