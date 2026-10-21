// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A job's attributes: everything `CHGJOB` changes and `RTVJOBA`
//! returns. A job starts with them resolved from its job description,
//! its class and the system values; `*SYSVAL`, `*JOBD` and `*USRPRF`
//! never survive into a running job.

use ipc::wire_struct;

use crate::obj::cls::Cls;
use crate::obj::jobd::JobD;
use crate::{Error, Result};

/// The library list: system part, product libraries, current library,
/// user part.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LibList {
    /// From `QSYSLIBL` and `CHGSYSLIBL`.
    pub sys: Vec<String>,
    /// Product library slots (two).
    pub prd: Vec<String>,
    /// The current library, if any.
    pub cur: Option<String>,
    /// From `QUSRLIBL` or `INLLIBL`, then `ADDLIBLE` and the rest.
    pub usr: Vec<String>,
}

wire_struct!(LibList { sys, prd, cur, usr });

/// At most this many libraries in the user part.
pub const MAX_USR: usize = 250;
/// At most this many in the system part.
pub const MAX_SYS: usize = 15;

impl LibList {
    /// The whole list in search order.
    #[must_use]
    pub fn search(&self) -> Vec<String> {
        let mut v = self.sys.clone();
        v.extend(self.prd.iter().cloned());
        v.extend(self.cur.iter().cloned());
        v.extend(self.usr.iter().cloned());
        v
    }

    /// Whether a library is anywhere on it.
    #[must_use]
    pub fn contains(&self, lib: &str) -> bool {
        self.search().iter().any(|l| l == lib)
    }

    /// `ADDLIBLE LIB POSITION(*FIRST|*LAST)`.
    pub fn add(&mut self, lib: &str, first: bool) -> Result<()> {
        if self.contains(lib) {
            return Err(Error::value(
                "CPF2103",
                format!("library {lib} is already in the library list"),
            ));
        }
        if self.usr.len() >= MAX_USR {
            return Err(Error::value("CPF2183", "the library list is full"));
        }
        if first {
            self.usr.insert(0, lib.to_string());
        } else {
            self.usr.push(lib.to_string());
        }
        Ok(())
    }

    /// `RMVLIBLE LIB`.
    pub fn remove(&mut self, lib: &str) -> Result<()> {
        let before = self.usr.len();
        self.usr.retain(|l| l != lib);
        if self.usr.len() == before {
            return Err(Error::value(
                "CPF2104",
                format!("library {lib} is not in the user part of the library list"),
            ));
        }
        Ok(())
    }

    /// `CHGLIBL LIBL(...)`: the user part replaced.
    pub fn change(&mut self, libs: Vec<String>) -> Result<()> {
        if libs.len() > MAX_USR {
            return Err(Error::value("CPF2183", "the library list is full"));
        }
        for (i, l) in libs.iter().enumerate() {
            if libs[..i].contains(l) || self.sys.contains(l) || self.cur.as_ref() == Some(l) {
                return Err(Error::value(
                    "CPF2184",
                    format!("library {l} appears twice"),
                ));
            }
        }
        self.usr = libs;
        Ok(())
    }

    /// `CHGSYSLIBL`: the system part replaced.
    pub fn change_sys(&mut self, libs: Vec<String>) -> Result<()> {
        if libs.len() > MAX_SYS {
            return Err(Error::value("CPF2183", "the system library list is full"));
        }
        self.sys = libs;
        Ok(())
    }
}

/// The attributes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobAttrs {
    /// `JOBD` it started from, `lib/name`.
    pub jobd: String,
    /// `JOBQ`, `lib/name`.
    pub jobq: String,
    /// `JOBPTY`.
    pub jobpty: u8,
    /// `OUTPTY`.
    pub outpty: u8,
    /// `PRTDEV`.
    pub prtdev: String,
    /// `OUTQ`, `lib/name`.
    pub outq: String,
    /// `PRTTXT`.
    pub prttxt: String,
    /// `RTGDTA`.
    pub rtgdta: String,
    /// `RQSDTA`.
    pub rqsdta: String,
    /// The library list.
    pub libl: LibList,
    /// `INLASPGRP`.
    pub aspgrp: String,
    /// `LOG` level.
    pub log_level: u8,
    /// `LOG` severity.
    pub log_sev: u8,
    /// `LOG` text.
    pub log_text: String,
    /// `LOGCLPGM`.
    pub logclpgm: String,
    /// `LOGOUTPUT`.
    pub logoutput: String,
    /// `INQMSGRPY`.
    pub inqmsgrpy: String,
    /// `DEVRCYACN`.
    pub devrcyacn: String,
    /// `TSEPOOL`.
    pub tsepool: String,
    /// `ENDSEV`.
    pub endsev: u8,
    /// `SYNTAX`; 255 is `*NOCHK`.
    pub syntax: u8,
    /// `JOBMSGQMX` in megabytes.
    pub jobmsgqmx: u16,
    /// `JOBMSGQFL`.
    pub jobmsgqfl: String,
    /// `ALWMLTTHD`.
    pub alwmltthd: bool,
    /// `SPLFACN`.
    pub splfacn: String,
    /// `DDMCNV`.
    pub ddmcnv: String,
    /// `RUNPTY`.
    pub runpty: u8,
    /// `TIMESLICE` in milliseconds.
    pub timeslice: u32,
    /// `PURGE`.
    pub purge: bool,
    /// `DFTWAIT` in seconds; `u32::MAX` is `*NOMAX`.
    pub dftwait: u32,
    /// `CPUTIME` in milliseconds; `u32::MAX` is `*NOMAX`.
    pub cputime: u32,
    /// `MAXTMPSTG` in megabytes; `u32::MAX` is `*NOMAX`.
    pub maxtmpstg: u32,
    /// `MAXTHD`; `u32::MAX` is `*NOMAX`.
    pub maxthd: u32,
    /// `CCSID`.
    pub ccsid: u32,
    /// `DFTCCSID`.
    pub dftccsid: u32,
    /// `LANGID`.
    pub langid: String,
    /// `CNTRYID`.
    pub cntryid: String,
    /// `DATFMT`: `*YMD`, `*MDY`, `*DMY`, `*JUL`.
    pub datfmt: String,
    /// `DATSEP`.
    pub datsep: String,
    /// `TIMSEP`.
    pub timsep: String,
    /// `DECFMT`: blank, `J`, `I`.
    pub decfmt: String,
    /// `SRTSEQ`.
    pub srtseq: String,
    /// `BRKMSG`: `*NORMAL`, `*NOTIFY`, `*HOLD`.
    pub brkmsg: String,
    /// `STSMSG`: `*NORMAL`, `*NONE`, `*USRPRF`, `*SYSVAL`.
    pub stsmsg: String,
    /// `SCDDATE` as `YYYYMMDD`, blank for none.
    pub scddate: String,
    /// `SCDTIME` as `HHMMSS`, blank for none.
    pub scdtime: String,
    /// The job date, `YYYYMMDD`; blank follows the system date.
    pub date: String,
    /// `SWS`: eight `0` or `1`.
    pub sws: String,
    /// `PRTKEYFMT`.
    pub prtkeyfmt: String,
    /// `DUPJOBOPT`.
    pub dupjobopt: String,
}

wire_struct!(JobAttrs {
    jobd,
    jobq,
    jobpty,
    outpty,
    prtdev,
    outq,
    prttxt,
    rtgdta,
    rqsdta,
    libl,
    aspgrp,
    log_level,
    log_sev,
    log_text,
    logclpgm,
    logoutput,
    inqmsgrpy,
    devrcyacn,
    tsepool,
    endsev,
    syntax,
    jobmsgqmx,
    jobmsgqfl,
    alwmltthd,
    splfacn,
    ddmcnv,
    runpty,
    timeslice,
    purge,
    dftwait,
    cputime,
    maxtmpstg,
    maxthd,
    ccsid,
    dftccsid,
    langid,
    cntryid,
    datfmt,
    datsep,
    timsep,
    decfmt,
    srtseq,
    brkmsg,
    stsmsg,
    scddate,
    scdtime,
    date,
    sws,
    prtkeyfmt,
    dupjobopt
});

/// Where `*SYSVAL` values come from: the value's text, blank when the
/// system value does not exist.
pub type SysvalFn<'a> = &'a dyn Fn(&str) -> String;

fn or_sysval(v: &str, sv: SysvalFn<'_>, name: &str) -> String {
    if v == "*SYSVAL" {
        sv(name)
    } else {
        v.to_string()
    }
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_string).collect()
}

impl JobAttrs {
    /// Resolve a job description and a class for `user`: `*SYSVAL`
    /// through `sv`, `*USRPRF` through the user's defaults
    /// (`MIGRATE(phase6)`: the profile's own values).
    #[must_use]
    pub fn resolve(jobd_name: &str, d: &JobD, c: &Cls, sv: SysvalFn<'_>) -> Self {
        let num = |name: &str, dft: u32| sv(name).trim().parse().unwrap_or(dft);
        let outq = match d.outq.1.as_str() {
            "*USRPRF" | "*DEV" | "*WRKSTN" | "" => "QGPL/QPRINT".to_string(),
            n => format!(
                "{}/{n}",
                if d.outq.0.is_empty() {
                    "*LIBL"
                } else {
                    &d.outq.0
                }
            ),
        };
        let prtdev = match d.prtdev.as_str() {
            "*USRPRF" | "*SYSVAL" | "*WRKSTN" => sv("QPRTDEV"),
            p => p.to_string(),
        };
        let usr = d.inllibl.clone().unwrap_or_else(|| words(&sv("QUSRLIBL")));
        let limit = |v: Option<u32>| v.unwrap_or(u32::MAX);
        Self {
            jobd: jobd_name.to_string(),
            jobq: format!("{}/{}", d.jobq.0, d.jobq.1),
            jobpty: d.jobpty,
            outpty: d.outpty,
            prtdev,
            outq,
            prttxt: or_sysval(&d.prttxt, sv, "QPRTTXT"),
            rtgdta: d.rtgdta.clone(),
            rqsdta: if d.rqsdta == "*NONE" {
                String::new()
            } else {
                d.rqsdta.clone()
            },
            libl: LibList {
                sys: words(&sv("QSYSLIBL")),
                prd: Vec::new(),
                cur: None,
                usr,
            },
            aspgrp: d.inlaspgrp.clone(),
            log_level: d.log_level,
            log_sev: d.log_sev,
            log_text: d.log_text.clone(),
            logclpgm: d.logclpgm.clone(),
            logoutput: or_sysval(&d.logoutput, sv, "QLOGOUTPUT"),
            inqmsgrpy: d.inqmsgrpy.clone(),
            devrcyacn: or_sysval(&d.devrcyacn, sv, "QDEVRCYACN"),
            tsepool: or_sysval(&d.tsepool, sv, "QTSEPOOL"),
            endsev: d.endsev,
            syntax: d.syntax.unwrap_or(255),
            jobmsgqmx: d
                .jobmsgqmx
                .unwrap_or_else(|| u16::try_from(num("QJOBMSGQMX", 16)).unwrap_or(16)),
            jobmsgqfl: or_sysval(&d.jobmsgqfl, sv, "QJOBMSGQFL"),
            alwmltthd: d.alwmltthd,
            splfacn: or_sysval(&d.splfacn, sv, "QSPLFACN"),
            ddmcnv: d.ddmcnv.clone(),
            runpty: c.runpty,
            timeslice: c.timeslice,
            purge: c.purge,
            dftwait: limit(c.dftwait),
            cputime: limit(c.cputime),
            maxtmpstg: limit(c.maxtmpstg),
            maxthd: limit(c.maxthd),
            ccsid: num("QCCSID", 65535),
            dftccsid: 37,
            langid: sv("QLANGID"),
            cntryid: sv("QCNTRYID"),
            datfmt: sv("QDATFMT"),
            datsep: sv("QDATSEP"),
            timsep: sv("QTIMSEP"),
            decfmt: sv("QDECFMT"),
            srtseq: sv("QSRTSEQ"),
            brkmsg: "*NORMAL".into(),
            stsmsg: "*SYSVAL".into(),
            scddate: String::new(),
            scdtime: String::new(),
            date: String::new(),
            sws: "00000000".into(),
            prtkeyfmt: "*SYSVAL".into(),
            dupjobopt: "*SELECT".into(),
        }
    }

    /// `CHGJOB KEYWORD(value)` for one keyword. Keywords this does not
    /// know, or values out of range, are errors.
    #[allow(clippy::too_many_lines)] // one arm per keyword
    pub fn change(&mut self, kw: &str, value: &str) -> Result<()> {
        let bad = || Error::value("CPD0084", format!("{kw}({value}) is not valid"));
        let small = |lo: u8, hi: u8| -> Result<u8> {
            value
                .parse::<u8>()
                .ok()
                .filter(|v| (lo..=hi).contains(v))
                .ok_or_else(bad)
        };
        let v = value.to_ascii_uppercase();
        match kw {
            "JOBPTY" => self.jobpty = small(0, 9)?,
            "OUTPTY" => self.outpty = small(0, 9)?,
            "RUNPTY" => self.runpty = small(1, 99)?,
            "PRTDEV" => self.prtdev = v,
            "OUTQ" => self.outq = v,
            "PRTTXT" => self.prttxt = value.trim_matches('\'').to_string(),
            "LOG" => {
                let w = words(&v);
                if w.len() != 3 {
                    return Err(bad());
                }
                let level = w[0]
                    .parse::<u8>()
                    .ok()
                    .filter(|l| *l <= 4)
                    .ok_or_else(bad)?;
                let sev = w[1]
                    .parse::<u8>()
                    .ok()
                    .filter(|s| *s <= 99)
                    .ok_or_else(bad)?;
                if !matches!(w[2].as_str(), "*MSG" | "*SECLVL" | "*NOLIST") {
                    return Err(bad());
                }
                self.log_level = level;
                self.log_sev = sev;
                self.log_text.clone_from(&w[2]);
            }
            "LOGCLPGM" if matches!(v.as_str(), "*YES" | "*NO") => self.logclpgm = v,
            "INQMSGRPY" if matches!(v.as_str(), "*RQD" | "*DFT" | "*SYSRPYL") => {
                self.inqmsgrpy = v;
            }
            "BRKMSG" if matches!(v.as_str(), "*NORMAL" | "*NOTIFY" | "*HOLD") => self.brkmsg = v,
            "STSMSG" if matches!(v.as_str(), "*NORMAL" | "*NONE" | "*USRPRF" | "*SYSVAL") => {
                self.stsmsg = v;
            }
            "DATFMT" if matches!(v.as_str(), "*YMD" | "*MDY" | "*DMY" | "*JUL") => {
                self.datfmt = v;
            }
            "DATSEP" | "TIMSEP" => {
                let c = value.trim_matches('\'');
                if c.chars().count() != 1 {
                    return Err(bad());
                }
                if kw == "DATSEP" {
                    self.datsep = c.into();
                } else {
                    self.timsep = c.into();
                }
            }
            "DECFMT" => {
                self.decfmt = match v.as_str() {
                    "*BLANK" | "" => String::new(),
                    "J" | "I" => v,
                    _ => return Err(bad()),
                };
            }
            "SWS" => {
                let s = value.trim_matches('\'');
                if s.len() != 8 || !s.chars().all(|c| matches!(c, '0' | '1' | 'X')) {
                    return Err(bad());
                }
                // X keeps the switch as it is.
                self.sws = s
                    .chars()
                    .zip(self.sws.chars())
                    .map(|(n, o)| if n == 'X' { o } else { n })
                    .collect();
            }
            "DATE" => {
                if v.len() != 8 || !v.chars().all(|c| c.is_ascii_digit()) {
                    return Err(bad());
                }
                self.date = v;
            }
            "CCSID" => self.ccsid = value.parse().map_err(|_| bad())?,
            "LANGID" => self.langid = v,
            "CNTRYID" => self.cntryid = v,
            "SRTSEQ" => self.srtseq = v,
            "TIMESLICE" => self.timeslice = value.parse().map_err(|_| bad())?,
            "PURGE" if matches!(v.as_str(), "*YES" | "*NO") => self.purge = v == "*YES",
            "DFTWAIT" => {
                self.dftwait = if v == "*NOMAX" {
                    u32::MAX
                } else {
                    value.parse().map_err(|_| bad())?
                };
            }
            "CPUTIME" => {
                self.cputime = if v == "*NOMAX" {
                    u32::MAX
                } else {
                    value.parse().map_err(|_| bad())?
                };
            }
            "JOBMSGQFL" if matches!(v.as_str(), "*NOWRAP" | "*WRAP" | "*PRTWRAP") => {
                self.jobmsgqfl = v;
            }
            "JOBMSGQMX" => {
                self.jobmsgqmx = value
                    .parse()
                    .ok()
                    .filter(|n| (2..=64).contains(n))
                    .ok_or_else(bad)?;
            }
            "DDMCNV" if matches!(v.as_str(), "*KEEP" | "*DROP") => self.ddmcnv = v,
            "SPLFACN" if matches!(v.as_str(), "*KEEP" | "*DETACH") => self.splfacn = v,
            "LOGOUTPUT" if matches!(v.as_str(), "*JOBLOGSVR" | "*JOBEND" | "*PND") => {
                self.logoutput = v;
            }
            "TSEPOOL" if matches!(v.as_str(), "*NONE" | "*BASE") => self.tsepool = v,
            "PRTKEYFMT" => self.prtkeyfmt = v,
            "DUPJOBOPT" if matches!(v.as_str(), "*SELECT" | "*MSG") => self.dupjobopt = v,
            _ => return Err(bad()),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sv(name: &str) -> String {
        match name {
            "QSYSLIBL" => "QSYS QSYS2 QHLPSYS".into(),
            "QUSRLIBL" => "QGPL QTEMP".into(),
            "QPRTDEV" => "PRT01".into(),
            "QLOGOUTPUT" => "*JOBEND".into(),
            "QJOBMSGQMX" => "16".into(),
            "QJOBMSGQFL" => "*NOWRAP".into(),
            "QCCSID" => "65535".into(),
            "QDATFMT" => "*MDY".into(),
            _ => String::new(),
        }
    }

    #[test]
    fn resolve() {
        let a = JobAttrs::resolve("QGPL/QDFTJOBD", &JobD::default(), &Cls::default(), &sv);
        assert_eq!(
            a.libl.search(),
            ["QSYS", "QSYS2", "QHLPSYS", "QGPL", "QTEMP"]
        );
        assert_eq!(a.prtdev, "PRT01");
        assert_eq!(a.outq, "QGPL/QPRINT");
        assert_eq!(a.logoutput, "*JOBEND");
        assert_eq!(a.jobmsgqmx, 16);
        assert_eq!(a.syntax, 255);
        assert_eq!(a.runpty, 50);
        assert_eq!(a.dftwait, 120);
        assert_eq!(a.cputime, u32::MAX);
        let back: JobAttrs = ipc::decode(&ipc::encode(&a)).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn change() {
        let mut a = JobAttrs::resolve("X", &JobD::default(), &Cls::default(), &sv);
        a.change("JOBPTY", "3").unwrap();
        assert!(a.change("JOBPTY", "10").is_err());
        a.change("LOG", "2 20 *SECLVL").unwrap();
        assert_eq!(
            (a.log_level, a.log_sev, a.log_text.as_str()),
            (2, 20, "*SECLVL")
        );
        a.change("SWS", "1X0X0000").unwrap();
        assert_eq!(a.sws, "10000000");
        assert!(a.change("NOSUCH", "1").is_err());
        assert!(a.change("INQMSGRPY", "*MAYBE").is_err());
    }

    #[test]
    fn libl() {
        let mut l = LibList {
            sys: vec!["QSYS".into()],
            usr: vec!["QGPL".into()],
            ..LibList::default()
        };
        l.add("A", true).unwrap();
        l.add("B", false).unwrap();
        assert_eq!(l.usr, ["A", "QGPL", "B"]);
        assert!(l.add("QSYS", false).is_err());
        l.remove("A").unwrap();
        assert!(l.remove("A").is_err());
        assert!(l.change(vec!["X".into(), "X".into()]).is_err());
        l.cur = Some("C".into());
        assert_eq!(l.search(), ["QSYS", "C", "QGPL", "B"]);
    }
}
