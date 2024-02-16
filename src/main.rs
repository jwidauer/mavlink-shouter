use anyhow::Result;
use clap::Parser;
use log::info;
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};
use std::path;
use std::sync::Arc;

mod config;
pub(crate) mod definitions;
pub(crate) mod endpoints;
mod log_error;
pub(crate) mod mavlink;
pub(crate) mod router;

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The configuration file to use.
    #[arg(short, long, default_value = "config/default.yml")]
    config: path::PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    TermLogger::init(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

    let args = Args::parse();
    let settings = config::Settings::load(&args.config)?;

    // Load the message offsets from the XML definitions
    let deserializer =
        definitions::try_get_offsets_from_xml(settings.definitions).map(|offsets| {
            info!("Found {} targeted messages.", offsets.len());
            Arc::new(mavlink::Deserializer::new(offsets))
        })?;

    let (tx, mut router) = router::Router::new();

    info!("Creating endpoints...");
    let endpoints = settings
        .endpoints
        .into_iter()
        .map(|endpoint| {
            let transmitter = match endpoint.kind {
                config::EndpointKind::Udp(settings) => {
                    endpoints::udp::UdpTransmitter::new(settings.address)
                        .map_err(|e| anyhow::anyhow!("Failed to create UDP transmitter: {}", e))
                        .unwrap()
                }
                config::EndpointKind::Serial => {
                    unimplemented!("Serial endpoints are not yet supported.")
                }
            };

            let (endpoint_tx, endpoint) = endpoints::Endpoint::new(
                endpoint.name,
                transmitter,
                tx.clone(),
                deserializer.clone(),
            );

            router.add_endpoint(endpoint_tx);

            endpoint
        })
        .collect::<Vec<_>>();

    info!("Starting endpoints...");
    for endpoint in endpoints {
        endpoint.start().await;
    }

    info!("Starting router...");
    router.start_routing().await;

    Ok(())
}
