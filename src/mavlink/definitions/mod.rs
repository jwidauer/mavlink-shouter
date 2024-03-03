use std::collections::HashMap;
use std::path::PathBuf;

use parser::{ParseError, Parser};

mod msg_parser;
mod parser;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offsets {
    pub system_id: usize,
    pub component_id: Option<usize>,
}

impl Offsets {
    pub fn new(system_id: usize, component_id: Option<usize>) -> Self {
        Self {
            system_id,
            component_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetedMessage {
    pub id: u32,
    pub offsets: Offsets,
}

pub type ID = u32;

pub fn try_get_offsets_from_xml(xml: PathBuf) -> Result<HashMap<ID, Offsets>, ParseError> {
    let mut parser = Parser::new();
    parser.parse_xml(xml)?;

    let mut offsets = HashMap::new();
    let has_unique_ids = parser
        .targeted_messages
        .into_iter()
        .all(|m| offsets.insert(m.id, m.offsets).is_none());
    if !has_unique_ids {
        return Err(ParseError::MultipleMessagesWithSameId);
    }
    Ok(offsets)
}
