use std::{env, error::Error};

use tokio::fs;

use crate::server::db;

pub mod app;
pub mod error;
pub mod model;
pub mod server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv().ok();

    fs::create_dir_all("data").await?;

    let db_url =
        env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://data/tune.db_url".to_owned());

    let pool = db::connect(&db_url).await?;

    Ok(())
}
