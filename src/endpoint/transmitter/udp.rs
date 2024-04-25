use futures::stream::StreamExt;
use futures_sink::Sink;
use log::debug;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::UdpSocket;
use tokio_stream::Stream;
use tokio_util::udp::UdpFramed;

use crate::mavlink::Codec;

use super::{Data, RecvResult, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    // The address to bind to.
    pub address: SocketAddr,
}

pub struct UdpTransmitter {
    stream: UdpFramed<Codec>,
}

impl UdpTransmitter {
    pub fn new(codec: Codec, settings: Settings) -> Result<Self> {
        // let channel_size = 16;
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
        let socket = UdpSocket::from_std(socket)?;

        let stream = UdpFramed::new(socket, codec);

        // Spawn tasks to send and receive messages, with corresponding channels
        // let receiver = start_receiver_task(socket.clone(), channel_size);
        // let sender = start_sender_task(socket, channel_size);

        Ok(Self { stream })
    }

    pub fn split(
        self,
    ) -> (
        impl Sink<Data, Error = std::io::Error>,
        impl Stream<Item = RecvResult>,
    ) {
        self.stream.split()
    }
}

// fn start_sender_task(socket: Arc<UdpSocket>, channel_size: usize) -> super::Sender {
//     // Spawn a task to send messages
//     let (tx, rx) = mpsc::channel(channel_size);
//     tokio::spawn(async move {
//         send(socket, rx).await;
//     });
//     tx
// }

// fn start_receiver_task(socket: Arc<UdpSocket>, channel_size: usize) -> super::Receiver {
//     // Spawn a task to receive messages
//     let (tx, rx) = mpsc::channel(channel_size);
//     tokio::spawn(async move {
//         recv(socket, tx).await;
//     });
//     rx
// }

// async fn recv(socket: Arc<UdpSocket>, tx: mpsc::Sender<RecvResult>) {
//     let mut buf = [0; 65535];
//     loop {
//         let data = socket
//             .recv_from(&mut buf)
//             .await
//             .map(|(amt, addr)| (buf[..amt].to_vec().into(), addr));
//         if tx.send(data).await.log_error().is_none() {
//             // The receiver has been dropped
//             break;
//         }
//     }
// }

// async fn send(socket: Arc<UdpSocket>, mut rx: mpsc::Receiver<(Message, SocketAddr)>) {
//     while let Some((msg, target)) = rx.recv().await {
//         socket.send_to(&msg.data, target).await.log_error();
//     }
// }
