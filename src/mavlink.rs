use crate::definitions::Offsets;
use anyhow::Result;
use log::debug;
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;

pub const MIN_PACKET_LEN: usize = 12;
pub const MAX_PACKET_LEN: usize = 280;
pub const PACKET_MAGIC: u8 = 0xFD;
pub const HEADER_LEN: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SysCompId(u8, u8);

impl From<(u8, u8)> for SysCompId {
    fn from((sys_id, comp_id): (u8, u8)) -> Self {
        Self(sys_id, comp_id)
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub sender: SysCompId,
    pub target: SysCompId,
    pub data: Arc<[u8]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Error)]
pub enum DeserializationError {
    #[error("The message is too short.")]
    TooShort,
    #[error("The message is too long.")]
    TooLong,
    #[error("The message has an invalid magic byte.")]
    InvalidMagic,
}

#[derive(Debug)]
pub struct Deserializer {
    offsets: HashMap<u32, Offsets>,
}

impl Deserializer {
    pub fn new(offsets: HashMap<u32, Offsets>) -> Self {
        Self { offsets }
    }

    pub fn deserialize(&self, msg: &[u8]) -> Result<Message, DeserializationError> {
        if msg.len() < MIN_PACKET_LEN {
            return Err(DeserializationError::TooShort);
        }
        if msg.len() > MAX_PACKET_LEN {
            return Err(DeserializationError::TooLong);
        }
        if msg[0] != PACKET_MAGIC {
            return Err(DeserializationError::InvalidMagic);
        }

        let payload_len = msg[1] as usize;
        let sender = (msg[5], msg[6]).into();
        let msg_id = u32::from_le_bytes([msg[7], msg[8], msg[9], 0]);

        debug!("sender: {:?}, msg_id: {}", sender, msg_id);

        // The payload is the message minus the header and checksum.
        let payload = &msg[HEADER_LEN..payload_len + HEADER_LEN];

        let target = self
            .offsets
            .get(&msg_id)
            .map(|offsets| {
                let target_sys_id = payload.get(offsets.system_id).unwrap_or(&0).to_owned();
                let target_comp_id = offsets
                    .component_id
                    .and_then(|i| payload.get(i))
                    .unwrap_or(&0)
                    .to_owned();
                (target_sys_id, target_comp_id)
            })
            .unwrap_or((0, 0))
            .into();

        Ok(Message {
            sender,
            target,
            data: msg.to_vec().into(),
        })
    }
}
