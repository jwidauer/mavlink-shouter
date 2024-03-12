use super::{target_database::TargetDatabase, transmitter::Transmitter, Name};
use crate::{log_error::LogError, mavlink};
use log::debug;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, thiserror::Error)]
enum SenderError {
    #[error("[{0}] Failed to send message: {1}")]
    Send(Name, #[source] std::io::Error),
}

pub struct Sender {
    name: Name,
    transmitter: Arc<dyn Transmitter + Sync + Send>,
    discovered_targets: Arc<TargetDatabase>,
    msg_rx: mpsc::Receiver<mavlink::Message>,
}

impl Sender {
    pub fn new(
        name: Name,
        transmitter: Arc<dyn Transmitter + Sync + Send>,
        discovered_targets: Arc<TargetDatabase>,
        msg_rx: mpsc::Receiver<mavlink::Message>,
    ) -> Self {
        Self {
            name,
            transmitter,
            discovered_targets,
            msg_rx,
        }
    }

    async fn send(&self, msg: mavlink::Message) -> Result<(), SenderError> {
        for target in self
            .discovered_targets
            .get_target_addresses(&msg.routing_info)
        {
            debug!("[{}] Sending message to: {}", self.name, target);
            self.transmitter
                .send_to(&msg, target)
                .await
                .map_err(|e| SenderError::Send(self.name.clone(), e))?;
        }

        Ok(())
    }

    pub async fn run(&mut self) {
        while let Some(msg) = self.msg_rx.recv().await {
            self.send(msg).await.log_error();
        }
    }
}
