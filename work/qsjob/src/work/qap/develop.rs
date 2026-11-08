// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QAP's `Develop` service (version 8, draft) in the connection job.
//! The library list runs in the lane, in the job's runtime: read from
//! the job's attributes, changed by `CHGLIBL` itself. The rest runs on a
//! secondary thread's link, as the signed-on profile: libraries,
//! objects and members as `WRKLIB`, `WRKOBJ`, `DSPOBJD` and `DSPFD`
//! read them; `ReadMember`, `WriteMember` and `UnlockMember` through
//! the service's requests 780 to 782 (`stgsvc/qap/develop.rs`, over
//! `db::source`, the locks the job's); the member commands `ADDPFM`,
//! `CPYSRCF`, `RNMM`, `RMVM` and `CHGPFM` analyzed as typed lines and
//! run in the service, with their authority checks and messages.

use std::fmt::Write as _;

use qap_proto::Status;
use qap_proto::ops::develop::{
    Attribute, ChangeMember, CopyMember, CreateMember, DescribeObject, Library, LibraryKind,
    LibraryList, ListLibraries, ListMembers, ListObjects, MemberEntry, ObjectDescription,
    ObjectEntry, RenameMember, SetLibraryList,
};
use qap_proto::types::MemberName;
use qapd::Refusal;
use qapd::develop::generic;
use stg::{Client, Link};

use super::manage::{Ctx, analyze, service};
use crate::console::Console;

/// Seconds from 1970-01-01 to a civil date at midnight (UTC).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `CYYMMDDHHMMSS` in local time `offset_min` from UTC, as milliseconds
/// since 1970; 0 when it is not one.
#[allow(clippy::many_single_char_names)] // the date's fields
pub(super) fn ms_of(t: &str, offset_min: i32) -> i64 {
    let t = t.trim();
    let n = |a: usize, b: usize| t.get(a..b).and_then(|x| x.parse::<i64>().ok());
    let (Some(c), Some(yy), Some(mo), Some(d), Some(h), Some(mi), Some(s)) = (
        n(0, 1),
        n(1, 3),
        n(3, 5),
        n(5, 7),
        n(7, 9),
        n(9, 11),
        n(11, 13),
    ) else {
        return 0;
    };
    let days = days_from_civil(1900 + c * 100 + yy, mo, d);
    let local = days * 86_400 + h * 3600 + mi * 60 + s;
    (local - i64::from(offset_min) * 60) * 1000
}

fn refuse(cx: &Ctx<'_>, status: Status, id: &str, data: &[&str]) -> Refusal {
    let message = super::host::message(&mut cx.shared.borrow_mut(), id, data);
    Refusal { status, message }
}

/// A name as a command line takes it: upper case, trimmed.
fn up(s: &str) -> String {
    s.trim().to_ascii_uppercase()
}

/// `'text'` with its apostrophes doubled.
fn quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn file_of(m: &MemberName) -> String {
    let lib = if m.file.library.trim().is_empty() {
        "*LIBL".to_string()
    } else {
        up(&m.file.library)
    };
    format!("{lib}/{}", up(&m.file.name))
}

/// The status a member command's message stands for.
fn status_of(id: &str, was: Status) -> Status {
    match id {
        "CPF5812" | "CPF3207" | "CPF7310" | "CPF2112" => Status::Exists,
        "CPF9812" | "CPF9815" | "CPF3C26" | "CPF2110" | "CPF9810" | "CPF3141" | "CPF9801" => {
            Status::NotFound
        }
        "CPF1002" | "CPF3330" | "CPF3203" | "CPF3156" | "CPF9803" | "CPF4128" => Status::InUse,
        _ => was,
    }
}

/// Analyze `line` as a typed command and run it in the service, the
/// status by its escape message.
pub(super) fn command(cx: &Ctx<'_>, line: &str) -> Result<(), Refusal> {
    let run = analyze(cx, line).and_then(|r| service(cx, &r).map(|_| ()));
    run.map_err(|mut r| {
        r.status = status_of(&r.message.id, r.status);
        r
    })
}

/// `GetLibraryList`: the job's list as `DSPLIBL` shows it.
pub(super) fn library_list(link: &mut Link) -> LibraryList {
    let a = link.with(Client::rtvjoba);
    let mut system = a.libl.sys.clone();
    system.extend(a.libl.prd.iter().filter(|p| !p.trim().is_empty()).cloned());
    LibraryList {
        current: a.libl.cur.clone().filter(|c| !c.trim().is_empty()),
        system,
        user: a.libl.usr.clone(),
    }
}

/// `SetLibraryList`: `CHGLIBL` run in the job's runtime, so the job's
/// list changes as the command changes it.
pub(super) fn set_library_list(console: &mut Console, req: &SetLibraryList) -> Result<(), Refusal> {
    let libl = if req.user.is_empty() {
        "*NONE".to_string()
    } else {
        req.user.iter().map(|l| up(l)).collect::<Vec<_>>().join(" ")
    };
    let cur = req.current.as_deref().map_or_else(|| "*CRTDFT".into(), up);
    let line = format!("CHGLIBL LIBL({libl}) CURLIB({cur})");
    let ran = super::commands::run(console, &line, &|| false);
    if ran.status == Status::Ok {
        return Ok(());
    }
    match ran.escape {
        Some(message) => Err(Refusal {
            status: ran.status,
            message,
        }),
        None => Err(Refusal {
            status: ran.status,
            message: qap_proto::ServerMessage {
                id: "CPF9898".into(),
                severity: 40,
                ..Default::default()
            },
        }),
    }
}

/// Every library the profile can see, `QSYS` first, as `WRKLIB *ALL`.
fn all_libraries(cx: &Ctx<'_>) -> Vec<String> {
    let libs = cx
        .shared
        .borrow_mut()
        .with(|l| l.list_objects("QSYS".into(), "*ALL".into(), "*LIB".into()));
    let mut v: Vec<String> = libs.into_iter().map(|o| o.name).collect();
    if !v.iter().any(|l| l == "QSYS") {
        v.push("QSYS".into());
    }
    v.sort();
    v
}

/// `ListLibraries`.
pub(super) fn list_libraries(cx: &Ctx<'_>, req: &ListLibraries) -> Vec<Library> {
    let libs = cx
        .shared
        .borrow_mut()
        .with(|l| l.list_objects("QSYS".into(), "*ALL".into(), "*LIB".into()));
    let mut out: Vec<Library> = Vec::new();
    for o in libs.into_iter().filter(|o| generic(&o.name, &req.pattern)) {
        let detail = cx
            .shared
            .borrow_mut()
            // Libraries resolve through the list's QSYS, not as QSYS/name.
            .with(|l| l.obj_detail("*LIBL".into(), o.name.clone(), "*LIB".into()));
        let test = detail
            .unwrap_or_default()
            .iter()
            .any(|(k, v)| k == "LIBTYPE" && v == "TEST")
            || o.attr.trim() == "TEST";
        out.push(Library {
            name: o.name,
            kind: if test {
                LibraryKind::Test
            } else {
                LibraryKind::Production
            },
            text: o.text.trim().to_string(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The libraries `library` names, in search order: `CPF2110` for one
/// not there.
fn search(cx: &Ctx<'_>, library: &str) -> Result<Vec<String>, Refusal> {
    let a = cx.shared.borrow_mut().with(Client::rtvjoba);
    let l = &a.libl;
    Ok(match up(library).as_str() {
        "*LIBL" | "" => l.search(),
        "*CURLIB" => vec![l.cur.clone().unwrap_or_else(|| "QGPL".into())],
        "*USRLIBL" => l.cur.iter().chain(l.usr.iter()).cloned().collect(),
        "*ALL" => all_libraries(cx),
        name => {
            let found = name == "QSYS"
                || cx
                    .shared
                    .borrow_mut()
                    .with(|k| k.object("QSYS".into(), name.into(), "*LIB".into()))
                    .is_some();
            if !found {
                return Err(refuse(cx, Status::NotFound, "CPF2110", &[name]));
            }
            vec![name.to_string()]
        }
    })
}

/// `ListObjects`.
pub(super) fn list_objects(cx: &Ctx<'_>, req: &ListObjects) -> Result<Vec<ObjectEntry>, Refusal> {
    let libs = search(cx, &req.library)?;
    let offset = cx.shared.borrow_mut().with(Client::now).offset_min;
    let pattern = if req.pattern.trim().is_empty() {
        "*ALL".to_string()
    } else {
        up(&req.pattern)
    };
    let types: Vec<String> = if req.types.is_empty() {
        vec!["*ALL".into()]
    } else {
        req.types.iter().map(|t| up(t)).collect()
    };
    let mut out = Vec::new();
    for lib in libs {
        let mut here: Vec<ObjectEntry> = Vec::new();
        for ty in &types {
            let (l, p, t) = (lib.clone(), pattern.clone(), ty.clone());
            let objs = cx
                .shared
                .borrow_mut()
                .with(|k| k.list_objects(l.clone(), p.clone(), t.clone()));
            let sizes = cx.shared.borrow_mut().with(|k| k.obj_sizes(l, p, t));
            for o in objs {
                if req
                    .attribute
                    .as_ref()
                    .is_some_and(|a| up(a) != o.attr.trim())
                {
                    continue;
                }
                let size = sizes
                    .iter()
                    .find(|(sl, sn, st, _)| *sl == o.lib && *sn == o.name && *st == o.ty)
                    .map_or(0, |s| s.3);
                here.push(ObjectEntry {
                    library: o.lib.clone(),
                    name: o.name.clone(),
                    r#type: o.ty.clone(),
                    attribute: o.attr.trim().to_string(),
                    text: o.text.trim().to_string(),
                    size,
                    changed: ms_of(&o.changed, offset),
                    owner: o.owner.trim().to_string(),
                });
            }
        }
        here.sort_by(|a, b| (&a.name, &a.r#type).cmp(&(&b.name, &b.r#type)));
        here.dedup_by(|a, b| a.name == b.name && a.r#type == b.r#type);
        out.extend(here);
    }
    Ok(out)
}

/// The library of `name` (type `ty`) that `library` names, through the
/// job's own list for `*LIBL` and `*CURLIB` (the provider's requests do
/// not see the job's list); the name as given when none has it.
fn library_of(cx: &Ctx<'_>, library: &str, name: &str, ty: &str) -> String {
    let lib = up(library);
    if !matches!(lib.as_str(), "" | "*LIBL" | "*CURLIB" | "*USRLIBL") {
        return lib;
    }
    let Ok(libs) = search(cx, &lib) else {
        return lib;
    };
    for l in libs {
        let (ll, n, t) = (l.clone(), name.to_string(), ty.to_string());
        if cx
            .shared
            .borrow_mut()
            .with(|k| k.object(ll, n, t))
            .is_some()
        {
            return l;
        }
    }
    if lib.is_empty() { "*LIBL".into() } else { lib }
}

/// What `DSPOBJD DETAIL(*FULL)`'s fields are called here.
fn label(field: &str) -> &str {
    match field {
        "LIBTYPE" => "Library type",
        "CRTAUT" => "Create authority",
        "ASPNUM" => "ASP number",
        "OBJECTS" => "Objects",
        "CREATOR" => "Created by",
        "SYSTEM" => "System created on",
        "RELEASE" => "Release",
        "SIZE" => "Size",
        "DOMAIN" => "Domain",
        "DAMAGED" => "Damaged",
        "PGP" => "Primary group",
        "AUDIT" => "Auditing",
        "LASTUSED" => "Last used",
        "DAYSUSED" => "Days used",
        "RESET" => "Days used reset",
        f => f,
    }
}

/// `DescribeObject`.
pub(super) fn describe_object(
    cx: &Ctx<'_>,
    req: &DescribeObject,
) -> Result<ObjectDescription, Refusal> {
    let (lib, name, ty) = (
        if req.object.library.trim().is_empty() {
            "*LIBL".to_string()
        } else {
            up(&req.object.library)
        },
        up(&req.object.name),
        up(&req.r#type),
    );
    let lib = library_of(cx, &lib, &name, &ty);
    let (l, nm, t) = (lib.clone(), name.clone(), ty.clone());
    let Some(o) = cx.shared.borrow_mut().with(|k| k.object(l, nm, t)) else {
        return Err(refuse(cx, Status::NotFound, "CPF9801", &[&name, &lib, &ty]));
    };
    let detail = cx
        .shared
        .borrow_mut()
        .with(|k| k.obj_detail(o.lib.clone(), o.name.clone(), o.ty.clone()))
        .unwrap_or_default();
    let a = |n: &str, v: &str| Attribute {
        name: n.to_string(),
        value: v.trim().to_string(),
    };
    let mut attributes = vec![
        a("Object", &o.name),
        a("Library", &o.lib),
        a("Type", &o.ty),
        a("Attribute", &o.attr),
        a("Owner", &o.owner),
        a("Text", &o.text),
        a("Created", &o.created),
        a("Changed", &o.changed),
        a("User-defined attribute", &o.user_attr),
    ];
    if let Some((f, l, m)) = &o.source {
        attributes.push(a("Source file", &format!("{l}/{f}")));
        attributes.push(a("Source member", m));
    }
    attributes.extend(detail.iter().map(|(k, v)| a(label(k), v)));
    Ok(ObjectDescription { attributes })
}

/// `ListMembers`.
pub(super) fn list_members(cx: &Ctx<'_>, req: &ListMembers) -> Result<Vec<MemberEntry>, Refusal> {
    let lib = if req.file.library.trim().is_empty() {
        "*LIBL".to_string()
    } else {
        up(&req.file.library)
    };
    let file = up(&req.file.name);
    let lib = library_of(cx, &lib, &file, "*FILE");
    let (l, f) = (lib.clone(), file.clone());
    if cx.shared.borrow_mut().with(|k| k.file(l, f)).is_none() {
        return Err(refuse(cx, Status::NotFound, "CPF9812", &[&file, &lib]));
    }
    let offset = cx.shared.borrow_mut().with(Client::now).offset_min;
    let members = cx.shared.borrow_mut().with(|k| k.members(lib, file));
    let mut out: Vec<MemberEntry> = members
        .into_iter()
        .filter(|m| generic(&m.name, &req.pattern))
        .map(|m| MemberEntry {
            name: m.name.trim().to_string(),
            srctype: m.srctype.trim().to_string(),
            text: m.text.trim().to_string(),
            records: m.records,
            created: ms_of(&m.created, offset),
            changed: ms_of(&m.changed, offset),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// `CreateMember`: `ADDPFM`.
pub(super) fn create_member(cx: &Ctx<'_>, req: &CreateMember) -> Result<(), Refusal> {
    let st = if req.srctype.trim().is_empty() {
        "*NONE".to_string()
    } else {
        up(&req.srctype)
    };
    command(
        cx,
        &format!(
            "ADDPFM FILE({}) MBR({}) SRCTYPE({st}) TEXT({})",
            file_of(&req.member),
            up(&req.member.member),
            quoted(&req.text)
        ),
    )
}

/// Whether member `m` exists.
fn exists(cx: &Ctx<'_>, m: &MemberName) -> bool {
    let (lib, file) = (
        if m.file.library.trim().is_empty() {
            "*LIBL".to_string()
        } else {
            up(&m.file.library)
        },
        up(&m.file.name),
    );
    let lib = library_of(cx, &lib, &file, "*FILE");
    let name = up(&m.member);
    cx.shared
        .borrow_mut()
        .with(|k| k.members(lib, file))
        .iter()
        .any(|x| x.name.trim() == name)
}

/// `CopyMember`: `CPYSRCF` of one member; without `replace` a member
/// already there is `Exists` (`CPF5812`).
pub(super) fn copy_member(cx: &Ctx<'_>, req: &CopyMember) -> Result<(), Refusal> {
    if !req.replace && exists(cx, &req.to) {
        let (m, f) = (up(&req.to.member), up(&req.to.file.name));
        let l = up(&req.to.file.library);
        return Err(refuse(cx, Status::Exists, "CPF5812", &[&m, &f, &l]));
    }
    command(
        cx,
        &format!(
            "CPYSRCF FROMFILE({}) TOFILE({}) FROMMBR({}) TOMBR({}) MBROPT(*REPLACE)",
            file_of(&req.from),
            file_of(&req.to),
            up(&req.from.member),
            up(&req.to.member)
        ),
    )
}

/// `RenameMember`: `RNMM`.
pub(super) fn rename_member(cx: &Ctx<'_>, req: &RenameMember) -> Result<(), Refusal> {
    let to = MemberName {
        file: req.member.file.clone(),
        member: req.new_name.clone(),
    };
    if exists(cx, &to) {
        let (m, f) = (up(&req.new_name), up(&req.member.file.name));
        let l = up(&req.member.file.library);
        return Err(refuse(cx, Status::Exists, "CPF5812", &[&m, &f, &l]));
    }
    command(
        cx,
        &format!(
            "RNMM FILE({}) MBR({}) NEWMBR({})",
            file_of(&req.member),
            up(&req.member.member),
            up(&req.new_name)
        ),
    )
}

/// `DeleteMember`: `RMVM`.
pub(super) fn delete_member(cx: &Ctx<'_>, m: &MemberName) -> Result<(), Refusal> {
    command(
        cx,
        &format!("RMVM FILE({}) MBR({})", file_of(m), up(&m.member)),
    )
}

/// `ChangeMember`: `CHGPFM`, only what is given.
pub(super) fn change_member(cx: &Ctx<'_>, req: &ChangeMember) -> Result<(), Refusal> {
    let mut line = format!(
        "CHGPFM FILE({}) MBR({})",
        file_of(&req.member),
        up(&req.member.member)
    );
    if let Some(t) = &req.srctype {
        let t = if t.trim().is_empty() {
            "*NONE".into()
        } else {
            up(t)
        };
        let _ = write!(line, " SRCTYPE({t})");
    }
    if let Some(t) = &req.text {
        let _ = write!(line, " TEXT({})", quoted(t));
    }
    if req.srctype.is_none() && req.text.is_none() {
        // Nothing to change: the member must still be there.
        return if exists(cx, &req.member) {
            Ok(())
        } else {
            let (m, f) = (up(&req.member.member), up(&req.member.file.name));
            let l = up(&req.member.file.library);
            Err(refuse(cx, Status::NotFound, "CPF9815", &[&f, &l, &m]))
        };
    }
    command(cx, &line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_and_lines() {
        // 2026-10-26 00:00:00 local at UTC-5 is 05:00 UTC.
        assert_eq!(ms_of("1261026000000", -300), 1_792_990_800_000);
        assert_eq!(ms_of("bad", 0), 0);
        assert_eq!(quoted("it's"), "'it''s'");
        assert_eq!(status_of("CPF5812", Status::Error), Status::Exists);
        assert_eq!(status_of("CPF9815", Status::Error), Status::NotFound);
    }
}
