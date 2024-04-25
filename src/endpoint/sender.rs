use futures::SinkExt;
use futures_sink::Sink;
use log::{debug, warn};
use std::{pin::Pin, sync::Arc};
use tokio::sync::broadcast::error::RecvError;

use super::{target_database::TargetDatabase, BroadcastRx, Data, Name};
use crate::{log_error::LogError, mavlink};

pub struct Sender {
    name: Name,
    sink: Pin<Box<dyn Sink<Data, Error = std::io::Error> + Send>>,
    discovered_targets: Arc<TargetDatabase>,
    msg_rx: BroadcastRx,
}

impl Sender {
    pub fn new(
        name: Name,
        sink: impl Sink<Data, Error = std::io::Error> + Send + 'static,
        discovered_targets: Arc<TargetDatabase>,
        msg_rx: BroadcastRx,
    ) -> Self {
        Self {
            name,
            sink: Box::pin(sink),
            discovered_targets,
            msg_rx,
        }
    }

    pub async fn run(mut self) {
        loop {
            match self.msg_rx.recv().await {
                Ok(msg) => self.send(msg).await.log_error().unwrap_or_default(),
                Err(RecvError::Lagged(nr)) => warn!(target: &self.name, "dropped {} msgs", nr),
                Err(RecvError::Closed) => break,
            };
        }
        warn!(target: &self.name, "sender stopping");
    }

    async fn send(&mut self, msg: mavlink::Message) -> Result<(), std::io::Error> {
        for target in self
            .discovered_targets
            .get_target_addresses(&msg.routing_info)
        {
            debug!(target: &self.name,
                "Sending message to: {} (sender: {}, target: {})",
                target, msg.routing_info.sender, msg.routing_info.target
            );
            self.sink.feed((msg.clone(), target)).await?;
        }
        self.sink.flush().await
    }
}
