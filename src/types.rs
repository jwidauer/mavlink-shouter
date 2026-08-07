use std::sync::mpsc;

use crate::mavlink;

pub type EndpointTx = mpsc::SyncSender<mavlink::Message>;
pub type EndpointRx = mpsc::Receiver<mavlink::Message>;

pub type RouterTx = mpsc::SyncSender<mavlink::Message>;
pub type RouterRx = mpsc::Receiver<mavlink::Message>;
