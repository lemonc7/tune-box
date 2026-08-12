pub mod error;
pub mod server;
pub mod shared;
pub mod web;

use std::error::Error;

use tokio::fs;

use crate::server::db;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    fs::create_dir_all("data").await?;
    let _pool = db::connect("sqlite://data/tune.db").await?;

    Ok(())
}
