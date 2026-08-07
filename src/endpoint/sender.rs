use super::{target_database::TargetDatabase, transmitter, Name};
use crate::{log_error::LogError, mavlink};
use log::debug;
use std::sync::Arc;

use crate::types::*;

pub struct Sender {
    name: Name,
    sender: transmitter::Sender,
    discovered_targets: Arc<TargetDatabase>,
    msg_rx: EndpointRx,
}

impl Sender {
    pub fn new(
        name: Name,
        sender: transmitter::Sender,
        discovered_targets: Arc<TargetDatabase>,
        msg_rx: EndpointRx,
    ) -> Self {
        Self {
            name,
            sender,
            discovered_targets,
            msg_rx,
        }
    }

    fn send(&self, msg: mavlink::Message) {
        for target in self
            .discovered_targets
            .get_target_addresses(&msg.routing_info)
        {
            debug!("[{}] Sending message to: {}", self.name, target);
            self.sender.send((msg.data.clone(), target)).log_error();
        }
    }

    pub fn run(&mut self) {
        while let Ok(msg) = self.msg_rx.recv() {
            self.send(msg);
        }
    }
}
