use std::cmp::max;

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
struct Position {
    row: usize,
    col: usize,
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
        self.active_position.col = max(self.active_position.col - 1, 0);
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
        self.active_position.row = max(self.active_position.row - 1, 0);
    }

    pub fn activate_first_cell(&mut self) {
        self.active_position.col = 0;
    }

    // XXX replace with real formatting
    pub fn render(&self) -> String {
        let mut result = String::new();
        for line in self.lines.iter() {
            for cell in line.cells.iter() {
                if let Some(grapheme) = cell.grapheme.as_ref() {
                    result += grapheme;
                } else {
                    result += " ";
                }
            }
            result.push('\n');
        }
        result
    }
}
