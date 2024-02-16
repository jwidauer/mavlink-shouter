use config::Config;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpEndpointSettings {
    pub address: SocketAddr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EndpointKind {
    Udp(UdpEndpointSettings),
    Serial,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointSettings {
    pub name: String,
    pub kind: EndpointKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// The path to the XML definition file.
    pub definitions: path::PathBuf,
    pub endpoints: Vec<EndpointSettings>,
}

impl Settings {
    pub fn load(config: &path::Path) -> Result<Self, config::ConfigError> {
        Config::builder()
            .add_source(config::File::from(config))
            .add_source(config::Environment::with_prefix("MAVLINK_SHOUTER"))
            .build()?
            .try_deserialize()
    }
}
