// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console port: the console's grid as cells, not a 5250 data
//! stream and not ANSI. On each connection of the peer, and when it
//! asks ([`ToSystem::Refresh`]), the system sends a whole [`Frame`]:
//! the size, every cell's character and attribute, the cursor's place
//! and shape, the indicators and, when the console shows the SRC
//! display, its code. Afterwards only what changed, as an [`Update`]
//! of [`Run`]s. The peer sends its hello and [`Key`]s: the keys the
//! console's keyboard gives after its mapping.
//!
//! [`Grid`] is a peer's copy of the screen, kept from frames and
//! updates; [`runs`] is what the system sends of a change.

use ipc::{Dec, Wire, WireError, wire_struct};

use crate::Error;
use crate::frame::Frame as RawFrame;
use crate::wire::{body, frame_of, open_enum};

/// Message kinds on the console port.
pub mod kind {
    /// The system's hello.
    pub const HELLO: u16 = 0x0201;
    /// A whole frame.
    pub const FRAME: u16 = 0x0202;
    /// What changed.
    pub const UPDATE: u16 = 0x0203;
    /// The peer's hello.
    pub const PEER_HELLO: u16 = 0x0301;
    /// A key.
    pub const KEY: u16 = 0x0302;
    /// Ask for a whole frame.
    pub const REFRESH: u16 = 0x0303;
}

/// The most rows a frame has (27 by 132 is the wide display).
pub const MAX_ROWS: u8 = 27;
/// The most columns.
pub const MAX_COLS: u8 = 132;

/// A cell's attribute bits. The low three bits are the color.
pub mod attr {
    /// The color's bits.
    pub const COLOR: u16 = 0x0007;
    /// Green, the default.
    pub const GREEN: u16 = 0;
    /// White.
    pub const WHITE: u16 = 1;
    /// Red.
    pub const RED: u16 = 2;
    /// Blue.
    pub const BLUE: u16 = 3;
    /// Turquoise.
    pub const TURQUOISE: u16 = 4;
    /// Pink.
    pub const PINK: u16 = 5;
    /// Yellow.
    pub const YELLOW: u16 = 6;
    /// High intensity.
    pub const HIGH: u16 = 1 << 3;
    /// Reverse image.
    pub const REVERSE: u16 = 1 << 4;
    /// Underline.
    pub const UNDERLINE: u16 = 1 << 5;
    /// Blink.
    pub const BLINK: u16 = 1 << 6;
    /// Column separators.
    pub const COLUMN_SEP: u16 = 1 << 7;
    /// Nondisplay: the cell shows nothing.
    pub const NONDISPLAY: u16 = 1 << 8;
    /// The second half of a double-width character.
    pub const CONTINUATION: u16 = 1 << 9;
    /// The first half of a double-width character.
    pub const DOUBLE_WIDTH: u16 = 1 << 10;
}

/// The indicators' bits.
pub mod indicator {
    /// Input inhibited: keys are not taken until Reset.
    pub const INHIBITED: u16 = 1 << 0;
    /// Insert mode.
    pub const INSERT: u16 = 1 << 1;
    /// The console is a 5250 session's terminal.
    pub const SESSION: u16 = 1 << 2;
}

/// One cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// Its character; a space when empty.
    pub ch: char,
    /// Its attribute bits ([`attr`]).
    pub attr: u16,
}
wire_struct!(Cell { ch, attr });

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            attr: attr::GREEN,
        }
    }
}

open_enum!(
    /// How the cursor is drawn.
    Shape {
        /// An underline, as the framebuffer draws it.
        Underline = 1,
        /// A block.
        Block = 2,
        /// Not drawn.
        Hidden = 3,
    }
);

/// The SRC display's current code, when that is what the console shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SrcShown {
    /// The code.
    pub code: String,
    /// Its text.
    pub text: String,
}
wire_struct!(SrcShown { code, text });

/// What goes with the cells of every frame and update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// The cursor's row, 0-based.
    pub row: u8,
    /// The cursor's column, 0-based.
    pub col: u8,
    /// Its shape.
    pub shape: Shape,
    /// The indicators ([`indicator`]).
    pub indicators: u16,
    /// The SRC display's code, when it shows.
    pub src: Option<SrcShown>,
}
wire_struct!(Status {
    row,
    col,
    shape,
    indicators,
    src
});

impl Default for Status {
    fn default() -> Self {
        Self {
            row: 0,
            col: 0,
            shape: Shape::Underline,
            indicators: 0,
            src: None,
        }
    }
}

/// A whole frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// A number the system counts up with each frame and update.
    pub seq: u32,
    /// Rows.
    pub rows: u8,
    /// Columns.
    pub cols: u8,
    /// Every cell, a row at a time.
    pub cells: Vec<Cell>,
    /// The cursor and the rest.
    pub status: Status,
}
wire_struct!(Frame {
    seq,
    rows,
    cols,
    cells,
    status
});

/// Cells that changed, side by side on one row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    /// The row, 0-based.
    pub row: u8,
    /// The first column, 0-based.
    pub col: u8,
    /// The cells from there.
    pub cells: Vec<Cell>,
}
wire_struct!(Run { row, col, cells });

/// What changed since the last frame or update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    /// As in [`Frame`].
    pub seq: u32,
    /// The cells that changed.
    pub runs: Vec<Run>,
    /// The cursor and the rest, whole.
    pub status: Status,
}
wire_struct!(Update { seq, runs, status });

/// Either side's hello on the console port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hello {
    /// Its protocol version.
    pub version: u16,
    /// Its capabilities (`crate::caps`).
    pub caps: u64,
}
wire_struct!(Hello { version, caps });

/// A key, as the console's keyboard gives it after its mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A printable character.
    Char(char),
    /// Enter.
    Enter,
    /// Field Advance.
    FieldAdvance,
    /// Field Backspace.
    FieldBackspace,
    /// Field Exit.
    FieldExit,
    /// Backspace within a field.
    Backspace,
    /// Home.
    Home,
    /// Insert.
    Insert,
    /// Delete.
    Delete,
    /// F1 to F24.
    Function(u8),
    /// Attention.
    Attention,
    /// System Request.
    SysReq,
    /// Reset.
    Reset,
    /// Cursor left.
    Left,
    /// Cursor right.
    Right,
    /// Cursor up.
    Up,
    /// Cursor down.
    Down,
    /// Roll Down (Page Up).
    PageUp,
    /// Roll Up (Page Down).
    PageDown,
    /// A key a later version has: its tag.
    Other(u8),
}

impl Wire for Key {
    fn put(&self, out: &mut Vec<u8>) {
        let tag: u8 = match self {
            Self::Char(c) => {
                out.push(0);
                c.put(out);
                return;
            }
            Self::Function(n) => {
                out.push(9);
                out.push(*n);
                return;
            }
            Self::Enter => 1,
            Self::FieldAdvance => 2,
            Self::FieldBackspace => 3,
            Self::FieldExit => 4,
            Self::Backspace => 5,
            Self::Home => 6,
            Self::Insert => 7,
            Self::Delete => 8,
            Self::Attention => 10,
            Self::SysReq => 11,
            Self::Reset => 12,
            Self::Left => 13,
            Self::Right => 14,
            Self::Up => 15,
            Self::Down => 16,
            Self::PageUp => 17,
            Self::PageDown => 18,
            Self::Other(t) => *t,
        };
        out.push(tag);
    }

    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(d)? {
            0 => Self::Char(char::get(d)?),
            1 => Self::Enter,
            2 => Self::FieldAdvance,
            3 => Self::FieldBackspace,
            4 => Self::FieldExit,
            5 => Self::Backspace,
            6 => Self::Home,
            7 => Self::Insert,
            8 => Self::Delete,
            9 => Self::Function(u8::get(d)?),
            10 => Self::Attention,
            11 => Self::SysReq,
            12 => Self::Reset,
            13 => Self::Left,
            14 => Self::Right,
            15 => Self::Up,
            16 => Self::Down,
            17 => Self::PageUp,
            18 => Self::PageDown,
            t => Self::Other(t),
        })
    }
}

/// What the system sends on the console port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FromSystem {
    /// Its hello.
    Hello(Hello),
    /// A whole frame.
    Frame(Frame),
    /// What changed.
    Update(Update),
}

/// What the peer sends on the console port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToSystem {
    /// Its hello.
    Hello(Hello),
    /// A key.
    Key(Key),
    /// Ask for a whole frame.
    Refresh,
}

fn size_ok(rows: u8, cols: u8) -> bool {
    (1..=MAX_ROWS).contains(&rows) && (1..=MAX_COLS).contains(&cols)
}

fn check_status(kind: u16, s: &Status, rows: u8, cols: u8) -> Result<(), Error> {
    if s.row >= rows || s.col >= cols {
        return Err(Error::Invalid {
            kind,
            why: "the cursor is off the grid",
        });
    }
    Ok(())
}

impl Frame {
    /// The checks a receiver makes: the size within the limits, a cell
    /// for every place, the cursor on the grid.
    ///
    /// # Errors
    /// One fails.
    pub fn check(&self) -> Result<(), Error> {
        let bad = |why| Error::Invalid {
            kind: kind::FRAME,
            why,
        };
        if !size_ok(self.rows, self.cols) {
            return Err(bad("the size is out of range"));
        }
        if self.cells.len() != usize::from(self.rows) * usize::from(self.cols) {
            return Err(bad("the cells do not fill the grid"));
        }
        check_status(kind::FRAME, &self.status, self.rows, self.cols)
    }
}

impl Update {
    /// The checks a receiver makes against a grid of `rows` by `cols`:
    /// every run on the grid, the cursor too.
    ///
    /// # Errors
    /// One fails.
    pub fn check(&self, rows: u8, cols: u8) -> Result<(), Error> {
        for r in &self.runs {
            let end = usize::from(r.col) + r.cells.len();
            if r.row >= rows || r.cells.is_empty() || end > usize::from(cols) {
                return Err(Error::Invalid {
                    kind: kind::UPDATE,
                    why: "a run is off the grid",
                });
            }
        }
        check_status(kind::UPDATE, &self.status, rows, cols)
    }
}

impl FromSystem {
    /// Its frame's bytes.
    ///
    /// # Errors
    /// It does not fit a frame.
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        match self {
            Self::Hello(m) => frame_of(kind::HELLO, m),
            Self::Frame(m) => frame_of(kind::FRAME, m),
            Self::Update(m) => frame_of(kind::UPDATE, m),
        }
    }

    /// A frame from the system; `None` for a kind this version does not
    /// know. A whole frame is checked here; an update is checked
    /// against the grid it applies to ([`Grid::apply`]).
    ///
    /// # Errors
    /// The body does not read, or a frame fails its checks.
    pub fn decode(f: &RawFrame) -> Result<Option<Self>, Error> {
        Ok(Some(match f.kind {
            kind::HELLO => Self::Hello(body(f)?),
            kind::FRAME => {
                let m: Frame = body(f)?;
                m.check()?;
                Self::Frame(m)
            }
            kind::UPDATE => Self::Update(body(f)?),
            _ => return Ok(None),
        }))
    }
}

impl ToSystem {
    /// Its frame's bytes.
    ///
    /// # Errors
    /// It does not fit a frame.
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        match self {
            Self::Hello(m) => frame_of(kind::PEER_HELLO, m),
            Self::Key(k) => frame_of(kind::KEY, k),
            Self::Refresh => frame_of(kind::REFRESH, &()),
        }
    }

    /// A frame from the peer; `None` for a kind this version does not
    /// know, which the system passes over.
    ///
    /// # Errors
    /// The body does not read.
    pub fn decode(f: &RawFrame) -> Result<Option<Self>, Error> {
        Ok(Some(match f.kind {
            kind::PEER_HELLO => Self::Hello(body(f)?),
            kind::KEY => Self::Key(body(f)?),
            kind::REFRESH => Self::Refresh,
            _ => return Ok(None),
        }))
    }
}

/// Unchanged cells between two changes that a run carries rather than
/// starting a new one: a run's header costs about as much.
const BRIDGE: usize = 3;

/// The runs that turn `old` into `new`, two grids of `cols` columns
/// and the same size.
#[must_use]
pub fn runs(old: &[Cell], new: &[Cell], cols: u8) -> Vec<Run> {
    let cols = usize::from(cols.max(1));
    let mut out = Vec::new();
    for (row, (o, n)) in old.chunks(cols).zip(new.chunks(cols)).enumerate() {
        let mut c = 0;
        while c < n.len() {
            if o.get(c) == Some(&n[c]) {
                c += 1;
                continue;
            }
            let start = c;
            let mut end = c + 1;
            let mut same = 0;
            let mut i = end;
            while i < n.len() {
                if o.get(i) == Some(&n[i]) {
                    same += 1;
                    if same > BRIDGE {
                        break;
                    }
                } else {
                    same = 0;
                    end = i + 1;
                }
                i += 1;
            }
            out.push(Run {
                row: u8::try_from(row).unwrap_or(u8::MAX),
                col: u8::try_from(start).unwrap_or(u8::MAX),
                cells: n[start..end].to_vec(),
            });
            c = end;
        }
    }
    out
}

/// A peer's copy of the console: frames and updates applied.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Grid {
    /// Rows; 0 before the first frame.
    pub rows: u8,
    /// Columns.
    pub cols: u8,
    /// The cells, a row at a time.
    pub cells: Vec<Cell>,
    /// The cursor and the rest.
    pub status: Status,
    /// The last frame's or update's number.
    pub seq: u32,
}

impl Grid {
    /// Whether a whole frame has come.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.rows > 0
    }

    /// Apply a message from the system; hellos change nothing.
    ///
    /// # Errors
    /// An update before any frame, or one off the grid: the peer should
    /// ask for a whole frame.
    pub fn apply(&mut self, m: &FromSystem) -> Result<(), Error> {
        match m {
            FromSystem::Hello(_) => {}
            FromSystem::Frame(f) => {
                f.check()?;
                self.rows = f.rows;
                self.cols = f.cols;
                self.cells.clone_from(&f.cells);
                self.status = f.status.clone();
                self.seq = f.seq;
            }
            FromSystem::Update(u) => {
                if !self.ready() {
                    return Err(Error::Invalid {
                        kind: kind::UPDATE,
                        why: "an update before a whole frame",
                    });
                }
                u.check(self.rows, self.cols)?;
                let cols = usize::from(self.cols);
                for r in &u.runs {
                    let at = usize::from(r.row) * cols + usize::from(r.col);
                    self.cells[at..at + r.cells.len()].copy_from_slice(&r.cells);
                }
                self.status = u.status.clone();
                self.seq = u.seq;
            }
        }
        Ok(())
    }

    /// Row `r` as text: nondisplay cells as spaces.
    #[must_use]
    pub fn row_text(&self, r: usize) -> String {
        let cols = usize::from(self.cols);
        self.cells
            .get(r * cols..(r + 1) * cols)
            .unwrap_or_default()
            .iter()
            .map(|c| {
                if c.attr & attr::NONDISPLAY != 0 {
                    ' '
                } else {
                    c.ch
                }
            })
            .collect()
    }

    /// Every row as text, one line each.
    #[must_use]
    pub fn text(&self) -> String {
        let mut s = String::new();
        for r in 0..usize::from(self.rows) {
            s.push_str(&self.row_text(r));
            s.push('\n');
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Reader;

    fn grid_of(text: &[&str], cols: u8) -> Vec<Cell> {
        let mut v = Vec::new();
        for line in text {
            let mut row: Vec<Cell> = line
                .chars()
                .map(|ch| Cell {
                    ch,
                    attr: attr::GREEN,
                })
                .collect();
            row.resize(usize::from(cols), Cell::default());
            v.extend(row);
        }
        v
    }

    fn decode_one(bytes: &[u8]) -> FromSystem {
        let mut r = Reader::new();
        r.push(bytes);
        FromSystem::decode(&r.next_frame().unwrap())
            .unwrap()
            .unwrap()
    }

    #[test]
    fn a_frame_then_updates_keep_the_peer_equal() {
        let old = grid_of(&["Sign On", "", "User"], 20);
        let mut new = old.clone();
        new[20 * 2 + 10] = Cell {
            ch: 'Q',
            attr: attr::UNDERLINE,
        };
        new[20 * 2 + 12] = Cell {
            ch: 'S',
            attr: attr::UNDERLINE,
        };
        new[19] = Cell {
            ch: '*',
            attr: attr::WHITE | attr::HIGH,
        };
        let status = Status {
            row: 2,
            col: 13,
            ..Status::default()
        };
        let f = FromSystem::Frame(Frame {
            seq: 1,
            rows: 3,
            cols: 20,
            cells: old.clone(),
            status: Status::default(),
        });
        let rs = runs(&old, &new, 20);
        // The gap of one cell is bridged; the other row is its own run.
        assert_eq!(rs.len(), 2);
        assert_eq!((rs[1].row, rs[1].col, rs[1].cells.len()), (2, 10, 3));
        let u = FromSystem::Update(Update {
            seq: 2,
            runs: rs,
            status: status.clone(),
        });
        let mut g = Grid::default();
        g.apply(&decode_one(&f.encode().unwrap())).unwrap();
        assert_eq!(g.row_text(0).trim_end(), "Sign On");
        g.apply(&decode_one(&u.encode().unwrap())).unwrap();
        assert_eq!(g.cells, new);
        assert_eq!(g.status, status);
        assert_eq!(g.seq, 2);
        assert_eq!(runs(&new, &new, 20), Vec::new());
    }

    #[test]
    fn the_wide_display_fits_one_frame() {
        let cells = vec![
            Cell {
                ch: '\u{4E00}',
                attr: 0x07FF,
            };
            usize::from(MAX_ROWS) * usize::from(MAX_COLS)
        ];
        let f = FromSystem::Frame(Frame {
            seq: 0,
            rows: MAX_ROWS,
            cols: MAX_COLS,
            cells,
            status: Status {
                src: Some(SrcShown {
                    code: "C6001090".into(),
                    text: "IPL complete".into(),
                }),
                ..Status::default()
            },
        });
        let bytes = f.encode().unwrap();
        assert!(bytes.len() <= crate::frame::HEADER + crate::frame::MAX_BODY);
        assert_eq!(decode_one(&bytes), f);
    }

    #[test]
    fn keys_round_trip() {
        let keys = [
            Key::Char('a'),
            Key::Char('é'),
            Key::Enter,
            Key::FieldAdvance,
            Key::FieldBackspace,
            Key::FieldExit,
            Key::Backspace,
            Key::Home,
            Key::Insert,
            Key::Delete,
            Key::Function(24),
            Key::Attention,
            Key::SysReq,
            Key::Reset,
            Key::Left,
            Key::Right,
            Key::Up,
            Key::Down,
            Key::PageUp,
            Key::PageDown,
            Key::Other(200),
        ];
        for k in keys {
            let mut r = Reader::new();
            r.push(&ToSystem::Key(k).encode().unwrap());
            let got = ToSystem::decode(&r.next_frame().unwrap()).unwrap();
            assert_eq!(got, Some(ToSystem::Key(k)));
        }
        let mut r = Reader::new();
        r.push(&ToSystem::Refresh.encode().unwrap());
        assert_eq!(
            ToSystem::decode(&r.next_frame().unwrap()).unwrap(),
            Some(ToSystem::Refresh)
        );
    }

    #[test]
    fn hostile_frames_are_refused() {
        let bad = Frame {
            seq: 0,
            rows: 2,
            cols: 2,
            cells: vec![Cell::default(); 3],
            status: Status::default(),
        };
        assert!(bad.check().is_err());
        let off = Frame {
            cells: vec![Cell::default(); 4],
            status: Status {
                row: 2,
                ..Status::default()
            },
            ..bad.clone()
        };
        assert!(off.check().is_err());
        let mut g = Grid::default();
        let u = Update {
            seq: 1,
            runs: vec![Run {
                row: 0,
                col: 1,
                cells: vec![Cell::default(); 2],
            }],
            status: Status::default(),
        };
        assert!(g.apply(&FromSystem::Update(u.clone())).is_err());
        g.apply(&FromSystem::Frame(Frame {
            cells: vec![Cell::default(); 4],
            ..bad
        }))
        .unwrap();
        assert!(g.apply(&FromSystem::Update(u)).is_err());
    }
}
