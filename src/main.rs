use anyhow::Result;
use clap::Parser;
use log::info;
use std::path;
use std::sync::Arc;

mod config;
mod endpoint;
mod log_error;
mod mavlink;
mod router;

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The configuration file to use.
    #[arg(short, long, default_value = "config/default.yml")]
    config: path::PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::builder()
        .format_module_path(false)
        .format_target(false)
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    let args = Args::parse();
    let settings = config::Settings::load(&args.config)?;

    // Load the message offsets from the XML definitions
    let deserializer = mavlink::definitions::try_get_offsets_from_xml(settings.definitions)
        .inspect(|offsets| info!("Found {} targeted messages.", offsets.len()))
        .map(mavlink::Deserializer::new)
        .map(Arc::new)?;

    let mut router = router::Router::from_settings(settings.router, deserializer).await?;

    info!("Starting router...");
    router.start_routing().await;

    Ok(())
}
