use anyhow::{anyhow, bail, Context, Result};
use log::{debug, info};
use quick_xml::{
    events::{BytesStart, Event},
    reader::Reader,
};
use std::{collections::HashSet, num::NonZeroUsize, path::PathBuf};

#[derive(Debug)]
pub struct Offsets {
    pub system_id: usize,
    pub component_id: Option<usize>,
}

#[derive(Debug)]
pub struct TargetedMessage {
    pub id: u16,
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
            bail!("The path '{}' does not exist.", xml.display())
        }
        if !xml.is_file() {
            bail!("The path '{}' is not a file.", xml.display())
        }

        let parent = match xml.parent() {
            Some(p) => p,
            None => xml.as_path(),
        };
        info!("Parsing MAVLink definition '{}'.", xml.display());
        let contents = std::fs::read_to_string(&xml)?;

        let mut reader = Reader::from_str(contents.as_str());
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
                            Some(id) => id.unescape_value()?.parse::<u16>()?,
                            None => {
                                bail!(
                                    "A message definition in '{}' does not have an ID.",
                                    xml.display()
                                )
                            }
                        };
                        let offsets = get_offsets_from_msg(&mut reader).context(format!(
                            "Failed to get the offsets for a message definition in '{}'.",
                            xml.display()
                        ))?;
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

pub fn parse_xml(xml: PathBuf) -> Result<Vec<TargetedMessage>> {
    let mut parser = Parser::new();
    parser.parse_xml(xml)?;
    Ok(parser.targeted_messages)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MessageFieldType {
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

impl MessageFieldType {
    fn from_str(s: &str) -> Result<(Self, NonZeroUsize)> {
        let (s, n) = match s.find('[') {
            Some(i) => {
                let (s, n) = s.split_at(i);
                let end = n
                    .find(']')
                    .ok_or_else(|| anyhow!("A field has a malformed array size."))?;
                let n = n[1..end].parse::<usize>()?;
                (
                    s,
                    NonZeroUsize::new(n)
                        .ok_or_else(|| anyhow!("A field has a zero array size."))?,
                )
            }
            None => (s, NonZeroUsize::new(1).unwrap()),
        };
        let type_ = match s {
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
            _ => bail!("Unknown type '{}' for message field.", s),
        };
        Ok((type_, n))
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

struct MessageField {
    name: String,
    field_type: MessageFieldType,
    multiplicity: NonZeroUsize,
}

impl MessageField {
    fn from_bytes(bytes: &BytesStart) -> Result<Self> {
        let name = match bytes.try_get_attribute("name")? {
            Some(name) => name.unescape_value()?,
            None => bail!("A field definition does not have a name."),
        };
        let type_ = match bytes.try_get_attribute("type")? {
            Some(type_) => type_.unescape_value()?,
            None => bail!("A field definition does not have a type."),
        };
        let (field_type, multiplicity) = MessageFieldType::from_str(&type_)?;
        Ok(Self {
            name: name.to_string(),
            field_type,
            multiplicity,
        })
    }
}

fn get_offsets_from_msg(reader: &mut Reader<&[u8]>) -> Result<Option<Offsets>> {
    let mut msg_fields = Vec::new();
    let mut extensions_start_idx = None;
    let mut is_targeted_msg = false;

    let mut system_offset = None;
    let mut component_offset = None;
    loop {
        match reader.read_event()? {
            Event::Start(ref f) => {
                if f.name().0 == b"extensions" {
                    // Gotta record the index of the extensions field so we can sort properly.
                    extensions_start_idx = match extensions_start_idx {
                        Some(_) => bail!("A message definition has multiple extensions fields."),
                        None => Some(msg_fields.len()),
                    };
                }
                if f.name().0 != b"field" {
                    continue;
                }
                let field = MessageField::from_bytes(f)?;

                match field.name.as_str() {
                    "target_system" | "target_component" => {
                        is_targeted_msg = true;
                        if field.field_type != MessageFieldType::U8 {
                            bail!("A message definition has a target_system or target_component field that is not a u8.")
                        }
                        if field.multiplicity.get() != 1 {
                            bail!("A message definition has a target_system or target_component field that is not a single value.")
                        }
                    }
                    _ => {}
                }

                msg_fields.push(field);
            }
            Event::End(ref f) => {
                if f.name().0 != b"message" {
                    continue;
                }
                if !is_targeted_msg {
                    break;
                }

                // Sort the fields in decending order so that the extensions fields stay at the end and in the same
                // order as in the XML.
                let num_fields = msg_fields.len();
                let fields_to_sort = &mut msg_fields[..extensions_start_idx.unwrap_or(num_fields)];
                fields_to_sort.sort_by(|a, b| b.field_type.size().cmp(&a.field_type.size()));

                msg_fields.iter().fold(0, |offset, field| {
                    match field.name.as_str() {
                        "target_system" => system_offset = Some(offset),
                        "target_component" => component_offset = Some(offset),
                        _ => {}
                    }
                    offset + field.field_type.size() * field.multiplicity.get()
                });

                break;
            }
            Event::Eof => {
                bail!("A message definition does not have a closing tag.")
            }
            _ => {}
        }
    }
    match (system_offset, component_offset) {
        (Some(system_offset), Some(component_offset)) => Ok(Some(Offsets {
            system_id: system_offset,
            component_id: Some(component_offset),
        })),
        (Some(system_offset), None) => Ok(Some(Offsets {
            system_id: system_offset,
            component_id: None,
        })),
        (None, Some(_)) => bail!(
            "A message definition has a target_component field but not a target_system field."
        ),
        (None, None) => Ok(None),
    }
}
