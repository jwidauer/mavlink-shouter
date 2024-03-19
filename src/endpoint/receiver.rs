use super::{target_database::TargetDatabase, transmitter, Name};
use crate::{log_error::LogError, mavlink, router};
use log::{debug, error};
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, thiserror::Error)]
pub enum ReceiverError {
    #[error("[{0}] Failed to receive message")]
    Receive(Name, #[source] std::io::Error),
    #[error("[{0}] Failed to deserialize message")]
    Deserialization(Name, #[source] mavlink::DeserializationError),
    #[error("[{0}] Failed to send message to router")]
    SendToRouter(Name, #[source] mpsc::error::SendError<mavlink::Message>),
}

pub struct Receiver {
    name: Name,
    receiver: transmitter::Receiver,
    discovered_targets: Arc<TargetDatabase>,
    msg_tx: router::RouterTx,
    deserializer: Arc<mavlink::Deserializer>,
}

impl Receiver {
    pub fn new(
        name: Name,
        receiver: transmitter::Receiver,
        discovered_targets: Arc<TargetDatabase>,
        msg_tx: router::RouterTx,
        deserializer: Arc<mavlink::Deserializer>,
    ) -> Self {
        Self {
            name,
            receiver,
            discovered_targets,
            msg_tx,
            deserializer,
        }
    }

    fn deserialize(&self, data: transmitter::Data) -> Result<mavlink::Message, ReceiverError> {
        let (msg, addr) = data;
        self.deserializer
            .deserialize(msg)
            .inspect(|_| debug!("[{}] Received message from: {}", self.name, addr))
            .inspect(|msg| self.validate_and_update_db(msg, addr))
            .map_err(|e| ReceiverError::Deserialization(self.name.clone(), e))
    }

    pub async fn run(&mut self) {
        while let Some(data) = self.receiver.recv().await {
            if let Some(msg) = data
                .map_err(|e| ReceiverError::Receive(self.name.clone(), e))
                .and_then(|data| self.deserialize(data))
                .log_error()
            {
                if self
                    .msg_tx
                    .send(msg)
                    .await
                    .map_err(|e| ReceiverError::SendToRouter(self.name.clone(), e))
                    .log_error()
                    .is_none()
                {
                    // If the router is gone, we should stop receiving messages
                    return;
                }
            }
        }
    }

    fn validate_and_update_db(&self, msg: &mavlink::Message, addr: std::net::SocketAddr) {
        if msg.routing_info.sender.is_valid_sender() {
            self.discovered_targets
                .insert_or_update(msg.routing_info.sender, addr);
        } else {
            error!(
                "[{}] Received message from '{}' with invalid sender id: {}",
                self.name, addr, msg.routing_info.sender
            );
        }
    }
}
