use crate::{log_error::LogError, mavlink};
use tokio::sync::mpsc;

pub type RouterTx = mpsc::Sender<mavlink::Message>;

pub struct Router {
    msg_tx: RouterTx,
    msg_rx: mpsc::Receiver<mavlink::Message>,
    endpoints_tx: Vec<mpsc::Sender<mavlink::Message>>,
}

impl Router {
    pub fn new() -> Self {
        // Create a channel for sending messages to the router
        let (msg_tx, msg_rx) = mpsc::channel(16);

        Self {
            msg_tx,
            msg_rx,
            endpoints_tx: Vec::new(),
        }
    }

    pub fn tx(&self) -> mpsc::Sender<mavlink::Message> {
        self.msg_tx.clone()
    }

    pub fn add_endpoint(&mut self, tx: mpsc::Sender<mavlink::Message>) {
        self.endpoints_tx.push(tx);
    }

    pub async fn start_routing(&mut self) {
        while let Some(msg) = self.msg_rx.recv().await {
            for tx in &self.endpoints_tx {
                tx.send(msg.clone()).await.log_error();
            }
        }
    }
}
