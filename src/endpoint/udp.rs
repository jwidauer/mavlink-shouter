use std::net::{SocketAddr, ToSocketAddrs};
use tokio::net::UdpSocket;

pub struct UdpTransmitter {
    socket: UdpSocket,
}

impl UdpTransmitter {
    pub fn new<A: ToSocketAddrs>(addr: A) -> Result<Self, std::io::Error> {
        let socket = std::net::UdpSocket::bind(addr)?;
        let socket = UdpSocket::from_std(socket)?;
        Ok(Self { socket })
    }
}

#[async_trait::async_trait]
impl super::Transmitter for UdpTransmitter {
    async fn send_to(
        &self,
        msg: &super::mavlink::Message,
        target: SocketAddr,
    ) -> Result<(), std::io::Error> {
        self.socket.send_to(&msg.data, target).await.map(|_| ())
    }

    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr), std::io::Error> {
        self.socket.recv_from(buf).await
    }
}
