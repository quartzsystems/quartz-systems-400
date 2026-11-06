// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The DDM and DRDA commands in the job: the relational database
//! directory (`ADDRDBDIRE`, `CHGRDBDIRE`, `RMVRDBDIRE`, `WRKRDBDIRE`),
//! DDM files (`CRTDDMF`, `CHGDDMF`, `WRKDDMF`), server authentication
//! entries (`ADDSVRAUTE`, `CHGSVRAUTE`, `RMVSVRAUTE`) and the server's
//! attributes (`CHGDDMTCPA`). The payloads are `ddm::objects`'. The
//! directory is the user space `QUSRSYS/QRWRDBDIR`; a DDM file is a
//! `*FILE` of attribute `DDMF` the service keeps; the attributes are
//! the server table's `*DDM` entry; authentication entries are the
//! service's alone.

use std::fmt::Write as _;

use mi::Job;
use stg::tcp::{TcpOp, TcpReply};
use stg::{Client, Shared};

use cmd::{Resolved, Value};
use ddm::objects::panel;
use ddm::objects::{DdmFile, DdmTcpAttrs, RdbDirectory, RdbEntry};

use super::Fail;
use crate::console::host::Host;

/// The directory's user space.
const RDBDIR: (&str, &str) = ("QUSRSYS", "QRWRDBDIR");
/// The data area an older release kept the directory in.
const OLD_RDBDIR: &str = "QRWRDBDIR";
/// The user space's size.
const SPACE: u64 = 65_536;

/// Run one of the commands; `None` for a name that is not one.
pub fn run(job: &mut Job, host: &Host, r: &Resolved) -> Option<Result<String, Fail>> {
    let _ = job;
    Some(match r.name.as_str() {
        "ADDRDBDIRE" => addrdbdire(host, r),
        "CHGRDBDIRE" => chgrdbdire(host, r),
        "RMVRDBDIRE" => rmvrdbdire(host, r),
        "WRKRDBDIRE" => Ok(wrkrdbdire(host, r)),
        "CRTDDMF" => crtddmf(host, r),
        "CHGDDMF" => chgddmf(host, r),
        "WRKDDMF" => Ok(wrkddmf(host, r)),
        "CHGDDMTCPA" => chgddmtcpa(host, r),
        "ADDSVRAUTE" => svraute(host, r, "ADD"),
        "CHGSVRAUTE" => svraute(host, r, "CHG"),
        "RMVSVRAUTE" => svraute(host, r, "RMV"),
        _ => return None,
    })
}

fn system_name(link: &Shared) -> String {
    crate::work::ddm::system_name(link)
}

/// The directory: the user space's text (a length, then the text), or
/// the data area an older release kept.
pub fn directory_of(link: &Shared) -> RdbDirectory {
    let space = link.borrow_mut().with(|l| l.us_load(RDBDIR.1.into(), RDBDIR.0.into()));
    let text = match space {
        Some(b) if b.len() >= 4 => {
            let n = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize;
            String::from_utf8_lossy(b.get(4..4 + n).unwrap_or_default()).into_owned()
        }
        _ => link
            .borrow_mut()
            .with(|l| l.da_read(OLD_RDBDIR.into(), RDBDIR.0.into()))
            .ok()
            .map(|v| ccsid::from_ebcdic(&v.value).trim_end().to_string())
            .unwrap_or_default(),
    };
    RdbDirectory::decode(&text, &system_name(link))
}

pub(crate) fn directory(host: &Host) -> RdbDirectory {
    directory_of(&host.link)
}

fn save_directory(host: &Host, d: &RdbDirectory) -> Result<(), Fail> {
    let text = d.encode();
    let mut bytes = u32::try_from(text.len()).unwrap_or(0).to_be_bytes().to_vec();
    bytes.extend(text.as_bytes());
    if bytes.len() as u64 > SPACE {
        return Err(Fail::check("ADDRDBDIRE", "the relational database directory is full"));
    }
    let link = &host.link;
    if link.borrow_mut().with(|l| l.us_load(RDBDIR.1.into(), RDBDIR.0.into())).is_none() {
        link.borrow_mut()
            .with(|l| l.us_create(RDBDIR.1.into(), RDBDIR.0.into(), SPACE, 0, "Relational database directory".into()))
            .map_err(|e| Fail::new("CPF9870", e))?;
    }
    if !link.borrow_mut().with(|l| l.us_store(RDBDIR.1.into(), RDBDIR.0.into(), bytes)) {
        return Err(Fail::new("CPF9870", "The relational database directory could not be written."));
    }
    Ok(())
}

fn value<'a>(r: &'a Resolved, kw: &str) -> Option<&'a Value> {
    r.get(kw).and_then(|b| b.value.as_ref())
}

fn text(v: &Value) -> String {
    v.source().trim().trim_matches('\'').to_string()
}

fn elem(r: &Resolved, kw: &str, i: usize) -> Option<String> {
    match value(r, kw)? {
        Value::Elems(e) => e.get(i)?.as_ref().map(text),
        v if i == 0 => Some(text(v)),
        _ => None,
    }
}

fn plain(r: &Resolved, kw: &str) -> Option<String> {
    value(r, kw).map(text)
}

fn qual(v: &Value) -> String {
    match v {
        Value::Qual(p) => {
            let name = p.first().map(text).unwrap_or_default();
            match p.get(1).map(text) {
                Some(l) if !l.is_empty() => format!("{l}/{name}"),
                _ => name,
            }
        }
        other => text(other),
    }
}

/// Set `f` from a keyword unless it says `*SAME`.
fn set(f: &mut String, v: Option<String>) {
    if let Some(v) = v.filter(|v| !v.eq_ignore_ascii_case("*SAME")) {
        *f = if v.eq_ignore_ascii_case("*BLANK") { String::new() } else { v };
    }
}

fn apply_entry(e: &mut RdbEntry, r: &Resolved) {
    if let Some(v) = value(r, "RMTLOCNAME") {
        match v {
            Value::Elems(_) => {
                set(&mut e.rmtloc, elem(r, "RMTLOCNAME", 0));
                set(&mut e.loc_type, elem(r, "RMTLOCNAME", 1));
            }
            v => set(&mut e.rmtloc, Some(text(v))),
        }
    }
    set(&mut e.text, plain(r, "TEXT"));
    if let Some(v) = value(r, "ARDPGM") {
        let a = match v {
            Value::Elems(p) => p.first().cloned().flatten().map(|q| qual(&q)).unwrap_or_default(),
            other => qual(other),
        };
        if !a.eq_ignore_ascii_case("*SAME") {
            e.ardpgm = if a.eq_ignore_ascii_case("*DRDA") { String::new() } else { a };
        }
    }
    for (f, k) in [
        (&mut e.dev, "DEV"),
        (&mut e.lcllocname, "LCLLOCNAME"),
        (&mut e.rmtnetid, "RMTNETID"),
        (&mut e.mode, "MODE"),
        (&mut e.tnspgm, "TNSPGM"),
        (&mut e.port, "PORT"),
        (&mut e.seccnn, "SECCNN"),
        (&mut e.encalg, "ENCALG"),
    ] {
        set(f, plain(r, k));
    }
    set(&mut e.auth, elem(r, "RMTAUTMTH", 0));
    set(&mut e.allow_lower, elem(r, "RMTAUTMTH", 1));
    if let Some(a) = elem(r, "RDB", 1) {
        set(&mut e.alias, Some(a));
    }
}

fn refused(e: &ddm::objects::Refused) -> Fail {
    Fail::new(e.id, e.text.clone())
}

fn addrdbdire(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let name = elem(r, "RDB", 0).unwrap_or_default().to_ascii_uppercase();
    let mut e = RdbEntry::new(&name, "", "*SNA");
    apply_entry(&mut e, r);
    let name = e.name().to_string();
    let mut d = directory(host);
    d.add(e).map_err(|e| refused(&e))?;
    save_directory(host, &d)?;
    Ok(format!("Entry {name} added to the relational database directory.\n"))
}

fn chgrdbdire(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let name = elem(r, "RDB", 0).unwrap_or_default().to_ascii_uppercase();
    let mut d = directory(host);
    d.change(&name, |e| apply_entry(e, r)).map_err(|e| refused(&e))?;
    save_directory(host, &d)?;
    Ok(format!("Entry {name} changed.\n"))
}

fn rmvrdbdire(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let name = plain(r, "RDB").unwrap_or_default().to_ascii_uppercase();
    let mut d = directory(host);
    let n = d.remove(&name).map_err(|e| refused(&e))?;
    save_directory(host, &d)?;
    Ok(format!("{n} relational database directory entries removed.\n"))
}

fn wrkrdbdire(host: &Host, r: &Resolved) -> String {
    let d = directory(host);
    let which = plain(r, "RDB").unwrap_or_else(|| "*ALL".into());
    let rows = panel::wrkrdbdire(&d.select(&which), false);
    let mut out = String::new();
    for row in rows.iter().take(23) {
        out.push_str(row.trim_end());
        out.push('\n');
    }
    out
}

fn file_name(r: &Resolved, dft_lib: &str) -> (String, String) {
    match value(r, "FILE") {
        Some(Value::Qual(p)) => (
            p.get(1).map(text).filter(|l| !l.is_empty()).unwrap_or_else(|| dft_lib.into()).to_ascii_uppercase(),
            p.first().map(text).unwrap_or_default().to_ascii_uppercase(),
        ),
        Some(v) => (dft_lib.into(), text(v).to_ascii_uppercase()),
        None => (dft_lib.into(), String::new()),
    }
}

fn apply_ddmf(f: &mut DdmFile, r: &Resolved) {
    if let Some(v) = value(r, "RMTFILE") {
        let s = match v {
            Value::Elems(p) => match (p.first().cloned().flatten(), p.get(1).cloned().flatten()) {
                (Some(q), _) if !text(&q).eq_ignore_ascii_case("*NONSTD") => qual(&q),
                (_, Some(n)) => format!("'{}'", text(&n)),
                (Some(q), None) => qual(&q),
                (None, None) => String::new(),
            },
            other => qual(other),
        };
        set(&mut f.rmtfile, Some(s));
    }
    if let Some(v) = value(r, "RMTLOCNAME") {
        match v {
            Value::Elems(_) => {
                set(&mut f.rmtloc, elem(r, "RMTLOCNAME", 0));
                set(&mut f.loc_type, elem(r, "RMTLOCNAME", 1));
            }
            v => set(&mut f.rmtloc, Some(text(v))),
        }
    }
    set(&mut f.rdb, plain(r, "RDB"));
    set(&mut f.text, plain(r, "TEXT"));
    if let Some(p) = plain(r, "PORT").and_then(|p| p.parse::<i64>().ok())
        && p > 0
    {
        f.port = u16::try_from(p).unwrap_or(446);
    }
    set(&mut f.dev, elem(r, "DEV", 0));
    set(&mut f.lcllocname, plain(r, "LCLLOCNAME"));
    set(&mut f.mode, plain(r, "MODE"));
    set(&mut f.rmtnetid, plain(r, "RMTNETID"));
    if let Some(a) = elem(r, "ACCMTH", 0) {
        set(&mut f.accmth, Some(a));
    }
    let yes = |v: Option<String>, cur: bool| match v.as_deref() {
        Some("Y" | "*YES") => true,
        Some("N" | "*NO") => false,
        _ => cur,
    };
    f.share = yes(plain(r, "SHARE"), f.share);
    f.ptccnv = yes(plain(r, "PTCCNV"), f.ptccnv);
    match plain(r, "LVLCHK").as_deref() {
        Some("Y" | "*RMTFILE") => f.lvlchk = "*RMTFILE".into(),
        Some("N" | "*NO") => f.lvlchk = "*NO".into(),
        _ => {}
    }
}

/// A service refusal as the command's escape: its leading message ID.
fn service_fail(e: &str) -> Fail {
    let id = super::jobsite::leading_msgid(e.trim()).unwrap_or("CPF9898");
    Fail::new(id, e.trim().to_string())
}

fn crtddmf(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let (lib, file) = file_name(r, "*CURLIB");
    let mut f = DdmFile::new(&lib, &file, "", "", "*SNA");
    apply_ddmf(&mut f, r);
    let replace = plain(r, "REPLACE").is_none_or(|v| v == "Y" || v == "*YES");
    let payload = f.encode();
    let text = f.text.clone();
    let lib = host
        .link
        .borrow_mut()
        .with(|l| l.ddmf_put(lib.clone(), file.clone(), payload, text, true, replace))
        .map_err(|e| service_fail(&e))?;
    Ok(format!("DDM file {file} created in library {lib}.\n"))
}

fn chgddmf(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let (lib, file) = file_name(r, "*LIBL");
    let (found, text) = host
        .link
        .borrow_mut()
        .with(|l| l.ddmf_get(lib.clone(), file.clone()))
        .map_err(|e| service_fail(&e))?;
    let mut f = DdmFile::decode(&text).ok_or_else(|| Fail::new("CPF3204", format!("DDM file {found}/{file} is damaged.")))?;
    apply_ddmf(&mut f, r);
    let payload = f.encode();
    let t = f.text.clone();
    host.link
        .borrow_mut()
        .with(|l| l.ddmf_put(found.clone(), file.clone(), payload, t, false, false))
        .map_err(|e| service_fail(&e))?;
    Ok(format!("DDM file {file} in library {found} changed.\n"))
}

/// The DDM files a `FILE(lib/name)` names (`*ALL` and generic names).
pub fn ddm_files(link: &Shared, lib: &str, name: &str) -> Vec<(String, DdmFile)> {
    let pattern = if name.is_empty() || name == "*ALL" { "*ALL".to_string() } else { name.to_string() };
    let objs = link.borrow_mut().with(|l| l.list_objects(lib.into(), pattern, "*FILE".into()));
    let mut out = Vec::new();
    for o in objs.into_iter().filter(|o| o.attr.trim() == mi::io::DDM_ATTRIBUTE) {
        if let Ok((l, text)) = link.borrow_mut().with(|c| c.ddmf_get(o.lib.clone(), o.name.clone()))
            && let Some(f) = DdmFile::decode(&text)
        {
            out.push((l, f));
        }
    }
    out
}

fn wrkddmf(host: &Host, r: &Resolved) -> String {
    let (lib, file) = file_name(r, "*LIBL");
    let files = ddm_files(&host.link, &lib, &file);
    let mut out = String::from("                              Work with DDM Files\n\n");
    out.push_str("                                                              Remote\n");
    out.push_str(" Opt  Local File            Remote File                       Location\n");
    for (l, f) in &files {
        let local = format!("{l}/{}", f.file);
        let location = if f.rmtloc == "*RDB" { f.rdb.clone() } else { f.rmtloc.clone() };
        let _ = writeln!(out, "      {local:<21} {:<33} {}", f.rmtfile, location.trim_matches('\''));
    }
    if files.is_empty() {
        let _ = writeln!(out, "  (No DDM files)");
    }
    out
}

fn chgddmtcpa(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let current = crate::work::ddm::server::attrs(&host.link);
    let mut a = current;
    if let Some(v) = plain(r, "PWDRQD").filter(|v| v != "*SAME") {
        a.pwdrqd = DdmTcpAttrs::normal_pwdrqd(&v);
    }
    if let Some(v) = plain(r, "ENCALG").filter(|v| v != "*SAME") {
        a.encalg = v;
    }
    let autostart = match plain(r, "AUTOSTART").as_deref() {
        Some("*YES") => Some(true),
        Some("*NO") => Some(false),
        _ => None,
    };
    let op = TcpOp::SetServer {
        server: "*DDM".into(),
        autostart,
        bag: a.to_bag(),
    };
    match host.link.borrow_mut().with(|l| l.tcp(op)) {
        TcpReply::Done { .. } => Ok("DDM TCP/IP attributes changed.\n".into()),
        TcpReply::Failed { note } => Err(Fail::new(&note.msgid, note.line())),
        other => Err(Fail::check("CHGDDMTCPA", format!("unexpected answer {other:?}"))),
    }
}

fn svraute(host: &Host, r: &Resolved, op: &str) -> Result<String, Fail> {
    let user = plain(r, "USRPRF").unwrap_or_else(|| "*CURRENT".into());
    let server = plain(r, "SERVER").unwrap_or_default();
    if server.is_empty() {
        return Err(Fail::new("CPD0071", "SERVER is needed."));
    }
    let keep = |v: Option<String>| v.filter(|v| !v.eq_ignore_ascii_case("*SAME"));
    let (usrid, password) = if op == "RMV" {
        (None, None)
    } else {
        (keep(plain(r, "USRID")), keep(plain(r, "PASSWORD")))
    };
    host.link
        .borrow_mut()
        .with(|l| l.svraute(op.into(), user, server, usrid, password))
        .map_err(|e| service_fail(&e))
}
