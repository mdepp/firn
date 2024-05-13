use log::debug;
use log::error;
use log::info;
use unicode_segmentation::UnicodeSegmentation;

use crate::parser::Node;

/**
 * A safe way to interact with a ragged array of cells, indexed
 * by an 'active position' (cursor)
 */
pub struct DataComponent {
    lines: Vec<Line>,
    active_position: Position,
}

struct Line {
    cells: Vec<Cell>,
}

pub struct Cell {
    pub grapheme: Option<String>,
}

/** Unlike the standard, is 0-indexed */
#[derive(Clone, PartialEq, Debug)]
pub struct Position {
    pub row: usize,
    pub col: usize,
}

impl DataComponent {
    pub fn new() -> Self {
        Self {
            lines: vec![Line {
                cells: vec![Cell { grapheme: None }],
            }],
            active_position: Position { row: 0, col: 0 },
        }
    }

    pub fn get_active_position(&self) -> Position {
        self.active_position.clone()
    }

    fn get_active_line(&self) -> &Line {
        &self.lines[self.active_position.row]
    }

    fn get_active_line_mut(&mut self) -> &mut Line {
        &mut self.lines[self.active_position.row]
    }

    pub fn get_active_cell(&self) -> &Cell {
        &self.get_active_line().cells[self.active_position.col]
    }

    pub fn get_active_cell_mut(&mut self) -> &mut Cell {
        &mut self.lines[self.active_position.row].cells[self.active_position.col]
    }

    /** Move the active cell to the right, adding a new empty cell if one does not already exist. */
    pub fn activate_next_cell(&mut self) {
        self.active_position.col += 1;
        assert!(self.active_position.col <= self.get_active_line().cells.len());
        if self.active_position.col == self.get_active_line().cells.len() {
            self.get_active_line_mut()
                .cells
                .push(Cell { grapheme: None });
        }
    }

    /** Move the active cell to the left, unless already at the left-most cell on a line */
    pub fn activate_prev_cell(&mut self) {
        self.active_position.col = if self.active_position.col > 0 {
            self.active_position.col - 1
        } else {
            0
        };
    }

    /* Move the active cell to the beginning of the next line, making a new line if necessary */
    pub fn activate_next_line(&mut self) {
        self.active_position.row += 1;
        self.active_position.col = 0;
        assert!(self.active_position.row <= self.lines.len());
        if self.active_position.row == self.lines.len() {
            self.lines.push(Line {
                cells: vec![Cell { grapheme: None }],
            })
        }
    }

    /* Move the active cell to the beginning of the previous line, or to the beginning of the current line if already at the first line */
    pub fn activate_prev_line(&mut self) {
        self.active_position.col = 0;
        self.active_position.row = if self.active_position.row > 0 {
            self.active_position.row - 1
        } else {
            0
        };
    }

    pub fn activate_first_cell(&mut self) {
        self.active_position.col = 0;
    }

    pub fn erase_in_line(&mut self, n: Option<&str>) {
        match n {
            Some("0") | None => {
                let current_length = self.active_position.col + 1;
                self.get_active_line_mut().cells.truncate(current_length);
            }
            Some("1") => {
                for cell in self.get_active_line_mut().cells.iter_mut() {
                    cell.grapheme = None
                }
            }
            Some("2") => {
                self.get_active_line_mut().cells.clear();
            }
            _ => {
                error!("Unexpected EL argument {n:?}")
            }
        }
    }

    pub fn delete_character(&mut self, n: &str) {
        let n: Result<usize, _> = n.parse();
        if let Ok(n) = n {
            let i = self.get_active_position().col + 1;
            self.get_active_line_mut().cells.splice(i..(i + n), vec![]);
        } else {
            error!("Unable to parse {n:?}");
        }
    }

    // XXX replace with real formatting
    pub fn render(&self, max_lines: usize) -> String {
        let mut result = String::new();
        result.clear();
        for (row_index, line) in self
            .lines
            .iter()
            .skip(self.lines.len().saturating_sub(max_lines))
            .enumerate()
        {
            for (col_index, cell) in line.cells.iter().enumerate() {
                if let Some(grapheme) = cell.grapheme.as_ref() {
                    result += grapheme;
                } else {
                    result += " ";
                }
                if row_index == self.active_position.row && col_index == self.active_position.col {
                    result += "\u{5f}";
                }
            }
            result = result.trim_end().to_string() + "\n";
        }
        result.pop();
        result
    }

    pub fn write_node(&mut self, node: &Node) {
        debug!("{node:?}");
        match node {
            Node::Text(text) => self.write_text(text),
            Node::C0Control('\x08') => self.activate_prev_cell(),
            Node::C0Control('\x0A') => self.activate_next_line(),
            Node::C0Control('\x0D') => self.activate_first_cell(),
            Node::C1Control('\x45') => self.activate_first_cell(),
            Node::C1Control('\x4D') => self.activate_prev_line(),
            Node::ControlSequence {
                parameter_bytes: None,
                intermediate_bytes: None,
                final_byte: 'C',
            } => self.activate_next_cell(),
            Node::ControlSequence {
                parameter_bytes: n,
                intermediate_bytes: _,
                final_byte: 'K',
            } => self.erase_in_line(n.as_deref()),
            Node::ControlSequence {
                parameter_bytes: Some(n),
                intermediate_bytes: None,
                final_byte: 'P',
            } => self.delete_character(n),
            node => info!("Ignoring node {node:?}"),
        };
    }

    fn write_text(&mut self, text: &str) {
        let combined_text = self
            .get_active_cell()
            .grapheme
            .to_owned()
            .unwrap_or_default()
            + text;
        let mut graphemes = combined_text.graphemes(true);

        if let Some(grapheme) = graphemes.next() {
            self.get_active_cell_mut().grapheme = Some(grapheme.to_string());
        }
        for grapheme in graphemes {
            self.activate_next_cell();
            self.get_active_cell_mut().grapheme = Some(grapheme.to_string());
        }
    }
}
