// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Time zone descriptions: `CRTTIMZON`, `CHGTIMZON`, `DLTTIMZON` and the
//! text form of `WRKTIMZON` (the panels are the job's). The objects
//! live in QSYS; after each change the service installs the set again
//! (`stgsvc::timzon::reload`), so `QTIMZON` follows at once.

use std::fmt::Write as _;

use cmd::block::Parms;
use cmd::value::Value;
use qsname::Name;
use sec::profile::SpecialAuth;
use stg::proto::{CmdMsg, CmdReply};
use store::obj::library::Library;
use store::obj::{JobHandle, ops, types};
use sysval::tz::{DstRule, Edge, Zone};

use super::direct::failed;
use super::done;
use crate::ipl::system::{System, now_unix};
use crate::stgsvc::spool::with_store;

const MONTHS: [&str; 12] = ["*JAN", "*FEB", "*MAR", "*APR", "*MAY", "*JUN", "*JUL", "*AUG", "*SEP", "*OCT", "*NOV", "*DEC"];
const DAYS: [&str; 7] = ["*SUN", "*MON", "*TUE", "*WED", "*THU", "*FRI", "*SAT"];
/// `*SAME` of the numeric parameters.
const SAME: i64 = 9999;

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "CRTTIMZON" => crt(system, parms),
        "CHGTIMZON" => chg(system, parms),
        "DLTTIMZON" => dlt(system, parms),
        "WRKTIMZON" => Ok(CmdReply { text: listing(system, &parms.name("TIMZON")), ..CmdReply::default() }),
        _ => return None,
    };
    Some(r.unwrap_or_else(|e| *e))
}

/// An escape with its data.
fn fail(msgid: &str, fields: &[(&str, usize)]) -> Box<CmdReply> {
    Box::new(CmdReply {
        text: String::new(),
        escape: Some(CmdMsg { msgid: msgid.into(), kind: 7, data: cmd::error::data(fields) }),
        ..CmdReply::default()
    })
}

fn not_found(name: &str) -> Box<CmdReply> {
    fail(
        "CPF2105",
        &[(name, 10), ("QSYS", 10), ("*TIMZON", 10)],
    )
}

fn text_of(v: Option<&Value>) -> Option<String> {
    v.map(|v| v.text().map_or_else(|| v.source(), str::to_string).trim_end().to_string())
}

fn int_of(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Num(d)) => d.mantissa_i128().and_then(|m| i64::try_from(m).ok()),
        Some(Value::Char(s)) => s.trim().parse().ok(),
        _ => None,
    }
}

/// `DSTSTR`/`DSTEND`: month, day, relative day, time; `None` for
/// `*SAME` or when not given.
fn edge(parms: &Parms, kwd: &str) -> Result<Option<Edge>, Box<CmdReply>> {
    let elems = parms.elems(kwd);
    if elems.is_empty() || matches!(elems.as_slice(), [Some(Value::Char(v))] if v.eq_ignore_ascii_case("*SAME")) {
        return Ok(None);
    }
    let bad = || fail("CPD0072", &[(kwd, 10)]);
    let get = |i: usize| elems.get(i).copied().flatten().and_then(|v| text_of(Some(v))).map(|v| v.to_ascii_uppercase());
    let (Some(mon), Some(day), Some(rel), Some(time)) = (get(0), get(1), get(2), get(3)) else {
        return Err(bad());
    };
    let month = MONTHS.iter().position(|x| *x == mon).ok_or_else(bad)?;
    let weekday = DAYS.iter().position(|x| *x == day).ok_or_else(bad)?;
    let week: u8 = if rel == "*LAST" { 5 } else { rel.parse().map_err(|_| bad())? };
    let digits: String = time.chars().filter(char::is_ascii_digit).collect();
    let num: u32 = digits.parse().map_err(|_| bad())?;
    let (hh, mm, ss) = if digits.len() <= 4 { (num / 100, num % 100, 0) } else { (num / 10_000, num / 100 % 100, num % 100) };
    if hh > 23 || mm > 59 || ss > 59 {
        return Err(bad());
    }
    Ok(Some(Edge {
        month: u8::try_from(month + 1).unwrap_or(1),
        weekday: u8::try_from(weekday).unwrap_or(0),
        week,
        secs: hh * 3600 + mm * 60 + ss,
    }))
}

/// `UTC-05:00S`: the generated abbreviation of an offset.
fn generated(offset: i32, dst: bool) -> String {
    let sign = if offset < 0 { '-' } else { '+' };
    format!("UTC{sign}{:02}:{:02}{}", offset.abs() / 60, offset.abs() % 60, if dst { 'D' } else { 'S' })
}

/// The names a message gives: its text as the full name, the
/// abbreviation of the shipped zone that uses it.
fn from_msg(system: &mut System, msgid: &str, dst: bool) -> (String, String) {
    let full = crate::stgsvc::msgs::text(system, msgid, &[]);
    let abbr = sysval::tz::shipped()
        .iter()
        .find_map(|z| {
            if !dst && z.std_msg == msgid {
                Some(z.std_abbr.clone())
            } else if dst && z.dst_msg == msgid {
                Some(z.dst_abbr.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();
    (abbr, full.trim().to_string())
}

/// Apply `STDNAME`/`DSTNAME` and their messages to `z`.
fn names(system: &mut System, parms: &Parms, z: &mut Zone, create: bool) -> Result<(), Box<CmdReply>> {
    let std = parms.elems("STDNAME");
    let (abbr, full) = (text_of(std.first().copied().flatten()), text_of(std.get(1).copied().flatten()));
    let abbr = abbr.unwrap_or_else(|| if create { "*GEN".into() } else { "*SAME".into() });
    match abbr.to_ascii_uppercase().as_str() {
        "*SAME" => {}
        "*GEN" => {
            z.std_abbr = generated(z.offset, false);
            z.std_full = full.unwrap_or_default();
            z.std_msg.clear();
        }
        "*MSG" => {
            let id = parms.name("STDMSG");
            if id.is_empty() || id == "*SAME" {
                if z.std_msg.is_empty() {
                    return Err(fail("CPD0071", &[("STDMSG", 10)]));
                }
            } else {
                z.std_msg = id;
            }
            let id = z.std_msg.clone();
            (z.std_abbr, z.std_full) = from_msg(system, &id, false);
        }
        _ => {
            z.std_abbr = abbr;
            z.std_full = full.unwrap_or_default();
            z.std_msg.clear();
        }
    }
    let dst = parms.elems("DSTNAME");
    let (abbr, full) = (text_of(dst.first().copied().flatten()), text_of(dst.get(1).copied().flatten()));
    let abbr = abbr.unwrap_or_else(|| if create { "*NONE".into() } else { "*SAME".into() });
    match abbr.to_ascii_uppercase().as_str() {
        "*SAME" => {}
        "*NONE" => {
            z.dst_abbr.clear();
            z.dst_full.clear();
            z.dst_msg.clear();
            z.dst = None;
        }
        "*MSG" => {
            let id = parms.name("DSTMSG");
            if id.is_empty() || id == "*SAME" {
                if z.dst_msg.is_empty() {
                    return Err(fail("CPD0071", &[("DSTMSG", 10)]));
                }
            } else {
                z.dst_msg = id;
            }
            let id = z.dst_msg.clone();
            (z.dst_abbr, z.dst_full) = from_msg(system, &id, true);
        }
        _ => {
            z.dst_abbr = abbr;
            z.dst_full = full.unwrap_or_default();
            z.dst_msg.clear();
        }
    }
    let (m, l) = parms.qual("MSGF");
    if !m.is_empty() && !m.eq_ignore_ascii_case("*SAME") {
        z.msgf = (m.to_ascii_uppercase(), if l.is_empty() { "*LIBL".into() } else { l.to_ascii_uppercase() });
    }
    Ok(())
}

/// Apply the rule parameters; a zone with daylight names needs both edges.
fn rule(parms: &Parms, z: &mut Zone) -> Result<(), Box<CmdReply>> {
    if z.dst_abbr.is_empty() {
        z.dst = None;
        return Ok(());
    }
    let start = edge(parms, "DSTSTR")?;
    let end = edge(parms, "DSTEND")?;
    let shift = int_of(parms.get("DSTSHIFT")).filter(|s| *s != SAME);
    let old = z.dst;
    let (Some(start), Some(end)) = (start.or(old.map(|r| r.start)), end.or(old.map(|r| r.end))) else {
        let kwd = if start.is_none() && old.is_none() { "DSTSTR" } else { "DSTEND" };
        return Err(fail("CPD0072", &[(kwd, 10)]));
    };
    let shift = shift.map_or_else(|| old.map_or(60, |r| r.shift), |s| u16::try_from(s).unwrap_or(60));
    z.dst = Some(DstRule { start, end, shift });
    Ok(())
}

/// The rest: text, year offset, alternate name.
fn rest(parms: &Parms, z: &mut Zone) {
    let t = parms.text("TEXT");
    if parms.get("TEXT").is_some() && !t.eq_ignore_ascii_case("*SAME") {
        z.text = if t.eq_ignore_ascii_case("*BLANK") { String::new() } else { t };
    }
    if let Some(y) = int_of(parms.get("YEAROFS")).filter(|y| *y != SAME) {
        z.year_offset = i16::try_from(y).unwrap_or(0);
    }
    let a = parms.text("ALTNAME");
    if parms.get("ALTNAME").is_some() && !a.eq_ignore_ascii_case("*SAME") {
        z.alt = a;
    }
}

fn crt(system: &mut System, parms: &Parms) -> Result<CmdReply, Box<CmdReply>> {
    let what = txt(system, "TXT0E29", &[]);
    crate::stgsvc::security::require(SpecialAuth::ALLOBJ, &what)
        .map_err(|e| Box::new(failed("CRTTIMZON", &e)))?;
    let name = parms.name("TIMZON");
    let offset = int_of(parms.get("OFFSET")).ok_or_else(|| fail("CPD0071", &[("OFFSET", 10)]))?;
    let mut z = Zone {
        name: name.clone(),
        offset: i32::try_from(offset).unwrap_or(0),
        std_abbr: String::new(),
        std_full: String::new(),
        std_msg: String::new(),
        dst_abbr: String::new(),
        dst_full: String::new(),
        dst_msg: String::new(),
        msgf: ("QCPFMSG".into(), "*LIBL".into()),
        dst: None,
        year_offset: 0,
        alt: String::new(),
        text: String::new(),
    };
    names(system, parms, &mut z, true)?;
    rule(parms, &mut z)?;
    rest(parms, &mut z);
    let n = Name::parse(&name).map_err(|_| fail("CPD0078", &[(&name, 10)]))?;
    let user = crate::stgsvc::security::actor().map_or_else(|| "QSYS".to_string(), |p| p.user.clone());
    let made = with_store(system, |txn, qsys| {
        let lib = Library::load(txn, qsys).map_err(|e| e.to_string())?;
        if lib.lookup(txn, types::TIMZON, n).map_err(|e| e.to_string())?.is_some() {
            return Ok(false);
        }
        let stamp = wm::obj::Stamp { owner: user, system: "QS400".into(), now: now_unix() };
        wm::obj::create(txn, qsys, n, types::TIMZON, &z.text, &stamp, &wm::obj::timzon::encode(&z))
            .map_err(|e| e.to_string())?;
        Ok(true)
    })
    .map_err(|e| Box::new(super::check("CRTTIMZON", &e)))?;
    if !made {
        let what = ccsid::to_ebcdic(&name).unwrap_or_default();
        let mut data = u16::try_from(what.len()).unwrap_or(0).to_be_bytes().to_vec();
        data.extend(what);
        return Err(Box::new(CmdReply { escape: Some(CmdMsg { msgid: "CPF2112".into(), kind: 7, data }), ..CmdReply::default() }));
    }
    crate::stgsvc::timzon::reload(system);
    Ok(CmdReply { text: txt(system, "TXT0E19", &[&name]) + "\n", ..CmdReply::default() })
}

fn chg(system: &mut System, parms: &Parms) -> Result<CmdReply, Box<CmdReply>> {
    let what = txt(system, "TXT0E2A", &[]);
    crate::stgsvc::security::require(SpecialAuth::ALLOBJ, &what)
        .map_err(|e| Box::new(failed("CHGTIMZON", &e)))?;
    let name = parms.name("TIMZON");
    let Some(mut z) = load(system, &name) else {
        return Err(not_found(&name));
    };
    if let Some(o) = int_of(parms.get("OFFSET")).filter(|o| *o != SAME) {
        z.offset = i32::try_from(o).unwrap_or(z.offset);
    }
    names(system, parms, &mut z, false)?;
    rule(parms, &mut z)?;
    rest(parms, &mut z);
    let payload = wm::obj::timzon::encode(&z);
    let text = z.text.clone();
    let n = Name::parse(&name).map_err(|_| fail("CPD0078", &[(&name, 10)]))?;
    with_store(system, |txn, qsys| {
        let lib = Library::load(txn, qsys).map_err(|e| e.to_string())?;
        let id = lib.lookup(txn, types::TIMZON, n).map_err(|e| e.to_string())?.ok_or("gone")?;
        wm::obj::write(txn, id, &payload).map_err(|e| e.to_string())?;
        ops::change_text(txn, id, &text, now_unix()).map_err(|e| e.to_string())
    })
    .map_err(|_| not_found(&name))?;
    crate::stgsvc::timzon::reload(system);
    Ok(CmdReply { text: txt(system, "TXT0E1A", &[&name]) + "\n", ..CmdReply::default() })
}

fn dlt(system: &mut System, parms: &Parms) -> Result<CmdReply, Box<CmdReply>> {
    let what = txt(system, "TXT0E2B", &[]);
    crate::stgsvc::security::require(SpecialAuth::ALLOBJ, &what)
        .map_err(|e| Box::new(failed("DLTTIMZON", &e)))?;
    let name = parms.name("TIMZON");
    let current = system
        .sysvals
        .as_ref()
        .and_then(|v| v.get("QTIMZON").ok())
        .map(|v| v.text().trim().to_string())
        .unwrap_or_default();
    let prefix = name.strip_suffix('*').map(str::to_string);
    let me = JobHandle(u64::from(system.current_job));
    let (deleted, kept) = with_store(system, |txn, qsys| {
        let lib = Library::load(txn, qsys).map_err(|e| e.to_string())?;
        let mut deleted = Vec::new();
        let mut kept = Vec::new();
        for (k, id) in lib.list(txn, Some(types::TIMZON)).map_err(|e| e.to_string())? {
            let n = k.name.to_string();
            let hit = prefix.as_ref().map_or(n == name, |p| n.starts_with(p.as_str()));
            if !hit {
                continue;
            }
            if n == current {
                kept.push(n);
                continue;
            }
            ops::delete(txn, me, id).map_err(|e| e.to_string())?;
            deleted.push(n);
        }
        Ok((deleted, kept))
    })
    .map_err(|e| Box::new(super::check("DLTTIMZON", &e)))?;
    if deleted.is_empty() && kept.is_empty() {
        return Err(not_found(&name));
    }
    crate::stgsvc::timzon::reload(system);
    let mut text = String::new();
    for n in &deleted {
        let line = txt(system, "CPC2191", &[n, "QSYS", "*TIMZON"]);
        let _ = writeln!(text, "{line}");
    }
    if !kept.is_empty() {
        let line = txt(system, "TXT0E1B", &[&kept.join(", ")]);
        let _ = writeln!(text, "{line}");
        let d = i32::try_from(deleted.len()).unwrap_or(0);
        let k = i32::try_from(kept.len()).unwrap_or(0);
        let mut data = d.to_be_bytes().to_vec();
        data.extend_from_slice(&k.to_be_bytes());
        data.extend(cmd::error::data(&[("TIMZON", 7)]));
        return Err(Box::new(CmdReply { text, escape: Some(CmdMsg { msgid: "CPF2117".into(), kind: 7, data }), ..CmdReply::default() }));
    }
    if let [one] = deleted.as_slice() {
        return Ok(done(text, "CPC2191", &[(one, 10), ("QSYS", 10), ("*TIMZON", 10)]));
    }
    Ok(CmdReply { text, ..CmdReply::default() })
}

/// One description from the store.
fn load(system: &mut System, name: &str) -> Option<Zone> {
    let n = Name::parse(name).ok()?;
    with_store(system, |txn, qsys| {
        let lib = Library::load(txn, qsys).map_err(|e| e.to_string())?;
        let id = lib.lookup(txn, types::TIMZON, n).map_err(|e| e.to_string())?.ok_or("missing")?;
        let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
        wm::obj::timzon::decode(name, &b).map_err(|e| e.to_string())
    })
    .ok()
}

/// `CHGTIMZON`'s current values in keyword form (`QSPMTOVR`).
pub fn override_values(system: &mut System, key: &str) -> String {
    let p = wm::cmd::Params::parse(&format!("x {key}"));
    let Some(name) = p.upper("TIMZON") else {
        return String::new();
    };
    let Some(z) = load(system, &name) else {
        return String::new();
    };
    let q = |s: &str| format!("'{}'", s.replace('\'', "''"));
    let std = if z.std_msg.is_empty() { format!("({} {})", q(&z.std_abbr), q(&z.std_full)) } else { "(*MSG)".into() };
    let dst = if z.dst_abbr.is_empty() {
        "(*NONE)".to_string()
    } else if z.dst_msg.is_empty() {
        format!("({} {})", q(&z.dst_abbr), q(&z.dst_full))
    } else {
        "(*MSG)".into()
    };
    let mut s = format!("OFFSET({}) STDNAME{std} DSTNAME{dst}", z.offset);
    if !z.std_msg.is_empty() {
        let _ = write!(s, " STDMSG({})", z.std_msg);
    }
    if !z.dst_msg.is_empty() {
        let _ = write!(s, " DSTMSG({})", z.dst_msg);
    }
    if let Some(r) = z.dst {
        for (k, e) in [("DSTSTR", r.start), ("DSTEND", r.end)] {
            let week = if e.week >= 5 { "*LAST".to_string() } else { e.week.to_string() };
            let _ = write!(
                s,
                " {k}({} {} {week} '{:02}:{:02}:{:02}')",
                MONTHS[usize::from(e.month.clamp(1, 12) - 1)],
                DAYS[usize::from(e.weekday % 7)],
                e.secs / 3600,
                e.secs % 3600 / 60,
                e.secs % 60
            );
        }
        let _ = write!(s, " DSTSHIFT({})", r.shift);
    }
    let _ = write!(s, " TEXT({}) YEAROFS({})", if z.text.is_empty() { "*BLANK".into() } else { q(&z.text) }, z.year_offset);
    if !z.alt.is_empty() {
        let _ = write!(s, " ALTNAME({})", q(&z.alt));
    }
    s
}

/// `WRKTIMZON` without a display: the list as text.
fn listing(system: &mut System, filter: &str) -> String {
    let prefix = filter.strip_suffix('*');
    let mut zones = sysval::tz::all();
    zones.sort_by(sysval::tz::list_order);
    let mut out = txt(system, "TXT0E1D", &[]) + "\n";
    for z in zones.iter().filter(|z| match prefix {
        Some(p) => z.name.starts_with(p),
        None => filter.is_empty() || filter == "*ALL" || z.name == filter,
    }) {
        let sign = match z.offset.signum() {
            -1 => '-',
            1 => '+',
            _ => ' ',
        };
        let _ = writeln!(
            out,
            "{sign}{:02}:{:02}  {:<10}  {:<10}  {}",
            z.offset.abs() / 60,
            z.offset.abs() % 60,
            z.name,
            z.std_abbr,
            z.std_full
        );
    }
    out
}

/// `-05:00`, ` 00:00`, `+01:00`.
fn offset_text(min: i32) -> String {
    let sign = match min.signum() {
        -1 => '-',
        1 => '+',
        _ => ' ',
    };
    format!("{sign}{:02}:{:02}", min.abs() / 60, min.abs() % 60)
}

fn current_name(system: &System) -> String {
    system.sysvals.as_ref().map(sysval::SysVals::zone_name).unwrap_or_default()
}

/// `WRKTIMZON`'s list (`QSTIMZON`'s `ZONES`): the descriptions a name
/// or generic name selects, the one `QTIMZON` names marked.
pub fn list_panel(system: &mut System, filter: &str) -> stg::proto::PanelData {
    let current = current_name(system);
    let prefix = filter.strip_suffix('*');
    let mut zones = sysval::tz::all();
    zones.sort_by(sysval::tz::list_order);
    let rows = zones
        .iter()
        .filter(|z| match prefix {
            Some(p) => z.name.starts_with(p),
            None => filter.is_empty() || filter == "*ALL" || z.name == filter,
        })
        .map(|z| {
            [
                ("OFFSET", offset_text(z.offset)),
                ("MARK", if z.name == current { ">".to_string() } else { String::new() }),
                ("TIMZON", z.name.clone()),
                ("ABBR", z.std_abbr.clone()),
                ("FULL", z.std_full.clone()),
                ("DABBR", z.dst_abbr.clone()),
                ("DFULL", z.dst_full.clone()),
                ("ALT", z.alt.clone()),
                ("TEXT", z.text.clone()),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
        })
        .collect();
    (Vec::new(), rows)
}

/// A month, day or week word (`QSTXTMSG`): `base` the identifier of the
/// first of its kind (January, Sunday, First).
fn word(system: &mut System, base: u32, n: usize) -> String {
    let id = format!("TXT0E{:02X}", base + u32::try_from(n).unwrap_or(0));
    txt(system, &id, &[])
}

/// The details display's variables, `PANEL` naming the panel (`DSPTZ`
/// with daylight saving time, `DSPTZN` without).
fn detail_vars(system: &mut System, z: &Zone) -> Vec<(String, String)> {
    let mut v = vec![
        ("PANEL", if z.dst.is_some() { "DSPTZ" } else { "DSPTZN" }.to_string()),
        ("ZONE", z.name.clone()),
        ("ZOFFSET", offset_text(z.offset)),
        ("STDFULL", z.std_full.clone()),
        ("STDABBR", z.std_abbr.clone()),
        ("STDMSG", z.std_msg.clone()),
        ("DSTFULL", z.dst_full.clone()),
        ("DSTABBR", z.dst_abbr.clone()),
        ("DSTMSG", z.dst_msg.clone()),
        ("MSGF", z.msgf.0.clone()),
        ("MSGFLIB", z.msgf.1.clone()),
        ("YEAROFS", z.year_offset.to_string()),
        ("ALTNAME", z.alt.clone()),
        ("ZTEXT", z.text.clone()),
    ];
    if let Some(r) = z.dst {
        for (keys, e) in [(["SMONTH", "SDAY", "SREL", "STIME"], r.start), (["EMONTH", "EDAY", "EREL", "ETIME"], r.end)] {
            v.push((keys[0], word(system, 0x01, usize::from(e.month.clamp(1, 12) - 1))));
            v.push((keys[1], word(system, 0x0D, usize::from(e.weekday % 7))));
            v.push((keys[2], word(system, 0x14, usize::from(e.week.clamp(1, 5) - 1))));
            v.push((keys[3], format!("{:02}:{:02}:{:02}", e.secs / 3600, e.secs % 3600 / 60, e.secs % 60)));
        }
        v.push(("SHIFT", r.shift.to_string()));
    }
    v.into_iter().map(|(k, x)| (k.to_string(), x)).collect()
}

/// The escape of a description that is not there, as panel data errors are.
fn missing(name: &str) -> String {
    format!("CPF2105 {name} QSYS *TIMZON")
}

/// Option 5: one description's details.
pub fn detail_panel(system: &mut System, name: &str) -> Result<stg::proto::PanelData, String> {
    let z = load(system, name).ok_or_else(|| missing(name))?;
    Ok((detail_vars(system, &z), Vec::new()))
}

/// Option 6: the details printed to `QSYSPRT`.
pub fn print_panel(system: &mut System, name: &str) -> Result<stg::proto::PanelData, String> {
    let z = load(system, name).ok_or_else(|| missing(name))?;
    let vars = detail_vars(system, &z);
    let get = |k: &str| vars.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()).unwrap_or_default();
    let label = |system: &mut System, id: &str| format!("  {:<30}:", txt(system, id, &[]));
    let mut lines = vec![format!("{} {}", txt(system, "TXT0E1E", &[]), z.name), String::new()];
    lines.push(format!("{}   {}", label(system, "TXT0E1F"), get("ZOFFSET")));
    lines.push(format!("{}   {}  {}  {}", label(system, "TXT0E20"), get("STDABBR"), get("STDFULL"), get("STDMSG")));
    if z.dst.is_some() {
        lines.push(format!("{}   {}  {}  {}", label(system, "TXT0E21"), get("DSTABBR"), get("DSTFULL"), get("DSTMSG")));
        let edge = |p: &str| format!("{} {} {} {}", get(&format!("{p}MONTH")), get(&format!("{p}DAY")), get(&format!("{p}REL")), get(&format!("{p}TIME")));
        lines.push(format!("{}   {}", label(system, "TXT0E22"), edge("S")));
        lines.push(format!("{}   {}", label(system, "TXT0E23"), edge("E")));
        lines.push(format!("{}   {}", label(system, "TXT0E24"), get("SHIFT")));
    }
    lines.push(format!("{}   {}/{}", label(system, "TXT0E25"), get("MSGFLIB"), get("MSGF")));
    lines.push(format!("{}   {}", label(system, "TXT0E26"), get("YEAROFS")));
    lines.push(format!("{}   {}", label(system, "TXT0E27"), get("ALTNAME")));
    lines.push(format!("{}   {}", label(system, "TXT0E28"), get("ZTEXT")));
    crate::svc::security::print_as(system, "QSYSPRT", "WRKTIMZON", &lines, false)?;
    let done = txt(system, "TXT0E1C", &[&z.name]);
    Ok((vec![("DONE".into(), done)], Vec::new()))
}

/// A display text (`QSTXTMSG`) or message text.
fn txt(system: &mut System, id: &str, values: &[&str]) -> String {
    crate::stgsvc::msgs::text(system, id, values)
}
