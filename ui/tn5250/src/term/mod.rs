// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The terminal: what a 5250 display station keeps and does. It
//! applies outbound records to its cells and format table, runs the
//! keyboard locally (`keys`), answers reads, saves and queries, and
//! dumps itself as text (`dump`). The capture driver and the local
//! console are built on it.

mod dump;
mod keys;

pub use keys::{Key, KeyResult};

use crate::Error;
use crate::ds::build::Builder;
use crate::ds::field::Ffw;
use crate::ds::parse::{Command, Order, parse};
use crate::ds::wdsf::{Capabilities, Sf, kind, query_reply};
use crate::ds::{DUP, NULL, aid, attr, cc1, cc2};
use crate::record::{Record, opcode};

/// An input field of the format table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// The first cell, linear.
    pub start: usize,
    /// Cells.
    pub len: usize,
    /// The Field Format Word.
    pub ffw: Ffw,
    /// The Field Control Words.
    pub fcws: Vec<u16>,
}

impl Field {
    /// One past the last cell.
    pub fn end(&self) -> usize {
        self.start + self.len
    }

    /// Whether the address is inside the field.
    pub fn contains(&self, a: usize) -> bool {
        a >= self.start && a < self.end()
    }
}

/// The read the host has outstanding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    /// Read MDT Fields.
    Mdt,
    /// Read MDT Fields Alternate.
    MdtAlt,
    /// Read Input Fields.
    Input,
}

/// A window the host created as a GUI construct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    /// Top-left cell (the border's), linear.
    pub origin: usize,
    /// Rows inside.
    pub rows: usize,
    /// Columns inside.
    pub cols: usize,
}

/// A display station.
#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)] // the station's indicator lights
pub struct Terminal {
    pub(crate) rows: usize,
    pub(crate) cols: usize,
    pub(crate) cells: Vec<u8>,
    pub(crate) fields: Vec<Field>,
    pub(crate) cursor: usize,
    pub(crate) home: Option<usize>,
    pub(crate) insert: bool,
    pub(crate) locked: bool,
    pub(crate) error: Option<u16>,
    pub(crate) error_row: usize,
    pub(crate) read: Option<Read>,
    pub(crate) invited: bool,
    pub(crate) message_light: bool,
    pub(crate) alarms: u32,
    pub(crate) caps: Capabilities,
    pub(crate) outbox: Vec<Record>,
    pub(crate) windows: Vec<Window>,
    pub(crate) structured: Vec<Sf>,
    /// Lines the trace wants: what each record did.
    pub(crate) notes: Vec<String>,
    /// The start of header's three command key switch bytes.
    pub(crate) key_switches: [u8; 3],
}

impl Terminal {
    /// A powered-on station: 24×80, blank, keyboard locked until the
    /// host writes.
    pub fn new(caps: Capabilities) -> Self {
        Self {
            rows: 24,
            cols: 80,
            cells: vec![NULL; 24 * 80],
            fields: Vec::new(),
            cursor: 0,
            home: None,
            insert: false,
            locked: true,
            error: None,
            error_row: 23,
            read: None,
            invited: false,
            message_light: false,
            alarms: 0,
            caps,
            outbox: Vec::new(),
            windows: Vec::new(),
            structured: Vec::new(),
            notes: Vec::new(),
            key_switches: [0; 3],
        }
    }

    /// The command key switches of the last start of header: three
    /// bytes, F24 to F17, F16 to F9, F8 to F1.
    pub fn key_switches(&self) -> [u8; 3] {
        self.key_switches
    }

    /// Rows on the display now.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Columns on the display now.
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Whether the keyboard is locked waiting for the host.
    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// The operator error the keyboard is inhibited with, until Reset.
    pub fn error(&self) -> Option<u16> {
        self.error
    }

    /// Whether the host is waiting for input.
    pub fn is_invited(&self) -> bool {
        self.invited
    }

    /// Whether the message light is on.
    pub fn message_light(&self) -> bool {
        self.message_light
    }

    /// How many times the alarm sounded.
    pub fn alarms(&self) -> u32 {
        self.alarms
    }

    /// Cursor row and column, 1-based.
    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor / self.cols + 1, self.cursor % self.cols + 1)
    }

    /// The fields in screen order.
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// The structured fields received that were not answered here.
    pub fn structured(&self) -> &[Sf] {
        &self.structured
    }

    /// The windows created as GUI constructs.
    pub fn windows(&self) -> &[Window] {
        &self.windows
    }

    /// Records to send to the host, in order.
    pub fn take_outbox(&mut self) -> Vec<Record> {
        std::mem::take(&mut self.outbox)
    }

    /// Trace lines since the last call.
    pub fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    fn addr(&self, row: u8, col: u8) -> Result<usize, Error> {
        let (r, c) = (usize::from(row), usize::from(col));
        if r == 0 || c == 0 || r > self.rows || c > self.cols {
            return Err(Error::Address { row, col });
        }
        Ok((r - 1) * self.cols + (c - 1))
    }

    fn resize(&mut self, wide: bool) {
        let (rows, cols) = if wide { (27, 132) } else { (24, 80) };
        self.rows = rows;
        self.cols = cols;
        self.cells = vec![NULL; rows * cols];
        self.fields.clear();
        self.windows.clear();
        self.cursor = 0;
        self.home = None;
        self.error_row = rows - 1;
    }

    fn clear_unit(&mut self, wide: bool) {
        self.resize(wide);
        self.locked = true;
        self.error = None;
        self.read = None;
        self.insert = false;
    }

    /// The attribute in effect at a cell: the nearest attribute byte at
    /// or before it, wrapping from the top to the bottom of the screen.
    pub(crate) fn attr_at(&self, a: usize) -> u8 {
        let n = self.cells.len();
        for i in 0..n {
            let p = (a + n - i) % n;
            let b = self.cells[p];
            if attr::is_attr(b) {
                return b;
            }
        }
        attr::GREEN
    }

    /// The field holding an address.
    pub(crate) fn field_at(&self, a: usize) -> Option<usize> {
        self.fields.iter().position(|f| f.contains(a))
    }

    fn put_field(&mut self, start: usize, len: usize, ffw: Ffw, fcws: Vec<u16>) {
        let f = Field {
            start,
            len,
            ffw,
            fcws,
        };
        // A field written over an existing one replaces it; the table
        // stays in screen order.
        self.fields
            .retain(|g| !(g.start < f.end() && f.start < g.end()));
        let at = self.fields.partition_point(|g| g.start < f.start);
        self.fields.insert(at, f);
    }

    fn control1(&mut self, c: u8) {
        let high = c & 0xE0;
        let reset_mdt = matches!(high, cc1::RESET_MDT | cc1::RESET_NULL | cc1::RESET_NULL_MDT);
        let reset_all = matches!(high, cc1::RESET_ALL_MDT | cc1::RESET_ALL_NULL);
        let null_mdt = matches!(high, cc1::NULL_MDT | cc1::RESET_NULL_MDT);
        let null_all = matches!(high, cc1::RESET_NULL | cc1::RESET_ALL_NULL);
        for i in 0..self.fields.len() {
            let f = self.fields[i].clone();
            let bypass = f.ffw.bypass;
            if (null_all && !bypass) || (null_mdt && !bypass && f.ffw.mdt) {
                for a in f.start..f.end() {
                    self.cells[a] = NULL;
                }
            }
            if reset_all || (reset_mdt && !bypass) {
                self.fields[i].ffw.mdt = false;
            }
        }
        if high != 0 {
            self.locked = true;
        }
    }

    fn control2(&mut self, c: u8) {
        if c & cc2::MESSAGE_ON != 0 {
            self.message_light = true;
        } else if c & cc2::MESSAGE_OFF != 0 {
            self.message_light = false;
        }
        if c & cc2::ALARM != 0 {
            self.alarms += 1;
        }
        if c & cc2::UNLOCK != 0 {
            self.locked = false;
            self.cursor_to_first_field();
        }
    }

    fn write(&mut self, a: &mut usize, b: u8) {
        let n = self.cells.len();
        self.cells[*a % n] = b;
        *a = (*a + 1) % n;
    }

    fn orders(&mut self, orders: &[Order], error_line: bool) -> Result<(), Error> {
        let mut a = if error_line {
            self.error_row * self.cols
        } else {
            self.cursor
        };
        let mut pending_ic = None;
        for o in orders {
            match o {
                Order::Sba(r, c) => a = self.addr(*r, *c)?,
                Order::Ic(r, c) => pending_ic = Some(self.addr(*r, *c)?),
                Order::Mc(r, c) => self.cursor = self.addr(*r, *c)?,
                Order::Ra(r, c, byte) => {
                    // Up to and including the address, which may not
                    // be before the current one.
                    let to = self.addr(*r, *c)?;
                    if to < a {
                        return Err(Error::Address { row: *r, col: *c });
                    }
                    self.cells[a..=to].fill(*byte);
                    a = (to + 1) % self.cells.len();
                }
                Order::Ea(r, c, _) => {
                    // Characters and attributes become nulls; the
                    // terminal keeps no extended attributes to erase.
                    let to = self.addr(*r, *c)?;
                    if to < a {
                        return Err(Error::Address { row: *r, col: *c });
                    }
                    self.cells[a..=to].fill(NULL);
                    a = (to + 1) % self.cells.len();
                }
                Order::Td(bytes) | Order::Data(bytes) => {
                    for &b in bytes {
                        self.write(&mut a, b);
                    }
                }
                Order::Wea(..) => {}
                Order::Soh(bytes) => {
                    self.fields.clear();
                    self.locked = true;
                    self.error = None;
                    if let Some(&row) = bytes.get(3)
                        && row >= 1
                        && usize::from(row) <= self.rows
                    {
                        self.error_row = usize::from(row) - 1;
                    } else {
                        self.error_row = self.rows - 1;
                    }
                    for (k, b) in self.key_switches.iter_mut().enumerate() {
                        *b = bytes.get(4 + k).copied().unwrap_or(0);
                    }
                }
                Order::Sf {
                    ffw,
                    fcws,
                    attr: at,
                    len,
                } => {
                    self.write(&mut a, *at);
                    let start = a;
                    let ffw = Ffw::decode(*ffw);
                    let len = usize::from(*len).min(self.cells.len() - start);
                    self.put_field(start, len, ffw, fcws.clone());
                }
            }
        }
        if let Some(ic) = pending_ic {
            self.home = Some(ic);
            self.cursor = ic;
        }
        Ok(())
    }

    /// Apply one record from the host. The commands it held come back
    /// for the trace; answers it needs go to the outbox.
    pub fn apply(&mut self, rec: &Record) -> Result<Vec<Command>, Error> {
        let commands = parse(&rec.data)?;
        match rec.opcode {
            opcode::INVITE | opcode::PUT_GET => self.invited = true,
            opcode::OUTPUT_ONLY => self.invited = false,
            opcode::CANCEL_INVITE => {
                self.invited = false;
                self.outbox
                    .push(Record::new(opcode::CANCEL_INVITE, Vec::new()));
            }
            opcode::MESSAGE_ON => self.message_light = true,
            opcode::MESSAGE_OFF => self.message_light = false,
            _ => {}
        }
        for c in &commands {
            self.command(c)?;
        }
        Ok(commands)
    }

    fn command(&mut self, c: &Command) -> Result<(), Error> {
        match c {
            Command::ClearUnit => self.clear_unit(false),
            Command::ClearUnitAlt(p) => self.clear_unit(*p == 0 && self.caps.model.wide()),
            Command::ClearFormatTable => {
                self.fields.clear();
                self.locked = true;
                self.read = None;
            }
            Command::Wtd { cc1, cc2, orders } => {
                self.control1(*cc1);
                self.orders(orders, false)?;
                self.control2(*cc2);
            }
            Command::WriteErrorCode(orders) => {
                let row = self.error_row;
                for a in row * self.cols..(row + 1) * self.cols {
                    self.cells[a] = NULL;
                }
                self.orders(orders, true)?;
                self.error = Some(0);
                self.locked = false;
            }
            Command::WriteErrorCodeWindow(_, _, orders) => {
                self.orders(orders, true)?;
                self.error = Some(0);
                self.locked = false;
            }
            Command::ReadInputFields(c1, c2) => self.read_command(*c1, *c2, Read::Input),
            Command::ReadMdtFields(c1, c2) => self.read_command(*c1, *c2, Read::Mdt),
            Command::ReadMdtFieldsAlt(c1, c2) => self.read_command(*c1, *c2, Read::MdtAlt),
            Command::ReadScreen(_) => {
                let data = self.cells.clone();
                self.outbox.push(Record::new(opcode::NOP, data));
            }
            Command::ReadImmediate | Command::ReadImmediateAlt => {
                let data = self.inbound_data(0x00, Some(Read::Input));
                self.outbox.push(Record::new(opcode::NOP, data));
            }
            Command::SaveScreen | Command::SavePartialScreen(_) => {
                let data = self.save_data();
                self.outbox.push(Record::new(opcode::SAVE_SCREEN, data));
            }
            Command::RestoreScreen | Command::RestorePartialScreen | Command::CopyToPrinter(_) => {}
            Command::Roll {
                up,
                lines,
                top,
                bottom,
            } => self.roll(
                *up,
                usize::from(*lines),
                usize::from(*top),
                usize::from(*bottom),
            ),
            Command::Wsf(bytes) => {
                if let Some(sf) = Sf::decode(bytes) {
                    if sf.kind == kind::QUERY {
                        let data = query_reply(self.caps);
                        self.outbox.push(Record::new(opcode::NOP, data));
                    } else {
                        self.gui(&sf);
                    }
                }
            }
        }
        Ok(())
    }

    fn read_command(&mut self, c1: u8, c2: u8, read: Read) {
        self.control1(c1);
        self.control2(c2);
        self.read = Some(read);
        self.locked = false;
        self.invited = true;
        self.cursor_to_first_field();
    }

    /// Without an Insert Cursor order the cursor rests in the first
    /// input field when the keyboard opens.
    fn cursor_to_first_field(&mut self) {
        if self.home.is_none()
            && let Some(f) = self.fields.iter().find(|f| !f.ffw.bypass)
        {
            self.cursor = f.start;
        }
    }

    #[allow(clippy::many_single_char_names)] // rows, width, address
    fn roll(&mut self, up: bool, lines: usize, top: usize, bottom: usize) {
        if lines == 0 || top == 0 || bottom < top || bottom > self.rows {
            return;
        }
        let (t, b) = (top - 1, bottom);
        let w = self.cols;
        // t: first row, b: one past the last, w: width.
        let rows: Vec<Vec<u8>> = (t..b)
            .map(|r| self.cells[r * w..(r + 1) * w].to_vec())
            .collect();
        let n = rows.len();
        for (i, r) in (t..b).enumerate() {
            let src = if up { i + lines } else { i.wrapping_sub(lines) };
            let line: Vec<u8> = if src < n {
                rows[src].clone()
            } else {
                vec![NULL; w]
            };
            self.cells[r * w..(r + 1) * w].copy_from_slice(&line);
        }
        let shift = |a: usize| -> Option<usize> {
            let r = a / w;
            if r < t || r >= b {
                return Some(a);
            }
            let nr = if up { r.checked_sub(lines)? } else { r + lines };
            if nr < t || nr >= b {
                None
            } else {
                Some(nr * w + a % w)
            }
        };
        let fields = std::mem::take(&mut self.fields);
        self.fields = fields
            .into_iter()
            .filter_map(|mut f| {
                f.start = shift(f.start)?;
                Some(f)
            })
            .collect();
    }

    /// The Save Screen answer: commands that rebuild this screen.
    fn save_data(&self) -> Vec<u8> {
        let mut b = Builder::new();
        b.restore_screen();
        if self.cols == 132 {
            b.clear_unit_alt(0x00);
        } else {
            b.clear_unit();
        }
        b.wtd(0x00, 0x00);
        b.sba(1, 1);
        let mut a = 0;
        while a < self.cells.len() {
            if let Some(f) = self.fields.iter().find(|f| f.start == a + 1) {
                b.sf(f.ffw.encode(), &f.fcws, self.cells[a], f.len as u16);
                a += 1;
                continue;
            }
            b.data(&[self.cells[a]]);
            a += 1;
        }
        let (r, c) = self.cursor();
        b.ic(r as u8, c as u8);
        b.into_vec()
    }

    /// The data of an inbound record for an AID: cursor, AID, fields
    /// per the read outstanding.
    pub(crate) fn inbound_data(&self, aid: u8, read: Option<Read>) -> Vec<u8> {
        let (r, c) = self.cursor();
        let mut out = vec![r as u8, c as u8, aid];
        let Some(read) = read else {
            return out;
        };
        for f in &self.fields {
            if f.ffw.bypass {
                continue;
            }
            if matches!(read, Read::Mdt | Read::MdtAlt) && !f.ffw.mdt {
                continue;
            }
            let mut data = self.cells[f.start..f.end()].to_vec();
            match read {
                Read::Mdt | Read::MdtAlt => {
                    while data.last() == Some(&NULL) {
                        data.pop();
                    }
                    if read == Read::MdtAlt {
                        while data.last() == Some(&0x40) {
                            data.pop();
                        }
                    }
                }
                Read::Input => {
                    for b in &mut data {
                        if *b == NULL {
                            *b = 0x40;
                        }
                    }
                }
            }
            out.push(crate::ds::order::SBA);
            out.push((f.start / self.cols + 1) as u8);
            out.push((f.start % self.cols + 1) as u8);
            out.extend(data);
        }
        out
    }

    /// Send an AID with the fields the outstanding read wants. Locks
    /// the keyboard until the host answers.
    pub(crate) fn send_aid(&mut self, a: u8, with_fields: bool) {
        let read = if with_fields { self.read } else { None };
        let data = self.inbound_data(a, read);
        self.outbox.push(Record::new(opcode::NOP, data));
        self.locked = true;
        self.read = None;
        self.invited = false;
        self.insert = false;
    }

    /// Send a record with only flags (System Request, Attention, Test
    /// Request).
    pub(crate) fn send_flags(&mut self, flags: u16) {
        let (r, c) = self.cursor();
        let data = if flags == crate::record::flags::SRQ {
            vec![r as u8, c as u8, 0x00]
        } else {
            Vec::new()
        };
        self.outbox.push(Record {
            flags,
            opcode: opcode::NOP,
            data,
        });
        self.locked = true;
    }

    /// A GUI structured field: windows are drawn with their borders,
    /// the rest is kept for the trace.
    fn gui(&mut self, sf: &Sf) {
        match sf.kind {
            kind::CREATE_WINDOW => {
                // Flags, reserved, rows, columns, then minor structures;
                // the window's top-left is the current cursor.
                let rows = usize::from(*sf.body.get(2).unwrap_or(&0));
                let cols = usize::from(*sf.body.get(3).unwrap_or(&0));
                self.windows.push(Window {
                    origin: self.cursor,
                    rows,
                    cols,
                });
            }
            kind::REMOVE_WINDOW | kind::REMOVE_ALL_GUI => self.windows.clear(),
            _ => {}
        }
        self.structured.push(sf.clone());
    }

    /// The 1-based row and column of every field whose attribute is
    /// nondisplay: what a trace must not print.
    pub fn hidden_fields(&self) -> Vec<(u8, u8)> {
        self.fields
            .iter()
            .filter(|f| f.start > 0 && attr::is_nondisplay(self.attr_at(f.start - 1)))
            .map(|f| {
                (
                    (f.start / self.cols + 1) as u8,
                    (f.start % self.cols + 1) as u8,
                )
            })
            .collect()
    }

    /// Whether any cell shows this text (a row-local search).
    pub fn contains(&self, text: &str) -> bool {
        self.text_rows().iter().any(|r| r.contains(text))
    }

    /// Whether a dup character is in the cells.
    pub fn has_dup(&self) -> bool {
        self.cells.contains(&DUP)
    }

    /// The AID of `Fn`.
    pub fn function_aid(n: u8) -> u8 {
        aid::function(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ds::wdsf::Model;

    fn term() -> Terminal {
        Terminal::new(Capabilities {
            model: Model::M3179,
            gui: false,
            color: true,
        })
    }

    fn signon() -> Record {
        let mut b = Builder::new();
        b.clear_unit().wtd(0x00, 0x00).soh(&[0, 0, 0, 24, 0]);
        b.sba(1, 36).attr(attr::WHITE).text("Sign On");
        b.sba(6, 17).attr(attr::GREEN).text("User  . . . . . .");
        b.sba(6, 52)
            .sf(0x4020, &[], attr::GREEN_UL, 10)
            .ra(6, 62, 0x00);
        b.attr(attr::GREEN);
        b.sba(7, 52)
            .sf(0x4020, &[], attr::NONDISPLAY, 10)
            .ra(7, 62, 0x00);
        b.ic(6, 53).read_mdt_fields(0x00, 0x00);
        Record::new(opcode::PUT_GET, b.into_vec())
    }

    #[test]
    fn applies_a_sign_on_and_answers_enter() {
        let mut t = term();
        t.apply(&signon()).unwrap();
        assert!(!t.is_locked());
        assert_eq!(t.cursor(), (6, 53));
        assert_eq!(t.fields().len(), 2);
        let rows = t.text_rows();
        assert_eq!(&rows[0][36..43], "Sign On");
        assert_eq!(&rows[5][17..34], "User  . . . . . .");
        for ch in "QSECOFR".chars() {
            assert_eq!(t.key(Key::Char(ch)), KeyResult::Done);
        }
        assert_eq!(t.key(Key::Tab), KeyResult::Done);
        assert_eq!(t.cursor(), (7, 53));
        t.key(Key::Char('x'));
        assert_eq!(t.key(Key::Enter), KeyResult::Sent);
        let out = t.take_outbox();
        assert_eq!(out.len(), 1);
        let inb = crate::ds::inbound::Inbound::decode(&out[0].data).unwrap();
        assert_eq!(inb.aid, aid::ENTER);
        assert_eq!(inb.fields.len(), 2);
        assert_eq!(inb.field_text(6, 53).unwrap(), "QSECOFR");
        assert!(t.is_locked());
    }

    #[test]
    fn save_restores_the_same_screen() {
        let mut t = term();
        t.apply(&signon()).unwrap();
        t.key(Key::Char('A'));
        let before = t.text_rows();
        t.apply(&Record::new(opcode::SAVE_SCREEN, vec![0x04, 0x02]))
            .unwrap();
        let saved = t.take_outbox().remove(0);
        assert_eq!(saved.opcode, opcode::SAVE_SCREEN);
        let mut u = term();
        u.apply(&Record::new(opcode::RESTORE_SCREEN, saved.data))
            .unwrap();
        assert_eq!(u.text_rows(), before);
        assert_eq!(u.fields().len(), 2);
        assert!(u.fields()[0].ffw.mdt);
    }

    #[test]
    fn rolls_lines() {
        let mut t = term();
        let mut b = Builder::new();
        b.clear_unit()
            .wtd(0, 0)
            .sba(2, 1)
            .text("two")
            .sba(3, 1)
            .text("three");
        t.apply(&Record::new(opcode::OUTPUT_ONLY, b.into_vec()))
            .unwrap();
        t.apply(&Record::new(
            opcode::OUTPUT_ONLY,
            vec![0x04, 0x23, 0x81, 1, 24],
        ))
        .unwrap();
        let rows = t.text_rows();
        assert!(rows[0].starts_with("two"));
        assert!(rows[1].starts_with("three"));
    }

    #[test]
    fn a_wrapping_field_keeps_its_attribute_on_the_next_row() {
        // MAIN's command line as the server writes it: one SF of 153
        // at row 20 column 7.
        let mut m = crate::screen::ScreenModel::new(false);
        m.put(20, 2, "===>", attr::GREEN);
        m.field(20, 7, 153, attr::GREEN_UL, "", Ffw::default(), &[]);
        let out = m.render(crate::screen::Read::MdtFields);
        let mut t = term();
        t.apply(&Record::new(out.opcode, out.data)).unwrap();
        assert_eq!(t.fields().len(), 1);
        assert_eq!(t.fields()[0].len, 153);
        // The cells as pub400's MAIN shows them: the field attribute
        // through row 21 column 79, the ending attribute at column 80.
        let golden = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/conformance/golden/verified/pub400/main-menu.attr"),
        )
        .unwrap();
        let theirs: Vec<&str> = golden.lines().map(|l| l.trim_end_matches('\r')).collect();
        let ours = t.attr_rows();
        assert_eq!(ours[19], theirs[19]);
        assert_eq!(ours[20], theirs[20]);
        // Typing past row 20 goes on at row 21 column 1.
        for ch in "A".repeat(80).chars() {
            assert_eq!(t.key(Key::Char(ch)), KeyResult::Done);
        }
        assert_eq!(t.cursor(), (21, 7));
        assert!(t.text_rows()[20].starts_with("AAAAAA "));
    }

    #[test]
    fn repeats_and_erases_to_an_address() {
        let mut t = term();
        let mut b = Builder::new();
        b.clear_unit().wtd(0x00, 0x00);
        // Repeat across a row end, then erase part of it again.
        b.sba(2, 79).ra(3, 2, 0x5C);
        b.sba(5, 1).text("abcdef").sba(5, 3).ea(5, 4);
        t.apply(&Record::new(opcode::OUTPUT_ONLY, b.into_vec()))
            .unwrap();
        let rows = t.text_rows();
        assert_eq!(&rows[1][78..], "**");
        assert_eq!(&rows[2][..3], "** ");
        assert_eq!(&rows[4][..6], "ab  ef");
        assert_eq!(t.cells()[4 * 80 + 2..4 * 80 + 4], [NULL, NULL]);
        // An address before the current one is an error.
        let mut b = Builder::new();
        b.wtd(0x00, 0x00).sba(5, 10).ra(5, 2, 0x5C);
        assert!(
            t.apply(&Record::new(opcode::OUTPUT_ONLY, b.into_vec()))
                .is_err()
        );
        // Erase to Address takes one to four attribute types.
        let bad = [0x04, 0x11, 0x00, 0x00, 0x03, 0x05, 0x01, 0x01];
        assert!(
            t.apply(&Record::new(opcode::OUTPUT_ONLY, bad.to_vec()))
                .is_err()
        );
    }
}
