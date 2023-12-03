use crate::{
    data::DataComponent,
    parser::{Node, NodeParseResult},
};
use anyhow::Result;
use log::error;
use utf8::{DecodeError, Incomplete};

pub struct Translator {
    text_buffer: String,
    incomplete: Incomplete,
}

impl Translator {
    pub fn new() -> Result<Self> {
        Ok(Self {
            text_buffer: String::new(),
            incomplete: Incomplete::empty(),
        })
    }

    pub fn write(&mut self, input: &[u8], data: &mut DataComponent) {
        self.read_bytes_to_buffer(input);
        self.write_buffer_to_data(data);
    }

    pub fn read_bytes_to_buffer(&mut self, mut input: &[u8]) {
        if !self.incomplete.is_empty() {
            match self.incomplete.try_complete(input) {
                Some((Ok(text), remaining_input)) => {
                    self.text_buffer += text;
                    input = remaining_input;
                }
                Some((Err(invalid_sequence), remaining_input)) => {
                    error!("Could not decode to valid utf-8 {invalid_sequence:?}");
                    self.text_buffer += &char::REPLACEMENT_CHARACTER.to_string();
                    input = remaining_input;
                }
                None => return,
            }
        }

        loop {
            match utf8::decode(input) {
                Ok(text) => {
                    self.text_buffer += text;
                    return;
                }
                Err(DecodeError::Incomplete {
                    valid_prefix,
                    incomplete_suffix,
                }) => {
                    self.text_buffer += valid_prefix;
                    self.incomplete = incomplete_suffix;
                    return;
                }
                Err(DecodeError::Invalid {
                    valid_prefix,
                    invalid_sequence,
                    remaining_input,
                }) => {
                    self.text_buffer += valid_prefix;
                    error!("Could not decode to valid utf-8 {invalid_sequence:?}");
                    self.text_buffer += &char::REPLACEMENT_CHARACTER.to_string();
                    input = remaining_input;
                }
            }
        }
    }

    pub fn write_buffer_to_data(&mut self, data: &mut DataComponent) {
        let mut chars = self.text_buffer.chars();
        while let NodeParseResult::Match(remaining_chars, node) = Node::parse(chars.clone()) {
            chars = remaining_chars;
            data.write_node(&node);
        }
        self.text_buffer = chars.collect();
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use crate::data::Position;

    use super::*;

    #[test]
    fn test_write_text() {
        let mut data = DataComponent::new();
        let mut translator = Translator::new().unwrap();
        translator.write(b"hello world", &mut data);
        assert_eq!(data.render(10), "hello world");
        assert_eq!(data.get_active_position(), Position { row: 0, col: 10 });
    }

    #[test]
    fn test_write_text_incomplete_utf8() {
        let mut data = DataComponent::new();
        let mut translator = Translator::new().unwrap();

        let bytes = b"\xd0";
        // `bytes` is not valid utf8 (at least on its own...)
        assert_matches!(String::from_utf8(bytes.into()), Err(_));

        translator.write(b"\xd0", &mut data);
        assert_eq!(data.render(10), "");
        assert_eq!(data.get_active_position(), Position { row: 0, col: 0 });
    }

    #[test]
    fn test_split_utf8() {
        let mut data = DataComponent::new();
        let mut translator = Translator::new().unwrap();

        let first_byte = b"\xd0";
        assert_matches!(String::from_utf8(first_byte.into()), Err(_));

        let second_byte = b"\xa3";
        assert_matches!(String::from_utf8(second_byte.into()), Err(_));

        assert_eq!(b"\xd0\xa3", "У".as_bytes());

        translator.write(first_byte, &mut data);
        translator.write(second_byte, &mut data);
        assert_eq!(data.render(10), "У");
        assert_eq!(data.get_active_position(), Position { row: 0, col: 0 });
    }
}
