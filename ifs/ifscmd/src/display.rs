// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the IFS displays show, read through a client: the links an
//! `OBJ` pattern names (`WRKLNK`, `DSPLNK`), an object's attributes as
//! the Display Attributes panel's variables, and the rows of its
//! authority (`DSPAUT`, `WRKAUT`). The job's panels and the service's
//! text forms both build on these.

use ifs::client::IfsClient;
use ifs::meta::{CrtObjAud, ObjAudit, Scan, StgOpt};
use ifs::ops::{Attr, Stat};
use ifs::{Errno, Result};

/// One link a display lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The name shown: the last component, or `/` for the root.
    pub name: String,
    /// The full path.
    pub path: String,
    /// The type without its asterisk (`DIR`, `STMF`, `SYMLNK` ...).
    pub objtype: String,
    /// The attribute column (a `/QSYS.LIB` object's attribute).
    pub attr: String,
    /// The text description.
    pub text: String,
}

impl Link {
    /// Whether the object is a directory the displays can open.
    #[must_use]
    pub fn is_dir(&self) -> bool {
        matches!(self.objtype.as_str(), "DIR" | "LIB" | "FILE" | "FLR" | "DDIR")
    }
}

/// The links a pattern names.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Listing {
    /// The `Directory` the display shows: blank for the root itself.
    pub dir: String,
    /// The links in name order.
    pub links: Vec<Link>,
}

/// A path made absolute against the current directory, `.` and `..`
/// left for the resolver except a plain `*`.
#[must_use]
pub fn absolute(cwd: &str, obj: &str) -> String {
    let obj = obj.trim();
    if obj.starts_with('/') {
        return obj.to_string();
    }
    let obj = if obj.is_empty() { "*" } else { obj };
    if cwd.ends_with('/') {
        format!("{cwd}{obj}")
    } else {
        format!("{cwd}/{obj}")
    }
}

/// The directory and the last component of an absolute path.
#[must_use]
pub fn split(path: &str) -> (String, String) {
    let t = path.trim_end_matches('/');
    if t.is_empty() {
        return (String::new(), "/".into());
    }
    match t.rfind('/') {
        Some(0) => ("/".into(), t[1..].to_string()),
        Some(i) => (t[..i].to_string(), t[i + 1..].to_string()),
        None => (String::new(), t.to_string()),
    }
}

/// A path under a directory.
#[must_use]
pub fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// Whether a name holds a pattern character.
#[must_use]
pub fn is_pattern(name: &str) -> bool {
    name.contains('*') || name.contains('?')
}

/// IBM's pattern match: `*` any run, `?` one character; `fold` for a
/// file system that ignores case.
#[must_use]
pub fn matches(pattern: &str, name: &str, fold: bool) -> bool {
    let up = |s: &str| -> Vec<char> {
        if fold {
            s.chars().flat_map(char::to_uppercase).collect()
        } else {
            s.chars().collect()
        }
    };
    let (p, n) = (up(pattern), up(name));
    // Backtracking over the last star.
    let (mut i, mut j) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while j < n.len() {
        if i < p.len() && (p[i] == '?' || p[i] == n[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some(i);
            mark = j;
            i += 1;
        } else if let Some(s) = star {
            i = s + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == '*' {
        i += 1;
    }
    i == p.len()
}

/// Whether names under `dir` fold: everything but `/QOpenSys`.
#[must_use]
pub fn folds(dir: &str) -> bool {
    let d = dir.to_ascii_uppercase();
    !(d == "/QOPENSYS" || d.starts_with("/QOPENSYS/"))
}

/// Whether an object of type `objtype` (with its asterisk) passes an
/// `OBJTYPE` value (`*ALL`, `*ALLDIR`, a type).
#[must_use]
pub fn type_passes(filter: &str, objtype: &str) -> bool {
    match filter.trim() {
        "" | "*ALL" | "ALL" => true,
        "*ALLDIR" | "ALLDIR" => matches!(objtype, "*DIR" | "*LIB" | "*FILE" | "*FLR" | "*DDIR"),
        f => {
            let f = f.trim_start_matches('*');
            objtype.trim_start_matches('*').eq_ignore_ascii_case(f)
        }
    }
}

fn link_of(c: &dyn IfsClient, path: &str, name: &str) -> Link {
    let st = c.stat(path, false);
    let objtype = st
        .as_ref()
        .map(|s| s.objtype.trim_start_matches('*').to_string())
        .unwrap_or_default();
    let text = text_of(c, path);
    Link {
        name: name.to_string(),
        path: path.to_string(),
        objtype,
        attr: String::new(),
        text,
    }
}

fn text_of(c: &dyn IfsClient, path: &str) -> String {
    c.get_attrs(path, &[ifs::ops::AttrId::Text], false)
        .ok()
        .and_then(|v| {
            v.into_iter().find_map(|a| match a {
                Attr::Text(t) => Some(t),
                _ => None,
            })
        })
        .unwrap_or_default()
}

/// The links `obj` (relative to the current directory, a pattern in
/// its last component) names, of the types `objtype` passes. A
/// directory named without a pattern is listed as itself; its contents
/// are `DIR/*`.
pub fn links(c: &dyn IfsClient, obj: &str, objtype: &str) -> Result<Listing> {
    let cwd = c.getcwd()?;
    let path = absolute(&cwd, obj);
    let (dir, last) = split(&path);
    let mut out = Listing {
        dir: dir.clone(),
        links: Vec::new(),
    };
    if !is_pattern(&last) {
        let shown = if dir.is_empty() { "/".to_string() } else { last.clone() };
        let full = if dir.is_empty() { "/".to_string() } else { join(&dir, &last) };
        c.stat(&full, false)?;
        let l = link_of(c, &full, &shown);
        if type_passes(objtype, &format!("*{}", l.objtype)) {
            out.links.push(l);
        }
        return Ok(out);
    }
    let fold = folds(&dir);
    let mut names: Vec<String> = c
        .read_dir(&dir)?
        .into_iter()
        .map(|e| e.name)
        .filter(|n| matches(&last, n, fold))
        .collect();
    names.sort_by(|a, b| {
        a.to_uppercase()
            .cmp(&b.to_uppercase())
            .then_with(|| a.cmp(b))
    });
    for n in names {
        let l = link_of(c, &join(&dir, &n), &n);
        if type_passes(objtype, &format!("*{}", l.objtype)) {
            out.links.push(l);
        }
    }
    Ok(out)
}

/// A name in the 20 columns the lists give it: longer names cut to 18
/// with ` >`.
#[must_use]
pub fn shown_name(name: &str) -> String {
    if name.chars().count() <= 20 {
        name.to_string()
    } else {
        let head: String = name.chars().take(18).collect();
        format!("{head} >")
    }
}

/// A time as the displays show it: `MM/DD/YY  HH:MM:SS` (UTC).
#[must_use]
pub fn date_time(ns: i64) -> String {
    let secs = ns.div_euclid(1_000_000_000);
    let (y, m, d) = civil(secs.div_euclid(86_400));
    let t = secs.rem_euclid(86_400);
    format!(
        "{m:02}/{d:02}/{:02}  {:02}:{:02}:{:02}",
        y.rem_euclid(100),
        t / 3600,
        t % 3600 / 60,
        t % 60
    )
}

/// A date as the displays show it: `MM/DD/YY` (UTC).
#[must_use]
pub fn date(ns: i64) -> String {
    date_time(ns)[..8].to_string()
}

/// Year, month, day of a day number from 1970-01-01.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn yes(b: bool) -> String {
    if b { "Yes" } else { "No" }.to_string()
}

fn stg(o: StgOpt) -> String {
    match o {
        StgOpt::Normal => "*NORMAL",
        StgOpt::Minimize => "*MINIMIZE",
        StgOpt::Dynamic => "*DYNAMIC",
    }
    .to_string()
}

fn scan(s: Scan) -> String {
    match s {
        Scan::No => "*NO",
        Scan::Yes => "*YES",
        Scan::ChgOnly => "*CHGONLY",
    }
    .to_string()
}

fn audit(a: ObjAudit) -> String {
    match a {
        ObjAudit::None => "*NONE",
        ObjAudit::UsrPrf => "*USRPRF",
        ObjAudit::Change => "*CHANGE",
        ObjAudit::All => "*ALL",
    }
    .to_string()
}

/// The variables of the Display Attributes panel, in the panel's
/// order: `(variable, value)`. Directories have `DIRFMT`, `CRTAUD`,
/// `CRTSCAN`; stream files `FILEFMT`; a checked-out object `CHKOUTU`
/// and `CHKOUTT`. `AUDIT` shows `*NOTAVL` unless the caller has
/// `*AUDIT` or `*ALLOBJ` (`have_audit`).
#[allow(clippy::too_many_lines)] // the panel's variables in its order
pub fn attributes(c: &dyn IfsClient, path: &str, have_audit: bool) -> Result<Vec<(&'static str, String)>> {
    let st: Stat = c.stat(path, false)?;
    let attrs = c.get_attrs(path, &[], false)?;
    let mut v: Vec<(&'static str, String)> = Vec::new();
    let dir = st.objtype == "*DIR" || st.objtype == "*LIB" || st.objtype == "*FILE";
    v.push(("OBJ", path.to_string()));
    v.push(("OBJTYPE", st.objtype.trim_start_matches('*').to_string()));
    v.push(("OWNER", st.owner.clone()));
    v.push(("SYSTEM", "Local".into()));
    v.push(("ASP", "1".into()));
    v.push(("OVERFLOW", "No".into()));
    let mut by = |a: &Attr| match a {
        Attr::Ccsid(n) => Some(("CCSID", n.to_string())),
        Attr::PcHidden(b) => Some(("HIDDEN", yes(*b))),
        Attr::PcSystem(b) => Some(("PCSYSTEM", yes(*b))),
        Attr::PcReadOnly(b) => Some(("RDONLY", yes(*b))),
        Attr::PcArchive(b) => Some(("PCARC", yes(*b))),
        Attr::SystemArchive(b) => Some(("SYSARC", yes(*b))),
        Attr::CreateTime(t) => Some(("CRTDT", date_time(*t))),
        Attr::AccessTime(t) => Some(("ACCDT", date_time(*t))),
        Attr::ModifyTime(t) => Some(("CHGDT", date_time(*t))),
        Attr::ChangeTime(t) => Some(("ATRDT", date_time(*t))),
        Attr::DataSize(n) => Some(("SIZE", n.to_string())),
        Attr::AllocSize(n) => Some(("ALLOC", n.to_string())),
        Attr::ExtendedAttrSize(n) => Some(("EASIZE", n.to_string())),
        Attr::StgFree(b) => Some(("STGFREE", yes(*b))),
        Attr::Temporary(b) => Some(("TEMP", yes(*b))),
        Attr::DiskStgOpt(o) => Some(("DISKOPT", stg(*o))),
        Attr::MainStgOpt(o) => Some(("MAINOPT", stg(*o))),
        Attr::Audit(a) => Some((
            "AUDIT",
            if have_audit { audit(*a) } else { "*NOTAVL".into() },
        )),
        Attr::Crtobjaud(a) => Some((
            "CRTAUD",
            if have_audit {
                match a {
                    CrtObjAud::SysVal => "*SYSVAL".into(),
                    CrtObjAud::Value(v) => audit(*v),
                }
            } else {
                "*NOTAVL".into()
            },
        )),
        Attr::Crtobjscan(s) => Some(("CRTSCAN", scan(*s))),
        Attr::Scan(s) => Some(("SCAN", scan(*s))),
        Attr::Alwsav(b) => Some(("ALWSAV", yes(*b))),
        Attr::Alwckpwrt(b) => Some(("ALWCKPWRT", yes(*b))),
        Attr::Rstdrnmunl(b) => Some(("RSTDRNMUNL", yes(*b))),
        Attr::Suid(b) => Some(("SUID", yes(*b))),
        Attr::Sgid(b) => Some(("SGID", yes(*b))),
        Attr::Text(t) => Some(("TEXT", t.clone())),
        _ => None,
    };
    let mut found: Vec<(&'static str, String)> = attrs.iter().filter_map(&mut by).collect();
    let mut take = |k: &str| -> String {
        found
            .iter()
            .position(|(n, _)| *n == k)
            .map(|i| found.remove(i).1)
            .unwrap_or_default()
    };
    for k in ["CCSID", "HIDDEN", "PCSYSTEM", "RDONLY", "PCARC", "SYSARC"] {
        let val = take(k);
        v.push((k, val));
    }
    for k in ["CRTDT", "ACCDT", "CHGDT", "ATRDT", "SIZE", "ALLOC"] {
        let val = take(k);
        v.push((k, val));
    }
    v.push(if dir {
        ("DIRFMT", "*TYPE2".into())
    } else {
        ("FILEFMT", "*TYPE2".into())
    });
    for k in ["EASIZE", "STGFREE", "TEMP"] {
        let val = take(k);
        v.push((k, val));
    }
    if !dir {
        for k in ["DISKOPT", "MAINOPT"] {
            let val = take(k);
            v.push((k, val));
        }
    }
    let val = take("AUDIT");
    v.push(("AUDIT", val));
    v.push(("AUTCOL", "*NONE".into()));
    v.push(("DOMAIN", "*SYSTEM".into()));
    v.push(("NLINK", st.nlink.to_string()));
    if dir {
        let a = take("CRTAUD");
        v.push(("CRTAUD", a));
        let s = take("CRTSCAN");
        v.push(("CRTSCAN", s));
        v.push(("CRTRUNEXIT", "No".into()));
    }
    for k in ["SUID", "SGID", "RSTDRNMUNL", "SCAN", "ALWSAV", "ALWCKPWRT", "TEXT"] {
        let val = take(k);
        v.push((k, val));
    }
    if !dir {
        v.push(("LASTUSED", date(st.atime)));
        v.push(("DAYSUSED", "1".into()));
        v.push(("RESETDT", String::new()));
    }
    if let Some(co) = attrs.iter().find_map(|a| match a {
        Attr::CheckedOut(Some(co)) => Some(co.clone()),
        _ => None,
    }) {
        v.push(("CHKOUT", "Yes".into()));
        v.push(("CHKOUTU", co.user));
        v.push(("CHKOUTT", date_time(co.time)));
    } else {
        v.push(("CHKOUT", "No".into()));
    }
    Ok(v)
}

/// The labels of the attributes for the text form, by variable.
pub const LABELS: &[(&str, &str)] = &[
    ("OBJ", "Object"),
    ("OBJTYPE", "Type"),
    ("OWNER", "Owner"),
    ("SYSTEM", "System object is on"),
    ("ASP", "Auxiliary storage pool"),
    ("OVERFLOW", "Object overflowed"),
    ("CCSID", "Coded character set ID"),
    ("HIDDEN", "Hidden file"),
    ("PCSYSTEM", "PC system file"),
    ("RDONLY", "Read only"),
    ("PCARC", "Need to archive (PC)"),
    ("SYSARC", "Need to archive (System)"),
    ("CRTDT", "Creation date/time"),
    ("ACCDT", "Last access date/time"),
    ("CHGDT", "Data change date/time"),
    ("ATRDT", "Attribute change date/time"),
    ("SIZE", "Size of object data in bytes"),
    ("ALLOC", "Allocated size of object"),
    ("FILEFMT", "File format"),
    ("DIRFMT", "Directory format"),
    ("EASIZE", "Size of extended attributes"),
    ("STGFREE", "Storage freed"),
    ("TEMP", "Temporary object"),
    ("DISKOPT", "Disk storage option"),
    ("MAINOPT", "Main storage option"),
    ("AUDIT", "Auditing value"),
    ("AUTCOL", "Authority collection value"),
    ("DOMAIN", "Object domain"),
    ("NLINK", "Number of hard links"),
    ("CRTAUD", "Create object auditing"),
    ("CRTSCAN", "Create object scanning"),
    ("CRTRUNEXIT", "Create run exit program"),
    ("SUID", "Set effective user ID"),
    ("SGID", "Set effective group ID"),
    ("RSTDRNMUNL", "Restricted rename and unlink"),
    ("SCAN", "Object scanning"),
    ("ALWSAV", "Allow save"),
    ("ALWCKPWRT", "Allow write during save"),
    ("TEXT", "Text"),
    ("LASTUSED", "Last used date"),
    ("DAYSUSED", "Days used count"),
    ("RESETDT", "Reset date"),
    ("CHKOUT", "Checked out"),
    ("CHKOUTU", "Checked out by"),
    ("CHKOUTT", "Checked out date/time"),
];

/// A label for a variable.
#[must_use]
pub fn label(var: &str) -> &'static str {
    LABELS
        .iter()
        .find(|(k, _)| *k == var)
        .map_or("", |(_, l)| *l)
}

/// The `errno` a display shows for a missing object.
#[must_use]
pub fn missing(e: Errno) -> bool {
    matches!(e, Errno::ENOENT | Errno::ENOTDIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns() {
        assert!(matches("*", "abc", true));
        assert!(matches("qsnet*", "QSNET.TXT", true));
        assert!(!matches("qsnet*", "QSNET.TXT", false));
        assert!(matches("a?c", "abc", false));
        assert!(matches("*.txt", "x.y.txt", false));
        assert!(!matches("*.txt", "x.txtx", false));
        assert_eq!(split("/"), (String::new(), "/".into()));
        assert_eq!(split("/etc"), ("/".into(), "etc".into()));
        assert_eq!(split("/home/a/b*"), ("/home/a".into(), "b*".into()));
        assert_eq!(absolute("/home", "*"), "/home/*");
        assert_eq!(absolute("/", "x"), "/x");
        assert_eq!(shown_name("core.20261001.140157.txt"), "core.20261001.1401 >");
        assert_eq!(date_time(0), "01/01/70  00:00:00");
        assert_eq!(date_time(1_793_000_000 * 1_000_000_000), "10/26/26  07:33:20");
        assert!(type_passes("*ALLDIR", "*LIB"));
        assert!(type_passes("*STMF", "*STMF"));
        assert!(!type_passes("*DIR", "*STMF"));
        assert!(folds("/home") && !folds("/QOpenSys/usr"));
    }
}
