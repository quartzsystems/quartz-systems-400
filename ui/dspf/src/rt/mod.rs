// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The display-file runtime: a program's `WRITE` of a record format
//! placed on a `tn5250` screen model, and its `READ` built from what
//! the display sent back. The job owns the model and the device and
//! moves the stream; this module knows the display file: option
//! indicators, overlay and clearing, subfiles and their pages, keys,
//! editing, checks and response indicators.

pub mod edit;
pub mod input;
pub mod render;

use std::collections::BTreeMap;

use tn5250::ds::attr;
use tn5250::screen::ScreenModel;

pub use input::{Input, KeyPressed, Problem};
pub use render::{DateOrder, Env, Look};
use render::{cond_on, constant_text, ffw_of, fit, look_of, value_text};

use crate::model::{
    Clrl, DisplayFile, Field, KeyName, Record, RecordKind, SflEnd, SflRole, Slno, Usage,
    WindowOrigin,
};

/// Why a write could not be done.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RtError {
    /// No such record format in the file.
    #[error("record format {0} is not in the display file")]
    NoFormat(String),
    /// A subfile control record whose subfile is missing.
    #[error("subfile record {0} is not in the display file")]
    NoSubfile(String),
}

/// Indicator states, 1 to 99.
type Inds = [bool; 100];

/// A record on the display.
#[derive(Clone, Debug)]
struct Shown {
    name: String,
    lo: usize,
    hi: usize,
    row_shift: i32,
    col_shift: i32,
}

/// A subfile's records and page.
#[derive(Clone, Debug, Default)]
struct Sfl {
    /// Records by relative record number minus one.
    records: Vec<Option<Vec<u8>>>,
    /// Index of the first record on the page.
    top: usize,
    /// The last relative record number written.
    next_rrn: usize,
    /// The page is on the display.
    displayed: bool,
}

/// One open display file's state on one display.
#[derive(Clone, Debug)]
pub struct Runtime {
    df: DisplayFile,
    size: usize,
    shown: Vec<Shown>,
    last_out: BTreeMap<String, Vec<u8>>,
    last_inds: BTreeMap<String, Inds>,
    subfiles: BTreeMap<String, Sfl>,
    reset_inds: Vec<u8>,
}

impl Runtime {
    /// Over a compiled file, for a 24x80 or a 27x132 display: the
    /// file's size for that display when it has one, else its primary.
    #[must_use]
    pub fn new(df: DisplayFile, wide: bool) -> Self {
        let want = if wide { 132 } else { 80 };
        let size = df
            .file
            .sizes
            .iter()
            .position(|s| s.cols == want)
            .unwrap_or(0);
        Self {
            df,
            size,
            shown: Vec::new(),
            last_out: BTreeMap::new(),
            last_inds: BTreeMap::new(),
            subfiles: BTreeMap::new(),
            reset_inds: Vec::new(),
        }
    }

    /// The file.
    #[must_use]
    pub fn file(&self) -> &DisplayFile {
        &self.df
    }

    /// The display size in use, as an index into the file's sizes.
    #[must_use]
    pub fn size_index(&self) -> usize {
        self.size
    }

    fn size_name(&self) -> String {
        self.df
            .file
            .sizes
            .get(self.size)
            .map_or_else(|| "*DS3".into(), |s| s.name.clone())
    }

    /// The option indicators a write carries: the separate area with
    /// `INDARA`, else the bytes at the front of the output buffer.
    fn inds_of(&self, rec: &Record, out: &[u8], indara: &[u8]) -> Inds {
        let mut inds = [false; 100];
        let on = |b: u8| matches!(b, 0xF1 | b'1' | 1);
        if self.df.file.indara {
            for (k, b) in indara.iter().take(99).enumerate() {
                inds[k + 1] = on(*b);
            }
        } else {
            for (k, n) in rec.layout.output.indicators.iter().enumerate() {
                if let Some(b) = out.get(k) {
                    inds[usize::from(*n)] = on(*b);
                }
            }
        }
        inds
    }

    /// Whether a record-level keyword is in effect.
    fn kw_on(&self, rec: &Record, name: &str, inds: &Inds) -> bool {
        let size = self.size_name();
        rec.keywords
            .iter()
            .any(|k| k.name == name && cond_on(&k.cond, inds, &size))
    }

    /// The `CHGINPDFT` in force for a field: the field's, the record's,
    /// the file's.
    fn chginpdft_for(&self, rec: &Record, f: &Field) -> Option<Vec<String>> {
        let words = |kws: &[crate::model::KeywordUse]| {
            kws.iter().find(|k| k.name == "CHGINPDFT").map(|k| {
                k.parms
                    .iter()
                    .filter_map(|p| p.as_word().map(str::to_string))
                    .collect::<Vec<_>>()
            })
        };
        words(&f.keywords)
            .or_else(|| words(&rec.keywords))
            .or_else(|| self.df.file.chginpdft.clone())
    }

    /// Show a record format from a program's output buffer.
    pub fn write(
        &mut self,
        m: &mut ScreenModel,
        env: &Env,
        format: &str,
        out: &[u8],
        indara: &[u8],
    ) -> Result<(), RtError> {
        let rec = self
            .df
            .record(format)
            .cloned()
            .ok_or_else(|| RtError::NoFormat(format.to_string()))?;
        let inds = self.inds_of(&rec, out, indara);
        self.last_out.insert(rec.name.clone(), out.to_vec());
        self.last_inds.insert(rec.name.clone(), inds);
        if rec.kind == RecordKind::Subfile {
            self.put_sfl_record(&rec, out);
            return Ok(());
        }
        let rows = m.rows();
        let msgloc = rec
            .keyword("MSGLOC")
            .and_then(|k| k.parms.first())
            .and_then(|p| p.as_word())
            .and_then(|w| w.parse::<usize>().ok())
            .or_else(|| self.df.file.msgloc.map(usize::from))
            .unwrap_or(rows);
        m.set_error_row(msgloc);
        let overlay = self.kw_on(&rec, "OVERLAY", &inds) || self.kw_on(&rec, "PUTOVR", &inds);
        let (row_shift, col_shift) = self.shifts(&rec, out);
        let extent = rec.extent(self.size).map(|(lo, hi)| {
            (
                shifted(usize::from(lo), row_shift),
                shifted(usize::from(hi), row_shift),
            )
        });
        self.clear_for(m, &rec, &inds, overlay, extent);
        if self.kw_on(&rec, "ERASEINP", &inds) {
            for i in 0..m.fields().len() {
                m.set_value(i, "");
            }
        }
        if self.kw_on(&rec, "PROTECT", &inds) {
            for i in 0..m.fields().len() {
                m.set_bypass(i, true);
            }
        }
        if self.kw_on(&rec, "ALARM", &inds) {
            m.alarm();
        }
        if let Some(w) = &rec.window {
            draw_border(m, w, row_shift, col_shift);
        }
        if rec.kind == RecordKind::SubfileControl {
            self.write_ctl(m, env, &rec, &inds, out)?;
        } else {
            self.place(m, env, &rec, &inds, Some(out), row_shift, col_shift);
        }
        self.cursor_for(m, &rec, &inds, out);
        if let Some((lo, hi)) = extent {
            self.shown.retain(|s| s.name != rec.name);
            self.shown.push(Shown {
                name: rec.name.clone(),
                lo,
                hi,
                row_shift,
                col_shift,
            });
        }
        Ok(())
    }

    /// Make the display ready for a record: everything goes without
    /// `OVERLAY`; with it, `ERASE`d records, `CLRL` lines or the
    /// records the new one overlaps.
    fn clear_for(
        &mut self,
        m: &mut ScreenModel,
        rec: &Record,
        inds: &Inds,
        overlay: bool,
        extent: Option<(usize, usize)>,
    ) {
        if !overlay {
            m.clear();
            self.shown.clear();
            for s in self.subfiles.values_mut() {
                s.displayed = false;
            }
            return;
        }
        let erased: Vec<String> = rec
            .keywords
            .iter()
            .filter(|k| k.name == "ERASE" && cond_on(&k.cond, inds, &self.size_name()))
            .flat_map(|k| k.parms.iter().filter_map(|p| p.as_word().map(str::to_string)))
            .collect();
        for name in erased {
            self.remove_shown(m, &name);
        }
        let rows = m.rows();
        match (rec.clrl, extent) {
            (Some(Clrl::No), _) | (_, None) => {}
            (Some(Clrl::Lines(n)), Some((lo, _))) => {
                self.clear_rows(m, lo, lo + usize::from(n).saturating_sub(1));
            }
            (Some(Clrl::End), Some((lo, _))) => self.clear_rows(m, lo, rows),
            (Some(Clrl::All), _) => {
                m.clear();
                self.shown.clear();
            }
            (None, Some((lo, hi))) => {
                let overlapped: Vec<String> = self
                    .shown
                    .iter()
                    .filter(|s| s.lo <= hi && s.hi >= lo && s.name != rec.name)
                    .map(|s| s.name.clone())
                    .collect();
                for name in overlapped {
                    self.remove_shown(m, &name);
                }
                if rec.window.is_none() {
                    self.clear_rows(m, lo, hi);
                }
            }
        }
    }

    /// `CSRLOC(row col)`: the cursor from two hidden fields.
    fn cursor_for(&self, m: &mut ScreenModel, rec: &Record, inds: &Inds, out: &[u8]) {
        let Some(k) = rec
            .keywords
            .iter()
            .find(|k| k.name == "CSRLOC" && cond_on(&k.cond, inds, &self.size_name()))
        else {
            return;
        };
        let value = |name: Option<&str>| {
            name.and_then(|n| rec.field(n))
                .and_then(|(i, _)| field_bytes(rec, i, out))
                .and_then(|b| ccsid::from_ebcdic(&b).trim().parse::<usize>().ok())
        };
        let r = value(k.parms.first().and_then(|p| p.as_word()));
        let c = value(k.parms.get(1).and_then(|p| p.as_word()));
        if let (Some(r), Some(c)) = (r, c) {
            m.set_cursor(r, c);
        }
    }

    /// `SLNO` and `WINDOW` move a record's fields.
    fn shifts(&self, rec: &Record, out: &[u8]) -> (i32, i32) {
        let mut row = 0i32;
        let mut col = 0i32;
        match rec.slno {
            Some(Slno::Line(n)) => row = i32::from(n) - 1,
            Some(Slno::Var) => {
                // The starting line is the first two bytes of the buffer.
                if let Some(b) = out.get(..2)
                    && let Ok(n) = ccsid::from_ebcdic(b).trim().parse::<i32>()
                {
                    row = n - 1;
                }
            }
            None => {}
        }
        if let Some(w) = &rec.window {
            let origin = match &w.origin {
                WindowOrigin::At(r, c) => Some((i32::from(*r), i32::from(*c))),
                WindowOrigin::Vars(rv, cv) => {
                    let get = |name: &str| {
                        rec.field(name)
                            .and_then(|(i, _)| field_bytes(rec, i, out))
                            .and_then(|b| ccsid::from_ebcdic(&b).trim().parse::<i32>().ok())
                    };
                    Some((get(rv).unwrap_or(1), get(cv).unwrap_or(1)))
                }
                WindowOrigin::From(name) => self
                    .shown
                    .iter()
                    .find(|s| &s.name == name)
                    .map(|s| (s.row_shift + 1, s.col_shift + 1)),
                WindowOrigin::Default => Some((1, 1)),
            };
            if let Some((r, c)) = origin {
                // Fields are placed relative to the window's inside.
                row += r;
                col += c;
            }
        }
        (row, col)
    }

    /// Place a record's displayed fields and constants.
    #[allow(clippy::too_many_arguments)]
    fn place(
        &mut self,
        m: &mut ScreenModel,
        env: &Env,
        rec: &Record,
        inds: &Inds,
        out: Option<&[u8]>,
        row_shift: i32,
        col_shift: i32,
    ) {
        let size = self.size_name();
        for (i, f) in rec.fields.iter().enumerate() {
            if !f.is_displayed() || !cond_on(&f.cond, inds, &size) {
                continue;
            }
            let Some(p) = f.position(self.size) else {
                continue;
            };
            let row = shifted(usize::from(p.row), row_shift);
            let col = shifted(usize::from(p.col), col_shift);
            let text = if let Some(c) = &f.constant {
                constant_text(c, f, env)
            } else {
                let bytes = out.and_then(|o| field_bytes(rec, i, o));
                value_text(f, bytes.as_deref())
            };
            let look = look_of(f, inds, &size, self.chginpdft_for(rec, f).as_deref());
            let width = f.screen_len.max(1) as usize;
            if f.usage.is_input() {
                let ffw = ffw_of(f, look);
                m.field(row, col, width, look.attr, &text, ffw, &[]);
            } else {
                m.put(row, col, &fit(&text, width), look.attr);
            }
            if look.cursor {
                m.set_cursor(row, col);
            }
            for e in &f.errmsg {
                if !cond_on(&e.cond, inds, &size) {
                    continue;
                }
                let text = e
                    .text
                    .clone()
                    .or_else(|| e.id.as_ref().and_then(|(id, _)| env.messages.get(id).cloned()))
                    .unwrap_or_default();
                m.error_line(&text, attr::WHITE);
                if let Some(ix) = m.field_index_at(row, col) {
                    m.set_attr(ix, look.attr | 0x01);
                    m.set_cursor(row, col);
                }
                m.alarm();
                if let Some(ind) = e.ind {
                    self.reset_inds.push(ind);
                }
            }
        }
    }

    /// Take a record off the display: its rows are blanked.
    fn remove_shown(&mut self, m: &mut ScreenModel, name: &str) {
        if let Some(pos) = self.shown.iter().position(|s| s.name == name) {
            let s = self.shown.remove(pos);
            m.clear_rows(s.lo, s.hi);
        }
    }

    /// Blank rows and forget the records wholly inside them.
    fn clear_rows(&mut self, m: &mut ScreenModel, lo: usize, hi: usize) {
        m.clear_rows(lo, hi);
        self.shown.retain(|s| !(s.lo >= lo && s.hi <= hi));
    }

    /// A subfile record written: the next relative record number.
    fn put_sfl_record(&mut self, rec: &Record, out: &[u8]) {
        let st = self.subfiles.entry(rec.name.clone()).or_default();
        let rrn = st.next_rrn + 1;
        if st.records.len() < rrn {
            st.records.resize(rrn, None);
        }
        st.records[rrn - 1] = Some(out.to_vec());
        st.next_rrn = rrn;
    }

    /// Position the next subfile record write at `rrn` (1-based): a
    /// program's `WRITE` with a relative record number, or `UPDATE`.
    pub fn set_sfl_rrn(&mut self, sfl: &str, rrn: usize) {
        let st = self.subfiles.entry(sfl.to_string()).or_default();
        st.next_rrn = rrn.saturating_sub(1);
    }

    /// The records of a subfile, by relative record number minus one.
    #[must_use]
    pub fn sfl_records(&self, sfl: &str) -> Vec<Option<Vec<u8>>> {
        self.subfiles
            .get(sfl)
            .map(|s| s.records.clone())
            .unwrap_or_default()
    }

    /// A blank subfile record: zeros in numbers, blanks elsewhere.
    fn blank_record(rec: &Record) -> Vec<u8> {
        let mut out = vec![0xF0u8; rec.layout.output.indicators.len()];
        for bf in &rec.layout.output.fields {
            let f = &rec.fields[bf.field];
            if f.ty.is_numeric() {
                out.extend(edit::zero_bytes(f));
            } else {
                out.extend(std::iter::repeat_n(ccsid::SPACE, bf.len as usize));
            }
        }
        out.resize(rec.layout.output.len as usize, ccsid::SPACE);
        out
    }

    /// Write a subfile control record: clear, initialize, show the
    /// page, show the control fields.
    fn write_ctl(
        &mut self,
        m: &mut ScreenModel,
        env: &Env,
        rec: &Record,
        inds: &Inds,
        out: &[u8],
    ) -> Result<(), RtError> {
        let sfl = rec
            .subfile
            .clone()
            .ok_or_else(|| RtError::NoSubfile(rec.name.clone()))?;
        let sfl_rec = self
            .df
            .record(&sfl.record)
            .cloned()
            .ok_or_else(|| RtError::NoSubfile(sfl.record.clone()))?;
        let page = usize::from(sfl.size(self.size).page).max(1);
        let total = usize::from(sfl.size(self.size).size).max(page);
        let first_line = usize::from(
            sfl.first_line
                .get(self.size)
                .or(sfl.first_line.first())
                .copied()
                .unwrap_or(0),
        );
        let rec_lines = usize::from(sfl.record_lines.max(1));
        let clr = self.kw_on(rec, "SFLCLR", inds);
        let inz = self.kw_on(rec, "SFLINZ", inds);
        let dsp = self.kw_on(rec, "SFLDSP", inds);
        let dspctl = self.kw_on(rec, "SFLDSPCTL", inds);
        let rcdnbr = rec.fields.iter().enumerate().find_map(|(i, f)| match f.sfl_role {
            Some(SflRole::RcdNbr { cursor, top }) => field_bytes(rec, i, out)
                .and_then(|b| edit::decode_number(f, &b))
                .and_then(|d| d.to_i128())
                .map(|n| (usize::try_from(n).unwrap_or(0), cursor, top)),
            _ => None,
        });
        {
            let st = self.subfiles.entry(sfl.record.clone()).or_default();
            if clr {
                st.records.clear();
                st.top = 0;
                st.next_rrn = 0;
                st.displayed = false;
                if first_line > 0 {
                    m.clear_rows(first_line, first_line + page * rec_lines - 1);
                }
            }
            if inz {
                st.records = vec![Some(Self::blank_record(&sfl_rec)); total];
            }
            if let Some((n, _, to_top)) = rcdnbr
                && n >= 1
            {
                let i = (n - 1).min(st.records.len().saturating_sub(1));
                st.top = if to_top { i } else { (i / page) * page };
            }
        }
        if dspctl {
            self.place(m, env, rec, inds, Some(out), 0, 0);
        }
        if dsp && first_line > 0 {
            self.draw_page(m, env, rec, inds, &sfl_rec, first_line, page, rec_lines, sfl.end);
            if let Some((n, true, _)) = rcdnbr {
                let st = &self.subfiles[&sfl.record];
                if n >= 1 && n > st.top && n - 1 - st.top < page {
                    let line = first_line + (n - 1 - st.top) * rec_lines;
                    if let Some(f) = m.fields().iter().find(|f| f.row >= line && f.row < line + rec_lines) {
                        m.set_cursor(f.row, f.col);
                    }
                }
            }
        }
        Ok(())
    }

    /// Draw the current page of a subfile.
    #[allow(clippy::too_many_arguments)]
    fn draw_page(
        &mut self,
        m: &mut ScreenModel,
        env: &Env,
        ctl: &Record,
        ctl_inds: &Inds,
        sfl_rec: &Record,
        first_line: usize,
        page: usize,
        rec_lines: usize,
        end: Option<SflEnd>,
    ) {
        let (top, records) = {
            let st = self.subfiles.entry(sfl_rec.name.clone()).or_default();
            st.displayed = true;
            (st.top, st.records.clone())
        };
        let sfl_first = sfl_rec
            .extent(self.size)
            .map_or(first_line, |(lo, _)| usize::from(lo));
        m.clear_rows(first_line, first_line + page * rec_lines - 1);
        for k in 0..page {
            let Some(Some(buf)) = records.get(top + k) else {
                continue;
            };
            let line = first_line + k * rec_lines;
            let inds = self.inds_of(sfl_rec, buf, &[]);
            let shift = i32::try_from(line).unwrap_or(0) - i32::try_from(sfl_first).unwrap_or(0);
            self.place(m, env, sfl_rec, &inds, Some(buf), shift, 0);
        }
        let more = top + page < records.len();
        let end_kw = ctl.keywords.iter().find(|k| k.name == "SFLEND");
        if let (Some(k), Some(e)) = (end_kw, end) {
            let last_line = first_line + page * rec_lines - 1;
            let end_on = cond_on(&k.cond, ctl_inds, &self.size_name());
            let bottom = end_on && !more;
            let cols = m.cols();
            match e {
                SflEnd::Plus => {
                    if !bottom {
                        m.put(last_line, cols - 1, "+", attr::GREEN);
                    }
                }
                SflEnd::More | SflEnd::ScrBar | SflEnd::Text => {
                    let line = (last_line + 1).min(m.rows());
                    if bottom {
                        m.put(line, cols - 6, "Bottom", attr::GREEN);
                    } else {
                        m.put(line, cols - 7, "More...", attr::GREEN);
                    }
                }
            }
        }
    }

    /// A roll key on a subfile control record whose subfile has more
    /// than a page: the page moves. `None` when it cannot.
    fn roll(&mut self, m: &mut ScreenModel, env: &Env, rec: &Record, up: bool) -> Option<()> {
        let sfl = rec.subfile.as_ref()?;
        let sfl_rec = self.df.record(&sfl.record).cloned()?;
        let page = usize::from(sfl.size(self.size).page).max(1);
        let first_line = usize::from(sfl.first_line.get(self.size).or(sfl.first_line.first()).copied()?);
        let rec_lines = usize::from(sfl.record_lines.max(1));
        let inds = self.last_inds.get(&rec.name).copied().unwrap_or([false; 100]);
        {
            let st = self.subfiles.get_mut(&sfl.record)?;
            if !st.displayed {
                return None;
            }
            if up {
                if st.top + page >= st.records.len() {
                    return None;
                }
                st.top += page;
            } else {
                if st.top == 0 {
                    return None;
                }
                st.top = st.top.saturating_sub(page);
            }
        }
        self.draw_page(m, env, rec, &inds, &sfl_rec, first_line, page, rec_lines, sfl.end);
        Some(())
    }

    /// Whether `key` is enabled for the record: the response indicator
    /// it sets and whether field data comes with it.
    fn key_check(&self, rec: &Record, key: KeyPressed, inds: &Inds) -> Result<(Option<u8>, bool), Problem> {
        let size = self.size_name();
        let enabled: Vec<&crate::model::CmdKey> = rec
            .keys
            .iter()
            .chain(self.df.file.keys.iter())
            .filter(|k| cond_on(&k.cond, inds, &size))
            .collect();
        let find = |pred: &dyn Fn(KeyName) -> bool| enabled.iter().find(|k| pred(k.key));
        let number = |n: u8| find(&|k| k.number() == Some(n));
        let key = match key {
            KeyPressed::F(n) if self.df.file.alt_help.and_then(KeyName::number) == Some(n) => {
                KeyPressed::Help
            }
            KeyPressed::F(n) if self.df.file.alt_pageup.and_then(KeyName::number) == Some(n) => {
                KeyPressed::RollDown
            }
            KeyPressed::F(n) if self.df.file.alt_pagedwn.and_then(KeyName::number) == Some(n) => {
                KeyPressed::RollUp
            }
            k => k,
        };
        match key {
            KeyPressed::Enter => Ok((None, true)),
            KeyPressed::F(n) => match number(n) {
                Some(k) => Ok((k.ind, matches!(k.key, KeyName::Cf(_)))),
                None => Err(Problem::KeyNotAllowed),
            },
            KeyPressed::RollUp => find(&|k| matches!(k, KeyName::RollUp | KeyName::PageDown))
                .map(|k| (k.ind, true))
                .ok_or(Problem::KeyNotAllowed),
            KeyPressed::RollDown => find(&|k| matches!(k, KeyName::RollDown | KeyName::PageUp))
                .map(|k| (k.ind, true))
                .ok_or(Problem::KeyNotAllowed),
            KeyPressed::Help => find(&|k| k == KeyName::Help)
                .map(|k| (k.ind, false))
                .ok_or(Problem::KeyNotAllowed),
            KeyPressed::Print => Ok((find(&|k| k == KeyName::Print).and_then(|k| k.ind), false)),
            KeyPressed::Clear => find(&|k| k == KeyName::Clear)
                .map(|k| (k.ind, false))
                .ok_or(Problem::KeyNotAllowed),
            KeyPressed::RecordBackspace => Ok((None, false)),
            KeyPressed::Other(_) => Err(Problem::KeyNotAllowed),
        }
    }

    /// Build a record format's input from the display after a key.
    /// `cursor` is where the cursor was, 1-based.
    pub fn input(
        &mut self,
        m: &mut ScreenModel,
        env: &Env,
        format: &str,
        aid: u8,
        cursor: (u8, u8),
    ) -> Result<Input, Problem> {
        let rec = self
            .df
            .record(format)
            .cloned()
            .ok_or_else(|| Problem::NotShown(format.to_string()))?;
        let inds = self.last_inds.get(&rec.name).copied().unwrap_or([false; 100]);
        let key = KeyPressed::from_aid(aid);
        if rec.kind == RecordKind::SubfileControl
            && matches!(key, KeyPressed::RollUp | KeyPressed::RollDown)
            && self.key_check(&rec, key, &inds).is_err()
        {
            return match self.roll(m, env, &rec, key == KeyPressed::RollUp) {
                Some(()) => Err(Problem::Rolled),
                None => Err(Problem::KeyNotAllowed),
            };
        }
        let (key_ind, data) = self.key_check(&rec, key, &inds)?;
        let size = self.size_name();
        let mut resp = [false; 100];
        if let Some(i) = key_ind {
            resp[usize::from(i)] = true;
        }
        if key != KeyPressed::Enter {
            for k in rec.keys.iter().chain(self.df.file.keys.iter()) {
                if k.key == KeyName::VldCmdKey
                    && cond_on(&k.cond, &inds, &size)
                    && let Some(i) = k.ind
                {
                    resp[usize::from(i)] = true;
                }
            }
        }
        let out = self.last_out.get(&rec.name).cloned().unwrap_or_default();
        let (row_shift, col_shift) = self
            .shown
            .iter()
            .find(|s| s.name == rec.name)
            .map_or((0, 0), |s| (s.row_shift, s.col_shift));
        let mut parts: Vec<(u32, Vec<u8>)> = Vec::new();
        for bf in &rec.layout.input.fields {
            let taken = Taken {
                data,
                row_shift,
                col_shift,
            };
            let bytes = self.input_field(m, &rec, *bf, &out, taken, &mut resp)?;
            parts.push((bf.offset, bytes));
        }
        m.reset_mdt();
        for i in self.reset_inds.drain(..) {
            resp[usize::from(i)] = false;
        }
        let mut record: Vec<u8> = rec
            .layout
            .input
            .indicators
            .iter()
            .map(|n| if resp[usize::from(*n)] { 0xF1 } else { 0xF0 })
            .collect();
        for (offset, bytes) in parts {
            let offset = offset as usize;
            if record.len() < offset {
                record.resize(offset, ccsid::SPACE);
            }
            record.truncate(offset);
            record.extend(bytes);
        }
        record.resize(rec.layout.input.len as usize, ccsid::SPACE);
        let indicators = if self.df.file.indara {
            (1..=99usize)
                .map(|n| if resp[n] { 0xF1 } else { 0xF0 })
                .collect()
        } else {
            Vec::new()
        };
        let sfl_rrn = self.sfl_rrn_at(&rec, usize::from(cursor.0));
        Ok(Input {
            record,
            indicators,
            aid,
            cursor: (u16::from(cursor.0), u16::from(cursor.1)),
            sfl_rrn,
        })
    }

    /// One field's bytes for the input buffer: hidden fields from the
    /// last write, typed fields from the display when the key brings
    /// data (checked, with `CHANGE` and `BLANKS` noted), else as they
    /// were.
    fn input_field(
        &self,
        m: &ScreenModel,
        rec: &Record,
        bf: crate::model::BufferField,
        out: &[u8],
        taken: Taken,
        resp: &mut Inds,
    ) -> Result<Vec<u8>, Problem> {
        let f = &rec.fields[bf.field];
        let len = bf.len as usize;
        let from_out =
            || field_bytes(rec, bf.field, out).unwrap_or_else(|| vec![ccsid::SPACE; len]);
        let untouched = || {
            if f.usage == Usage::Both {
                from_out()
            } else if f.ty.is_numeric() {
                edit::zero_bytes(f)
            } else {
                vec![ccsid::SPACE; len]
            }
        };
        if !f.usage.is_input() {
            return Ok(if f.usage == Usage::Hidden {
                from_out()
            } else {
                vec![ccsid::SPACE; len]
            });
        }
        let at = f.position(self.size).map(|p| {
            (
                shifted(usize::from(p.row), taken.row_shift),
                shifted(usize::from(p.col), taken.col_shift),
            )
        });
        let idx = at.and_then(|(r, c)| m.field_index_at(r, c));
        let (Some(ix), Some((row, col))) = (idx, at) else {
            return Ok(untouched());
        };
        if !taken.data {
            return Ok(untouched());
        }
        let text = m.fields()[ix].text();
        if let Some(msg) = input::validate(f, &text) {
            return Err(Problem::Field { row, col, text: msg });
        }
        for k in &f.keywords {
            let ind = k
                .parms
                .first()
                .and_then(|p| p.as_word())
                .and_then(|w| w.parse::<u8>().ok());
            let set = match k.name.as_str() {
                "CHANGE" => m.fields()[ix].modified,
                "BLANKS" => text.trim().is_empty(),
                _ => false,
            };
            if let (true, Some(i)) = (set, ind) {
                resp[usize::from(i)] = true;
            }
        }
        input::typed_bytes(f, &text).map_err(|e| Problem::Field {
            row,
            col,
            text: input::bad_value_text(&e),
        })
    }

    /// The relative record number of the subfile record on `row`, for
    /// a control record whose page is displayed; 0 when none.
    fn sfl_rrn_at(&self, rec: &Record, row: usize) -> u32 {
        let Some(sfl) = rec.subfile.as_ref() else {
            return 0;
        };
        let Some(st) = self.subfiles.get(&sfl.record) else {
            return 0;
        };
        if !st.displayed {
            return 0;
        }
        let page = usize::from(sfl.size(self.size).page).max(1);
        let rec_lines = usize::from(sfl.record_lines.max(1));
        let first = usize::from(sfl.first_line.get(self.size).or(sfl.first_line.first()).copied().unwrap_or(0));
        if first == 0 || row < first || row >= first + page * rec_lines {
            return 0;
        }
        let k = (row - first) / rec_lines;
        match st.records.get(st.top + k) {
            Some(Some(_)) => u32::try_from(st.top + k + 1).unwrap_or(0),
            _ => 0,
        }
    }

    /// Show a read's problem: the message on the error line, the field
    /// in reverse image with the cursor on it, the alarm.
    pub fn show_problem(&self, m: &mut ScreenModel, p: &Problem) {
        match p {
            Problem::Field { row, col, text } => {
                m.error_line(text, attr::WHITE);
                if let Some(ix) = m.field_index_at(*row, *col) {
                    let a = m.fields()[ix].attr | 0x01;
                    m.set_attr(ix, a);
                }
                m.set_cursor(*row, *col);
                m.alarm();
            }
            Problem::KeyNotAllowed => {
                m.error_line("That key is not allowed here.", attr::WHITE);
                m.alarm();
            }
            Problem::Rolled | Problem::NotShown(_) => {}
        }
    }

    /// Whether the record format is on the display.
    #[must_use]
    pub fn is_shown(&self, format: &str) -> bool {
        self.shown.iter().any(|s| s.name == format)
    }
}

/// How a read takes a field.
#[derive(Clone, Copy)]
struct Taken {
    /// The key brings field data.
    data: bool,
    row_shift: i32,
    col_shift: i32,
}

/// A window's border: dots above and below, colons at the sides.
fn draw_border(m: &mut ScreenModel, w: &crate::model::Window, row_shift: i32, col_shift: i32) {
    let top = shifted(0, row_shift);
    let left = shifted(0, col_shift);
    let lines = usize::from(w.lines);
    let cols = usize::from(w.cols);
    let line: String = ".".repeat(cols + 2);
    m.put(top, left, &line, attr::GREEN);
    m.put(top + lines + 1, left, &line, attr::GREEN);
    for r in 1..=lines {
        m.put(top + r, left, &format!(":{}:", " ".repeat(cols)), attr::GREEN);
    }
}

/// The bytes of field `i` in a record's output buffer.
fn field_bytes(rec: &Record, i: usize, out: &[u8]) -> Option<Vec<u8>> {
    let bf = rec.layout.output.field(i)?;
    out.get(bf.offset as usize..(bf.offset + bf.len) as usize)
        .map(<[u8]>::to_vec)
}

/// A 1-based position moved by a shift, never below 1.
fn shifted(pos: usize, shift: i32) -> usize {
    usize::try_from(i32::try_from(pos).unwrap_or(1) + shift)
        .unwrap_or(1)
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompileOpts, NoRefs, Source, compile_dspf};
    use tn5250::ds::wdsf::{Capabilities, Model};
    use tn5250::record::Record as Rec;
    use tn5250::screen::Read;
    use tn5250::term::{Key, Terminal};

    const SIGNON: &str = "
     A                                      DSPSIZ(24 80 *DS3)
     A                                      CF03(03 'Exit')
     A                                      CA12(12)
     A          R SIGNON
     A                                  1 36'Sign On'
     A                                  2 48'System  . . . . . :'
     A            QSYSNAME       8A  O  2 70
     A                                  6 17'User  . . . . . . . . . . . . . .'
     A            USERID        10A  I  6 53
     A                                  7 17'Password  . . . . . . . . . . . .'
     A            PASSWRD       10A  I  7 53DSPATR(ND)
     A            AMOUNT         7S 2B  9 53EDTCDE(J) RANGE(0 99999)
     A  50                                  ERRMSG('Amount too high' 50)
     A            HID            5A  H
";

    fn compile(text: &str) -> DisplayFile {
        let (src, _) = Source::from_text(text);
        let c = compile_dspf(&src, &NoRefs, &CompileOpts::default());
        c.definition.unwrap_or_else(|| panic!("{}", c.listing))
    }

    fn env() -> Env {
        Env {
            date: (2026, 10, 23),
            sys_date: (2026, 10, 23),
            time: (1, 2, 3),
            datfmt: DateOrder::Mdy,
            datsep: '/',
            user: "QSECOFR".into(),
            sysname: "QS400".into(),
            messages: BTreeMap::new(),
        }
    }

    fn terminal() -> Terminal {
        Terminal::new(Capabilities {
            model: Model::M3179,
            gui: false,
            color: true,
        })
    }

    /// Send the model to a terminal and bring the terminal's reply back.
    fn round_trip(m: &mut ScreenModel, t: &mut Terminal, keys: &[Key], typed: &[&str]) -> (u8, (u8, u8)) {
        let out = m.render(Read::MdtFields);
        t.apply(&Rec::new(out.opcode, out.data)).unwrap();
        for s in typed {
            t.type_str(s);
            t.key(Key::FieldExit);
        }
        for k in keys {
            t.key(*k);
        }
        let rec = t.take_outbox().remove(0);
        let inb = m.accept(&rec.data).unwrap();
        (inb.aid, (inb.row, inb.col))
    }

    #[test]
    fn sign_on_round_trip() {
        let df = compile(SIGNON);
        let rec = df.record("SIGNON").unwrap().clone();
        let mut rt = Runtime::new(df, false);
        let mut m = ScreenModel::new(false);
        // Output buffer: indicator 50 off, QSYSNAME, AMOUNT, HID.
        let mut out = vec![0xF0u8];
        out.extend(input::padded("QS400", 8));
        out.extend(edit::encode_number(&rec.fields[rec.field("AMOUNT").unwrap().0], &qdecimal::Decimal::parse("12.5").unwrap()).unwrap());
        out.extend(input::padded("HIDE!", 5));
        assert_eq!(out.len(), rec.layout.output.len as usize);
        rt.write(&mut m, &env(), "SIGNON", &out, &[]).unwrap();
        let rows = m.text_rows();
        assert_eq!(&rows[0][35..42], "Sign On");
        assert_eq!(&rows[1][69..74], "QS400");
        assert!(rows[8].contains("12.50"), "{:?}", rows[8]);
        assert_eq!(m.fields().len(), 3);
        assert_eq!(m.fields()[1].attr, attr::NONDISPLAY);
        let mut t = terminal();
        let (aid, cursor) = round_trip(&mut m, &mut t, &[Key::Enter], &["cody", "secret", "20"]);
        let input = rt.input(&mut m, &env(), "SIGNON", aid, cursor).unwrap();
        // Input buffer: indicators 3, 12, 50; USERID, PASSWRD, AMOUNT, HID.
        assert_eq!(rec.layout.input.indicators, vec![3, 12, 50]);
        assert_eq!(&input.record[..3], &[0xF0, 0xF0, 0xF0]);
        assert_eq!(ccsid::from_ebcdic(&input.record[3..13]).trim_end(), "CODY");
        assert_eq!(ccsid::from_ebcdic(&input.record[13..23]).trim_end(), "SECRET");
        let amount = &rec.fields[rec.field("AMOUNT").unwrap().0];
        assert_eq!(
            edit::decode_number(amount, &input.record[23..30]).unwrap().char_text(2),
            "20.00"
        );
        assert_eq!(ccsid::from_ebcdic(&input.record[30..35]), "HIDE!");
        assert_eq!(input.sfl_rrn, 0);
    }

    #[test]
    fn keys_and_checks() {
        let df = compile(SIGNON);
        let mut rt = Runtime::new(df, false);
        let mut m = ScreenModel::new(false);
        let out = [vec![0xF0u8], input::padded("QS400", 8), vec![0xF0; 7], input::padded("", 5)].concat();
        rt.write(&mut m, &env(), "SIGNON", &out, &[]).unwrap();
        let mut t = terminal();
        let (aid, cursor) = round_trip(&mut m, &mut t, &[Key::F(3)], &["x"]);
        let input = rt.input(&mut m, &env(), "SIGNON", aid, cursor).unwrap();
        assert_eq!(&input.record[..3], &[0xF1, 0xF0, 0xF0]);
        assert_eq!(ccsid::from_ebcdic(&input.record[3..13]).trim_end(), "X");
        // CA12 returns no data: the field reads blank.
        rt.write(&mut m, &env(), "SIGNON", &out, &[]).unwrap();
        let (aid, cursor) = round_trip(&mut m, &mut t, &[Key::F(12)], &["y"]);
        let input = rt.input(&mut m, &env(), "SIGNON", aid, cursor).unwrap();
        assert_eq!(&input.record[..3], &[0xF0, 0xF1, 0xF0]);
        assert_eq!(ccsid::from_ebcdic(&input.record[3..13]).trim_end(), "");
        // F5 is not enabled.
        rt.write(&mut m, &env(), "SIGNON", &out, &[]).unwrap();
        let (aid, cursor) = round_trip(&mut m, &mut t, &[Key::F(5)], &[]);
        assert_eq!(rt.input(&mut m, &env(), "SIGNON", aid, cursor), Err(Problem::KeyNotAllowed));
        // An amount beyond RANGE is a field problem at the field.
        rt.write(&mut m, &env(), "SIGNON", &out, &[]).unwrap();
        let (aid, cursor) = round_trip(&mut m, &mut t, &[Key::Enter], &["", "", "123456"]);
        let p = rt.input(&mut m, &env(), "SIGNON", aid, cursor).unwrap_err();
        assert!(matches!(p, Problem::Field { row: 9, col: 53, .. }), "{p:?}");
        rt.show_problem(&mut m, &p);
        assert!(m.text_rows()[23].contains("range"));
        // ERRMSG at write: the message shows, the field is reversed and
        // indicator 50 comes back off.
        let mut flagged = out.clone();
        flagged[0] = 0xF1;
        rt.write(&mut m, &env(), "SIGNON", &flagged, &[]).unwrap();
        assert!(m.text_rows()[23].contains("Amount too high"));
        let (aid, cursor) = round_trip(&mut m, &mut t, &[Key::Enter], &[]);
        let input = rt.input(&mut m, &env(), "SIGNON", aid, cursor).unwrap();
        assert_eq!(input.record[2], 0xF0);
    }

    const LIST: &str = "
     A                                      DSPSIZ(24 80 *DS3)
     A                                      CF03(03)
     A          R SFL1                      SFL
     A            SEL            1A  B  9  3
     A            SNAME         20A  O  9  6
     A          R CTL1                      SFLCTL(SFL1)
     A                                      SFLSIZ(50) SFLPAG(3)
     A                                      OVERLAY
     A  30                                  SFLDSP
     A  31                                  SFLDSPCTL
     A  32                                  SFLCLR
     A N30                                  SFLEND(*MORE)
     A                                  1 30'Work with things'
     A          R FOOTER                    OVERLAY
     A                                 23  2'F3=Exit'
";

    #[test]
    fn subfile_pages_and_rolls() {
        let df = compile(LIST);
        let sfl = df.record("SFL1").unwrap().clone();
        let mut rt = Runtime::new(df, false);
        let mut m = ScreenModel::new(false);
        let e = env();
        // Clear, then five records, then display.
        rt.write(&mut m, &e, "CTL1", &[0xF0, 0xF0, 0xF1], &[]).unwrap();
        for i in 1..=5 {
            let rec = [input::padded(" ", 1), input::padded(&format!("Thing {i}"), 20)].concat();
            assert_eq!(rec.len(), sfl.layout.output.len as usize);
            rt.write(&mut m, &e, "SFL1", &rec, &[]).unwrap();
        }
        rt.write(&mut m, &e, "CTL1", &[0xF1, 0xF1, 0xF0], &[]).unwrap();
        rt.write(&mut m, &e, "FOOTER", &[], &[]).unwrap();
        let rows = m.text_rows();
        assert!(rows[0].contains("Work with things"));
        assert!(rows[8].contains("Thing 1"));
        assert!(rows[10].contains("Thing 3"));
        assert!(!rows[11].contains("Thing"));
        assert!(rows[11].contains("More..."), "{:?}", rows[11]);
        assert!(rows[22].contains("F3=Exit"));
        assert_eq!(m.fields().len(), 3);
        let mut t = terminal();
        let (aid, cursor) = round_trip(&mut m, &mut t, &[Key::PageDown], &[]);
        assert_eq!(rt.input(&mut m, &e, "CTL1", aid, cursor), Err(Problem::Rolled));
        let rows = m.text_rows();
        assert!(rows[8].contains("Thing 4"));
        assert!(rows[9].contains("Thing 5"));
        assert!(!rows[10].contains("Thing"));
        // SFLEND's indicator is off: still "More...", the subfile may
        // have more to come.
        assert!(rows[11].contains("More..."));
        let (aid, cursor) = round_trip(&mut m, &mut t, &[Key::PageDown], &[]);
        assert_eq!(rt.input(&mut m, &e, "CTL1", aid, cursor), Err(Problem::KeyNotAllowed));
        // Enter with the cursor on the second line of the page: RRN 5.
        let out = m.render(Read::MdtFields);
        t.apply(&Rec::new(out.opcode, out.data)).unwrap();
        t.key(Key::Enter);
        let rec = t.take_outbox().remove(0);
        let inb = m.accept(&rec.data).unwrap();
        let input = rt.input(&mut m, &e, "CTL1", inb.aid, (10, 3)).unwrap();
        assert_eq!(input.sfl_rrn, 5);
        assert_eq!(rt.sfl_records("SFL1").len(), 5);
    }

    #[test]
    fn overlay_and_clear() {
        let df = compile(LIST);
        let mut rt = Runtime::new(df, false);
        let mut m = ScreenModel::new(false);
        let e = env();
        rt.write(&mut m, &e, "FOOTER", &[], &[]).unwrap();
        assert!(rt.is_shown("FOOTER"));
        rt.write(&mut m, &e, "CTL1", &[0xF0, 0xF1, 0xF1], &[]).unwrap();
        // OVERLAY kept the footer.
        assert!(rt.is_shown("FOOTER"));
        assert!(m.text_rows()[22].contains("F3=Exit"));
        // A record without OVERLAY clears everything.
        let df2 = compile(SIGNON);
        let mut rt2 = Runtime::new(df2, false);
        rt2.write(&mut m, &e, "SIGNON", &[0x40; 21], &[]).unwrap();
        assert!(!m.text_rows()[22].contains("F3=Exit"));
    }
}
