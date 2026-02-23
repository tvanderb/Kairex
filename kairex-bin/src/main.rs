use std::path::Path;

use kairex::collection::CollectionLayer;
use kairex::config::{AssetsConfig, CollectionConfig};
use kairex::storage::Database;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let db = Database::open(Path::new("data/kairex.db"))?;
    let assets = AssetsConfig::load(Path::new("config/assets.toml"))?;
    let collection = CollectionConfig::load(Path::new("config/collection.toml"))?;

    let layer = CollectionLayer::new(db, assets, collection);
    let handles = layer.start().await?;

    // Run until all tasks complete (they run forever, so this blocks indefinitely)
    for handle in handles {
        handle.await?;
    }

    Ok(())
}
