// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Building `*JOBD` and `*CLS` from command parameters (`CRTJOBD`,
//! `CHGJOBD`, `CRTCLS`, `CHGCLS`) and showing them (`DSPJOBD`,
//! `DSPCLS`). A change starts from the object as it is; a create from
//! the command's defaults.

use std::fmt::Write as _;

use super::cls::Cls;
use super::jobd::JobD;
use crate::cmd::Params;
use crate::{Error, Result};

fn bad(kw: &str, v: &str) -> Error {
    Error::value("CPD0084", format!("{kw}({v}) is not valid"))
}

/// `LIB/NAME` or `NAME` (library `*LIBL`).
#[must_use]
pub fn qualified(v: &str) -> (String, String) {
    let v = v.to_ascii_uppercase();
    match v.split_once('/') {
        Some((l, n)) => (l.to_string(), n.to_string()),
        None => ("*LIBL".to_string(), v),
    }
}

fn number<T: std::str::FromStr>(kw: &str, v: &str) -> Result<T> {
    v.trim().parse().map_err(|_| bad(kw, v))
}

fn yes_no(kw: &str, v: &str) -> Result<bool> {
    match v.to_ascii_uppercase().as_str() {
        "*YES" => Ok(true),
        "*NO" => Ok(false),
        _ => Err(bad(kw, v)),
    }
}

fn limit(kw: &str, v: &str) -> Result<Option<u32>> {
    if v.eq_ignore_ascii_case("*NOMAX") {
        Ok(None)
    } else {
        number(kw, v).map(Some)
    }
}

/// Apply the `CRTJOBD`/`CHGJOBD` keywords of `p` to `d`.
#[allow(clippy::many_single_char_names)] // d, p, v as in cls below
pub fn jobd(mut d: JobD, p: &Params) -> Result<JobD> {
    for (kw, raw) in &p.kw {
        let v = raw.trim();
        let up = v.to_ascii_uppercase();
        match kw.as_str() {
            "JOBD" => {}
            "USER" => d.user = up,
            "JOBQ" => d.jobq = qualified(v),
            "JOBPTY" => d.jobpty = number(kw, v)?,
            "OUTPTY" => d.outpty = number(kw, v)?,
            "PRTDEV" => d.prtdev = up,
            "OUTQ" => {
                d.outq = if up.starts_with('*') {
                    (String::new(), up)
                } else {
                    qualified(v)
                };
            }
            "PRTTXT" => d.prttxt = v.trim_matches('\'').to_string(),
            "RTGDTA" => d.rtgdta = v.trim_matches('\'').to_string(),
            "RQSDTA" => d.rqsdta = v.trim_matches('\'').replace("''", "'"),
            "INLLIBL" => {
                d.inllibl = match up.as_str() {
                    "*SYSVAL" => None,
                    "*NONE" => Some(Vec::new()),
                    _ => Some(up.split_whitespace().map(str::to_string).collect()),
                };
            }
            "INLASPGRP" => d.inlaspgrp = up,
            "LOG" => {
                let w: Vec<&str> = up.split_whitespace().collect();
                let [l, s, t] = w.as_slice() else {
                    return Err(bad(kw, v));
                };
                d.log_level = number(kw, l)?;
                d.log_sev = number(kw, s)?;
                d.log_text = (*t).to_string();
            }
            "LOGCLPGM" => d.logclpgm = up,
            "LOGOUTPUT" => d.logoutput = up,
            "INQMSGRPY" => d.inqmsgrpy = up,
            "HOLD" => d.hold = yes_no(kw, v)?,
            "DEVRCYACN" => d.devrcyacn = up,
            "TSEPOOL" => d.tsepool = up,
            "ENDSEV" => d.endsev = number(kw, v)?,
            "SYNTAX" => {
                d.syntax = if up == "*NOCHK" {
                    None
                } else {
                    Some(number(kw, v)?)
                };
            }
            "JOBMSGQMX" => {
                d.jobmsgqmx = if up == "*SYSVAL" {
                    None
                } else {
                    Some(number(kw, v)?)
                };
            }
            "JOBMSGQFL" => d.jobmsgqfl = up,
            "ALWMLTTHD" => d.alwmltthd = yes_no(kw, v)?,
            "SPLFACN" => d.splfacn = up,
            "DDMCNV" => d.ddmcnv = up,
            "TEXT" => d.text = v.trim_matches('\'').replace("''", "'"),
            _ => {
                return Err(Error::value(
                    "CPD0043",
                    format!("keyword {kw} not valid for this command"),
                ));
            }
        }
    }
    d.validate()?;
    Ok(d)
}

/// Apply the `CRTCLS`/`CHGCLS` keywords of `p` to `c`.
pub fn cls(mut c: Cls, p: &Params) -> Result<Cls> {
    for (kw, raw) in &p.kw {
        let v = raw.trim();
        match kw.as_str() {
            "CLS" => {}
            "RUNPTY" => c.runpty = number(kw, v)?,
            "TIMESLICE" => c.timeslice = number(kw, v)?,
            "PURGE" => c.purge = yes_no(kw, v)?,
            "DFTWAIT" => c.dftwait = limit(kw, v)?,
            "CPUTIME" => c.cputime = limit(kw, v)?,
            "MAXTMPSTG" => c.maxtmpstg = limit(kw, v)?,
            "MAXTHD" => c.maxthd = limit(kw, v)?,
            "TEXT" => c.text = v.trim_matches('\'').replace("''", "'"),
            _ => {
                return Err(Error::value(
                    "CPD0043",
                    format!("keyword {kw} not valid for this command"),
                ));
            }
        }
    }
    c.validate()?;
    Ok(c)
}

fn or_star(v: Option<impl std::fmt::Display>, star: &str) -> String {
    v.map_or_else(|| star.to_string(), |v| v.to_string())
}

/// `DSPJOBD`: the attributes, one per line, IBM's order.
#[must_use]
pub fn show_jobd(lib: &str, name: &str, d: &JobD) -> String {
    let mut out = format!("Job description:   {name}     Library:   {lib}\n\n");
    let q = |l: &str, n: &str| {
        if l.is_empty() {
            n.to_string()
        } else {
            format!("{l}/{n}")
        }
    };
    let rows: Vec<(&str, String)> = vec![
        ("User profile", d.user.clone()),
        ("CL syntax check", or_star(d.syntax, "*NOCHK")),
        (
            "Hold on job queue",
            if d.hold { "*YES" } else { "*NO" }.into(),
        ),
        ("End severity", d.endsev.to_string()),
        ("Job date", "*SYSVAL".into()),
        ("Job switches", "00000000".into()),
        ("Inquiry message reply", d.inqmsgrpy.clone()),
        ("Job priority (on job queue)", d.jobpty.to_string()),
        ("Job queue", q(&d.jobq.0, &d.jobq.1)),
        ("Output priority (on output queue)", d.outpty.to_string()),
        ("Printer device", d.prtdev.clone()),
        ("Output queue", q(&d.outq.0, &d.outq.1)),
        ("Message logging: Level", d.log_level.to_string()),
        ("Message logging: Severity", d.log_sev.to_string()),
        ("Message logging: Text", d.log_text.clone()),
        ("Log CL program commands", d.logclpgm.clone()),
        ("Job log output", d.logoutput.clone()),
        (
            "Job message queue maximum size",
            or_star(d.jobmsgqmx, "*SYSVAL"),
        ),
        ("Job message queue full action", d.jobmsgqfl.clone()),
        (
            "Allow multiple threads",
            if d.alwmltthd { "*YES" } else { "*NO" }.into(),
        ),
        ("Spooled file action", d.splfacn.clone()),
        ("Device recovery action", d.devrcyacn.clone()),
        ("Time slice end pool", d.tsepool.clone()),
        ("DDM conversation", d.ddmcnv.clone()),
        ("Print text", d.prttxt.clone()),
        ("Routing data", d.rtgdta.clone()),
        ("Request data or command", d.rqsdta.clone()),
        ("Initial ASP group", d.inlaspgrp.clone()),
        (
            "Initial library list",
            d.inllibl
                .as_ref()
                .map_or_else(|| "*SYSVAL".to_string(), |l| l.join(" ")),
        ),
        ("Text", d.text.clone()),
    ];
    for (k, v) in rows {
        let _ = writeln!(out, "  {k:<38} {v}");
    }
    out
}

/// `DSPCLS`.
#[must_use]
pub fn show_cls(lib: &str, name: &str, c: &Cls) -> String {
    let mut out = format!("Class:   {name}     Library:   {lib}\n\n");
    let rows: Vec<(&str, String)> = vec![
        ("Run priority", c.runpty.to_string()),
        ("Time slice in milliseconds", c.timeslice.to_string()),
        (
            "Eligible for purge",
            if c.purge { "*YES" } else { "*NO" }.into(),
        ),
        ("Default wait time in seconds", or_star(c.dftwait, "*NOMAX")),
        (
            "Maximum CPU time in milliseconds",
            or_star(c.cputime, "*NOMAX"),
        ),
        (
            "Maximum temporary storage in megabytes",
            or_star(c.maxtmpstg, "*NOMAX"),
        ),
        ("Maximum threads", or_star(c.maxthd, "*NOMAX")),
        ("Text", c.text.clone()),
    ];
    for (k, v) in rows {
        let _ = writeln!(out, "  {k:<40} {v}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params() {
        let p = Params::parse(
            "crtjobd QGPL/X USER(QSECOFR) JOBQ(QGPL/QPGMR) LOG(2 10 *SECLVL) INLLIBL(QGPL QTEMP) HOLD(*YES) TEXT('It''s')",
        );
        let d = jobd(JobD::default(), &p).unwrap();
        assert_eq!(d.user, "QSECOFR");
        assert_eq!(d.jobq, ("QGPL".into(), "QPGMR".into()));
        assert_eq!((d.log_level, d.log_sev), (2, 10));
        assert_eq!(d.inllibl, Some(vec!["QGPL".into(), "QTEMP".into()]));
        assert!(d.hold);
        assert_eq!(d.text, "It's");
        assert!(jobd(JobD::default(), &Params::parse("x X JOBPTY(12)")).is_err());
        assert!(jobd(JobD::default(), &Params::parse("x X BOGUS(1)")).is_err());
        let c = cls(
            Cls::default(),
            &Params::parse("crtcls X RUNPTY(20) CPUTIME(*NOMAX) DFTWAIT(30)"),
        )
        .unwrap();
        assert_eq!((c.runpty, c.cputime, c.dftwait), (20, None, Some(30)));
        assert!(show_jobd("QGPL", "X", &d).contains("*SECLVL"));
        assert!(show_cls("QGPL", "X", &c).contains("*NOMAX"));
    }
}
