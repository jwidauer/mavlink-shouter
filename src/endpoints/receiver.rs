use super::target_database::TargetDatabase;
use super::transmitter::Transmitter;
use super::Name;
use crate::log_error::LogError;
use crate::mavlink;
use log::error;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, thiserror::Error)]
pub enum ReceiverError {
    #[error("[{0}] Failed to receive message: {1}")]
    Receive(Name, std::io::Error),
    #[error("[{0}] Failed to deserialize message: {1}")]
    Deserialization(Name, mavlink::DeserializationError),
    #[error("[{0}] Failed to send message to router: {1}")]
    SendToRouter(Name, mpsc::error::SendError<mavlink::Message>),
}

pub struct Receiver {
    name: Name,
    transmitter: Arc<dyn Transmitter + Sync + Send>,
    discovered_targets: Arc<TargetDatabase>,
    msg_tx: mpsc::Sender<mavlink::Message>,
    deserializer: Arc<mavlink::Deserializer>,
}

impl Receiver {
    pub fn new(
        name: Name,
        transmitter: Arc<dyn Transmitter + Sync + Send>,
        discovered_targets: Arc<TargetDatabase>,
        msg_tx: mpsc::Sender<mavlink::Message>,
        deserializer: Arc<mavlink::Deserializer>,
    ) -> Self {
        Self {
            name,
            transmitter,
            discovered_targets,
            msg_tx,
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
            .map(|msg| {
                self.discovered_targets.insert_or_update(msg.sender, addr);
                msg
            })
            .map_err(|e| ReceiverError::Deserialization(self.name.clone(), e))
    }

    pub async fn run(&mut self) {
        loop {
            if let Some(msg) = self.recv().await.log_error() {
                if self
                    .msg_tx
                    .send(msg)
                    .await
                    .map_err(|e| ReceiverError::SendToRouter(self.name.clone(), e))
                    .log_error()
                    .is_none()
                {
                    return;
                }
            }
        }
    }
}
