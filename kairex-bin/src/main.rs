use std::path::PathBuf;

use kairex::collection::CollectionLayer;
use kairex::config::{
    AnalysisConfig, AssetsConfig, CollectionConfig, DeliveryConfig, EvaluationConfig,
    FreeChannelConfig, LlmConfig, OperatorConfig, SchedulesConfig,
};
use kairex::delivery::{DeliveryLayer, TelegramClient};
use kairex::evaluation::EvaluationLayer;
use kairex::llm::LlmClient;
use kairex::operator::{self, OperatorBus, OperatorEvent};
use kairex::orchestrator::Orchestrator;
use kairex::scheduling::Scheduler;
use kairex::storage::Database;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Install rustls CryptoProvider before any TLS clients are created.
    // Both aws-lc-rs and ring are in the dep tree, so rustls can't auto-detect.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls CryptoProvider");

    // Create operator event bus (before anything else so all components can emit)
    let operator_bus = OperatorBus::new(64);

    let _otel_guard = kairex::observability::init(operator_bus.sender());

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
    let operator_config = OperatorConfig::load(&config_dir.join("operator.toml"))?;

    // Open DB
    let db = Database::open(&project_root.join("data/kairex.db"))?;

    // Create Telegram client (shared between delivery and operator subscriber)
    let telegram = TelegramClient::from_env()?;
    let subscriber_telegram = telegram.clone();

    // Spawn operator notification subscriber
    let operator_rx = operator_bus.subscribe();
    tokio::spawn(async move {
        operator::telegram::run_subscriber(
            operator_rx,
            subscriber_telegram,
            operator_config.verbosity,
        )
        .await;
    });

    // Start collection (background tasks)
    let collection = CollectionLayer::new(
        db.clone(),
        assets_config.clone(),
        collection_config,
        operator_bus.sender(),
    );
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
    let llm_client = LlmClient::new(llm_config)?;
    let delivery = DeliveryLayer::new(
        telegram,
        &delivery_config,
        free_channel_config,
        db.clone(),
        operator_bus.sender(),
    );

    // Emit system started
    let assets: Vec<String> = assets_config
        .symbols()
        .into_iter()
        .map(String::from)
        .collect();
    operator_bus.sender().emit(OperatorEvent::SystemStarted {
        assets: assets.clone(),
    });

    // Run orchestrator (blocks forever)
    let orchestrator = Orchestrator::new(
        db,
        Box::new(llm_client),
        delivery,
        analysis_config,
        assets,
        project_root,
        operator_bus.sender(),
    );
    orchestrator.run(schedule_rx, eval_rx).await;

    Ok(())
}
