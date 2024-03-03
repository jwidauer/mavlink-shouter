use log::{debug, info};
use quick_xml::{events::Event, reader::Reader};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use thiserror::Error;

use super::msg_parser::try_get_offsets_from_msg;
use super::TargetedMessage;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("QuickXML error: {0}")]
    QuickXml(#[from] quick_xml::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("The file '{0}' does not exist.")]
    FileDoesNotExist(PathBuf),
    #[error("The path '{0}' is not a file.")]
    NotAFile(PathBuf),
    #[error("A message definition does not have an ID.")]
    MessageWithoutId,
    #[error("A message definition has an invalid ID.")]
    InvalidMessageId(#[from] std::num::ParseIntError),
    #[error("Found multiple targeted messages with the same ID.")]
    MultipleMessagesWithSameId,
    #[error("A message definition could not be parsed: {0}")]
    MessageParser(#[from] super::msg_parser::MsgParseError),
}

pub struct Parser {
    pub targeted_messages: Vec<TargetedMessage>,
    visited_xml_files: HashSet<PathBuf>,
}

impl Parser {
    pub fn new() -> Self {
        Self {
            targeted_messages: Vec::new(),
            visited_xml_files: HashSet::new(),
        }
    }

    pub fn parse_xml(&mut self, xml: PathBuf) -> Result<(), ParseError> {
        if self.visited_xml_files.contains(&xml) {
            debug!(
                "Skipping file '{}' as it has already been parsed.",
                xml.display()
            );
            return Ok(());
        }
        if !xml.exists() {
            return Err(ParseError::FileDoesNotExist(xml));
        }
        if !xml.is_file() {
            return Err(ParseError::NotAFile(xml));
        }

        info!("Parsing MAVLink definition '{}'.", xml.display());
        let contents = std::fs::read_to_string(&xml)?;

        let parent = match xml.parent() {
            Some(p) => p,
            None => xml.as_path(),
        };

        self.parse_content(&contents, parent)?;
        self.visited_xml_files.insert(xml);
        Ok(())
    }

    fn parse_content(&mut self, content: &str, parent: &Path) -> Result<(), ParseError> {
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
                        let id = e
                            .try_get_attribute("id")?
                            .ok_or(ParseError::MessageWithoutId)
                            .and_then(|id| {
                                id.unescape_value()?
                                    .parse::<u32>()
                                    .map_err(ParseError::InvalidMessageId)
                            })?;

                        let offsets = try_get_offsets_from_msg(&mut reader)?;
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::Offsets;
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_parse_content() -> Result<(), ParseError> {
        let content = r#"
            <mavlink>
                <message id="1" name="msg1">
                    <field type="uint8_t" name="something_else">Something else</field>
                    <field type="uint8_t" name="target_system">Target system ID</field>
                    <field type="uint8_t" name="target_component">Target component ID</field>
                </message>
                <message id="2" name="msg2">
                    <field type="uint8_t" name="target_system">Target system ID</field>
                    <field type="uint8_t" name="something_else">Something else</field>
                </message>
            </mavlink>
        "#;
        let mut parser = Parser::new();
        parser.parse_content(content, Path::new(""))?;

        let mut expected = HashMap::new();
        expected.insert(1, Offsets::new(1, Some(2)));
        expected.insert(2, Offsets::new(0, None));

        assert_eq!(parser.targeted_messages.len(), 2);
        for msg in &parser.targeted_messages {
            assert_eq!(msg.offsets, expected[&msg.id]);
        }
        Ok(())
    }

    #[test]
    fn test_parse_content_no_msg_id() -> Result<(), ParseError> {
        let content = r#"
            <mavlink>
                <message name="msg1">
                    <field type="uint8_t" name="target_system">Target system ID</field>
                    <field type="uint8_t" name="target_component">Target component ID</field>
                </message>
            </mavlink>
        "#;
        let mut parser = Parser::new();
        let result = parser.parse_content(content, Path::new(""));
        assert!(matches!(result, Err(ParseError::MessageWithoutId)));
        Ok(())
    }

    #[test]
    fn test_parse_content_invalid_msg_id() -> Result<(), ParseError> {
        let content = r#"
            <mavlink>
                <message id="invalid" name="msg1">
                    <field type="uint8_t" name="target_system">Target system ID</field>
                    <field type="uint8_t" name="target_component">Target component ID</field>
                </message>
            </mavlink>
        "#;
        let mut parser = Parser::new();
        let result = parser.parse_content(content, Path::new(""));
        assert!(matches!(result, Err(ParseError::InvalidMessageId(_))));
        Ok(())
    }

    #[test]
    fn test_parse_content_missing_equals() -> Result<(), ParseError> {
        let content = r#"
            <mavlink>
                <message id"1" name="msg1">
                    <field type="uint8_t" name="target_system">Target system ID</field>
                    <field type="uint8_t" name="target_component">Target component ID</field>
                </message>
            </mavlink>
        "#;
        let mut parser = Parser::new();
        let result = parser.parse_content(content, Path::new(""));
        assert!(matches!(result, Err(ParseError::QuickXml(_))));
        Ok(())
    }
}
