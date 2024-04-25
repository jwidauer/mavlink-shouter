use super::{target_database::TargetDatabase, BroadcastTx, Name, RecvResult};
use crate::{
    log_error::LogError,
    mavlink::{self},
    // router,
};
use futures::Stream;
use log::{debug, error};
use std::{pin::Pin, sync::Arc};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;

#[derive(Debug, thiserror::Error)]
pub enum ReceiverError {
    // #[error("[{0}] Failed to deserialize message")]
    // Deserialization(Name, #[source] mavlink::DeserializationError),
    #[error("[{0}] Failed to send message to router")]
    SendToRouter(
        Name,
        #[source] broadcast::error::SendError<mavlink::Message>,
    ),
}

/// Receives messages from a stream and sends them to the router
pub struct Receiver {
    name: Name,
    stream: Pin<Box<dyn Stream<Item = RecvResult> + Send>>,
    discovered_targets: Arc<TargetDatabase>,
    msg_tx: BroadcastTx,
}

impl Receiver {
    pub fn new(
        name: Name,
        stream: impl Stream<Item = RecvResult> + Send + 'static,
        discovered_targets: Arc<TargetDatabase>,
        msg_tx: BroadcastTx,
    ) -> Self {
        Self {
            name,
            stream: Box::pin(stream),
            discovered_targets,
            msg_tx,
        }
    }

    pub async fn run(mut self) {
        while let Some(data) = self.stream.next().await {
            let (msg, addr) = match data.log_error() {
                Some(data) => data,
                None => continue,
            };
            debug!(target: &self.name,
                "Received message from: {} (sender: {}, target: {})",
                addr, msg.routing_info.sender, msg.routing_info.target
            );
            self.validate_and_update_db(&msg, addr);

            if self
                .msg_tx
                .send(msg)
                // .await
                .map_err(|e| ReceiverError::SendToRouter(self.name.clone(), e))
                .log_error()
                .is_none()
            {
                // If the router is gone, we should stop receiving messages
                return;
            }
        }
    }

    fn validate_and_update_db(&self, msg: &mavlink::Message, addr: std::net::SocketAddr) {
        if !msg.routing_info.sender.is_valid_sender() {
            error!(
                "[{}] Received message from '{}' with invalid sender id: {}",
                self.name, addr, msg.routing_info.sender
            );
            return;
        }

        self.discovered_targets
            .insert_or_update(msg.routing_info.sender, addr);
    }
}
