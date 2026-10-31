// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The DDM and DRDA commands in the job: the relational database
//! directory (`ADDRDBDIRE`, `CHGRDBDIRE`, `RMVRDBDIRE`, `WRKRDBDIRE`),
//! DDM files (`CRTDDMF`, `CHGDDMF`) and the server's attributes
//! (`CHGDDMTCPA`). The payloads are `ddm::objects`'; they are kept as
//! `KEYWORD(value)` text in data areas: the directory in
//! `QUSRSYS/QRWRDBDIR`, the attributes in `QUSRSYS/QRWDDMTCPA`, a
//! library's DDM files in its `QRWDDMF`.
// BLOCKED(phase10): DDM files belong in *FILE objects of attribute DDM
// and the directory in its own object; data areas stand in until the
// database's file kinds take a DDM file.

use mi::Job;
use stg::Client;

use cmd::{Resolved, Value};
use ddm::objects::panel;
use ddm::objects::{DdmFile, DdmTcpAttrs, RdbDirectory, RdbEntry};

use super::Fail;
use super::jobsite::leading_msgid;
use crate::console::host::Host;

/// The directory's data area.
const RDBDIR: (&str, &str) = ("QUSRSYS", "QRWRDBDIR");
/// The attributes' data area.
const TCPA: (&str, &str) = ("QUSRSYS", "QRWDDMTCPA");
/// A library's DDM files.
const DDMF: &str = "QRWDDMF";
/// The data areas' length.
const LEN: usize = 2000;

/// Run one of the commands; `None` for a name that is not one.
pub fn run(job: &mut Job, host: &Host, r: &Resolved) -> Option<Result<String, Fail>> {
    Some(match r.name.as_str() {
        "ADDRDBDIRE" => addrdbdire(job, host, r),
        "CHGRDBDIRE" => chgrdbdire(job, host, r),
        "RMVRDBDIRE" => rmvrdbdire(job, host, r),
        "WRKRDBDIRE" => Ok(wrkrdbdire(host, r)),
        "CRTDDMF" => crtddmf(job, host, r),
        "CHGDDMF" => chgddmf(job, host, r),
        "CHGDDMTCPA" => chgddmtcpa(job, host, r),
        _ => return None,
    })
}

fn read(host: &Host, lib: &str, name: &str) -> String {
    host.link
        .borrow_mut()
        .with(|l| l.da_read(name.into(), lib.into()))
        .ok()
        .map(|v| ccsid::from_ebcdic(&v.value).trim_end().to_string())
        .unwrap_or_default()
}

fn write(job: &mut Job, host: &Host, lib: &str, name: &str, text: &str, what: &str) -> Result<(), Fail> {
    if text.len() > LEN {
        return Err(Fail::new(
            "CPF1088",
            format!("{lib}/{name} holds {LEN} characters; this change needs {}.", text.len()),
        ));
    }
    let change = format!(
        "CHGDTAARA DTAARA({lib}/{name}) VALUE('{}')",
        text.replace('\'', "''")
    );
    let out = crate::command::run(job, host, &change).text;
    if leading_msgid(out.trim()).is_some_and(|id| id.starts_with("CPF")) {
        let made = crate::command::run(
            job,
            host,
            &format!("CRTDTAARA DTAARA({lib}/{name}) TYPE(*CHAR) LEN({LEN}) TEXT('{what}')"),
        )
        .text;
        if let Some(id) = leading_msgid(made.trim()) {
            return Err(Fail::new(id, made.trim().to_string()));
        }
        let again = crate::command::run(job, host, &change).text;
        if let Some(id) = leading_msgid(again.trim()) {
            return Err(Fail::new(id, again.trim().to_string()));
        }
    }
    Ok(())
}

fn system_name(host: &Host) -> String {
    crate::display::dds::system_name(host)
}

pub(crate) fn directory(host: &Host) -> RdbDirectory {
    RdbDirectory::decode(&read(host, RDBDIR.0, RDBDIR.1), &system_name(host))
}

fn save_directory(job: &mut Job, host: &Host, d: &RdbDirectory) -> Result<(), Fail> {
    write(job, host, RDBDIR.0, RDBDIR.1, &d.encode(), "Relational database directory")
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

fn addrdbdire(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    let name = elem(r, "RDB", 0).unwrap_or_default().to_ascii_uppercase();
    let mut e = RdbEntry::new(&name, "", "*SNA");
    apply_entry(&mut e, r);
    let mut d = directory(host);
    d.add(e).map_err(|e| refused(&e))?;
    save_directory(job, host, &d)?;
    Ok(format!("Entry {name} added to the relational database directory.\n"))
}

fn chgrdbdire(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    let name = elem(r, "RDB", 0).unwrap_or_default().to_ascii_uppercase();
    let mut d = directory(host);
    d.change(&name, |e| apply_entry(e, r)).map_err(|e| refused(&e))?;
    save_directory(job, host, &d)?;
    Ok(format!("Entry {name} changed.\n"))
}

fn rmvrdbdire(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    let name = plain(r, "RDB").unwrap_or_default().to_ascii_uppercase();
    let mut d = directory(host);
    let n = d.remove(&name).map_err(|e| refused(&e))?;
    save_directory(job, host, &d)?;
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

fn load_files(host: &Host, lib: &str) -> Vec<DdmFile> {
    DdmFile::decode_all(&read(host, lib, DDMF))
}

fn crtddmf(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    let (lib, file) = file_name(r, "QGPL");
    let lib = if lib == "*CURLIB" { "QGPL".to_string() } else { lib };
    let mut f = DdmFile::new(&lib, &file, "", "", "*SNA");
    apply_ddmf(&mut f, r);
    let mut files = load_files(host, &lib);
    let replace = plain(r, "REPLACE").is_none_or(|v| v == "Y" || v == "*YES");
    if files.iter().any(|x| x.file == file) {
        if !replace {
            return Err(Fail::new("CPF7302", format!("{lib}/{file} exists already.")));
        }
        files.retain(|x| x.file != file);
    }
    files.push(f);
    write(job, host, &lib, DDMF, &DdmFile::encode_all(&files), "DDM files")?;
    Ok(format!("DDM file {file} created in library {lib}.\n"))
}

fn chgddmf(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    let (lib, file) = file_name(r, "QGPL");
    let lib = if lib == "*LIBL" || lib == "*CURLIB" { "QGPL".to_string() } else { lib };
    let mut files = load_files(host, &lib);
    let Some(f) = files.iter_mut().find(|x| x.file == file) else {
        return Err(Fail::new("CPF9812", format!("No DDM file {file} in library {lib}.")));
    };
    apply_ddmf(f, r);
    write(job, host, &lib, DDMF, &DdmFile::encode_all(&files), "DDM files")?;
    Ok(format!("DDM file {file} in library {lib} changed.\n"))
}

fn chgddmtcpa(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    let mut a = DdmTcpAttrs::decode(&read(host, TCPA.0, TCPA.1));
    if let Some(v) = plain(r, "AUTOSTART").filter(|v| v != "*SAME") {
        a.autostart = v == "*YES";
    }
    if let Some(v) = plain(r, "PWDRQD").filter(|v| v != "*SAME") {
        a.pwdrqd = DdmTcpAttrs::normal_pwdrqd(&v);
    }
    if let Some(v) = plain(r, "ENCALG").filter(|v| v != "*SAME") {
        a.encalg = v;
    }
    write(job, host, TCPA.0, TCPA.1, &a.encode(), "DDM TCP/IP attributes")?;
    Ok("DDM TCP/IP attributes changed.\n".into())
}
