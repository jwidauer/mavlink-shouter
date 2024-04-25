use anyhow::Result;
use log::info;
use std::sync::Arc;
use tokio::sync::broadcast;

use endpoint::{Endpoint, EndpointSettings};

pub mod config;
mod endpoint;
mod log_error;
pub mod mavlink;
// mod router;

fn endpoints_from_settings(
    settings: Vec<EndpointSettings>,
    // router: &mut router::Router,
    codec: mavlink::Codec,
) -> Result<Vec<Endpoint>> {
    let (tx, _) = broadcast::channel(10000);

    settings
        .into_iter()
        .map(|settings| {
            let endpoint = Endpoint::from_settings(settings, tx.clone(), codec.clone())?;

            // router.add_endpoint(endpoint_tx);
            Ok(endpoint)
        })
        .collect()
}

pub struct MAVLinkShouter {
    // router: router::Router,
    endpoints: Vec<Endpoint>,
}

impl MAVLinkShouter {
    pub fn new(settings: config::Settings) -> Result<Self> {
        // Load the message offsets from the XML definitions
        let codec = mavlink::definitions::try_get_offsets_from_xml(settings.definitions)
            .inspect(|offsets| info!("Found {} targeted messages.", offsets.len()))
            .map(Arc::new)
            .map(mavlink::Codec::new)?;

        // let mut router = router::Router::default();

        info!("Creating endpoints...");
        let endpoints = endpoints_from_settings(settings.endpoints, codec)?;

        Ok(Self { endpoints })
    }

    pub fn run(self) {
        info!("Starting endpoints...");
        for endpoint in self.endpoints {
            endpoint.start();
        }

        // info!("Starting router...");
        // self.router.start();
    }
}
