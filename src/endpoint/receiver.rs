use super::{target_database::TargetDatabase, transmitter::Transmitter, Id, Name};
use crate::mavlink;
use crate::{log_error::LogError, router};
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
    SendToRouter(Name, #[source] mpsc::error::SendError<router::Message>),
}

pub struct Receiver {
    id: Id,
    name: Name,
    transmitter: Arc<dyn Transmitter + Sync + Send>,
    discovered_targets: Arc<TargetDatabase>,
    router_tx: router::RouterTx,
    deserializer: Arc<mavlink::Deserializer>,
}

impl Receiver {
    pub fn new(
        id: Id,
        name: Name,
        transmitter: Arc<dyn Transmitter + Sync + Send>,
        discovered_targets: Arc<TargetDatabase>,
        router_tx: router::RouterTx,
        deserializer: Arc<mavlink::Deserializer>,
    ) -> Self {
        Self {
            id,
            name,
            transmitter,
            discovered_targets,
            router_tx,
            deserializer,
        }
    }

    async fn recv(&self) -> Result<mavlink::Message, ReceiverError> {
        let mut buf = [0; 65535];
        let (amt, addr) = self
            .transmitter
            .recv_from(&mut buf)
            .await
            .map_err(|e| ReceiverError::Receive(self.name.clone(), e))?;
        let msg = &buf[..amt];
        self.deserializer
            .deserialize(msg)
            .inspect(|_| debug!("[{}] Received message from: {}", self.name, addr))
            .inspect(|msg| match msg.routing_info.sender.is_valid_sender() {
                true => self
                    .discovered_targets
                    .insert_or_update(msg.routing_info.sender, addr),
                false => error!(
                    "[{}] Received message from '{}' with invalid sender id",
                    self.name, addr
                ),
            })
            .map_err(|e| ReceiverError::Deserialization(self.name.clone(), e))
    }

    pub async fn run(&mut self) {
        loop {
            if let Some(msg) = self.recv().await.log_error() {
                if self
                    .router_tx
                    .send(router::Message {
                        endpoint_id: self.id,
                        msg,
                    })
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
}
