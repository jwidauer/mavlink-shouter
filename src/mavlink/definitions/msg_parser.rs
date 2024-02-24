use quick_xml::{
    events::{BytesStart, Event},
    reader::Reader,
};
use std::num::NonZeroUsize;
use thiserror::Error;

use super::Offsets;

#[derive(Debug, Error)]
pub enum MsgParseError {
    #[error("QuickXML error: {0}")]
    QuickXml(#[from] quick_xml::Error),
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
    fn from_str(p: &str) -> Result<(Self, NonZeroUsize), MsgParseError> {
        let (s, n) = match p.find('[') {
            Some(i) => {
                let (s, n) = p.split_at(i);
                let end = n.find(']').ok_or(MsgParseError::MalformedArraySize)?;
                let n = n[1..end]
                    .parse::<usize>()
                    .map_err(|_| MsgParseError::FailedToParseArraySize(p.to_string()))?;
                (s, NonZeroUsize::new(n).ok_or(MsgParseError::ZeroArraySize)?)
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
            _ => return Err(MsgParseError::UnknownType(p.to_string())),
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
    fn from_bytes(bytes: &BytesStart) -> Result<Self, MsgParseError> {
        let name = match bytes.try_get_attribute("name")? {
            Some(name) => name.unescape_value()?,
            None => return Err(MsgParseError::FieldWithoutName),
        };
        let field_type = match bytes.try_get_attribute("type")? {
            Some(type_) => type_.unescape_value()?,
            None => return Err(MsgParseError::FieldWithoutType),
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
    // The fields of the message.
    msg_fields: Vec<MessageField>,
    // The index where the extensions fields start in the msg_fields vector.
    extensions_start_idx: Option<usize>,
    // Whether the message has target_system and target_component fields.
    is_targeted_msg: bool,
}

impl MsgParser {
    pub fn new() -> Self {
        Self {
            msg_fields: Vec::new(),
            extensions_start_idx: None,
            is_targeted_msg: false,
        }
    }

    fn record_extension_start(&mut self) -> Result<(), MsgParseError> {
        // Deal with the 'extensions' field. All fields after this one are extensions, which
        // means they are not reordered, that's why we need to record the index of this field.
        match self.extensions_start_idx {
            Some(_) => return Err(MsgParseError::MultipleExtensionsFields),
            None => self.extensions_start_idx = Some(self.msg_fields.len()),
        }
        Ok(())
    }

    fn parse_msg_field(&mut self, msg_field: MessageField) -> Result<(), MsgParseError> {
        match msg_field.name.as_str() {
            "target_system" | "target_component" => {
                if msg_field.kind != MessageFieldKind::U8 {
                    return Err(MsgParseError::TargetFieldNotU8);
                }
                if msg_field.multiplicity.get() != 1 {
                    return Err(MsgParseError::TargetFieldNotSingleValue);
                }
                self.is_targeted_msg = true;
            }
            _ => {}
        }
        self.msg_fields.push(msg_field);
        Ok(())
    }

    fn compute_offsets(&mut self) -> Result<Option<Offsets>, MsgParseError> {
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
        // It's fine if a message is targeted but doesn't have a target_component field,
        // but it's not fine if it doesn't have a target_system field.
        match (system_offset, component_offset) {
            (Some(system_offset), Some(component_offset)) => {
                Ok(Some(Offsets::new(system_offset, Some(component_offset))))
            }
            (Some(system_offset), None) => Ok(Some(Offsets::new(system_offset, None))),
            (None, Some(_)) => Err(MsgParseError::MissingTargetSystem),
            (None, None) => Ok(None),
        }
    }
}

pub fn try_get_offsets_from_msg(
    reader: &mut Reader<&[u8]>,
) -> Result<Option<Offsets>, MsgParseError> {
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
            Event::Eof => return Err(MsgParseError::UnexpectedEof),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reader_from_str(xml: &str) -> Reader<&[u8]> {
        let mut reader = Reader::from_str(xml);
        reader.trim_text(true);
        reader
    }

    #[test]
    fn test_try_get_offsets_from_msg() -> Result<(), MsgParseError> {
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
    fn test_try_get_offsets_from_msg_no_target() -> Result<(), MsgParseError> {
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
    fn test_try_get_offsets_from_msg_no_component() -> Result<(), MsgParseError> {
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
    fn test_try_get_offsets_from_msg_with_extenstions() -> Result<(), MsgParseError> {
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
    fn test_try_get_offsets_from_msg_not_first_element() -> Result<(), MsgParseError> {
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
    fn test_try_get_offsets_from_msg_with_bigger_fields() -> Result<(), MsgParseError> {
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
    fn test_try_get_offsets_from_msg_with_array() -> Result<(), MsgParseError> {
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
    fn test_try_get_offsets_from_msg_with_array_and_extensions() -> Result<(), MsgParseError> {
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
        assert!(matches!(offsets, Err(MsgParseError::FieldWithoutName)));
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
        assert!(matches!(offsets, Err(MsgParseError::FieldWithoutType)));
    }

    #[test]
    fn test_message_field_kind_from_str_with_malformed_array_size() {
        let kind_str = "uint8_t[";
        let kind = MessageFieldKind::from_str(kind_str);

        assert!(matches!(kind, Err(MsgParseError::MalformedArraySize)));
    }

    #[test]
    fn test_message_field_kind_from_str_with_zero_array_size() {
        let kind_str = "uint8_t[0]";
        let kind = MessageFieldKind::from_str(kind_str);

        assert!(matches!(kind, Err(MsgParseError::ZeroArraySize)));
    }

    #[test]
    fn test_message_field_kind_from_str_with_unknown_type() {
        let kind_str = "uint8";
        let kind = MessageFieldKind::from_str(kind_str);

        assert!(matches!(
            kind,
            Err(MsgParseError::UnknownType(type_str)) if type_str == "uint8"
        ));
    }

    #[test]
    fn test_message_field_kind_from_str_with_unparsable_array_size() {
        let kind_str = "uint8_t[abc]";
        let kind = MessageFieldKind::from_str(kind_str);

        assert!(matches!(
        kind,
        Err(MsgParseError::FailedToParseArraySize(type_str)) if type_str == "uint8_t[abc]"
        ));
    }

    #[test]
    fn test_message_field_kind_from_str() -> Result<(), MsgParseError> {
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
        assert!(matches!(offsets, Err(MsgParseError::TargetFieldNotU8)));
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
        assert!(matches!(offsets, Err(MsgParseError::MissingTargetSystem)));
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
        assert!(matches!(
            offsets,
            Err(MsgParseError::MultipleExtensionsFields)
        ));
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
        assert!(matches!(
            offsets,
            Err(MsgParseError::TargetFieldNotSingleValue)
        ));
    }

    #[test]
    fn test_try_get_offsets_from_msg_unexpected_eof() {
        let mut reader = reader_from_str(
            r#"<message id="1">
                <field type="uint8_t" name="target_system">Target system ID</field>
                <field type="uint8_t" name="target_component">Target component ID</field>"#,
        );

        let offsets = try_get_offsets_from_msg(&mut reader);
        assert!(matches!(offsets, Err(MsgParseError::UnexpectedEof)));
    }
}
