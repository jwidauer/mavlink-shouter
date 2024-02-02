use anyhow::Result;
use clap::Parser;
use log::{debug, info, warn};
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};
use std::net::UdpSocket;
use std::{collections::HashMap, path};

mod definitions;

mod mavlink {
    pub const MAX_PACKET_LEN: usize = 280;
    pub const MIN_PACKET_LEN: usize = 12;
    pub const PACKET_MAGIC: u8 = 0xFD;
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The path to the MAVLink XML for the dialect to use.
    #[arg(short, long)]
    definition: path::PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();

    TermLogger::init(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

    let targeted_messages = definitions::parse_xml(args.definition)?;
    info!("Found {} targeted messages.", targeted_messages.len());

    let mut offsets = HashMap::new();
    let has_unique_ids = targeted_messages
        .into_iter()
        .all(|m| offsets.insert(m.id, m.offsets).is_none());

    if !has_unique_ids {
        warn!("Found multiple targeted messages with the same ID.");
    }

    let socket = UdpSocket::bind("127.0.0.1:14550")?;
    let mut buf = [0; 65535];
    loop {
        let (amt, _) = socket.recv_from(&mut buf)?;
        let msg = &buf[..amt];
        if msg.len() < mavlink::MIN_PACKET_LEN {
            warn!("Received a message that is too short.");
            continue;
        }
        if msg.len() > mavlink::MAX_PACKET_LEN {
            warn!("Received a message that is too long.");
            continue;
        }
        if msg[0] != mavlink::PACKET_MAGIC {
            warn!("Received a message with an invalid magic byte.");
            continue;
        }
        let sender_sys_id = msg[5];
        let sender_comp_id = msg[6];
        let msg_id = u16::from_le_bytes([msg[7], msg[8]]);

        debug!(
            "sys_id: {}, comp_id: {}, msg_id: {}",
            sender_sys_id, sender_comp_id, msg_id
        );

        let target_sys_comp_id = offsets
            .get(&msg_id)
            .map(|offsets| {
                let target_sys_id = msg.get(offsets.system_id).unwrap_or(&0).to_owned();
                let target_comp_id = offsets
                    .component_id
                    .map(|i| msg.get(i).unwrap_or(&0))
                    .unwrap_or(&0)
                    .to_owned();
                (target_sys_id, target_comp_id)
            })
            .unwrap_or((0, 0));
    }
}
