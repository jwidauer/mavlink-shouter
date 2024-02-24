use std::sync::Arc;

pub use self::deserializer::DeserializationError;
pub use self::deserializer::Deserializer;

pub mod definitions;
mod deserializer;

pub const MIN_PACKET_LEN: usize = 12;
pub const MAX_PACKET_LEN: usize = 280;
pub const PACKET_MAGIC: u8 = 0xFD;
pub const HEADER_LEN: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SysCompId(u8, u8);

impl SysCompId {
    pub fn is_valid_sender(&self) -> bool {
        self.0 != 0 && self.1 != 0
    }

    pub fn sys_id(&self) -> u8 {
        self.0
    }

    pub fn comp_id(&self) -> u8 {
        self.1
    }
}

impl From<(u8, u8)> for SysCompId {
    fn from((sys_id, comp_id): (u8, u8)) -> Self {
        Self(sys_id, comp_id)
    }
}

impl std::fmt::Display for SysCompId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sys_id: {}, comp_id: {}", self.sys_id(), self.comp_id())
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub sender: SysCompId,
    pub target: SysCompId,
    pub data: Arc<[u8]>,
}
