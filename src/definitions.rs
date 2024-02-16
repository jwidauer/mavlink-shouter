use anyhow::{bail, Context, Result};
use log::{debug, info};
use quick_xml::{
    events::{BytesStart, Event},
    reader::Reader,
};
use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
    path::PathBuf,
};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("The file '{0}' does not exist.")]
    FileDoesNotExist(PathBuf),
    #[error("The path '{0}' is not a file.")]
    NotAFile(PathBuf),
    #[error("A message definition in '{0}' does not have an ID.")]
    MessageWithoutId(PathBuf),
    #[error("Failed to get the offsets for a message definition in '{0}'.")]
    FailedToGetOffsets(PathBuf),
    #[error("A message definition has multiple 'extensions' fields.")]
    MultipleExtensionsFields,
    #[error(
        "A message definition has a target_system or target_component field that is not a u8."
    )]
    TargetFieldNotU8,
    #[error("A message definition has a target_system or target_component field that is not a single value.")]
    TargetFieldNotSingleValue,
    #[error("A message definition does not have a closing tag.")]
    UnexpectedEof,
    #[error("A message definition has a target_component field but not a target_system field.")]
    MissingTargetSystem,
    #[error("A field has a malformed array size.")]
    MalformedArraySize,
    #[error("Failed to parse the array size of a field with type '{0}'.")]
    FailedToParseArraySize(String),
    #[error("A field has a zero array size.")]
    ZeroArraySize,
    #[error("Unknown type '{0}' for message field.")]
    UnknownType(String),
    #[error("A field definition does not have a name.")]
    FieldWithoutName,
    #[error("A field definition does not have a type.")]
    FieldWithoutType,
    #[error("Found multiple targeted messages with the same ID.")]
    MultipleMessagesWithSameId,
}

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

pub struct Parser {
    targeted_messages: Vec<TargetedMessage>,
    visited_xml_files: HashSet<PathBuf>,
}

impl Parser {
    fn new() -> Self {
        Self {
            targeted_messages: Vec::new(),
            visited_xml_files: HashSet::new(),
        }
    }

    fn parse_xml(&mut self, xml: PathBuf) -> Result<()> {
        if self.visited_xml_files.contains(&xml) {
            debug!(
                "Skipping file '{}' as it has already been parsed.",
                xml.display()
            );
            return Ok(());
        }
        if !xml.exists() {
            bail!(ParseError::FileDoesNotExist(xml));
        }
        if !xml.is_file() {
            bail!(ParseError::NotAFile(xml));
        }

        info!("Parsing MAVLink definition '{}'.", xml.display());
        let contents = std::fs::read_to_string(&xml)?;

        self.parse_content(&contents, xml)
    }

    fn parse_content(&mut self, content: &str, xml: PathBuf) -> Result<()> {
        let parent = match xml.parent() {
            Some(p) => p,
            None => xml.as_path(),
        };

        let mut reader = Reader::from_str(content);
        reader.trim_text(true);

        loop {
            match reader.read_event()? {
                Event::Start(ref e) => match e.name().0 {
                    b"include" => {
                        let include_file = reader.read_text(e.name())?;
                        let include_file = parent.join(include_file.as_ref());
                        self.parse_xml(include_file)?;
                    }
                    b"message" => {
                        let id = match e.try_get_attribute("id")? {
                            Some(id) => id.unescape_value()?.parse::<u32>()?,
                            None => bail!(ParseError::MessageWithoutId(xml)),
                        };

                        let offsets = try_get_offsets_from_msg(&mut reader)
                            .with_context(|| ParseError::FailedToGetOffsets(xml.clone()))?;
                        if let Some(offsets) = offsets {
                            self.targeted_messages.push(TargetedMessage { id, offsets });
                        }
                    }
                    _ => {}
                },
                Event::Eof => break,
                _ => {}
            }
        }
        self.visited_xml_files.insert(xml);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MessageFieldKind {
    Char,
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
}

impl MessageFieldKind {
    fn from_str(p: &str) -> Result<(Self, NonZeroUsize)> {
        let (s, n) = match p.find('[') {
            Some(i) => {
                let (s, n) = p.split_at(i);
                let end = n.find(']').ok_or(ParseError::MalformedArraySize)?;
                let n = n[1..end]
                    .parse::<usize>()
                    .with_context(|| ParseError::FailedToParseArraySize(p.to_string()))?;
                (s, NonZeroUsize::new(n).ok_or(ParseError::ZeroArraySize)?)
            }
            None => (p, NonZeroUsize::new(1).unwrap()),
        };
        let kind = match s {
            "char" => Self::Char,
            "uint8_t" => Self::U8,
            "uint16_t" => Self::U16,
            "uint32_t" => Self::U32,
            "uint64_t" => Self::U64,
            "int8_t" => Self::I8,
            "uint8_t_mavlink_version" => Self::I8, // Cool special case...
            "int16_t" => Self::I16,
            "int32_t" => Self::I32,
            "int64_t" => Self::I64,
            "float" => Self::F32,
            "double" => Self::F64,
            _ => bail!(ParseError::UnknownType(p.to_string())),
        };
        Ok((kind, n))
    }

    fn size(&self) -> usize {
        match self {
            Self::Char => 1,
            Self::U8 => 1,
            Self::U16 => 2,
            Self::U32 => 4,
            Self::U64 => 8,
            Self::I8 => 1,
            Self::I16 => 2,
            Self::I32 => 4,
            Self::I64 => 8,
            Self::F32 => 4,
            Self::F64 => 8,
        }
    }
}

#[derive(Debug, Clone)]
struct MessageField {
    name: String,
    kind: MessageFieldKind,
    multiplicity: NonZeroUsize,
}

impl MessageField {
    fn from_bytes(bytes: &BytesStart) -> Result<Self> {
        let name = match bytes.try_get_attribute("name")? {
            Some(name) => name.unescape_value()?,
            None => bail!(ParseError::FieldWithoutName),
        };
        let field_type = match bytes.try_get_attribute("type")? {
            Some(type_) => type_.unescape_value()?,
            None => bail!(ParseError::FieldWithoutType),
        };
        let (kind, multiplicity) = MessageFieldKind::from_str(&field_type)?;
        Ok(Self {
            name: name.to_string(),
            kind,
            multiplicity,
        })
    }

    fn size(&self) -> usize {
        self.kind.size() * self.multiplicity.get()
    }
}

struct MsgParser {
    msg_fields: Vec<MessageField>,
    extensions_start_idx: Option<usize>,
    is_targeted_msg: bool,
}

impl MsgParser {
    fn new() -> Self {
        Self {
            msg_fields: Vec::new(),
            extensions_start_idx: None,
            is_targeted_msg: false,
        }
    }

    fn record_extension_start(&mut self) -> Result<()> {
        // Deal with the 'extensions' field. All fields after this one are extensions, which
        // means they are not reordered, that's why we need to record the index of this field.
        match self.extensions_start_idx {
            Some(_) => bail!(ParseError::MultipleExtensionsFields),
            None => self.extensions_start_idx = Some(self.msg_fields.len()),
        }
        Ok(())
    }

    fn parse_msg_field(&mut self, msg_field: MessageField) -> Result<()> {
        match msg_field.name.as_str() {
            "target_system" | "target_component" => {
                if msg_field.kind != MessageFieldKind::U8 {
                    bail!(ParseError::TargetFieldNotU8)
                }
                if msg_field.multiplicity.get() != 1 {
                    bail!(ParseError::TargetFieldNotSingleValue)
                }
                self.is_targeted_msg = true;
            }
            _ => {}
        }
        self.msg_fields.push(msg_field);
        Ok(())
    }

    fn compute_offsets(&mut self) -> Result<Option<Offsets>> {
        if !self.is_targeted_msg {
            return Ok(None);
        }

        let mut system_offset = None;
        let mut component_offset = None;

        // Sort the fields in decending order so that the extensions fields stay at the end and in the same
        // order as in the XML.
        let num_fields = self.msg_fields.len();
        let fields_to_sort =
            &mut self.msg_fields[..self.extensions_start_idx.unwrap_or(num_fields)];
        fields_to_sort.sort_by(|a, b| b.kind.size().cmp(&a.kind.size()));

        self.msg_fields.iter().fold(0, |offset, field| {
            match field.name.as_str() {
                "target_system" => system_offset = Some(offset),
                "target_component" => component_offset = Some(offset),
                _ => {}
            }
            offset + field.size()
        });

        // If the message is not targeted, we don't need to return any offsets.
        // It's fine if a message is targeted but doesn't have a target_component field, but it's not fine if it doesn't have a target_system field.
        match (system_offset, component_offset) {
            (Some(system_offset), Some(component_offset)) => {
                Ok(Some(Offsets::new(system_offset, Some(component_offset))))
            }
            (Some(system_offset), None) => Ok(Some(Offsets::new(system_offset, None))),
            (None, Some(_)) => Err(ParseError::MissingTargetSystem.into()),
            (None, None) => Ok(None),
        }
    }
}

fn try_get_offsets_from_msg(reader: &mut Reader<&[u8]>) -> Result<Option<Offsets>> {
    let mut parser = MsgParser::new();

    loop {
        match reader.read_event()? {
            Event::Start(ref f) if f.name().0 == b"field" => {
                let field = MessageField::from_bytes(f)?;
                parser.parse_msg_field(field)?;
            }
            Event::Empty(ref f) if f.name().0 == b"extensions" => {
                parser.record_extension_start()?;
            }
            Event::End(ref f) if f.name().0 == b"message" => {
                return parser.compute_offsets();
            }
            Event::Eof => bail!(ParseError::UnexpectedEof),
            _ => {}
        }
    }
}

pub type ID = u32;

pub fn try_get_offsets_from_xml(xml: PathBuf) -> Result<HashMap<ID, Offsets>> {
    let mut parser = Parser::new();
    parser.parse_xml(xml)?;

    let mut offsets = HashMap::new();
    let has_unique_ids = parser
        .targeted_messages
        .into_iter()
        .all(|m| offsets.insert(m.id, m.offsets).is_none());
    if !has_unique_ids {
        bail!(ParseError::MultipleMessagesWithSameId);
    }
    Ok(offsets)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reader_from_str(xml: &str) -> Reader<&[u8]> {
        let mut reader = Reader::from_str(xml);
        reader.trim_text(true);
        reader
    }

    fn result_as_parse_err<T>(e: Result<T>) -> Option<ParseError> {
        e.err().and_then(|e| e.downcast::<ParseError>().ok())
    }

    #[test]
    fn test_try_get_offsets_from_msg() -> Result<()> {
        let mut reader = reader_from_str(
            r#"<message id="1">
                <field type="uint8_t" name="target_system">Target system ID</field>
                <field type="uint8_t" name="target_component">Target component ID</field>
                <field type="uint8_t" name="something_else">Something else</field>
            </message>"#,
        );

        let offsets = try_get_offsets_from_msg(&mut reader)?;
        assert_eq!(offsets, Some(Offsets::new(0, Some(1))));
        Ok(())
    }

    #[test]
    fn test_try_get_offsets_from_msg_no_target() -> Result<()> {
        let mut reader = reader_from_str(
            r#"<message id="1">
                <field type="uint8_t" name="something_else">Something else</field>
            </message>"#,
        );

        let offsets = try_get_offsets_from_msg(&mut reader)?;
        assert_eq!(offsets, None);
        Ok(())
    }

    #[test]
    fn test_try_get_offsets_from_msg_no_component() -> Result<()> {
        let mut reader = reader_from_str(
            r#"<message id="1">
                <field type="uint8_t" name="target_system">Target system ID</field>
                <field type="uint8_t" name="something_else">Something else</field>
            </message>"#,
        );

        let offsets = try_get_offsets_from_msg(&mut reader)?;
        assert_eq!(offsets, Some(Offsets::new(0, None)));
        Ok(())
    }

    #[test]
    fn test_try_get_offsets_from_msg_with_extenstions() -> Result<()> {
        let mut reader = reader_from_str(
            r#"<message id="1">
                <field type="uint8_t" name="target_system">Target system ID</field>
                <field type="uint8_t" name="target_component">Target component ID</field>
                <field type="uint8_t" name="something_else">Something else</field>
                <extensions/>
                <field type="uint8_t" name="extension1">Extension 1</field>
                <field type="uint8_t" name="extension2">Extension 2</field>
            </message>"#,
        );

        let offsets = try_get_offsets_from_msg(&mut reader)?;
        assert_eq!(offsets, Some(Offsets::new(0, Some(1))));
        Ok(())
    }

    #[test]
    fn test_try_get_offsets_from_msg_not_first_element() -> Result<()> {
        let mut reader = reader_from_str(
            r#"<message id="1">
                <field type="uint8_t" name="something_else">Something else</field>
                <field type="uint8_t" name="target_system">Target system ID</field>
                <field type="uint8_t" name="target_component">Target component ID</field>
            </message>"#,
        );

        let offsets = try_get_offsets_from_msg(&mut reader)?;
        assert_eq!(offsets, Some(Offsets::new(1, Some(2))));
        Ok(())
    }

    #[test]
    fn test_try_get_offsets_from_msg_with_bigger_fields() -> Result<()> {
        let mut reader = reader_from_str(
            r#"<message id="1">
                <field type="uint8_t" name="target_system">Target system ID</field>
                <field type="uint8_t" name="target_component">Target component ID</field>
                <field type="uint16_t" name="something_else">Something else</field>
            </message>"#,
        );

        let offsets = try_get_offsets_from_msg(&mut reader)?;
        assert_eq!(offsets, Some(Offsets::new(2, Some(3))));
        Ok(())
    }

    #[test]
    fn test_try_get_offsets_from_msg_with_array() -> Result<()> {
        let mut reader = reader_from_str(
            r#"<message id="1">
                <field type="uint8_t[3]" name="something">Something</field>
                <field type="uint8_t[2]" name="something1">Something 1</field>
                <field type="uint16_t" name="something_else">Something else</field>
                <field type="uint8_t" name="target_system">Target system ID</field>
                <field type="uint8_t" name="target_component">Target component ID</field>
            </message>"#,
        );

        let offsets = try_get_offsets_from_msg(&mut reader)?;
        assert_eq!(offsets, Some(Offsets::new(7, Some(8))));
        Ok(())
    }

    #[test]
    fn test_try_get_offsets_from_msg_with_array_and_extensions() -> Result<()> {
        let mut reader = reader_from_str(
            r#"<message id="1">
                <field type="uint8_t[3]" name="something">Something</field>
                <field type="uint8_t[2]" name="something1">Something 1</field>
                <field type="uint16_t" name="something_else">Something else</field>
                <field type="uint8_t" name="target_system">Target system ID</field>
                <field type="uint8_t" name="target_component">Target component ID</field>
                <extensions/>
                <field type="uint16_t" name="extension1">Extension 1</field>
                <field type="uint16_t" name="extension2">Extension 2</field>
            </message>"#,
        );

        let offsets = try_get_offsets_from_msg(&mut reader)?;
        assert_eq!(offsets, Some(Offsets::new(7, Some(8))));
        Ok(())
    }

    #[test]
    fn test_try_get_offsets_from_msg_with_field_without_name() {
        let mut reader = reader_from_str(
            r#"<message id="1">
                <field type="uint8_t" name="target_system">Target system ID</field>
                <field type="uint8_t">Target component ID</field>
            </message>"#,
        );

        let offsets = try_get_offsets_from_msg(&mut reader);
        assert_eq!(
            result_as_parse_err(offsets),
            Some(ParseError::FieldWithoutName)
        );
    }

    #[test]
    fn test_try_get_offsets_from_msg_with_field_without_type() {
        let mut reader = reader_from_str(
            r#"<message id="1">
                <field type="uint8_t" name="target_system">Target system ID</field>
                <field name="target_component">Target component ID</field>
            </message>"#,
        );

        let offsets = try_get_offsets_from_msg(&mut reader);
        assert_eq!(
            result_as_parse_err(offsets),
            Some(ParseError::FieldWithoutType)
        );
    }

    #[test]
    fn test_message_field_kind_from_str_with_malformed_array_size() {
        let kind_str = "uint8_t[";
        let kind = MessageFieldKind::from_str(kind_str);

        assert_eq!(
            result_as_parse_err(kind),
            Some(ParseError::MalformedArraySize)
        );
    }

    #[test]
    fn test_message_field_kind_from_str_with_zero_array_size() {
        let kind_str = "uint8_t[0]";
        let kind = MessageFieldKind::from_str(kind_str);

        assert_eq!(result_as_parse_err(kind), Some(ParseError::ZeroArraySize));
    }

    #[test]
    fn test_message_field_kind_from_str_with_unknown_type() {
        let kind_str = "uint8";
        let kind = MessageFieldKind::from_str(kind_str);

        assert_eq!(
            result_as_parse_err(kind),
            Some(ParseError::UnknownType("uint8".to_string()))
        );
    }

    #[test]
    fn test_message_field_kind_from_str_with_unparsable_array_size() {
        let kind_str = "uint8_t[abc]";
        let kind = MessageFieldKind::from_str(kind_str);

        assert_eq!(
            result_as_parse_err(kind),
            Some(ParseError::FailedToParseArraySize(
                "uint8_t[abc]".to_string()
            ))
        );
    }

    #[test]
    fn test_message_field_kind_from_str() -> Result<()> {
        let kind_str = "uint8_t[3]";
        let (kind, multiplicity) = MessageFieldKind::from_str(kind_str)?;

        assert_eq!(kind, MessageFieldKind::U8);
        assert_eq!(multiplicity.get(), 3);
        Ok(())
    }

    #[test]
    fn test_message_field_size() {
        let field = MessageField {
            name: "something".to_string(),
            kind: MessageFieldKind::U16,
            multiplicity: NonZeroUsize::new(3).unwrap(),
        };

        assert_eq!(field.size(), 6);
    }

    #[test]
    fn test_try_get_offsets_from_msg_target_field_not_u8() {
        let mut reader = reader_from_str(
            r#"<message id="1">
                <field type="uint8_t" name="target_system">Target system ID</field>
                <field type="uint16_t" name="target_component">Target component ID</field>
            </message>"#,
        );

        let offsets = try_get_offsets_from_msg(&mut reader);
        assert_eq!(
            result_as_parse_err(offsets),
            Some(ParseError::TargetFieldNotU8)
        );
    }

    #[test]
    fn test_try_get_offsets_from_msg_no_target_system() {
        let mut reader = reader_from_str(
            r#"<message id="1">
                <field type="uint8_t" name="target_component">Target component ID</field>
                <field type="uint8_t" name="something_else">Something else</field>
            </message>"#,
        );

        let offsets = try_get_offsets_from_msg(&mut reader);
        assert_eq!(
            result_as_parse_err(offsets),
            Some(ParseError::MissingTargetSystem)
        );
    }

    #[test]
    fn test_try_get_offsets_from_msg_multiple_extensions() {
        let mut reader = reader_from_str(
            r#"<message id="1">
                <field type="uint8_t" name="target_system">Target system ID</field>
                <field type="uint8_t" name="target_component">Target component ID</field>
                <field type="uint8_t" name="something_else">Something else</field>
                <extensions/>
                <field type="uint8_t" name="extension1">Extension 1</field>
                <extensions/>
                <field type="uint8_t" name="extension2">Extension 2</field>
            </message>"#,
        );

        let offsets = try_get_offsets_from_msg(&mut reader);
        assert_eq!(
            result_as_parse_err(offsets),
            Some(ParseError::MultipleExtensionsFields)
        );
    }

    #[test]
    fn test_try_get_offsets_from_msg_target_field_not_single_value() {
        let mut reader = reader_from_str(
            r#"<message id="1">
                <field type="uint8_t[2]" name="target_system">Target system ID</field>
                <field type="uint8_t" name="target_component">Target component ID</field>
            </message>"#,
        );

        let offsets = try_get_offsets_from_msg(&mut reader);
        assert_eq!(
            result_as_parse_err(offsets),
            Some(ParseError::TargetFieldNotSingleValue)
        );
    }

    #[test]
    fn test_try_get_offsets_from_msg_unexpected_eof() {
        let mut reader = reader_from_str(
            r#"<message id="1">
                <field type="uint8_t" name="target_system">Target system ID</field>
                <field type="uint8_t" name="target_component">Target component ID</field>"#,
        );

        let offsets = try_get_offsets_from_msg(&mut reader);
        assert_eq!(
            result_as_parse_err(offsets),
            Some(ParseError::UnexpectedEof)
        );
    }
}
