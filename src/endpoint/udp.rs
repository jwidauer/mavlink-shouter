use log::debug;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use tokio::net::UdpSocket;

pub struct UdpTransmitter {
    socket: UdpSocket,
}

impl UdpTransmitter {
    pub fn new<A: ToSocketAddrs>(addr: A) -> Result<Self, std::io::Error> {
        let addr = addr.to_socket_addrs()?.next().ok_or(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Could not resolve to any address",
        ))?;

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
