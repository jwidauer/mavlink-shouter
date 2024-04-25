use futures::Stream;
use futures_sink::Sink;
use log::info;
use std::net::SocketAddr;

use crate::mavlink::{Codec, Message};

// pub mod tcp;
pub mod udp;

type Result<T> = std::result::Result<T, std::io::Error>;
pub type Data = (Message, SocketAddr);
pub type RecvResult = Result<Data>;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Settings {
    Udp(udp::Settings),
    // Tcp(tcp::Settings),
}

pub enum Transmitter {
    Udp(udp::UdpTransmitter),
    // Tcp(tcp::TcpTransmitter),
}

impl Transmitter {
    pub fn new(codec: Codec, settings: Settings) -> Result<Self> {
        info!("Creating transmitter with settings: {:?}", settings);
        match settings {
            Settings::Udp(settings) => udp::UdpTransmitter::new(codec, settings).map(Self::Udp),
            // Settings::Tcp(settings) => tcp::TcpTransmitter::new(settings).map(Self::Tcp),
        }
    }

    pub fn split(
        self,
    ) -> (
        impl Sink<Data, Error = std::io::Error>,
        impl Stream<Item = RecvResult>,
    ) {
        match self {
            Self::Udp(transmitter) => transmitter.split(),
            // Self::Tcp(transmitter) => transmitter.split(),
        }
    }
}
