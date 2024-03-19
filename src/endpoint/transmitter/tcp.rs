use log::debug;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener,
    },
    sync::{mpsc, Mutex},
};

use super::{Data, RecvResult, Result};
use crate::log_error::LogError;

type Connections = Arc<Mutex<HashMap<SocketAddr, OwnedWriteHalf>>>;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub address: SocketAddr,
}

pub struct TcpTransmitter {
    sender: super::Sender,
    receiver: super::Receiver,
}

impl TcpTransmitter {
    pub fn new(settings: Settings) -> Result<Self> {
        let channel_size = 16;
        let addr = settings.address;

        debug!("Binding TCP listener to {}", addr);
        let listener = std::net::TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        let listener = TcpListener::from_std(listener)?;

        // Create a map to store the writer half of the connections
        let connections: Connections = Arc::new(Mutex::new(HashMap::new()));

        // Spawn tasks to accept connections and send messages, with corresponding channels
        let receiver = start_acceptor_task(listener, connections.clone(), channel_size);
        let sender = start_sender_task(connections, channel_size);

        Ok(Self { sender, receiver })
    }

    pub fn split(self) -> (super::Sender, super::Receiver) {
        (self.sender, self.receiver)
    }
}

fn start_sender_task(connections: Connections, channel_size: usize) -> super::Sender {
    let (tx, rx) = mpsc::channel(channel_size);
    tokio::spawn(async move {
        write(rx, connections).await;
    });
    tx
}

fn start_acceptor_task(
    listener: TcpListener,
    connections: Connections,
    channel_size: usize,
) -> super::Receiver {
    let (tx, rx) = mpsc::channel(channel_size);
    tokio::spawn(async move {
        accept_connections(listener, tx, connections).await;
    });
    rx
}

fn start_receiver_task(
    reader: OwnedReadHalf,
    addr: SocketAddr,
    msg_tx: mpsc::Sender<RecvResult>,
    connections: Connections,
) {
    tokio::spawn(async move {
        recv(reader, addr, msg_tx, connections).await;
    });
}

async fn accept_connections(
    listener: TcpListener,
    msg_tx: mpsc::Sender<RecvResult>,
    connections: Connections,
) {
    loop {
        let (stream, addr) = match listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                debug!("Error accepting connection: {}", e);
                continue;
            }
        };
        debug!("Accepted connection from {}", addr);

        let (reader, writer) = stream.into_split();

        // Store the writer half of the connection
        connections.lock().await.insert(addr, writer);

        // Create a new task to receive messages from this connection
        start_receiver_task(reader, addr, msg_tx.clone(), connections.clone());
    }
}

async fn recv(
    mut reader: OwnedReadHalf,
    addr: SocketAddr,
    msg_tx: mpsc::Sender<RecvResult>,
    connections: Connections,
) {
    let mut buf = [0; 65535];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => {
                debug!("Connection closed by peer {}", addr);
                break;
            }
            res => {
                if msg_tx
                    .send(res.map(|n| (buf[..n].to_vec().into(), addr)))
                    .await
                    .log_error()
                    .is_none()
                {
                    break;
                }
            }
        }
    }
    connections.lock().await.remove(&addr);
}

async fn write(mut msg_rx: mpsc::Receiver<Data>, connections: Connections) {
    loop {
        let (msg, addr) = match msg_rx.recv().await {
            Some(msg) => msg,
            None => break,
        };

        let mut connections = connections.lock().await;
        let writer = match connections.get_mut(&addr) {
            Some(writer) => writer,
            None => {
                debug!("No connection to {}", addr);
                continue;
            }
        };
        writer.write_all(&msg).await.log_error();
    }
}
