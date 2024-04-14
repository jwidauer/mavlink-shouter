use super::definitions::Offsets;
use super::{v1, v2, Message, RoutingInfo, SysCompId};
use anyhow::Result;
use log::debug;
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Error)]
pub enum DeserializationError {
    #[error("The packet is too short.")]
    TooShort,
    #[error("The packet has an invalid length: expected: {0} actual: {1}")]
    InvalidLength(usize, usize),
    #[error("The packet has an invalid magic byte of '0x{0:02x}'.")]
    InvalidMagic(u8),
}

#[derive(Debug)]
pub struct Deserializer {
    offsets: HashMap<u32, Offsets>,
}

impl Deserializer {
    pub fn new(offsets: HashMap<u32, Offsets>) -> Self {
        Self { offsets }
    }

    pub fn deserialize(&self, msg: Arc<[u8]>) -> Result<Message, DeserializationError> {
        match msg.first() {
            Some(&v1::PACKET_MAGIC) => self.deserialize_v1(msg),
            Some(&v2::PACKET_MAGIC) => self.deserialize_v2(msg),
            Some(magic) => Err(DeserializationError::InvalidMagic(*magic)),
            None => Err(DeserializationError::TooShort),
        }
    }

    fn deserialize_v1(&self, msg: Arc<[u8]>) -> Result<Message, DeserializationError> {
        if msg.len() < v1::MIN_PACKET_LEN {
            return Err(DeserializationError::TooShort);
        }

        let payload_len = msg[1] as usize;
        let expected_len = v1::HEADER_LEN + payload_len + v1::CHECKSUM_LEN;
        if msg.len() != expected_len {
            return Err(DeserializationError::InvalidLength(expected_len, msg.len()));
        }

        let sender = (msg[3], msg[4]).into();
        let msg_id = msg[5] as u32;

        debug!("sender: {}, msg_id: {}", sender, msg_id);

        // The payload is the message minus the header and checksum.
        let payload = &msg[v1::HEADER_LEN..payload_len + v1::HEADER_LEN];

        let target = self.target_from_payload(msg_id, payload);

        Ok(Message {
            routing_info: RoutingInfo { sender, target },
            data: msg,
        })
    }

    fn deserialize_v2(&self, msg: Arc<[u8]>) -> Result<Message, DeserializationError> {
        if msg.len() < v2::MIN_PACKET_LEN {
            return Err(DeserializationError::TooShort);
        }

        let payload_len = msg[1] as usize;
        let mut expected_len = v2::HEADER_LEN + payload_len + v2::CHECKSUM_LEN;
        let inc_flags = msg[2];
        if inc_flags & v2::IFLAG_SIGNED != 0 {
            expected_len += v2::SIGNATURE_LEN;
        }
        if msg.len() != expected_len {
            return Err(DeserializationError::InvalidLength(expected_len, msg.len()));
        }

        let sender = (msg[5], msg[6]).into();
        let msg_id = u32::from_le_bytes([msg[7], msg[8], msg[9], 0]);

        debug!("sender: {}, msg_id: {}", sender, msg_id);

        // The payload is the message minus the header and checksum.
        let payload = &msg[v2::HEADER_LEN..payload_len + v2::HEADER_LEN];

        let target = self.target_from_payload(msg_id, payload);

        Ok(Message {
            routing_info: RoutingInfo { sender, target },
            data: msg,
        })
    }

    fn target_from_payload(&self, msg_id: u32, payload: &[u8]) -> SysCompId {
        self.offsets
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
            .into()
    }
}
