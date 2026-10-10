// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The 24×80 cell model with 5250 attributes and input fields.

/// Rows on the display.
pub const ROWS: usize = 24;
/// Columns on the display.
pub const COLS: usize = 80;

/// 5250 colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Color {
    /// Default text.
    #[default]
    Green,
    /// High-intensity text.
    White,
    /// Errors.
    Red,
    /// Blue.
    Blue,
    /// Turquoise.
    Turquoise,
    /// Pink.
    Pink,
    /// Yellow.
    Yellow,
}

/// Display attributes of one cell. These are the 5250 attribute bits, so
/// they stay as flags.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Attr {
    /// Color.
    pub color: Color,
    /// High intensity.
    pub high: bool,
    /// Reverse image.
    pub reverse: bool,
    /// Underline.
    pub underline: bool,
    /// Blink.
    pub blink: bool,
    /// Column separators.
    pub column_sep: bool,
    /// Nondisplay: the cell shows nothing.
    pub nondisplay: bool,
    /// Second half of a double-width character.
    pub continuation: bool,
    /// First half of a double-width character.
    pub double_width: bool,
}

impl Attr {
    /// Plain text in a color.
    pub const fn color(color: Color) -> Self {
        Self {
            color,
            high: false,
            reverse: false,
            underline: false,
            blink: false,
            column_sep: false,
            nondisplay: false,
            continuation: false,
            double_width: false,
        }
    }

    /// Default green.
    pub const GREEN: Self = Self::color(Color::Green);
    /// White, high intensity.
    pub const WHITE: Self = Self {
        high: true,
        ..Self::color(Color::White)
    };
    /// Red.
    pub const RED: Self = Self::color(Color::Red);
    /// An underlined green input field.
    pub const INPUT: Self = Self {
        underline: true,
        ..Self::color(Color::Green)
    };
    /// An input field that shows nothing (passwords).
    pub const INPUT_HIDDEN: Self = Self {
        underline: true,
        nondisplay: true,
        ..Self::color(Color::Green)
    };
}

/// One cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// Character. `' '` when empty.
    pub ch: char,
    /// Attributes.
    pub attr: Attr,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            attr: Attr::GREEN,
        }
    }
}

/// An input field, 1-based row and column like the screen layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    /// Row, 1-based.
    pub row: usize,
    /// Column, 1-based.
    pub col: usize,
    /// Length in cells.
    pub len: usize,
    /// Typed characters show nothing.
    pub nondisplay: bool,
}

/// The display.
#[derive(Clone, Debug)]
pub struct Screen {
    cells: Vec<Cell>,
    /// Input fields in tab order.
    pub fields: Vec<Field>,
    /// Cursor position, 0-based (row, col).
    pub cursor: (usize, usize),
}

impl Default for Screen {
    fn default() -> Self {
        Self::new()
    }
}

impl Screen {
    /// A blank screen.
    pub fn new() -> Self {
        Self {
            cells: vec![Cell::default(); ROWS * COLS],
            fields: Vec::new(),
            cursor: (0, 0),
        }
    }

    /// Blank every cell, drop the fields.
    pub fn clear(&mut self) {
        self.cells.iter_mut().for_each(|c| *c = Cell::default());
        self.fields.clear();
        self.cursor = (0, 0);
    }

    /// A cell by 0-based position.
    pub fn cell(&self, row: usize, col: usize) -> Cell {
        self.cells[row * COLS + col]
    }

    /// A mutable cell by 0-based position.
    pub fn cell_mut(&mut self, row: usize, col: usize) -> &mut Cell {
        &mut self.cells[row * COLS + col]
    }

    /// Write text at a 1-based position, clipped at the right edge.
    pub fn put(&mut self, row: usize, col: usize, text: &str, attr: Attr) {
        if row == 0 || row > ROWS || col == 0 {
            return;
        }
        for (i, ch) in text.chars().enumerate() {
            let c = col - 1 + i;
            if c >= COLS {
                break;
            }
            *self.cell_mut(row - 1, c) = Cell { ch, attr };
        }
    }

    /// Write text double width: each character takes two cells.
    pub fn put_double(&mut self, row: usize, col: usize, text: &str, attr: Attr) {
        if row == 0 || row > ROWS || col == 0 {
            return;
        }
        for (i, ch) in text.chars().enumerate() {
            let c = col - 1 + 2 * i;
            if c + 1 >= COLS {
                break;
            }
            *self.cell_mut(row - 1, c) = Cell {
                ch,
                attr: Attr {
                    double_width: true,
                    ..attr
                },
            };
            *self.cell_mut(row - 1, c + 1) = Cell {
                ch: ' ',
                attr: Attr {
                    continuation: true,
                    ..attr
                },
            };
        }
    }

    /// Define an input field: blank cells with the input attribute.
    pub fn field(&mut self, row: usize, col: usize, len: usize, nondisplay: bool) {
        let attr = if nondisplay {
            Attr::INPUT_HIDDEN
        } else {
            Attr::INPUT
        };
        self.put(row, col, &" ".repeat(len), attr);
        self.fields.push(Field {
            row,
            col,
            len,
            nondisplay,
        });
    }

    /// Text of a field as typed (nondisplay included).
    pub fn field_text(&self, f: &Field) -> String {
        (0..f.len)
            .map(|i| self.cell(f.row - 1, f.col - 1 + i).ch)
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    /// The whole grid as 24 lines of exactly 80 characters. Nondisplay
    /// cells and continuation cells read as spaces.
    pub fn dump(&self) -> String {
        let mut s = String::with_capacity(ROWS * (COLS + 1));
        for r in 0..ROWS {
            for c in 0..COLS {
                let cell = self.cell(r, c);
                s.push(if cell.attr.nondisplay || cell.attr.continuation {
                    ' '
                } else {
                    cell.ch
                });
            }
            s.push('\n');
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_and_dump() {
        let mut s = Screen::new();
        s.put(1, 36, "Sign On", Attr::WHITE);
        s.field(7, 53, 10, true);
        s.cell_mut(6, 52).ch = 'x';
        let d = s.dump();
        let lines: Vec<&str> = d.lines().collect();
        assert_eq!(lines.len(), 24);
        assert!(lines.iter().all(|l| l.chars().count() == 80));
        assert_eq!(&lines[0][35..42], "Sign On");
        assert_eq!(&lines[6][52..53], " ", "nondisplay hides typed text");
        assert_eq!(s.field_text(&s.fields[0]), "x");
    }

    #[test]
    fn double_width_takes_two_cells() {
        let mut s = Screen::new();
        s.put_double(4, 10, "C1", Attr::WHITE);
        assert_eq!(s.cell(3, 9).ch, 'C');
        assert!(s.cell(3, 9).attr.double_width);
        assert!(s.cell(3, 10).attr.continuation);
        assert_eq!(s.cell(3, 11).ch, '1');
    }
}
