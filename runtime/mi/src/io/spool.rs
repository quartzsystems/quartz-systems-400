// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Printer files: where their spooled files go ([`SpoolSink`]) and what
//! an open one holds ([`PrinterState`]). The job runs the line printer
//! and hands the sink SCS in chunks; the sink keeps the spooled file.

use std::collections::BTreeMap;

use dds::prtf::PrtfDefinition;
use spool::ddsprt::{self, Clock};
use spool::{PageFormat, Printer};
use wm::obj::prtf::Prtf;

use super::IoFail;

/// SCS kept before it is handed to the sink.
const CHUNK: usize = 32 * 1024;

/// A printer file opened: the spooled file and the attributes in force.
#[derive(Clone, Debug)]
pub struct PrinterOpen {
    /// The spooled file's identifier.
    pub id: u32,
    /// The printer file, overrides applied.
    pub prtf: Prtf,
}

/// Where spooled files go.
pub trait SpoolSink {
    /// Open printer file `lib/file` from activation group `actgrp`: a
    /// new spooled file.
    fn open(&mut self, file: &str, lib: &str, actgrp: &str) -> Result<PrinterOpen, IoFail>;
    /// Append SCS, with the page and record counts so far.
    fn put(&mut self, id: u32, data: &[u8], pages: u32, records: u32) -> Result<(), IoFail>;
    /// Close the spooled file.
    fn close(&mut self, id: u32, hold: bool) -> Result<(), IoFail>;
}

/// An open printer file.
#[derive(Clone, Debug)]
pub struct PrinterState {
    /// The spooled file.
    pub id: u32,
    /// The line printer.
    pub printer: Printer,
    /// The DDS formats; none for a program-described file.
    pub def: Option<PrtfDefinition>,
    /// The printer file.
    pub prtf: Prtf,
}

fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// The date and time `DATE` and `TIME` print: now, in UTC.
#[must_use]
pub fn clock() -> Clock {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (y, m, d) = civil(i64::try_from(secs / 86_400).unwrap_or(0));
    let t = secs % 86_400;
    Clock {
        date: format!("{m:02}{d:02}{:02}", y % 100),
        time: format!("{:02}{:02}{:02}", t / 3600, t % 3600 / 60, t % 60),
    }
}

impl PrinterState {
    /// A printer for what the sink opened.
    #[must_use]
    pub fn new(open: PrinterOpen) -> Self {
        let f = &open.prtf;
        let fmt = PageFormat {
            len: f.page_len,
            width: f.page_width,
            lpi: f.lpi,
            cpi: f.cpi,
            ovrflw: f.ovrflw,
            fold: f.fold,
        };
        let def = if f.formats.is_empty() {
            None
        } else {
            dds::prtf::codec::decode(&f.formats)
        };
        Self {
            id: open.id,
            printer: Printer::new(fmt),
            def,
            prtf: open.prtf,
        }
    }

    /// Write one record: a format of an externally described file, or a
    /// line of a program-described one (its first byte the carriage
    /// control under `CTLCHAR(*FCFC)`). Whether the overflow line was
    /// reached; a chunk goes to the sink when enough has built up.
    pub fn write(
        &mut self,
        sink: &mut dyn SpoolSink,
        fmt: &str,
        record: &[u8],
    ) -> Result<bool, IoFail> {
        if self.prtf.maxrcds > 0 && self.printer.records >= self.prtf.maxrcds {
            return Err(IoFail::new(1299, "CPF4072", &[fmt]));
        }
        let over = match &self.def {
            Some(def) => {
                let f = def
                    .format(fmt)
                    .ok_or_else(|| IoFail::new(1299, "CPF5054", &[fmt]))?;
                ddsprt::print_format(&mut self.printer, f, def.indara, &[], record, &clock())
            }
            None if self.prtf.fcfc => self.printer.write_fcfc(record),
            None => self.printer.write_line(record),
        };
        if self.printer.pending() >= CHUNK {
            self.flush(sink)?;
        }
        Ok(over)
    }

    /// Hand what has built up to the sink.
    pub fn flush(&mut self, sink: &mut dyn SpoolSink) -> Result<(), IoFail> {
        let data = self.printer.take();
        sink.put(self.id, &data, self.printer.pages, self.printer.records)
    }

    /// Flush and close.
    pub fn close(&mut self, sink: &mut dyn SpoolSink) -> Result<(), IoFail> {
        self.flush(sink)?;
        sink.close(self.id, self.prtf.hold)
    }
}

/// A spooled file kept in memory.
#[derive(Clone, Debug, Default)]
pub struct MemSplf {
    /// The printer file it came from.
    pub file: String,
    /// Its SCS.
    pub data: Vec<u8>,
    /// Pages.
    pub pages: u32,
    /// Records.
    pub records: u32,
    /// Closed.
    pub closed: bool,
    /// Closed held.
    pub held: bool,
}

/// Spooled files in memory, for tests and the in-memory kits: printer
/// files by name (any unknown one is program-described with the
/// defaults), spooled files by identifier.
#[derive(Clone, Debug, Default)]
pub struct MemSpool {
    /// Printer files by name.
    pub prtfs: BTreeMap<String, Prtf>,
    /// Spooled files, by identifier from 1.
    pub files: BTreeMap<u32, MemSplf>,
}

impl SpoolSink for MemSpool {
    fn open(&mut self, file: &str, _lib: &str, _actgrp: &str) -> Result<PrinterOpen, IoFail> {
        let id = u32::try_from(self.files.len() + 1).unwrap_or(u32::MAX);
        self.files.insert(
            id,
            MemSplf {
                file: file.to_string(),
                ..MemSplf::default()
            },
        );
        let prtf = self.prtfs.get(file).cloned().unwrap_or_default();
        Ok(PrinterOpen { id, prtf })
    }

    fn put(&mut self, id: u32, data: &[u8], pages: u32, records: u32) -> Result<(), IoFail> {
        let f = self
            .files
            .get_mut(&id)
            .ok_or_else(|| IoFail::new(1299, "CPF3309", &[]))?;
        f.data.extend_from_slice(data);
        f.pages = pages;
        f.records = records;
        Ok(())
    }

    fn close(&mut self, id: u32, hold: bool) -> Result<(), IoFail> {
        let f = self
            .files
            .get_mut(&id)
            .ok_or_else(|| IoFail::new(1299, "CPF3309", &[]))?;
        f.closed = true;
        f.held = hold;
        Ok(())
    }
}
