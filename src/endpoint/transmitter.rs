use std::net::SocketAddr;

use crate::mavlink;

#[async_trait::async_trait]
pub trait Transmitter {
    async fn send_to(
        &self,
        msg: &mavlink::Message,
        target: SocketAddr,
    ) -> Result<(), std::io::Error>;
    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr), std::io::Error>;
}
