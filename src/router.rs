use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::{
    endpoint::{self, Endpoint, EndpointSettings},
    log_error::LogError,
    mavlink,
};

pub struct Message {
    pub endpoint_id: endpoint::Id,
    pub msg: mavlink::Message,
}

pub type RouterTx = mpsc::Sender<Message>;

#[derive(Debug, Error)]
enum RoutingTableError {
    #[error("Sender {0} not found")]
    SenderNotFound(endpoint::Id),
}

// This is a simple routing table that maps endpoint IDs to their respective senders
// and target sys comp IDs to their respective senders.
//
// The router will use this table to route messages to the correct endpoint.
//
// It assumes that there are the same amount of endpoint::Sender and endpoint::Receiver instances.
// It can then be used to map the messages received from the endpoint::Receiver instances to only
// the endpoint::Sender instances that are interested in them.
struct RoutingTable {
    senders: HashMap<endpoint::Id, mpsc::Sender<mavlink::Message>>,
    targets: Vec<(mavlink::SysCompId, mpsc::Sender<mavlink::Message>)>,
}

impl RoutingTable {
    pub fn new() -> Self {
        Self {
            senders: HashMap::new(),
            targets: Vec::new(),
        }
    }

    pub fn insert_sender(&mut self, id: endpoint::Id, tx: mpsc::Sender<mavlink::Message>) {
        self.senders.insert(id, tx);
    }

    pub fn insert_or_update(
        &mut self,
        endpoint_id: endpoint::Id,
        sender: mavlink::SysCompId,
    ) -> Result<(), RoutingTableError> {
        let tx = self
            .senders
            .get(&endpoint_id)
            .cloned()
            .ok_or(RoutingTableError::SenderNotFound(endpoint_id))?;
        match self.targets.iter_mut().find(|(t, _)| t == &sender) {
            Some((_, tx_elem)) => {
                *tx_elem = tx;
            }
            None => {
                self.targets.push((sender, tx));
            }
        }
        Ok(())
    }

    pub fn get_target_senders<'a>(
        &'a self,
        routing_info: &'a mavlink::RoutingInfo,
    ) -> impl Iterator<Item = &'a mpsc::Sender<mavlink::Message>> + 'a {
        self.targets
            .iter()
            .filter(|(t, _)| routing_info.matches(t))
            .map(|(_, tx)| tx)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouterSettings {
    pub channel_size: Option<usize>,
    pub endpoints: Vec<EndpointSettings>,
}

pub struct Router {
    msg_rx: mpsc::Receiver<Message>,
    routing_table: RoutingTable,
}

impl Router {
    pub async fn from_settings(
        settings: RouterSettings,
        deserializer: Arc<mavlink::Deserializer>,
    ) -> Result<Self, std::io::Error> {
        let (msg_tx, msg_rx) = mpsc::channel(settings.channel_size.unwrap_or(16));

        let mut router = Self {
            msg_rx,
            routing_table: RoutingTable::new(),
        };

        info!("Creating endpoints...");
        for (id, endpoint_settings) in settings.endpoints.into_iter().enumerate() {
            let endpoint = Endpoint::from_settings(
                id,
                endpoint_settings,
                msg_tx.clone(),
                deserializer.clone(),
            )?;
            router.routing_table.insert_sender(id, endpoint.tx());

            endpoint.start().await;
        }

        Ok(router)
    }

    pub async fn start_routing(&mut self) {
        while let Some(msg) = self.msg_rx.recv().await {
            self.routing_table
                .insert_or_update(msg.endpoint_id, msg.msg.routing_info.sender)
                .log_error();
            for tx in self.routing_table.get_target_senders(&msg.msg.routing_info) {
                tx.send(msg.msg.clone()).await.log_error();
            }
        }
    }
}
