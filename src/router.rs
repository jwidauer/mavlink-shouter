use crate::{log_error::LogError, mavlink};
use std::sync::mpsc;

use crate::types::*;

pub struct Router {
    msg_tx: RouterTx,
    msg_rx: RouterRx,
    endpoints_tx: Vec<mpsc::SyncSender<mavlink::Message>>,
}

impl Router {
    pub fn tx(&self) -> RouterTx {
        self.msg_tx.clone()
    }

    pub fn add_endpoint(&mut self, tx: EndpointTx) {
        self.endpoints_tx.push(tx);
    }

    pub fn start(mut self) {
        std::thread::spawn(move || {
            self.route();
        });
    }

    fn route(&mut self) {
        while let Ok(msg) = self.msg_rx.recv() {
            for tx in &self.endpoints_tx {
                tx.send(msg.clone()).log_error();
            }
        }
    }
}

impl Default for Router {
    fn default() -> Self {
        // Create a channel for sending messages to the router
        let (msg_tx, msg_rx) = mpsc::sync_channel(128);

        Self {
            msg_tx,
            msg_rx,
            endpoints_tx: Vec::new(),
        }
    }
}
