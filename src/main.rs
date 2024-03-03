use anyhow::Result;
use clap::Parser;
use log::info;
use std::path;
use std::sync::Arc;

use endpoint::Endpoint;

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
        .init();

    let args = Args::parse();
    let settings = config::Settings::load(&args.config)?;

    // Load the message offsets from the XML definitions
    let deserializer = mavlink::definitions::try_get_offsets_from_xml(settings.definitions)
        .inspect(|offsets| info!("Found {} targeted messages.", offsets.len()))
        .map(mavlink::Deserializer::new)
        .map(Arc::new)?;

    let (tx, mut router) = router::Router::new();

    info!("Creating endpoints...");
    let endpoints = settings
        .endpoints
        .into_iter()
        .map(|settings| {
            let (endpoint_tx, endpoint) =
                Endpoint::from_settings(settings, tx.clone(), deserializer.clone())?;

            router.add_endpoint(endpoint_tx);

            Ok(endpoint)
        })
        .collect::<Result<Vec<_>, std::io::Error>>()?;

    info!("Starting endpoints...");
    for endpoint in endpoints {
        endpoint.start().await;
    }

    info!("Starting router...");
    router.start_routing().await;

    Ok(())
}
