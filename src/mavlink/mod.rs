use std::sync::Arc;

pub use self::deserializer::DeserializationError;
pub use self::deserializer::Deserializer;

pub mod definitions;
mod deserializer;

pub mod v1 {
    pub const PACKET_MAGIC: u8 = 0xFE;
    pub const MIN_PACKET_LEN: usize = 8;
    pub const MAX_PACKET_LEN: usize = 263;
    pub const HEADER_LEN: usize = 6;
    pub const CHECKSUM_LEN: usize = 2;
}
pub mod v2 {
    pub const PACKET_MAGIC: u8 = 0xFD;
    pub const IFLAG_SIGNED: u8 = 0x01; // Message is signed
    pub const MIN_PACKET_LEN: usize = 12;
    pub const MAX_PACKET_LEN: usize = 280;
    pub const HEADER_LEN: usize = 10;
    pub const CHECKSUM_LEN: usize = 2;
    pub const SIGNATURE_LEN: usize = 13;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SysId(u8);

impl PartialEq<u8> for SysId {
    fn eq(&self, other: &u8) -> bool {
        self.0 == *other
    }
}

impl From<u8> for SysId {
    fn from(sys_id: u8) -> Self {
        Self(sys_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompId(u8);

impl PartialEq<u8> for CompId {
    fn eq(&self, other: &u8) -> bool {
        self.0 == *other
    }
}

impl From<u8> for CompId {
    fn from(comp_id: u8) -> Self {
        Self(comp_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SysCompId {
    sys_id: SysId,
    comp_id: CompId,
}

impl SysCompId {
    #[inline]
    pub fn is_valid_sender(&self) -> bool {
        self.sys_id != 0 && self.comp_id != 0
    }

    #[inline]
    pub fn sys_id(&self) -> u8 {
        self.sys_id.0
    }

    #[inline]
    pub fn comp_id(&self) -> u8 {
        self.comp_id.0
    }

    #[inline]
    pub fn is_broadcast(&self) -> bool {
        self.sys_id == 0
    }

    #[inline]
    pub fn is_sys_broadcast(&self) -> bool {
        self.sys_id != 0 && self.comp_id == 0
    }

    #[inline]
    pub fn matches(&self, other: Self) -> bool {
        let is_broadcast = self.is_broadcast() || other.is_broadcast();
        let is_sys_broadcast = self.is_sys_broadcast() || other.is_sys_broadcast();
        is_broadcast || (is_sys_broadcast && self.sys_id == other.sys_id) || *self == other
    }
}

impl From<(u8, u8)> for SysCompId {
    fn from((sys_id, comp_id): (u8, u8)) -> Self {
        Self {
            sys_id: sys_id.into(),
            comp_id: comp_id.into(),
        }
    }
}

impl std::fmt::Display for SysCompId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sys_id: {}, comp_id: {}", self.sys_id(), self.comp_id())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RoutingInfo {
    pub sender: SysCompId,
    pub target: SysCompId,
}

impl RoutingInfo {
    pub fn matches(&self, target: SysCompId) -> bool {
        self.target.matches(target) && self.sender != target
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub routing_info: RoutingInfo,
    pub data: Arc<[u8]>,
}
