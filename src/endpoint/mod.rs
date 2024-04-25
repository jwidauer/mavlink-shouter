use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;

use receiver::Receiver;
use sender::Sender;
use target_database::TargetDatabase;
use transmitter::*;

use crate::{
    mavlink::{self, Codec},
    // router,
};

mod receiver;
mod sender;
mod target_database;
pub mod transmitter;

type BroadcastTx = broadcast::Sender<mavlink::Message>;
type BroadcastRx = broadcast::Receiver<mavlink::Message>;

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
    pub fn new(name: String, transmitter: Transmitter, broadcast_tx: BroadcastTx) -> Self {
        let name: Name = name.into();
        let (sink, stream) = transmitter.split();
        let discovered_targets = Arc::new(TargetDatabase::new());

        // Create a channel for sending messages to the endpoint
        // let (tx, rx) = mpsc::channel(16);

        let broadcast_rx = broadcast_tx.subscribe();

        let sender = Sender::new(name.clone(), sink, discovered_targets.clone(), broadcast_rx);
        let receiver = Receiver::new(name, stream, discovered_targets, broadcast_tx);
        Self { sender, receiver }
    }

    pub fn from_settings(
        settings: EndpointSettings,
        broadcaster: BroadcastTx,
        codec: Codec,
    ) -> Result<Self, std::io::Error> {
        let transmitter = Transmitter::new(codec, settings.kind)?;
        Ok(Self::new(settings.name, transmitter, broadcaster))
    }

    pub fn start(self) {
        // Start sending messages received from the router
        let sender = self.sender;
        tokio::spawn(async move {
            sender.run().await;
        });

        // Start receiving messages from the endpoint
        let receiver = self.receiver;
        tokio::spawn(async move {
            receiver.run().await;
        });
    }
}
