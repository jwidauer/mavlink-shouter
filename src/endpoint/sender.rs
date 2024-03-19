use super::{target_database::TargetDatabase, transmitter, Name};
use crate::{log_error::LogError, mavlink};
use log::debug;
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct Sender {
    name: Name,
    sender: transmitter::Sender,
    discovered_targets: Arc<TargetDatabase>,
    msg_rx: mpsc::Receiver<mavlink::Message>,
}

impl Sender {
    pub fn new(
        name: Name,
        sender: transmitter::Sender,
        discovered_targets: Arc<TargetDatabase>,
        msg_rx: mpsc::Receiver<mavlink::Message>,
    ) -> Self {
        Self {
            name,
            sender,
            discovered_targets,
            msg_rx,
        }
    }

    async fn send(&self, msg: mavlink::Message) {
        for target in self
            .discovered_targets
            .get_target_addresses(&msg.routing_info)
        {
            debug!("[{}] Sending message to: {}", self.name, target);
            self.sender
                .send((msg.data.clone(), target))
                .await
                .log_error();
        }
    }

    pub async fn run(&mut self) {
        while let Some(msg) = self.msg_rx.recv().await {
            self.send(msg).await;
        }
    }
}
