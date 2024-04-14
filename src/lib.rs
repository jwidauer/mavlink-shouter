use anyhow::Result;
use log::info;
use std::sync::Arc;

use endpoint::{Endpoint, EndpointSettings};

pub mod config;
mod endpoint;
mod log_error;
pub mod mavlink;
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

pub struct MAVLinkShouter {
    router: router::Router,
    endpoints: Vec<Endpoint>,
}

impl MAVLinkShouter {
    pub fn new(settings: config::Settings) -> Result<Self> {
        // Load the message offsets from the XML definitions
        let deserializer = mavlink::definitions::try_get_offsets_from_xml(settings.definitions)
            .inspect(|offsets| info!("Found {} targeted messages.", offsets.len()))
            .map(mavlink::Deserializer::new)
            .map(Arc::new)?;

        let mut router = router::Router::default();

        info!("Creating endpoints...");
        let endpoints = endpoints_from_settings(settings.endpoints, &mut router, deserializer)?;

        Ok(Self { router, endpoints })
    }

    pub fn run(self) {
        info!("Starting endpoints...");
        for endpoint in self.endpoints {
            endpoint.start();
        }

        info!("Starting router...");
        self.router.start();
    }
}
