use config::Config;
use serde::{Deserialize, Serialize};
use std::path;

use crate::router::RouterSettings;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// The path to the XML definition file.
    pub definitions: path::PathBuf,
    pub router: RouterSettings,
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

#[cfg(test)]
mod tests {
    use crate::endpoint::{EndpointKind, UdpEndpointSettings};

    use super::*;
    use std::net::{IpAddr, SocketAddr};
    use std::path::Path;

    #[test]
    fn test_load_settings() -> Result<(), config::ConfigError> {
        let config_path =
            Path::new(std::env!("CARGO_MANIFEST_DIR")).join("tests/resources/config.yml");
        let settings = Settings::load(config_path.as_path())?;
        assert_eq!(
            settings.definitions,
            path::PathBuf::from("tests/fixtures/definitions.xml")
        );
        assert_eq!(settings.router.endpoints.len(), 2);
        assert_eq!(settings.router.endpoints[0].name, "udp");
        assert_eq!(
            settings.router.endpoints[0].kind,
            EndpointKind::Udp(UdpEndpointSettings {
                address: SocketAddr::new(IpAddr::V4("127.0.0.1".parse().unwrap()), 14550)
            })
        );
        assert_eq!(settings.router.endpoints[1].name, "serial");
        assert_eq!(settings.router.endpoints[1].kind, EndpointKind::Serial);
        Ok(())
    }
}
