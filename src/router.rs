use crate::{log_error::LogError, mavlink};
use tokio::sync::{broadcast, mpsc};

pub type RouterTx = broadcast::Sender<mavlink::Message>;

pub struct Router {
    msg_tx: RouterTx,
    msg_rx: broadcast::Receiver<mavlink::Message>,
    endpoints_tx: Vec<mpsc::Sender<mavlink::Message>>,
}

impl Router {
    pub fn tx(&self) -> mpsc::Sender<mavlink::Message> {
        self.msg_tx.clone()
    }

    pub fn add_endpoint(&mut self, tx: mpsc::Sender<mavlink::Message>) {
        self.endpoints_tx.push(tx);
    }

    pub fn start(mut self) {
        tokio::spawn(async move {
            self.route().await;
        });
    }

    async fn route(&mut self) {
        while let Some(msg) = self.msg_rx.recv().await {
            for tx in &self.endpoints_tx {
                tx.send(msg.clone()).await.log_error();
            }
        }
    }
}

impl Default for Router {
    fn default() -> Self {
        // Create a channel for sending messages to the router
        let (msg_tx, msg_rx) = broadcast::channel(128);

        Self {
            msg_tx,
            msg_rx,
            endpoints_tx: Vec::new(),
        }
    }
}
