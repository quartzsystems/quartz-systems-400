// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Display files for the job's programs: the runtime's `DisplaySource`
//! over the compiled file the storage service hands out and the
//! job's screen (`Surface`).

use std::collections::{BTreeMap, HashMap};

use dspf::rt::{DateOrder, Env, Problem, Runtime};
use mi::io::IoFail;
use mi::io::display::{DisplayInput, DisplayOpen, DisplaySource};
use stg::Client;
use sysapi::LocalNow;
use tn5250::screen::Read;

use super::surface::Surface;
use crate::console::host::Host;

/// One open display file.
struct Open {
    rt: Runtime,
    lib: String,
}

/// The job's display files.
pub struct DdsDisplay {
    surface: Surface,
    files: HashMap<u32, Open>,
    next: u32,
    sysname: Option<String>,
}

fn fail(e: &str, names: &[&str]) -> IoFail {
    let id = e
        .split_whitespace()
        .next()
        .filter(|w| w.len() == 7 && w.starts_with("CP"))
        .unwrap_or("CPF4101");
    IoFail::new(1299, id, names)
}

/// Year, month and day of a day count since 1970-01-01.
fn civil(days: i64) -> (u16, u8, u8) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (
        u16::try_from(y).unwrap_or(1970),
        u8::try_from(m).unwrap_or(1),
        u8::try_from(d).unwrap_or(1),
    )
}

/// What the constants show: the clock, the date format, the user and
/// the system name.
#[must_use]
pub fn env_from(now: &LocalNow, datfmt: &str, datsep: &str, user: &str, sysname: &str) -> Env {
    let local = i64::try_from(now.unix).unwrap_or(0) + i64::from(now.offset_min) * 60;
    let days = local.div_euclid(86_400);
    let secs = local.rem_euclid(86_400);
    let date = civil(days);
    Env {
        date,
        sys_date: date,
        time: (
            u8::try_from(secs / 3600).unwrap_or(0),
            u8::try_from(secs % 3600 / 60).unwrap_or(0),
            u8::try_from(secs % 60).unwrap_or(0),
        ),
        datfmt: DateOrder::parse(datfmt),
        datsep: datsep.chars().next().unwrap_or('/'),
        user: user.to_string(),
        sysname: sysname.to_string(),
        messages: BTreeMap::new(),
    }
}

/// The environment of a job's display files.
pub fn env_of(host: &Host, sysname: &str) -> Env {
    let now = host.link.borrow_mut().with(stg::Client::now);
    let a = host.attrs();
    env_from(&now, &a.datfmt, &a.datsep, &host.start.user, sysname)
}

/// The system name as `QSYSNAME` says.
pub fn system_name(host: &Host) -> String {
    host.link
        .borrow_mut()
        .with(|l| l.sysval("QSYSNAME".into()))
        .map_or_else(
            || "QS400".to_string(),
            |v| ccsid::from_ebcdic(&v.data).trim().to_string(),
        )
}

/// An output buffer for `rec`: the named fields set, the others blank
/// or zero, the option indicators off.
#[must_use]
pub fn out_buffer(rec: &dspf::Record, values: &[(&str, &str)]) -> Vec<u8> {
    let mut out = vec![0xF0u8; rec.layout.output.indicators.len()];
    for bf in &rec.layout.output.fields {
        let f = &rec.fields[bf.field];
        if f.ty.is_numeric() {
            out.extend(dspf::rt::edit::zero_bytes(f));
        } else {
            out.extend(std::iter::repeat_n(ccsid::SPACE, bf.len as usize));
        }
    }
    out.resize(rec.layout.output.len as usize, ccsid::SPACE);
    for (name, value) in values {
        let Some((i, _)) = rec.field(name) else {
            continue;
        };
        let Some(bf) = rec.layout.output.field(i) else {
            continue;
        };
        let bytes = dspf::rt::input::padded(value, bf.len as usize);
        let at = bf.offset as usize;
        out[at..at + bytes.len()].copy_from_slice(&bytes);
    }
    out
}

/// The text of a named field in a record's input buffer.
#[must_use]
pub fn in_text(rec: &dspf::Record, record: &[u8], name: &str) -> Option<String> {
    let (i, _) = rec.field(name)?;
    let bf = rec.layout.input.field(i)?;
    let b = record.get(bf.offset as usize..(bf.offset + bf.len) as usize)?;
    Some(ccsid::from_ebcdic(b).trim_end().to_string())
}

impl DdsDisplay {
    /// Over the job's surroundings.
    #[must_use]
    pub fn new(host: Host) -> Self {
        Self {
            surface: Surface::new(host),
            files: HashMap::new(),
            next: 1,
            sysname: None,
        }
    }

    fn sysname(&mut self) -> String {
        if let Some(s) = &self.sysname {
            return s.clone();
        }
        let s = system_name(self.surface.host());
        self.sysname = Some(s.clone());
        s
    }

    fn env(&mut self) -> Env {
        let sysname = self.sysname();
        env_of(self.surface.host(), &sysname)
    }
}

impl DisplaySource for DdsDisplay {
    fn open(&mut self, file: &str, lib: &str, actgrp: &str) -> Result<DisplayOpen, IoFail> {
        let (payload, real) = self
            .surface
            .host()
            .link
            .borrow_mut()
            .with(|l| l.dsp_open(file.into(), lib.into(), actgrp.into()))
            .map_err(|e| fail(&e, &[file, lib]))?;
        let df = dspf::decode(&payload).map_err(|_| IoFail::new(1299, "CPF4101", &[file, lib]))?;
        let wide = self.surface.wide();
        let handle = self.next;
        self.next += 1;
        let real_lib = real.split('/').next().unwrap_or(lib).to_string();
        self.files.insert(
            handle,
            Open {
                rt: Runtime::new(df, wide),
                lib: real_lib.clone(),
            },
        );
        Ok(DisplayOpen {
            handle,
            lib: real_lib,
        })
    }

    fn write(
        &mut self,
        handle: u32,
        format: &str,
        record: &[u8],
        indicators: &[u8],
    ) -> Result<(), IoFail> {
        let env = self.env();
        let Some(mut open) = self.files.remove(&handle) else {
            return Err(IoFail::new(1299, "CPF4101", &[format]));
        };
        let r = self
            .surface
            .with_model(|m| open.rt.write(m, &env, format, record, indicators));
        let lib = open.lib.clone();
        self.files.insert(handle, open);
        r.map_err(|_| IoFail::new(1299, "CPF5123", &[format, &lib]))?;
        self.surface.flush(Read::None);
        Ok(())
    }

    fn read(&mut self, handle: u32, format: &str) -> Result<DisplayInput, IoFail> {
        let env = self.env();
        let Some(mut open) = self.files.remove(&handle) else {
            return Err(IoFail::new(1299, "CPF4101", &[format]));
        };
        let r = loop {
            self.surface.flush(Read::MdtFields);
            let Some((a, cursor)) = self.surface.wait() else {
                if crate::sysdsp::keys::cancelling() {
                    let mut f = IoFail::new(IoFail::REQUEST_ENDED, crate::sysdsp::keys::ENDED, &[]);
                    f.data.clear();
                    break Err(f);
                }
                break Err(IoFail::new(1299, "CPF5140", &[format, &open.lib]));
            };
            let got = self
                .surface
                .with_model(|m| open.rt.input(m, &env, format, a, cursor));
            match got {
                Ok(input) => {
                    break Ok(DisplayInput {
                        format: format.to_string(),
                        record: input.record,
                        indicators: input.indicators,
                        aid: input.aid,
                        cursor: input.cursor,
                        sfl_rrn: input.sfl_rrn,
                    });
                }
                Err(Problem::NotShown(_)) => {
                    break Err(IoFail::new(1299, "CPF5123", &[format, &open.lib]));
                }
                Err(p) => self.surface.with_model(|m| open.rt.show_problem(m, &p)),
            }
        };
        self.files.insert(handle, open);
        r
    }

    fn close(&mut self, handle: u32) -> Result<(), IoFail> {
        self.files.remove(&handle);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_749), (2026, 10, 23));
        assert_eq!(civil(19_782), (2024, 2, 29));
    }
}
