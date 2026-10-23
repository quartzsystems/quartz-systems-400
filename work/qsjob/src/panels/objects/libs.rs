// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The library and object displays: `DSPLIBL`, `WRKOBJ`, `DSPLIB`,
//! `DSPOBJD`, `DSPOBJAUT`, `WRKBNDDIR`.

use cmd::Resolved;
use mi::Job;
use stg::Client;
use sysapi::ObjInfo;

use super::{act, common, entry, fill, parm, qual, show, stamp};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Choice, Response, UimDialog};
use crate::panels::dialog;

/// The library list: `(name, type)` in search order.
fn library_list(host: &Host) -> Vec<(String, &'static str)> {
    let l = host.attrs().libl;
    let mut out: Vec<(String, &'static str)> = Vec::new();
    out.extend(l.sys.iter().map(|n| (n.clone(), "SYS")));
    out.extend(
        l.prd
            .iter()
            .filter(|n| !n.is_empty())
            .map(|n| (n.clone(), "PRD")),
    );
    out.extend(l.cur.iter().map(|n| (n.clone(), "CUR")));
    out.extend(l.usr.iter().map(|n| (n.clone(), "USR")));
    out
}

/// An object's description through the storage service.
fn object(host: &Host, lib: &str, name: &str, ty: &str) -> Option<ObjInfo> {
    host.link
        .borrow_mut()
        .with(|l| l.object(lib.into(), name.into(), ty.into()))
}

/// `DSPLIBL`: the library list; 5 shows a library's objects.
pub fn dsplibl(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    let mut d = dialog(host, "QUIDSPLIBL")?;
    d.copyright = true;
    loop {
        let rows = library_list(host)
            .into_iter()
            .map(|(n, t)| {
                let text = object(host, "QSYS", &n, "*LIB")
                    .map(|o| o.text)
                    .unwrap_or_default();
                entry(&[
                    ("LIBNAME", n),
                    ("LIBTYPE", t.to_string()),
                    ("ASPDEV", String::new()),
                    ("LIBTEXT", text),
                ])
            })
            .collect();
        fill(&mut d, "LIBS", rows);
        match show(&mut d, surface, "DSPLIBL")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Options(c) => act(job, host, &mut d, &c, &|_, _| None),
            _ => {}
        }
    }
}

/// The objects `lib` (a name, `*LIBL`, `*CURLIB`, `*USRLIBL`,
/// `*ALL`, `*ALLUSR`) holds of a name and type.
fn objects(host: &Host, lib: &str, name: &str, ty: &str) -> Vec<ObjInfo> {
    let libs: Vec<String> = match lib {
        "*CURLIB" => host.attrs().libl.cur.into_iter().collect(),
        "*USRLIBL" => {
            let l = host.attrs().libl;
            l.cur.into_iter().chain(l.usr).collect()
        }
        "*ALLUSR" => library_list(host)
            .into_iter()
            .filter(|(n, _)| !n.starts_with('Q') || n == "QGPL")
            .map(|(n, _)| n)
            .collect(),
        other => vec![other.to_string()],
    };
    let ty = if ty.is_empty() { "*ALL" } else { ty };
    let name = if name.is_empty() { "*ALL" } else { name };
    libs.iter()
        .flat_map(|l| {
            host.link
                .borrow_mut()
                .with(|c| c.list_objects(l.clone(), name.into(), ty.into()))
        })
        .collect()
}

/// The command that deletes an object of a type, prompted.
fn delete_command(name: &str, lib: &str, ty: &str) -> Option<String> {
    let (cmd, kwd, qualified) = match ty {
        "*PGM" => ("DLTPGM", "PGM", true),
        "*SRVPGM" => ("DLTSRVPGM", "SRVPGM", true),
        "*MODULE" => ("DLTMOD", "MODULE", true),
        "*FILE" => ("DLTF", "FILE", true),
        "*DTAARA" => ("DLTDTAARA", "DTAARA", true),
        "*DTAQ" => ("DLTDTAQ", "DTAQ", true),
        "*MSGF" => ("DLTMSGF", "MSGF", true),
        "*MSGQ" => ("DLTMSGQ", "MSGQ", true),
        "*CMD" => ("DLTCMD", "CMD", true),
        "*BNDDIR" => ("DLTBNDDIR", "BNDDIR", true),
        "*JOBD" => ("DLTJOBD", "JOBD", true),
        "*JOBQ" => ("DLTJOBQ", "JOBQ", true),
        "*OUTQ" => ("DLTOUTQ", "OUTQ", true),
        "*CLS" => ("DLTCLS", "CLS", true),
        "*SBSD" => ("DLTSBSD", "SBSD", true),
        "*JRN" => ("DLTJRN", "JRN", true),
        "*JRNRCV" => ("DLTJRNRCV", "JRNRCV", true),
        "*PNLGRP" => ("DLTPNLGRP", "PNLGRP", true),
        "*MENU" => ("DLTMNU", "MENU", true),
        "*LIB" => ("DLTLIB", "LIB", false),
        "*USRPRF" => ("DLTUSRPRF", "USRPRF", false),
        "*AUTL" => ("DLTAUTL", "AUTL", false),
        "*DEVD" => ("DLTDEVD", "DEVD", false),
        _ => return None,
    };
    Some(if qualified {
        format!("?{cmd} {kwd}({lib}/{name})")
    } else {
        format!("?{cmd} {kwd}({name})")
    })
}

/// A list entry's value.
fn cell(d: &UimDialog, c: &Choice, var: &str) -> String {
    d.row(&c.list, c.row)
        .into_iter()
        .find(|(v, _)| v == var)
        .map(|(_, x)| x)
        .unwrap_or_default()
}

/// `WRKOBJ`: the objects with their options; F17 positions the list at
/// the command line's name.
pub fn wrkobj(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    let (name, lib) = qual(r, "OBJ", "*LIBL");
    let ty = parm(r, "OBJTYPE").unwrap_or_else(|| "*ALL".into());
    let mut d = dialog(host, "QUIWRKOBJ")?;
    let mut last = String::new();
    let mut position = String::new();
    let mut reload = true;
    loop {
        if reload {
            let rows = objects(host, &lib, &name, &ty)
                .into_iter()
                .map(|o| {
                    entry(&[
                        ("OBJ", o.name),
                        ("OBJTYPE", o.ty),
                        ("OBJLIB", o.lib),
                        ("OBJATR", o.attr),
                        ("OBJTEXT", o.text),
                    ])
                })
                .collect();
            fill(&mut d, "OBJS", rows);
            reload = false;
        }
        let resp = show(&mut d, surface, "WRKOBJ")?;
        let Some(resp) = common(job, host, &mut d, resp, &mut last) else {
            reload = true;
            continue;
        };
        match resp {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Options(c) => {
                act(job, host, &mut d, &c, &|d, c| {
                    (c.option == "4").then(|| {
                        let (n, l, t) = (
                            cell(d, c, "OBJ"),
                            cell(d, c, "OBJLIB"),
                            cell(d, c, "OBJTYPE"),
                        );
                        delete_command(&n, &l, &t)
                            .unwrap_or_else(|| format!("?DLTOBJ OBJ({l}/{n}) OBJTYPE({t})"))
                    })
                });
                reload = true;
            }
            Response::Return(5) => reload = true,
            Response::Return(11) => d.view = 1 - d.view.min(1),
            Response::Return(n @ (16 | 17)) => {
                if n == 17 {
                    position = d.get(CMDLINE_VAR).trim().to_ascii_uppercase();
                    d.set(CMDLINE_VAR, "");
                }
                if position.is_empty() {
                    d.message = Some((
                        "Type a name on the command line, then press F17.".into(),
                        true,
                    ));
                    continue;
                }
                let l = d.list_mut("OBJS");
                let at = l.rows.iter().position(|row| {
                    row.iter()
                        .any(|(v, x)| v == "OBJ" && x.as_str() >= position.as_str())
                });
                match at {
                    Some(i) => l.top = i,
                    None => d.message = Some((format!("No object at or after {position}."), true)),
                }
            }
            _ => {}
        }
    }
}

/// The attribute and the size as `DSPOBJD`'s shared column shows them.
fn attr_size(attr: &str, size: u64) -> String {
    format!("{attr:<10}{size:>18}")
}

/// The objects `OBJ` and `OBJTYPE` select, a library at a time.
fn by_library(host: &Host, lib: &str, name: &str, ty: &str) -> Vec<(String, Vec<(ObjInfo, u64)>)> {
    let found = objects(host, lib, name, ty);
    let sizes = host
        .link
        .borrow_mut()
        .with(|l| l.obj_sizes(lib.into(), name.into(), ty.into()));
    let mut out: Vec<(String, Vec<(ObjInfo, u64)>)> = Vec::new();
    for o in found {
        let size = sizes
            .iter()
            .find(|(l, n, t, _)| *l == o.lib && *n == o.name && *t == o.ty)
            .map_or(0, |s| s.3);
        match out.iter_mut().find(|(l, _)| *l == o.lib) {
            Some((_, v)) => v.push((o, size)),
            None => out.push((o.lib.clone(), vec![(o, size)])),
        }
    }
    out
}

/// `Library i of n`, right-aligned in its column.
fn lib_pos(i: usize, n: usize) -> String {
    format!("{:>28}", format!("Library {i} of {n}"))
}

/// `DSPOBJD`: the objects a library at a time (basic) or each whole
/// (`DETAIL(*FULL)`).
pub fn dspobjd(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    let (name, lib) = qual(r, "OBJ", "*LIBL");
    let ty = parm(r, "OBJTYPE").unwrap_or_else(|| "*ALL".into());
    let detail = parm(r, "DETAIL").unwrap_or_else(|| "*BASIC".into());
    let groups = by_library(host, &lib, &name, &ty);
    if groups.is_empty() {
        return Err(Fail::new(
            "CPF2123",
            format!("No objects of type {ty} found for {lib}/{name}."),
        ));
    }
    let mut d = dialog(host, "QUIDSPOBJD")?;
    d.copyright = true;
    let n = groups.len();
    if detail != "*BASIC" {
        for (i, (_, objs)) in groups.iter().enumerate() {
            for (o, size) in objs {
                if !full(host, surface, &mut d, o, *size, &lib_pos(i + 1, n))? {
                    return Ok(String::new());
                }
            }
        }
        return Ok(String::new());
    }
    basic(job, host, surface, &mut d, &groups).map(|_| String::new())
}

/// The basic panel over each library's objects; false when F3 asked
/// to leave everything.
fn basic(
    _job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    d: &mut UimDialog,
    groups: &[(String, Vec<(ObjInfo, u64)>)],
) -> Result<bool, Fail> {
    let n = groups.len();
    let mut i = 0;
    while i < n {
        let (lib, objs) = &groups[i];
        d.set("LIBPOS", &lib_pos(i + 1, n));
        d.set("LIB", lib);
        let rows = objs
            .iter()
            .map(|(o, size)| {
                entry(&[
                    ("OBJ", o.name.clone()),
                    ("OBJTYPE", o.ty.clone()),
                    ("OBJLIB", o.lib.clone()),
                    ("ATRSIZE", attr_size(&o.attr, *size)),
                    ("OBJTEXT", o.text.clone()),
                ])
            })
            .collect();
        fill(d, "OBJS", rows);
        match show(d, surface, "BASIC")? {
            Response::Exit | Response::Gone => return Ok(false),
            Response::Cancel => {
                if i == 0 {
                    return Ok(true);
                }
                i -= 1;
            }
            Response::Options(choices) => {
                for c in &choices {
                    if let Some((o, size)) = objs.get(c.row)
                        && !full(host, surface, d, o, *size, &lib_pos(i + 1, n))?
                    {
                        return Ok(false);
                    }
                }
                for c in &choices {
                    d.list_mut(&c.list).options.clear();
                }
            }
            _ => i += 1,
        }
    }
    Ok(true)
}

/// The full description of one object; false when F3 asked to leave.
fn full(
    host: &Host,
    surface: &mut Surface,
    d: &mut UimDialog,
    o: &ObjInfo,
    size: u64,
    pos: &str,
) -> Result<bool, Fail> {
    let detail = host
        .link
        .borrow_mut()
        .with(|l| l.obj_detail(o.lib.clone(), o.name.clone(), o.ty.clone()))
        .unwrap_or_default();
    let get = |k: &str| {
        detail
            .iter()
            .find(|(f, _)| f == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let (cd, ct) = stamp(&o.created);
    let (hd, ht) = stamp(&o.changed);
    let (ud, _) = stamp(&get("LASTUSED"));
    let (rd, _) = stamp(&get("RESET"));
    d.set("LIBPOS", pos);
    d.set("OBJ", &o.name);
    d.set("OBJLIB", &o.lib);
    d.set("OBJTYPE", &o.ty);
    d.set("OBJATR", &o.attr);
    d.set("OWNER", &o.owner);
    d.set("PGP", &get("PGP"));
    d.set("UDATTR", &o.user_attr);
    d.set("OBJTEXT", &o.text);
    d.set("CRTDATE", &format!("{cd}  {ct}"));
    d.set("CRTUSER", &get("CREATOR"));
    d.set("CRTSYS", &get("SYSTEM"));
    d.set("DOMAIN", &get("DOMAIN"));
    d.set("CHGDATE", &format!("{hd}  {ht}"));
    let used = !ud.is_empty();
    d.set("USAGE", if used { "YES" } else { "NO" });
    d.set("LASTUSED", &ud);
    d.set("DAYSUSED", &get("DAYSUSED"));
    d.set("RESETDT", &rd);
    d.set("AUDIT", &get("AUDIT"));
    d.set("OBJSIZE", &size.to_string());
    d.set("DAMAGED", &get("DAMAGED"));
    d.set("RELEASE", &get("RELEASE"));
    d.set(
        "SRCFILE",
        &o.source
            .as_ref()
            .map_or_else(String::new, |(f, l, m)| format!("{l}/{f} {m}")),
    );
    d.page = 0;
    loop {
        match show(d, surface, "FULL")? {
            Response::Exit | Response::Gone => return Ok(false),
            Response::Cancel | Response::Return(_) => return Ok(true),
            _ => {}
        }
    }
}

/// `DSPLIB`: each library's attributes and objects.
pub fn dsplib(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    let libs: Vec<String> = match r.get("LIB").and_then(|b| b.value.as_ref()) {
        Some(cmd::Value::List(items)) => items
            .iter()
            .map(|v| v.source().trim().to_ascii_uppercase())
            .collect(),
        Some(v) => vec![v.source().trim().to_ascii_uppercase()],
        None => vec!["*LIBL".into()],
    };
    let libs: Vec<String> = libs
        .into_iter()
        .flat_map(|l| match l.as_str() {
            "*LIBL" | "*ALL" | "" => library_list(host)
                .into_iter()
                .map(|(n, _)| n)
                .collect::<Vec<_>>(),
            "*CURLIB" => host.attrs().libl.cur.into_iter().collect(),
            "*USRLIBL" => host.attrs().libl.usr,
            _ => vec![l],
        })
        .collect();
    let mut d = dialog(host, "QUIDSPLIB")?;
    d.copyright = true;
    let mut i = 0;
    while i < libs.len() {
        let lib = &libs[i];
        let attrs = host
            .link
            .borrow_mut()
            .with(|l| l.obj_detail("QSYS".into(), lib.clone(), "*LIB".into()))
            .ok_or_else(|| Fail::new("CPF2110", format!("Library {lib} not found.")))?;
        let get = |k: &str| {
            attrs
                .iter()
                .find(|(f, _)| f == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let objs = by_library(host, lib, "*ALL", "*ALL");
        let objs: Vec<(ObjInfo, u64)> = objs.into_iter().flat_map(|(_, v)| v).collect();
        d.set("LIB", lib);
        d.set("LIBTYPE", &get("LIBTYPE"));
        d.set("CRTAUT", &get("CRTAUT"));
        d.set("NOBJ", &objs.len().to_string());
        d.set("ASPNUM", &get("ASPNUM"));
        let rows = objs
            .iter()
            .map(|(o, size)| {
                entry(&[
                    ("OBJ", o.name.clone()),
                    ("OBJTYPE", o.ty.clone()),
                    ("OBJLIB", o.lib.clone()),
                    ("ATRSIZE", attr_size(&o.attr, *size)),
                    ("OBJTEXT", o.text.clone()),
                ])
            })
            .collect();
        fill(&mut d, "OBJS", rows);
        match show(&mut d, surface, "DSPLIB")? {
            Response::Exit | Response::Gone => return Ok(String::new()),
            Response::Cancel => {
                if i == 0 {
                    return Ok(String::new());
                }
                i -= 1;
            }
            Response::Options(choices) => {
                for c in &choices {
                    if let Some((o, size)) = objs.get(c.row) {
                        let mut f = dialog(host, "QUIDSPOBJD")?;
                        f.copyright = true;
                        if !full(host, surface, &mut f, o, *size, &lib_pos(1, 1))? {
                            return Ok(String::new());
                        }
                    }
                }
                for c in &choices {
                    d.list_mut(&c.list).options.clear();
                }
            }
            _ => i += 1,
        }
    }
    let _ = job;
    Ok(String::new())
}

/// One user's row of `DSPOBJAUT`: user, group, authority.
fn auth_rows(text: &str) -> Vec<(String, String, String)> {
    let mut rows = Vec::new();
    let mut table = false;
    for l in text.lines() {
        if l.trim_start().starts_with("User") {
            table = true;
            continue;
        }
        if !table || l.trim().is_empty() {
            continue;
        }
        let user = l.get(1..11).unwrap_or("").trim().to_string();
        let group = l.get(13..23).unwrap_or("").trim().to_string();
        let auth = l.get(25..).unwrap_or("").trim().to_string();
        rows.push((user, group, auth));
    }
    rows
}

/// The value after `label` on the lines of the text form.
fn after(text: &str, label: &str) -> String {
    text.lines()
        .find_map(|l| {
            let at = l.find(label)?;
            let rest = &l[at + label.len()..];
            let rest = rest.split_once(':')?.1;
            Some(
                rest.split("   ")
                    .map(str::trim)
                    .find(|s| !s.is_empty())
                    .unwrap_or("")
                    .to_string(),
            )
        })
        .unwrap_or_default()
}

/// `DSPOBJAUT`: who may use the object and how; F11 shows the
/// authorities one by one.
pub fn dspobjaut(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let (name, lib) = qual(r, "OBJ", "*LIBL");
    let ty = parm(r, "OBJTYPE").unwrap_or_default();
    let found = object(host, &lib, &name, &ty).ok_or_else(|| {
        Fail::new(
            "CPF9801",
            format!("Object {name} in library {lib} not found."),
        )
    })?;
    let line = format!(
        "dspobjaut OBJ({}/{}) OBJTYPE({})",
        found.lib, found.name, found.ty
    );
    let mut d = dialog(host, "QUIDSPOAUT")?;
    d.copyright = true;
    d.set("OBJ", &found.name);
    d.set("OBJLIB", &found.lib);
    d.set("OBJTYPE", &found.ty);
    d.set("OWNER", &found.owner);
    loop {
        let text = host.exec(&line).text;
        if crate::command::jobsite::leading_msgid(text.trim()).is_some() {
            return Err(Fail::new(&text[..7], text.trim().to_string()));
        }
        d.set("PGP", &after(&text, "Primary group"));
        let autl = after(&text, "Authorization list");
        d.set("AUTL", if autl.is_empty() { "*NONE" } else { &autl });
        let detail = host.exec(&format!("{line} AUTFMT(*DETAIL)")).text;
        let rows: Vec<Vec<(String, String)>> = auth_rows(&text)
            .into_iter()
            .map(|(u, g, a)| {
                let bits = detail
                    .lines()
                    .find(|l| l.get(1..11).is_some_and(|x| x.trim() == u))
                    .map(|l| l.get(13..).unwrap_or("").to_string())
                    .unwrap_or_default();
                entry(&[("USER", u), ("GROUP", g), ("AUT", a), ("BITS", bits)])
            })
            .collect();
        fill(&mut d, "AUTS", rows);
        match show(&mut d, surface, "DSPOBJAUT")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Return(11) => d.view = 1 - d.view.min(1),
            _ => {}
        }
    }
}

/// `WRKBNDDIR`: the binding directories with their options.
pub fn wrkbnddir(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    let (name, lib) = qual(r, "BNDDIR", "*LIBL");
    let name = if name.is_empty() {
        "*ALL".to_string()
    } else {
        name
    };
    let mut d = dialog(host, "QUIWRKBNDD")?;
    let mut last = String::new();
    loop {
        let mut rows = vec![entry(&[
            ("BNDDIR", String::new()),
            ("BNDLIB", String::new()),
        ])];
        rows.extend(
            objects(host, &lib, &name, "*BNDDIR")
                .into_iter()
                .map(|o| entry(&[("BNDDIR", o.name), ("BNDLIB", o.lib), ("BNDTEXT", o.text)])),
        );
        fill(&mut d, "DIRS", rows);
        let resp = show(&mut d, surface, "WRKBNDDIR")?;
        let Some(resp) = common(job, host, &mut d, resp, &mut last) else {
            continue;
        };
        match resp {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Options(c) => act(job, host, &mut d, &c, &|d, c| {
                let (n, l) = (cell(d, c, "BNDDIR"), cell(d, c, "BNDLIB"));
                match c.option.as_str() {
                    "1" if n.is_empty() => Some("?CRTBNDDIR".into()),
                    "1" => Some(format!(
                        "?CRTBNDDIR BNDDIR({}/{n})",
                        if l.is_empty() { "*CURLIB" } else { &l }
                    )),
                    "4" if !n.is_empty() => Some(format!("?DLTBNDDIR BNDDIR({l}/{n})")),
                    _ => None,
                }
            }),
            Response::Return(11) => d.view = 1 - d.view.min(1),
            _ => {}
        }
    }
}
