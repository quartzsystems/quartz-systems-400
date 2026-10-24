// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The screen model on the host side: what a job wants the display to
//! show, kept as cells and a format table, turned into the outbound
//! stream (a whole screen, or only the rows that changed since the
//! client last saw it) and updated from the inbound record. The DDS
//! runtime, UIM and the prompter all write through it.

use crate::ds::build::Builder;
use crate::ds::field::Ffw;
use crate::ds::inbound::Inbound;
use crate::ds::{NULL, aid, attr};

/// An input field of the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// Row, 1-based.
    pub row: usize,
    /// Column, 1-based, of the first cell.
    pub col: usize,
    /// Cells.
    pub len: usize,
    /// The field format word.
    pub ffw: Ffw,
    /// Field control words.
    pub fcws: Vec<u16>,
    /// The attribute in the cell before.
    pub attr: u8,
    /// The value, EBCDIC, exactly `len` bytes, nulls where empty.
    pub value: Vec<u8>,
    /// Modified by the user since the last read.
    pub modified: bool,
}

impl Field {
    /// The value as text, trailing blanks removed.
    pub fn text(&self) -> String {
        let s: String = self
            .value
            .iter()
            .map(|&b| if b == NULL { ' ' } else { ccsid::char_of(b) })
            .collect();
        s.trim_end().to_string()
    }

    fn start(&self, cols: usize) -> usize {
        (self.row - 1) * cols + (self.col - 1)
    }
}

/// What the display is asked to do after the write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    /// Read MDT Fields: wait for an attention key.
    MdtFields,
    /// No read: output only.
    None,
}

/// The host-side screen.
#[derive(Clone, Debug)]
pub struct ScreenModel {
    rows: usize,
    cols: usize,
    cells: Vec<u8>,
    fields: Vec<Field>,
    cursor: Option<(usize, usize)>,
    error_row: usize,
    /// The cells the client shows, after the last render.
    known: Option<Vec<u8>>,
    /// The client's keyboard is unlocked and a read is outstanding.
    reading: bool,
    /// Sound the alarm at the next render.
    alarm: bool,
    /// The command key switches the start of header carries: a bit on
    /// for each function key that returns no data (F24-F17, F16-F9,
    /// F8-F1).
    key_switches: [u8; 3],
}

/// What the model sends: the stream and the record's operation code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    /// The data stream.
    pub data: Vec<u8>,
    /// `PUT_GET` when a read follows, else `OUTPUT_ONLY`.
    pub opcode: u8,
}

impl ScreenModel {
    /// A blank model, 24×80 or 27×132.
    pub fn new(wide: bool) -> Self {
        let (rows, cols) = if wide { (27, 132) } else { (24, 80) };
        Self {
            rows,
            cols,
            cells: vec![NULL; rows * cols],
            fields: Vec::new(),
            cursor: None,
            error_row: rows,
            known: None,
            reading: false,
            alarm: false,
            key_switches: [0; 3],
        }
    }

    /// The function keys that return no field data (command attention
    /// keys): a changed set goes out with the next full render.
    pub fn set_no_data_keys(&mut self, keys: &[u8]) {
        let mut sw = [0u8; 3];
        for &n in keys.iter().filter(|n| (1..=24).contains(*n)) {
            let i = usize::from(2 - (n - 1) / 8);
            sw[i] |= 1 << ((n - 1) % 8);
        }
        if sw != self.key_switches {
            self.key_switches = sw;
            self.known = None;
        }
    }

    /// Rows.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Columns.
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Blank everything the job wants shown; the client's screen is
    /// cleared at the next render.
    pub fn clear(&mut self) {
        self.cells.iter_mut().for_each(|c| *c = NULL);
        self.fields.clear();
        self.cursor = None;
        self.error_row = self.rows;
    }

    /// Forget what the client shows: the next render sends everything.
    pub fn invalidate(&mut self) {
        self.known = None;
    }

    /// Sound the alarm with the next render.
    pub fn alarm(&mut self) {
        self.alarm = true;
    }

    /// The fields in screen order.
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// The fields' values, in order.
    pub fn values(&self) -> Vec<String> {
        self.fields.iter().map(Field::text).collect()
    }

    /// Where the cursor goes; the first input field when unset.
    pub fn set_cursor(&mut self, row: usize, col: usize) {
        self.cursor = Some((row, col));
    }

    /// The error line's row (1-based); the last row by default.
    pub fn set_error_row(&mut self, row: usize) {
        self.error_row = row.clamp(1, self.rows);
    }

    fn at(&self, row: usize, col: usize) -> Option<usize> {
        if row == 0 || col == 0 || row > self.rows || col > self.cols {
            return None;
        }
        Some((row - 1) * self.cols + (col - 1))
    }

    /// The cell before `(row, col)`, wrapping to the previous row's end.
    fn before(&self, row: usize, col: usize) -> Option<usize> {
        if col > 1 {
            self.at(row, col - 1)
        } else if row > 1 {
            self.at(row - 1, self.cols)
        } else {
            None
        }
    }

    /// Text at a position with its leading attribute in the cell before
    /// and a return to normal after, unless another attribute follows.
    pub fn put(&mut self, row: usize, col: usize, text: &str, a: u8) {
        let Some(start) = self.at(row, col) else {
            return;
        };
        if let Some(b) = self.before(row, col) {
            self.cells[b] = a;
        }
        let mut i = start;
        let end = (row * self.cols).min(self.cells.len());
        for ch in text.chars() {
            if i >= end {
                break;
            }
            self.cells[i] = ccsid::byte_of(ch).unwrap_or(0x6F);
            i += 1;
        }
        if i < end && !attr::is_attr(self.cells[i]) && !self.field_at(i) {
            self.cells[i] = attr::GREEN;
        }
        self.remove_fields_in(start, i);
    }

    /// Raw bytes at a position (attributes included), no framing.
    pub fn put_bytes(&mut self, row: usize, col: usize, bytes: &[u8]) {
        let Some(start) = self.at(row, col) else {
            return;
        };
        for (k, &b) in bytes.iter().enumerate() {
            if start + k < self.cells.len() {
                self.cells[start + k] = b;
            }
        }
        self.remove_fields_in(start, start + bytes.len());
    }

    fn field_at(&self, cell: usize) -> bool {
        self.fields.iter().any(|f| {
            let s = f.start(self.cols);
            cell >= s && cell < s + f.len
        })
    }

    fn remove_fields_in(&mut self, start: usize, end: usize) {
        let cols = self.cols;
        self.fields.retain(|f| {
            let s = f.start(cols);
            s + f.len <= start || s >= end
        });
    }

    /// An input field: its leading attribute goes in the cell before,
    /// its value fills it, normal follows. Replaces any field there.
    #[allow(clippy::too_many_arguments)] // the field's words
    pub fn field(
        &mut self,
        row: usize,
        col: usize,
        len: usize,
        a: u8,
        value: &str,
        ffw: Ffw,
        fcws: &[u16],
    ) {
        let Some(start) = self.at(row, col) else {
            return;
        };
        let len = len.min(self.cells.len() - start).max(1);
        self.remove_fields_in(start, start + len);
        if let Some(b) = self.before(row, col) {
            self.cells[b] = a;
        }
        let mut v = vec![NULL; len];
        for (k, ch) in value.chars().take(len).enumerate() {
            v[k] = ccsid::byte_of(ch).unwrap_or(0x6F);
        }
        while v.last() == Some(&0x40) {
            v.pop();
        }
        v.resize(len, NULL);
        self.cells[start..start + len].copy_from_slice(&v);
        let after = start + len;
        if after < self.cells.len() && !attr::is_attr(self.cells[after]) && !self.field_at(after) {
            self.cells[after] = attr::GREEN;
        }
        let f = Field {
            row,
            col,
            len,
            ffw,
            fcws: fcws.to_vec(),
            attr: a,
            value: v,
            modified: false,
        };
        let at = self.fields.partition_point(|g| g.start(self.cols) < start);
        self.fields.insert(at, f);
    }

    /// Blank rows `from` to `to` (1-based, inclusive) and drop the
    /// fields in them.
    pub fn clear_rows(&mut self, from: usize, to: usize) {
        let from = from.max(1);
        let to = to.min(self.rows);
        if from > to {
            return;
        }
        let start = (from - 1) * self.cols;
        let end = to * self.cols;
        self.cells[start..end].iter_mut().for_each(|c| *c = NULL);
        self.remove_fields_in(start, end);
        if self
            .cursor
            .is_some_and(|(r, _)| r >= from && r <= to)
        {
            self.cursor = None;
        }
    }

    /// Forget which fields the operator changed.
    pub fn reset_mdt(&mut self) {
        for f in &mut self.fields {
            f.modified = false;
        }
    }

    /// The index of the field starting at `(row, col)`.
    #[must_use]
    pub fn field_index_at(&self, row: usize, col: usize) -> Option<usize> {
        self.fields.iter().position(|f| f.row == row && f.col == col)
    }

    /// Protect or free a field by index.
    pub fn set_bypass(&mut self, index: usize, bypass: bool) {
        if let Some(f) = self.fields.get_mut(index) {
            f.ffw.bypass = bypass;
        }
    }

    /// The cursor, when the job set it.
    #[must_use]
    pub fn cursor(&self) -> Option<(usize, usize)> {
        self.cursor
    }

    /// Change a field's value (the job's own write) by index.
    pub fn set_value(&mut self, index: usize, value: &str) {
        let cols = self.cols;
        let Some(f) = self.fields.get_mut(index) else {
            return;
        };
        let mut v = vec![NULL; f.len];
        for (k, ch) in value.chars().take(f.len).enumerate() {
            v[k] = ccsid::byte_of(ch).unwrap_or(0x6F);
        }
        while v.last() == Some(&0x40) {
            v.pop();
        }
        v.resize(f.len, NULL);
        let s = f.start(cols);
        f.value.clone_from(&v);
        f.modified = false;
        self.cells[s..s + f.len].copy_from_slice(&v);
    }

    /// Change a field's attribute (reverse image for an error).
    pub fn set_attr(&mut self, index: usize, a: u8) {
        let (row, col) = match self.fields.get(index) {
            Some(f) => (f.row, f.col),
            None => return,
        };
        self.fields[index].attr = a;
        if let Some(b) = self.before(row, col) {
            self.cells[b] = a;
        }
    }

    /// Text on the error line: the row is cleared first.
    pub fn error_line(&mut self, text: &str, a: u8) {
        let row = self.error_row;
        let Some(start) = self.at(row, 1) else {
            return;
        };
        for i in start..start + self.cols {
            self.cells[i] = NULL;
        }
        self.remove_fields_in(start, start + self.cols);
        self.put(row, 2, text, a);
    }

    /// The stream that brings the client to this screen: everything
    /// after a clear when the client's screen is unknown, else the
    /// rows that differ. With `read` the keyboard opens for an
    /// attention key.
    pub fn render(&mut self, read: Read) -> Output {
        let mut b = Builder::new();
        let full = self
            .known
            .as_ref()
            .is_none_or(|k| k.len() != self.cells.len());
        let changed: Vec<usize> = (0..self.rows)
            .filter(|&r| {
                full || self.known.as_ref().is_some_and(|k| {
                    k[r * self.cols..(r + 1) * self.cols]
                        != self.cells[r * self.cols..(r + 1) * self.cols]
                })
            })
            .collect();
        if full {
            if self.cols == 132 {
                b.clear_unit_alt(0x00);
            } else {
                b.clear_unit();
            }
        }
        let cc1 = if full { 0x00 } else { 0x40 };
        let mut cc2 = 0x00u8;
        if self.alarm {
            cc2 |= crate::ds::cc2::ALARM;
            self.alarm = false;
        }
        b.wtd(cc1, cc2);
        if full {
            let err = self.error_row as u8;
            let k = self.key_switches;
            b.soh(&[0, 0, 0, err, k[0], k[1], k[2]]);
        }
        for r in changed {
            self.render_row(&mut b, r);
        }
        let (cr, cc) = self.cursor.unwrap_or_else(|| {
            self.fields
                .iter()
                .find(|f| !f.ffw.bypass)
                .map_or((1, 1), |f| (f.row, f.col))
        });
        b.ic(cr as u8, cc as u8);
        let opcode = match read {
            Read::MdtFields => {
                b.read_mdt_fields(0x00, 0x00);
                self.reading = true;
                crate::record::opcode::PUT_GET
            }
            Read::None => {
                self.reading = false;
                crate::record::opcode::OUTPUT_ONLY
            }
        };
        self.known = Some(self.cells.clone());
        for f in &mut self.fields {
            f.modified = false;
        }
        Output {
            data: b.into_vec(),
            opcode,
        }
    }

    /// One row: Set Buffer Address, then runs of data with Start of
    /// Field orders where fields begin, and Repeat to Address for the
    /// trailing nulls.
    fn render_row(&self, b: &mut Builder, r: usize) {
        let start = r * self.cols;
        let row = &self.cells[start..start + self.cols];
        let last = row.iter().rposition(|&c| c != NULL).map_or(0, |p| p + 1);
        b.sba(r as u8 + 1, 1);
        let mut i = 0;
        let mut run = Vec::new();
        while i < last {
            let cell = start + i;
            if let Some(f) = self.fields.iter().find(|f| f.start(self.cols) == cell + 1) {
                if !run.is_empty() {
                    b.data(&run);
                    run.clear();
                }
                b.sf(f.ffw.encode(), &f.fcws, row[i], f.len as u16);
                i += 1;
                continue;
            }
            run.push(row[i]);
            i += 1;
        }
        if !run.is_empty() {
            b.data(&run);
        }
        if last < self.cols {
            b.ra(r as u8 + 1, self.cols as u8, NULL);
        }
    }

    /// Take what the client sent: the fields' values are updated; the
    /// attention identifier comes back. Returns `None` when the record
    /// is not an inbound (no cursor and AID).
    pub fn accept(&mut self, data: &[u8]) -> Option<Inbound> {
        let inb = Inbound::decode(data)?;
        self.reading = false;
        let cols = self.cols;
        for sent in &inb.fields {
            let Some(f) = self
                .fields
                .iter_mut()
                .find(|f| f.row == usize::from(sent.row) && f.col == usize::from(sent.col))
            else {
                continue;
            };
            let mut v = sent.data.clone();
            v.truncate(f.len);
            v.resize(f.len, NULL);
            let s = f.start(cols);
            f.value.clone_from(&v);
            f.modified = true;
            self.cells[s..s + f.len].copy_from_slice(&v);
            if let Some(k) = self.known.as_mut() {
                k[s..s + f.len].copy_from_slice(&v);
            }
        }
        if inb.fields.is_empty()
            && let Some(k) = self.known.as_mut()
        {
            // A key that brings no data (a command attention key) may
            // leave typed text the model never saw: those rows go out
            // again with the next render.
            for f in self.fields.iter().filter(|f| !f.ffw.bypass) {
                let s = f.start(cols);
                k[s..s + f.len].fill(0xFF);
            }
        }
        self.cursor = None;
        Some(inb)
    }

    /// The function key number of an attention identifier: 0 for Enter.
    pub fn key_of(a: u8) -> Option<u8> {
        match a {
            aid::ENTER | aid::AUTO_ENTER => Some(0),
            _ => aid::function_of(a),
        }
    }

    /// Whether a read is outstanding at the client.
    pub fn is_reading(&self) -> bool {
        self.reading
    }

    /// The cells, for tests and dumps.
    pub fn cells(&self) -> &[u8] {
        &self.cells
    }

    /// The text grid, attributes and nulls as blanks.
    pub fn text_rows(&self) -> Vec<String> {
        (0..self.rows)
            .map(|r| {
                self.cells[r * self.cols..(r + 1) * self.cols]
                    .iter()
                    .map(|&b| {
                        if b == NULL || attr::is_attr(b) {
                            ' '
                        } else {
                            ccsid::char_of(b)
                        }
                    })
                    .collect()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ds::wdsf::{Capabilities, Model};
    use crate::record::Record;
    use crate::term::{Key, Terminal};

    fn terminal() -> Terminal {
        Terminal::new(Capabilities {
            model: Model::M3179,
            gui: false,
            color: true,
        })
    }

    #[test]
    fn round_trips_through_a_terminal() {
        let mut m = ScreenModel::new(false);
        m.put(1, 36, "Sign On", attr::WHITE);
        m.put(6, 17, "User  . . . . . .", attr::GREEN);
        m.field(6, 53, 10, attr::GREEN_UL, "", Ffw::default(), &[]);
        m.field(7, 53, 10, attr::NONDISPLAY, "", Ffw::default(), &[]);
        let out = m.render(Read::MdtFields);
        let mut t = terminal();
        t.apply(&Record::new(out.opcode, out.data)).unwrap();
        assert_eq!(&t.text_rows()[0][35..42], "Sign On");
        assert_eq!(t.fields().len(), 2);
        assert_eq!(t.cursor(), (6, 53));
        t.type_str("QSECOFR");
        t.key(Key::Tab);
        t.type_str("pw");
        t.key(Key::Enter);
        let rec = t.take_outbox().remove(0);
        let inb = m.accept(&rec.data).unwrap();
        assert_eq!(inb.aid, aid::ENTER);
        assert_eq!(m.values(), vec!["QSECOFR".to_string(), "pw".to_string()]);
        assert_eq!(ScreenModel::key_of(inb.aid), Some(0));
    }

    #[test]
    fn second_render_sends_only_changed_rows() {
        let mut m = ScreenModel::new(false);
        m.put(1, 2, "one", attr::GREEN);
        m.put(5, 2, "five", attr::GREEN);
        let first = m.render(Read::None);
        assert!(first.data.starts_with(&[0x04, 0x40]));
        m.put(5, 2, "FIVE", attr::WHITE);
        let second = m.render(Read::MdtFields);
        assert!(!second.data.starts_with(&[0x04, 0x40]));
        let mut t = terminal();
        t.apply(&Record::new(first.opcode, first.data)).unwrap();
        t.apply(&Record::new(second.opcode, second.data)).unwrap();
        let rows = t.text_rows();
        assert!(rows[0].starts_with(" one"));
        assert!(rows[4].starts_with(" FIVE"));
    }

    #[test]
    fn a_field_spans_rows() {
        let mut m = ScreenModel::new(false);
        m.put(18, 2, "===>", attr::WHITE);
        m.field(18, 7, 313, attr::GREEN_UL, "", Ffw::default(), &[]);
        let out = m.render(Read::MdtFields);
        let mut t = terminal();
        t.apply(&Record::new(out.opcode, out.data)).unwrap();
        assert_eq!(t.fields().len(), 1);
        assert_eq!(t.fields()[0].len, 313);
        let long = "x".repeat(100);
        t.type_str(&long);
        assert_eq!(t.cursor(), (19, 27));
        t.key(Key::Enter);
        let rec = t.take_outbox().remove(0);
        m.accept(&rec.data).unwrap();
        assert_eq!(m.values()[0], long);
    }

    #[test]
    fn field_at_column_one() {
        let mut m = ScreenModel::new(false);
        m.field(3, 1, 5, attr::GREEN_UL, "abc", Ffw::default(), &[]);
        let out = m.render(Read::MdtFields);
        let mut t = terminal();
        t.apply(&Record::new(out.opcode, out.data)).unwrap();
        assert_eq!(t.fields().len(), 1);
        assert_eq!(t.field_text(0).unwrap(), "abc  ");
    }
}
