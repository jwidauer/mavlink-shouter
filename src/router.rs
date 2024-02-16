use crate::mavlink;
use log::error;
use tokio::sync::mpsc;

pub struct Router {
    msg_rx: mpsc::Receiver<mavlink::Message>,
    endpoints_tx: Vec<mpsc::Sender<mavlink::Message>>,
}

impl Router {
    pub fn new() -> (mpsc::Sender<mavlink::Message>, Self) {
        // Create a channel for sending messages to the router
        let (msg_tx, msg_rx) = mpsc::channel(16);
        (
            msg_tx,
            Self {
                msg_rx,
                endpoints_tx: Vec::new(),
            },
        )
    }

    pub fn add_endpoint(&mut self, tx: mpsc::Sender<mavlink::Message>) {
        self.endpoints_tx.push(tx);
    }

    pub async fn start_routing(&mut self) {
        while let Some(msg) = self.msg_rx.recv().await {
            for tx in &self.endpoints_tx {
                match tx.send(msg.clone()).await {
                    Ok(_) => {}
                    Err(e) => {
                        error!("Failed to send message to endpoint: {}", e);
                    }
                }
            }
        }
    }
}
