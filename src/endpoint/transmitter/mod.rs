use std::{net::SocketAddr, sync::Arc};
use tokio::sync::mpsc;

pub mod tcp;
pub mod udp;

type Result<T> = std::result::Result<T, std::io::Error>;
pub type Data = (Arc<[u8]>, SocketAddr);
pub type RecvResult = Result<Data>;

pub type Sender = mpsc::Sender<Data>;
pub type Receiver = mpsc::Receiver<RecvResult>;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Settings {
    Udp(udp::Settings),
    Tcp(tcp::Settings),
}

pub enum Transmitter {
    Udp(udp::UdpTransmitter),
    Tcp(tcp::TcpTransmitter),
}

impl Transmitter {
    pub fn new(settings: Settings) -> Result<Self> {
        match settings {
            Settings::Udp(settings) => udp::UdpTransmitter::new(settings).map(Self::Udp),
            Settings::Tcp(settings) => tcp::TcpTransmitter::new(settings).map(Self::Tcp),
        }
    }

    pub fn split(self) -> (Sender, Receiver) {
        match self {
            Self::Udp(transmitter) => transmitter.split(),
            Self::Tcp(transmitter) => transmitter.split(),
        }
    }
}
