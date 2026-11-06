// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `RCVJRNE`: a journal's entries handed one at a time to the exit
//! program, called here in the job with IBM's two parameters: the entry
//! in the `*TYPE1` layout (`QJORDJE`), a zoned length of zeros after it,
//! and a three-byte indicator (`1` an entry, `0` none now, then `Y` or
//! `N` for more waiting, then `1`). A program that sets the first byte
//! to `9` ends the command. With `TOENT(*NONE)` (the default) it waits
//! `DELAY` seconds for more entries and goes on until the program ends
//! it or the job ends.

use std::fmt::Write as _;
use std::time::Duration;

use cmd::{Resolved, Value};
use mi::Job;
use stg::Client;

use qir::MsgType;

use super::{Fail, log};
use crate::console::host::Host;

/// The entry buffer: enough for a CL program's `*CHAR 9999` and more.
const BUFFER: usize = 32_767;

fn text(v: &Value) -> String {
    v.source().trim().trim_matches('\'').trim().to_ascii_uppercase()
}

fn parm(r: &Resolved, kwd: &str) -> String {
    r.get(kwd).and_then(|b| b.value.as_ref()).map(text).unwrap_or_default()
}

fn given(r: &Resolved, kwd: &str) -> bool {
    r.get(kwd).is_some_and(|b| b.specified)
}

fn qual(r: &Resolved, kwd: &str) -> (String, String) {
    match r.get(kwd).and_then(|b| b.value.as_ref()) {
        Some(Value::Qual(p)) => (
            p.first().map(text).unwrap_or_default(),
            p.get(1).map(text).filter(|l| !l.is_empty()).unwrap_or_else(|| "*LIBL".into()),
        ),
        Some(v) => (text(v), "*LIBL".into()),
        None => (String::new(), "*LIBL".into()),
    }
}

/// The values of a list parameter, each as its first element.
fn firsts(r: &Resolved, kwd: &str) -> Vec<String> {
    let first = |v: &Value| match v {
        Value::Elems(e) => e.first().and_then(Option::as_ref).map(text).unwrap_or_default(),
        Value::Qual(p) => {
            let n = p.first().map(text).unwrap_or_default();
            match p.get(1).map(text).filter(|l| !l.is_empty() && l != "*LIBL") {
                Some(l) => format!("{l}/{n}"),
                None => n,
            }
        }
        other => text(other),
    };
    match r.get(kwd).and_then(|b| b.value.as_ref()) {
        Some(Value::List(items)) => items.iter().map(first).collect(),
        Some(v) => vec![first(v)],
        None => Vec::new(),
    }
}

/// What this release's RCVJRNE does not take, refused by name.
fn unsupported(r: &Resolved) -> Option<Fail> {
    let refuse = |what: &str| {
        // BLOCKED(rel): RCVJRNE passes *TYPE1 entries one at a time from
        // the current receiver chain; the other formats, block mode, the
        // object and path selections, times and large sequence numbers
        // are refused.
        Some(Fail::msg("CPF3C3C", &[(what, 10)]))
    };
    let fmt = parm(r, "ENTFMT");
    if !(fmt.is_empty() || fmt == "*TYPE1") {
        return refuse("ENTFMT");
    }
    for k in ["OBJ", "OBJPATH", "PATTERN", "FROMTIME", "TOTIME", "FROMENTLRG", "TOENTLRG", "CCIDLRG", "CMTCYCID", "PGM", "USRPRF", "NULLINDLEN"] {
        if given(r, k) {
            return refuse(k);
        }
    }
    if given(r, "BLKLEN") && parm(r, "BLKLEN") != "*NONE" {
        return refuse("BLKLEN");
    }
    if given(r, "RCVRNG") && parm(r, "RCVRNG") != "*CURRENT" {
        return refuse("RCVRNG");
    }
    None
}

/// The service's `dspjrn` filter line for the entries from `from`.
fn filter(r: &Resolved, jrn: &str, from: u64, to: Option<u64>) -> String {
    let mut line = format!("{} {jrn} FROMENT({from})", "DSPJRN");
    if let Some(t) = to {
        let _ = write!(line, " TOENT({t})");
    }
    let files = firsts(r, "FILE");
    if let Some(f) = files.first().filter(|f| !f.is_empty() && *f != "*ALLFILE") {
        let _ = write!(line, " FILE({f})");
    }
    let codes: Vec<String> = firsts(r, "JRNCDE").into_iter().filter(|c| !c.starts_with('*')).collect();
    if !codes.is_empty() {
        let _ = write!(line, " JRNCDE({})", codes.join(" "));
    }
    let types: Vec<String> = firsts(r, "ENTTYP").into_iter().filter(|t| !t.starts_with('*')).collect();
    if !types.is_empty() {
        let _ = write!(line, " ENTTYP({})", types.join(" "));
    } else if firsts(r, "ENTTYP").iter().any(|t| t == "*RCD" || t == "*R") {
        line.push_str(" JRNCDE(R)");
    }
    if let Some(Value::Qual(p)) = r.get("JOB").and_then(|b| b.value.as_ref())
        && let Some(n) = p.get(2).map(text).filter(|n| !n.is_empty())
    {
        let _ = write!(line, " JOB({n})");
    }
    line
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .filter_map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok())
        .collect()
}

/// The entries from the service, and the last sequence number.
fn entries(host: &Host, line: &str) -> Result<(Vec<Vec<u8>>, u64), Fail> {
    let (vars, lists) = host
        .link
        .borrow_mut()
        .with(|l| l.sys2_panel("JRNE".into(), vec![line.to_string()]))
        .map_err(|e| Fail::read("RCVJRNE", &e))?;
    let last = vars
        .iter()
        .find(|(k, _)| k == "LAST")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let recs = lists
        .into_iter()
        .filter(|(n, _)| n == "ENTRIES")
        .flat_map(|(_, rows)| rows)
        .filter_map(|row| row.into_iter().find(|(k, _)| k == "REC").map(|(_, v)| unhex(&v)))
        .collect();
    Ok((recs, last))
}

/// Call the exit program; whether it asked to end (`9`).
fn call(job: &mut Job, (pgm, lib): (&str, &str), entry: Option<&[u8]>, more: bool) -> Result<bool, Fail> {
    let mut buf = vec![0x40u8; BUFFER];
    if let Some(e) = entry {
        let n = e.len().min(BUFFER - 5);
        buf[..n].copy_from_slice(&e[..n]);
        // A zoned length of zeros ends the entries passed.
        buf[n..n + 5].fill(0xF0);
    } else {
        buf[..5].fill(0xF0);
    }
    let digit = |c: char| ccsid::byte_of(c).unwrap_or(0x40);
    let mut ind = vec![
        digit(if entry.is_some() { '1' } else { '0' }),
        digit(if more { 'Y' } else { 'N' }),
        digit(if entry.is_some() { '1' } else { '0' }),
    ];
    let addrs = [buf.as_mut_ptr() as u64, ind.as_mut_ptr() as u64];
    let failed = || Fail::msg("CPF70AF", &[(pgm, 10), (lib, 10)]);
    job.call_program(pgm, lib, &addrs).map_err(|_| failed())?;
    if job.clear().is_some() {
        return Err(failed());
    }
    Ok(ind[0] == digit('9'))
}

/// `RCVJRNE`.
pub fn run(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    if let Some(f) = unsupported(r) {
        return Err(f);
    }
    let (jname, jlib) = qual(r, "JRN");
    let jrn = if jlib == "*LIBL" { jname.clone() } else { format!("{jlib}/{jname}") };
    let (pname, plib) = qual(r, "EXITPGM");
    let limit: Option<usize> = match parm(r, "NBRENT").as_str() {
        "*ALL" | "" | "0" => None,
        n => n.parse().ok(),
    };
    let to = match parm(r, "TOENT").as_str() {
        "*NONE" | "" | "-2" => None,
        "*LAST" | "-1" => Some(u64::MAX),
        n => n.parse().ok(),
    };
    let wait = match r.get("DELAY").and_then(|b| b.value.as_ref()) {
        Some(Value::Elems(e)) => e.first().and_then(Option::as_ref).map(text),
        Some(v) => Some(text(v)),
        None => None,
    };
    let wait = match wait.as_deref() {
        Some("*NEXTENT" | "0") => 1,
        Some(n) => n.parse().unwrap_or(30),
        None => 30,
    };
    let mut from: u64 = match parm(r, "FROMENT").as_str() {
        "*FIRST" | "" | "0" => 1,
        n => n.parse().unwrap_or(1),
    };
    let mut passed = 0usize;
    loop {
        let bound = to.filter(|t| *t != u64::MAX);
        let (recs, last) = entries(host, &filter(r, &jrn, from, bound))?;
        let n = recs.len();
        for (i, rec) in recs.iter().enumerate() {
            if limit.is_some_and(|l| passed >= l) {
                return Ok(done(job, &jname, &jlib, passed));
            }
            passed += 1;
            if call(job, (&pname, &plib), Some(rec), i + 1 < n)? {
                return Ok(done(job, &jname, &jlib, passed));
            }
        }
        if last > 0 {
            from = last + 1;
        }
        if to.is_some() || limit.is_some_and(|l| passed >= l) {
            if passed == 0 {
                return Err(Fail::msg("CPF7062", &[(&jname, 10), (&jlib, 10)]));
            }
            return Ok(done(job, &jname, &jlib, passed));
        }
        // No more now: the program hears so, then the wait.
        if call(job, (&pname, &plib), None, false)? {
            return Ok(done(job, &jname, &jlib, passed));
        }
        std::thread::sleep(Duration::from_secs(wait));
        host.attention(job);
    }
}

fn done(job: &mut Job, jname: &str, jlib: &str, n: usize) -> String {
    let count = n.to_string();
    let data = cmd::error::data(&[(jname, 10), (jlib, 10), ("", 10), (&count, 10)]);
    log(job, "CPC7061", &data, MsgType::Comp, None);
    String::new()
}
