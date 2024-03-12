use receiver::Receiver;
use sender::Sender;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};
use target_database::TargetDatabase;
use tokio::sync::mpsc;
use transmitter::Transmitter;

use crate::{mavlink, router};

mod receiver;
mod sender;
mod target_database;
mod transmitter;
pub mod udp;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UdpEndpointSettings {
    // The address to bind to.
    pub address: SocketAddr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EndpointKind {
    Udp(UdpEndpointSettings),
    Serial,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndpointSettings {
    pub name: String,
    pub kind: EndpointKind,
}

type Name = Arc<str>;
pub type Id = usize;

pub struct Endpoint {
    sender: Sender,
    receiver: Receiver,
    tx: mpsc::Sender<mavlink::Message>,
}

impl Endpoint {
    pub fn new(
        id: usize,
        name: String,
        transmitter: impl Transmitter + Send + Sync + 'static,
        routing_channel: router::RouterTx,
        deserializer: Arc<mavlink::Deserializer>,
    ) -> Self {
        let name: Name = name.into();
        let transmitter = Arc::new(transmitter);
        let discovered_targets = Arc::new(TargetDatabase::new());

        // Create a channel for sending messages to the endpoint
        let (tx, rx) = mpsc::channel(16);

        let sender = Sender::new(
            name.clone(),
            transmitter.clone(),
            discovered_targets.clone(),
            rx,
        );
        let receiver = Receiver::new(
            id,
            name,
            transmitter,
            discovered_targets,
            routing_channel,
            deserializer,
        );
        Self {
            sender,
            receiver,
            tx,
        }
    }

    pub fn from_settings(
        id: Id,
        settings: EndpointSettings,
        routing_channel: router::RouterTx,
        deserializer: Arc<mavlink::Deserializer>,
    ) -> Result<Self, std::io::Error> {
        let transmitter = match settings.kind {
            EndpointKind::Udp(udp_settings) => udp::UdpTransmitter::new(udp_settings.address)?,
            EndpointKind::Serial => unimplemented!("Serial endpoints are not yet supported."),
        };
        Ok(Self::new(
            id,
            settings.name,
            transmitter,
            routing_channel,
            deserializer,
        ))
    }

    pub fn tx(&self) -> mpsc::Sender<mavlink::Message> {
        self.tx.clone()
    }

    pub async fn start(self) {
        // Start sending messages received from the router
        let mut sender = self.sender;
        tokio::spawn(async move {
            sender.run().await;
        });

        // Start receiving messages from the endpoint
        let mut receiver = self.receiver;
        tokio::spawn(async move {
            receiver.run().await;
        });
    }
}
