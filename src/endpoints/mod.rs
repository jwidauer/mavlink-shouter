use crate::mavlink;
use receiver::Receiver;
use sender::Sender;
use std::sync::Arc;
use target_database::TargetDatabase;
use tokio::sync::mpsc;
use transmitter::Transmitter;

mod receiver;
mod sender;
mod target_database;
mod transmitter;
pub mod udp;

type Name = Arc<str>;

pub struct Endpoint {
    sender: Sender,
    receiver: Receiver,
}

impl Endpoint {
    pub fn new(
        name: String,
        transmitter: impl Transmitter + Send + Sync + 'static,
        routing_channel: mpsc::Sender<mavlink::Message>,
        deserializer: Arc<mavlink::Deserializer>,
    ) -> (mpsc::Sender<mavlink::Message>, Self) {
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
            name,
            transmitter,
            discovered_targets,
            routing_channel,
            deserializer,
        );
        (tx, Self { sender, receiver })
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
