use std::{collections::HashMap, sync::Arc};

use log::{debug, warn};
use tokio_util::{
    bytes::{Buf, BytesMut},
    codec::{Decoder, Encoder},
};

use super::{definitions::Offsets, v1, v2, Message, RoutingInfo, SysCompId};

#[derive(Debug, Clone)]
pub struct Codec {
    offsets: Arc<HashMap<u32, Offsets>>,
}

impl Codec {
    pub fn new(offsets: Arc<HashMap<u32, Offsets>>) -> Self {
        Self { offsets }
    }

    fn base_decode(&self, src: &mut BytesMut) -> Option<Message> {
        match src.first() {
            Some(&v1::PACKET_MAGIC) => self.decode_v1(src),
            Some(&v2::PACKET_MAGIC) => self.decode_v2(src),
            Some(magic) => {
                warn!("Received invalid magic byte {}. Trying to resync.", magic);
                self.resync(src)
            }
            None => None,
        }
    }

    fn decode_v1(&self, src: &mut BytesMut) -> Option<Message> {
        if src.len() < v1::MIN_PACKET_LEN {
            src.reserve(v1::MIN_PACKET_LEN - src.len());
            return None;
        }

        let payload_len = src[1] as usize;
        let expected_len = v1::HEADER_LEN + payload_len + v1::CHECKSUM_LEN;
        if src.len() < expected_len {
            src.reserve(expected_len - src.len());
            return None;
        }

        let data = src.split_to(expected_len).freeze();

        let sender = (data[3], data[4]).into();
        let msg_id = data[5] as u32;

        // The payload is the message minus the header and checksum.
        let payload = &data[v1::HEADER_LEN..payload_len + v1::HEADER_LEN];

        let target = self.target_from_payload(msg_id, payload);

        debug!("msg_id: {}, sender: {}, target: {}", msg_id, sender, target);

        Some(Message {
            routing_info: RoutingInfo { sender, target },
            data,
        })
    }

    fn decode_v2(&self, src: &mut BytesMut) -> Option<Message> {
        if src.len() < v2::MIN_PACKET_LEN {
            src.reserve(v2::MIN_PACKET_LEN - src.len());
            return None;
        }

        let payload_len = src[1] as usize;
        let mut expected_len = v2::HEADER_LEN + payload_len + v2::CHECKSUM_LEN;
        let inc_flags = src[2];
        if inc_flags & v2::IFLAG_SIGNED != 0 {
            expected_len += v2::SIGNATURE_LEN;
        }
        if src.len() < expected_len {
            src.reserve(expected_len - src.len());
            return None;
        }

        let data = src.split_to(expected_len).freeze();

        let sender = (data[5], data[6]).into();
        let msg_id = u32::from_le_bytes([data[7], data[8], data[9], 0]);

        // The payload is the message minus the header and checksum.
        let payload = &data[v2::HEADER_LEN..payload_len + v2::HEADER_LEN];

        let target = self.target_from_payload(msg_id, payload);

        debug!("msg_id: {}, sender: {}, target: {}", msg_id, sender, target);

        Some(Message {
            routing_info: RoutingInfo { sender, target },
            data,
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

    fn resync(&self, src: &mut BytesMut) -> Option<Message> {
        // We lost synchronization with the received mavlink msgs
        // We're goint to try to resync, by looking for the first valid mavlink magic byte
        src.iter()
            .position(|&b| b == v1::PACKET_MAGIC || b == v2::PACKET_MAGIC)
            .and_then(|pos| {
                // Advance the buffer to the first valid magic byte
                src.advance(pos);
                self.base_decode(src)
            })
            .or_else(|| {
                // If we can't find a valid magic byte, we're going to clear the buffer
                src.clear();
                None
            })
    }
}

impl Decoder for Codec {
    type Item = Message;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        Ok(self.base_decode(src))
    }
}

impl Encoder<Message> for Codec {
    type Error = std::io::Error;

    fn encode(&mut self, item: Message, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.extend_from_slice(&item.data);
        Ok(())
    }
}
