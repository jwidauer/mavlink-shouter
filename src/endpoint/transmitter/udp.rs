use log::debug;
use serde::{Deserialize, Serialize};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::{net::UdpSocket, sync::mpsc};

use crate::log_error::LogError;

use super::{RecvResult, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    // The address to bind to.
    pub address: SocketAddr,
}

pub struct UdpTransmitter {
    sender: super::Sender,
    receiver: super::Receiver,
}

impl UdpTransmitter {
    pub fn new(settings: Settings) -> Result<Self> {
        let channel_size = 16;
        let addr = settings.address;

        debug!("Binding UDP socket to {}", addr);
        let socket = std::net::UdpSocket::bind(addr)?;

        // Join multicast group if address is multicast
        if addr.ip().is_multicast() {
            debug!("Joining multicast group {}", addr.ip());
            match addr.ip() {
                IpAddr::V4(a) => socket.join_multicast_v4(&a, &Ipv4Addr::UNSPECIFIED)?,
                IpAddr::V6(a) => socket.join_multicast_v6(&a, 0)?,
            }
        }
        socket.set_nonblocking(true)?;
        let socket = Arc::new(UdpSocket::from_std(socket)?);

        // Spawn tasks to send and receive messages, with corresponding channels
        let receiver = start_receiver_task(socket.clone(), channel_size);
        let sender = start_sender_task(socket, channel_size);

        Ok(Self { sender, receiver })
    }

    pub fn split(self) -> (super::Sender, super::Receiver) {
        (self.sender, self.receiver)
    }
}

fn start_sender_task(socket: Arc<UdpSocket>, channel_size: usize) -> super::Sender {
    // Spawn a task to send messages
    let (tx, rx) = mpsc::channel(channel_size);
    tokio::spawn(async move {
        send(socket, rx).await;
    });
    tx
}

fn start_receiver_task(socket: Arc<UdpSocket>, channel_size: usize) -> super::Receiver {
    // Spawn a task to receive messages
    let (tx, rx) = mpsc::channel(channel_size);
    tokio::spawn(async move {
        recv(socket, tx).await;
    });
    rx
}

async fn recv(socket: Arc<UdpSocket>, tx: mpsc::Sender<RecvResult>) {
    let mut buf = [0; 65535];
    loop {
        let data = socket
            .recv_from(&mut buf)
            .await
            .map(|(amt, addr)| (buf[..amt].to_vec().into(), addr));
        if tx.send(data).await.log_error().is_none() {
            // The receiver has been dropped
            break;
        }
    }
}

async fn send(socket: Arc<UdpSocket>, mut rx: mpsc::Receiver<(Arc<[u8]>, SocketAddr)>) {
    while let Some((msg, target)) = rx.recv().await {
        socket.send_to(&msg, target).await.log_error();
    }
}
