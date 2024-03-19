use anyhow::Result;
use clap::Parser;
use log::info;
use std::path;
use std::sync::Arc;

use endpoint::{Endpoint, EndpointSettings};

mod config;
mod endpoint;
mod log_error;
mod mavlink;
mod router;

fn endpoints_from_settings(
    settings: Vec<EndpointSettings>,
    router: &mut router::Router,
    deserializer: Arc<mavlink::Deserializer>,
) -> Result<Vec<Endpoint>> {
    settings
        .into_iter()
        .map(|settings| {
            let (endpoint_tx, endpoint) =
                Endpoint::from_settings(settings, router.tx(), deserializer.clone())?;

            router.add_endpoint(endpoint_tx);
            Ok(endpoint)
        })
        .collect()
}

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

    let mut router = router::Router::new();

    info!("Creating endpoints...");
    let endpoints = endpoints_from_settings(settings.endpoints, &mut router, deserializer)?;

    info!("Starting endpoints...");
    for endpoint in endpoints {
        endpoint.start();
    }

    info!("Starting router...");
    router.start_routing().await;

    Ok(())
}
