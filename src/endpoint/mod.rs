use serde::{Deserialize, Serialize};
use std::sync::{mpsc, Arc};

use receiver::Receiver;
use sender::Sender;
use target_database::TargetDatabase;
use transmitter::*;

use crate::{mavlink, types::*};

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
        routing_channel: RouterTx,
        deserializer: Arc<mavlink::Deserializer>,
    ) -> (EndpointTx, Self) {
        let name: Name = name.into();
        let (transmitter_tx, transmitter_rx) = transmitter.split();
        let discovered_targets = Arc::new(TargetDatabase::new());

        // Create a channel for sending messages to the endpoint
        let (tx, rx) = mpsc::sync_channel(16);

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
        routing_channel: RouterTx,
        deserializer: Arc<mavlink::Deserializer>,
    ) -> Result<(EndpointTx, Self), std::io::Error> {
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
        std::thread::spawn(move || {
            sender.run();
        });

        // Start receiving messages from the endpoint
        let mut receiver = self.receiver;
        std::thread::spawn(move || {
            receiver.run();
        });
    }
}
