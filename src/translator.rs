use crate::{
    data::DataComponent,
    parser::{Node, NodeParseResult},
};

pub struct Translator {
    text_buffer: String,
}

impl Translator {
    pub fn new() -> Self {
        Self {
            text_buffer: String::new(),
        }
    }

    pub fn write(&mut self, text: &str, data: &mut DataComponent) {
        self.text_buffer += text;
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
    use crate::data::Position;

    use super::*;

    #[test]
    fn test_write_text() {
        let mut data = DataComponent::new();
        let mut translator = Translator::new();
        translator.write("hello world", &mut data);
        assert_eq!(data.render(), "hello world");
        assert_eq!(data.get_active_position(), Position { row: 0, col: 10 });
    }
}
