use std::{fs, path::Path};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: Server,
    pub providers: Providers,
}

#[derive(Debug, Deserialize)]
pub struct Server {
    pub port: u32,
    pub mcp_token: String,
    pub max_bytes: u64,
}

#[derive(Debug, Deserialize)]
pub struct Providers {
    pub kuwo: Kuwo,
}

#[derive(Debug, Deserialize)]
pub struct Kuwo {
    pub search_url: String,
    pub resolve_url: String,
    pub lyric_url: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: Server {
                port: 8080,
                mcp_token: String::from(""),
                max_bytes: 1024 * 1024 * 100,
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

pub fn load_config(path: impl AsRef<Path>) -> Config {
    let Ok(content) = fs::read_to_string(path) else {
        return Config::default();
    };

    toml::from_str(&content).unwrap_or(Config::default())
}
