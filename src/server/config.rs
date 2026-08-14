use std::{fs, io, path::Path};

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

impl Default for Config {
    fn default() -> Self {
        Self {
            server: Server {
                port: 8080,
                http_timeout: 30,
                download_read_timeout: 30,
                request_timeout: 30,
                mcp_token: String::from("tune_box"),
                max_bytes: 1024 * 1024 * 100,
                max_http_concurrent: 32,
                max_download_concurrent: 3,
            },
            providers: Providers {
                kuwo: Kuwo {
                    search_url: String::from("https://search.kuwo.cn/r.s"),
                    resolve_url: String::from("https://mobi.kuwo.cn/mobi.s"),
                    lyric_url: String::from("https://www.kuwo.cn/openapi/v1/www/lyric/getlyric"),
                },
            },
        }
    }
}

pub fn load_config(path: impl AsRef<Path>) -> Result<Config, String> {
    let config = match fs::read_to_string(path) {
        Ok(content) => {
            toml::from_str(&content).map_err(|error| format!("读取配置失败：{error}"))?
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Config::default(),
        Err(error) => return Err(format!("读取配置文件失败：{error}")),
    };

    config
        .validate()
        .map_err(|error| format!("配置文件无效: {error}"))?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_config_without_download_read_timeout_keeps_working() {
        let config: Config = toml::from_str(
            r#"
                [server]
                port = 8080
                http_timeout = 30
                request_timeout = 30
                download_read_timeout = 30
                mcp_token = "tune_box"
                max_bytes = 104857600
                max_http_concurrent = 32
                max_download_concurrent = 3

                [providers.kuwo]
                search_url = "https://search.kuwo.cn/r.s"
                resolve_url = "https://mobi.kuwo.cn/mobi.s"
                lyric_url = "https://www.kuwo.cn/openapi/v1/www/lyric/getlyric"
            "#,
        )
        .unwrap();

        assert_eq!(config.server.download_read_timeout, 30);
    }
}
