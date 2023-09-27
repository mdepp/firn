use std::{
    ops::{ControlFlow, FromResidual, RangeInclusive, Try},
    str::Chars,
};

// See https://www.ecma-international.org/wp-content/uploads/ECMA-48_5th_edition_june_1991.pdf
#[derive(Debug)]
pub enum Node {
    Text(String),
    C0Control(char),
    C1Control(char),
    ControlSequence {
        parameter_bytes: Option<String>,
        intermediate_bytes: Option<String>,
        final_byte: char,
    },
    IndependentControlFunction(char),
    ControlString {
        opening: char,
        character_string: String,
    },
    Unknown(char),
}

#[derive(Debug)]
pub enum NodeParseResult<'a> {
    Match(Chars<'a>, Node),
    Indeterminate,
}

enum IntermediateResultResidual {
    NoMatch,
    Indeterminate,
}

enum TryIntermediateResult<'a, T = ()> {
    Match(Chars<'a>, T),
    NoMatch,
    Indeterminate,
}

impl<'a, T> Try for TryIntermediateResult<'a, T> {
    type Output = (Chars<'a>, T);

    type Residual = IntermediateResultResidual;

    fn from_output(output: Self::Output) -> Self {
        Self::Match(output.0, output.1)
    }

    fn branch(self) -> std::ops::ControlFlow<Self::Residual, Self::Output> {
        match self {
            Self::Match(chars, val) => ControlFlow::Continue((chars, val)),
            Self::NoMatch => ControlFlow::Break(IntermediateResultResidual::NoMatch),
            Self::Indeterminate => ControlFlow::Break(IntermediateResultResidual::Indeterminate),
        }
    }
}

impl<'a, T> FromResidual<IntermediateResultResidual> for TryIntermediateResult<'a, T> {
    fn from_residual(residual: IntermediateResultResidual) -> Self {
        match residual {
            IntermediateResultResidual::NoMatch => Self::NoMatch,
            IntermediateResultResidual::Indeterminate => Self::Indeterminate,
        }
    }
}

impl<'a, T> TryIntermediateResult<'a, T> {
    fn optional(self, chars: Chars<'a>) -> TryIntermediateResult<'a, Option<T>> {
        match self {
            Self::Match(chars, val) => TryIntermediateResult::Match(chars, Some(val)),
            Self::NoMatch => TryIntermediateResult::Match(chars, None),
            Self::Indeterminate => TryIntermediateResult::Indeterminate,
        }
    }
}

impl Node {
    fn skip_delimiter<'a>(mut chars: Chars<'a>, prefix: &str) -> TryIntermediateResult<'a> {
        let mut prefix = prefix.chars();
        loop {
            let prev_chars = chars.clone();
            match (chars.next(), prefix.next()) {
                (Some(ch1), Some(ch2)) if ch1 == ch2 => {}
                (Some(_), Some(_)) => return TryIntermediateResult::NoMatch,
                (_, None) => return TryIntermediateResult::Match(prev_chars, ()),
                (None, Some(_)) => return TryIntermediateResult::Indeterminate,
            }
        }
    }

    fn capture_single(
        mut chars: Chars<'_>,
        func: impl FnOnce(char) -> bool,
    ) -> TryIntermediateResult<'_, char> {
        match chars.next() {
            Some(ch) if func(ch) => TryIntermediateResult::Match(chars, ch),
            Some(_) => TryIntermediateResult::NoMatch,
            None => TryIntermediateResult::Indeterminate,
        }
    }

    fn capture_single_range(
        chars: Chars<'_>,
        range: RangeInclusive<char>,
    ) -> TryIntermediateResult<'_, char> {
        Self::capture_single(chars, |ch| range.contains(&ch))
    }

    fn capture_group(
        chars: Chars<'_>,
        mut func: impl FnMut(char) -> bool,
    ) -> TryIntermediateResult<'_, String> {
        let mut result = String::new();
        let (mut chars, ch) = Self::capture_single(chars, &mut func)?;
        result.push(ch);

        loop {
            let prev_chars = chars.clone();
            match chars.next() {
                Some(ch) if func(ch) => result.push(ch),
                Some(_) => return TryIntermediateResult::Match(prev_chars, result),
                None => return TryIntermediateResult::Indeterminate,
            }
        }
    }

    fn capture_group_lazy(
        mut chars: Chars<'_>,
        mut func: impl FnMut(char) -> bool,
    ) -> TryIntermediateResult<'_, String> {
        let mut result = String::new();
        match chars.next() {
            Some(ch) if func(ch) => result.push(ch),
            Some(_) => return TryIntermediateResult::NoMatch,
            None => return TryIntermediateResult::Indeterminate,
        };

        loop {
            let prev_chars = chars.clone();
            match chars.next() {
                Some(ch) if func(ch) => result.push(ch),
                Some(_) => return TryIntermediateResult::Match(prev_chars, result),
                None => return TryIntermediateResult::Match(prev_chars, result),
            }
        }
    }

    fn capture_group_range(
        chars: Chars<'_>,
        range: RangeInclusive<char>,
    ) -> TryIntermediateResult<String> {
        Self::capture_group(chars, |ch| range.contains(&ch))
    }

    fn parse_c0_control(chars: Chars) -> TryIntermediateResult<Self> {
        let (chars, code) = Self::capture_single_range(chars, '\x00'..='\x1F')?;
        TryIntermediateResult::Match(chars, Self::C0Control(code))
    }

    fn parse_c1_control(chars: Chars) -> TryIntermediateResult<Self> {
        let (chars, _) = Self::skip_delimiter(chars, "\x1B")?;
        let (chars, code) = Self::capture_single_range(chars, '\x40'..='\x5F')?;
        TryIntermediateResult::Match(chars, Self::C1Control(code))
    }

    fn parse_control_sequence(chars: Chars) -> TryIntermediateResult<Self> {
        let (chars, _) = Self::skip_delimiter(chars, "\x1B[")?;
        let (chars, parameter_bytes) =
            Self::capture_group_range(chars.clone(), '\x30'..='\x3F').optional(chars)?;
        let (chars, intermediate_bytes) =
            Self::capture_group_range(chars.clone(), '\x20'..='\x2F').optional(chars)?;
        let (chars, final_byte) = Self::capture_single_range(chars, '\x40'..='\x7E')?;
        TryIntermediateResult::Match(
            chars,
            Self::ControlSequence {
                parameter_bytes,
                intermediate_bytes,
                final_byte,
            },
        )
    }

    fn parse_independent_control_function(chars: Chars) -> TryIntermediateResult<Self> {
        let (chars, _) = Self::skip_delimiter(chars, "\x1B")?;
        let (chars, code) = Self::capture_single_range(chars, '\x60'..='\x7E')?;
        TryIntermediateResult::Match(chars, Self::IndependentControlFunction(code))
    }

    // A 'character string' is a sequence of any bit combination except
    // SOS or ST. In practice, it is implemented as any bit combination
    // delimited by ST or BELL.
    // This function reads both the string and the end delimiter but only
    // returns the string.
    fn capture_character_string(mut chars: Chars) -> TryIntermediateResult<String> {
        let mut character_string = String::new();
        loop {
            match Self::skip_delimiter(chars.clone(), "\x1B\x5C") {
                TryIntermediateResult::Match(chars, _) => {
                    return TryIntermediateResult::Match(chars, character_string)
                }
                TryIntermediateResult::NoMatch => {}
                TryIntermediateResult::Indeterminate => {
                    return TryIntermediateResult::Indeterminate
                }
            }
            match Self::skip_delimiter(chars.clone(), "\x07") {
                TryIntermediateResult::Match(chars, _) => {
                    return TryIntermediateResult::Match(chars, character_string)
                }
                TryIntermediateResult::NoMatch => {}
                TryIntermediateResult::Indeterminate => {
                    return TryIntermediateResult::Indeterminate
                }
            }
            match chars.next() {
                Some(ch) => character_string.push(ch),
                None => return TryIntermediateResult::Indeterminate,
            }
        }
    }

    fn parse_control_string(chars: Chars) -> TryIntermediateResult<Self> {
        const APC: char = '\x5F';
        const DCS: char = '\x50';
        const OSC: char = '\x5D';
        const PM: char = '\x5E';
        const SOS: char = '\x58';

        let (chars, _) = Self::skip_delimiter(chars, "\x1B")?;
        let (chars, opening) =
            Self::capture_single(chars, |ch| matches!(ch, APC | DCS | OSC | PM | SOS))?;
        let (chars, character_string) = Self::capture_character_string(chars)?;
        TryIntermediateResult::Match(
            chars,
            Self::ControlString {
                opening,
                character_string,
            },
        )
    }

    fn parse_text(chars: Chars) -> TryIntermediateResult<Self> {
        let (chars, text) = Self::capture_group_lazy(chars.clone(), |ch| !ch.is_control())?;
        TryIntermediateResult::Match(chars, Self::Text(text))
    }

    fn parse_unknown(chars: Chars) -> TryIntermediateResult<Self> {
        let (chars, ch) = Self::capture_single(chars, |_| true)?;
        TryIntermediateResult::Match(chars, Self::Unknown(ch))
    }

    pub fn parse(chars: Chars) -> NodeParseResult {
        let parse_fns = [
            Self::parse_control_string,
            Self::parse_independent_control_function,
            Self::parse_control_sequence,
            Self::parse_c1_control,
            Self::parse_c0_control,
            Self::parse_text,
            Self::parse_unknown,
        ];
        for parse_fn in parse_fns.iter() {
            match parse_fn(chars.clone()) {
                TryIntermediateResult::Match(chars, node) => {
                    return NodeParseResult::Match(chars, node)
                }
                TryIntermediateResult::Indeterminate => return NodeParseResult::Indeterminate,
                TryIntermediateResult::NoMatch => {}
            }
        }
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use super::*;

    #[test]
    fn test_parse_c0() {
        let text = String::from("\x07world");
        let result = Node::parse(text.chars());
        assert_matches!(result, NodeParseResult::Match(_, Node::C0Control('\x07')));
    }

    #[test]
    fn test_parse_c1() {
        let text = String::from("\x1B\x40world");
        let result = Node::parse(text.chars());
        assert_matches!(result, NodeParseResult::Match(_, Node::C1Control('\x40')));
    }

    #[test]
    fn test_parse_control_sequence() {
        let text = String::from("\x1B[0;1;2!mworld");
        let result = Node::parse(text.chars());
        assert_matches!(
            result,
            NodeParseResult::Match(
                _,
                Node::ControlSequence {
                    parameter_bytes: Some(parameter_bytes),
                    intermediate_bytes: Some(intermediate_bytes),
                    final_byte
                }
            ) if parameter_bytes == "0;1;2" && intermediate_bytes == "!" && final_byte == 'm'
        )
    }

    #[test]
    fn test_parse_control_sequence_without_parameter_bytes() {
        let text = String::from("\x1B[!mworld");
        let result = Node::parse(text.chars());
        assert_matches!(
            result,
            NodeParseResult::Match(
                _,
                Node::ControlSequence {
                    parameter_bytes: None,
                    intermediate_bytes: Some(intermediate_bytes),
                    final_byte
                }
            ) if intermediate_bytes == "!" && final_byte == 'm'
        )
    }

    #[test]
    fn test_parse_control_sequence_without_intermediate_bytes() {
        let text = String::from("\x1B[0;1;2mworld");
        let result = Node::parse(text.chars());
        assert_matches!(
            result,
            NodeParseResult::Match(
                _,
                Node::ControlSequence {
                    parameter_bytes: Some(parameter_bytes),
                    intermediate_bytes: None,
                    final_byte
                }
            ) if parameter_bytes == "0;1;2" && final_byte == 'm'
        )
    }

    #[test]
    fn test_parse_independent_control_function() {
        let text = String::from("\x1B\x60world");
        let result = Node::parse(text.chars());
        assert_matches!(
            result,
            NodeParseResult::Match(_, Node::IndependentControlFunction('\x60'))
        );
    }

    #[test]
    fn test_parse_text() {
        let text = String::from("Hello, world");
        let result = Node::parse(text.chars());
        assert_matches!(
            result,
            NodeParseResult::Match(_, Node::Text(text)) if text == "Hello, world"
        );
    }

    #[test]
    fn test_parse_control_string() {
        let text = String::from("\x1B]0;Hello\x07world");
        let result = Node::parse(text.chars());
        assert_matches!(
            result,
            NodeParseResult::Match(_, Node::ControlString{opening: ']', character_string}) if character_string == "0;Hello"
        );
    }
}
