use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;

use receiver::Receiver;
use sender::Sender;
use target_database::TargetDatabase;
use transmitter::*;

use crate::{mavlink, router};

mod receiver;
mod sender;
mod target_database;
pub mod transmitter;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndpointSettings {
    pub name: String,
    pub kind: transmitter::Settings,
}

type Name = Arc<str>;

pub struct Endpoint {
    sender: Sender,
    receiver: Receiver,
}

impl Endpoint {
    pub fn new(
        name: String,
        transmitter: Transmitter,
        routing_channel: router::RouterTx,
        deserializer: Arc<mavlink::Deserializer>,
    ) -> (mpsc::Sender<mavlink::Message>, Self) {
        let name: Name = name.into();
        let (transmitter_tx, transmitter_rx) = transmitter.split();
        let discovered_targets = Arc::new(TargetDatabase::new());

        // Create a channel for sending messages to the endpoint
        let (tx, rx) = mpsc::channel(16);

        let sender = Sender::new(name.clone(), transmitter_tx, discovered_targets.clone(), rx);
        let receiver = Receiver::new(
            name,
            transmitter_rx,
            discovered_targets,
            routing_channel,
            deserializer,
        );
        (tx, Self { sender, receiver })
    }

    pub fn from_settings(
        settings: EndpointSettings,
        routing_channel: mpsc::Sender<mavlink::Message>,
        deserializer: Arc<mavlink::Deserializer>,
    ) -> Result<(mpsc::Sender<mavlink::Message>, Self), std::io::Error> {
        let transmitter = Transmitter::new(settings.kind)?;
        Ok(Self::new(
            settings.name,
            transmitter,
            routing_channel,
            deserializer,
        ))
    }

    pub fn start(self) {
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
