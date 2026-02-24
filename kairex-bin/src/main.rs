use std::path::PathBuf;

use kairex::collection::CollectionLayer;
use kairex::config::{
    AnalysisConfig, AssetsConfig, CollectionConfig, DeliveryConfig, EvaluationConfig,
    FreeChannelConfig, LlmConfig, SchedulesConfig,
};
use kairex::delivery::DeliveryLayer;
use kairex::evaluation::EvaluationLayer;
use kairex::llm::AnthropicClient;
use kairex::orchestrator::Orchestrator;
use kairex::scheduling::Scheduler;
use kairex::storage::Database;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let project_root = PathBuf::from(".");
    let config_dir = project_root.join("config");

    // Load all configs
    let assets_config = AssetsConfig::load(&config_dir.join("assets.toml"))?;
    let collection_config = CollectionConfig::load(&config_dir.join("collection.toml"))?;
    let analysis_config = AnalysisConfig::load(&config_dir.join("analysis.toml"))?;
    let eval_config = EvaluationConfig::load(&config_dir.join("evaluation.toml"))?;
    let schedules_config = SchedulesConfig::load(&config_dir.join("schedules.toml"))?;
    let llm_config = LlmConfig::load(&config_dir.join("llm.toml"))?;
    let delivery_config = DeliveryConfig::load(&config_dir.join("delivery.toml"))?;
    let free_channel_config = FreeChannelConfig::load(&config_dir.join("free_channel.toml"))?;

    // Open DB
    let db = Database::open(&project_root.join("data/kairex.db"))?;

    // Start collection (background tasks)
    let collection = CollectionLayer::new(db.clone(), assets_config.clone(), collection_config);
    collection.start().await?;

    // Start event sources
    let scheduler = Scheduler::new(schedules_config);
    let schedule_rx = scheduler.start();

    let evaluation = EvaluationLayer::new(
        db.clone(),
        eval_config,
        analysis_config.clone(),
        project_root.clone(),
    );
    let eval_rx = evaluation.start();

    // Create pipeline components
    let llm_client = AnthropicClient::new(llm_config)?;
    let delivery = DeliveryLayer::new(&delivery_config, free_channel_config, db.clone())?;

    // Run orchestrator (blocks forever)
    let assets = assets_config
        .symbols()
        .into_iter()
        .map(String::from)
        .collect();
    let orchestrator = Orchestrator::new(
        db,
        llm_client,
        delivery,
        analysis_config,
        assets,
        project_root,
    );
    orchestrator.run(schedule_rx, eval_rx).await;

    Ok(())
}
