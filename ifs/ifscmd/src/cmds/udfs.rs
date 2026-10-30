// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CRTUDFS`, `DLTUDFS`, `MOUNT`/`ADDMFS` and `UNMOUNT`/`RMVMFS` for
//! `TYPE(*UDFS)`, and `DSPUDFS`'s text. Network file systems (`*NFS`,
//! `*NETWORK`) are not supported (`CPFA0AD`).

use std::fmt::Write as _;

use ifs::client::IfsClient;
use ifs::meta::{CrtObjAud, ObjAudit};

use crate::display::date_time;
use crate::msg::{Msg, Reply};

/// `CRTUDFS UDFS CASE CRTOBJAUD TEXT`.
pub fn crtudfs(c: &dyn IfsClient, udfs: &str, case_mixed: bool, crtobjaud: &str, text: &str) -> Reply {
    let aud = match crtobjaud.trim().to_ascii_uppercase().as_str() {
        "" | "*SYSVAL" => CrtObjAud::SysVal,
        "*NONE" => CrtObjAud::Value(ObjAudit::None),
        "*USRPRF" => CrtObjAud::Value(ObjAudit::UsrPrf),
        "*CHANGE" => CrtObjAud::Value(ObjAudit::Change),
        "*ALL" => CrtObjAud::Value(ObjAudit::All),
        w => return Reply::fail(Msg::new("CPFA0CB").chars(w, 10)),
    };
    let text = if text.eq_ignore_ascii_case("*BLANK") { "" } else { text };
    match c.crtudfs(udfs, case_mixed, aud, text) {
        Ok(()) => Reply::done(Msg::path("CPCA1C0", udfs)),
        Err(e) => Reply::errno(e, udfs),
    }
}

/// `DLTUDFS`.
pub fn dltudfs(c: &dyn IfsClient, udfs: &str) -> Reply {
    match c.dltudfs(udfs) {
        Ok(()) => Reply::done(Msg::path("CPCA1C1", udfs)),
        Err(e) => Reply::errno(e, udfs),
    }
}

/// Whether a `TYPE` is a user-defined file system.
fn udfs_type(ty: &str) -> bool {
    matches!(ty.trim().to_ascii_uppercase().as_str(), "" | "*UDFS")
}

/// `MOUNT TYPE(*UDFS) MFS MNTOVRDIR OPTIONS`: `ro` in the options
/// mounts read-only.
pub fn mount(c: &dyn IfsClient, ty: &str, mfs: &str, over: &str, options: &str) -> Reply {
    if !udfs_type(ty) {
        return Reply::fail(Msg::path("CPFA0AD", mfs));
    }
    let ro = options
        .split(',')
        .any(|o| o.trim().eq_ignore_ascii_case("ro"));
    match c.mount(mfs, over, ro) {
        Ok(_) => Reply::done(Msg::paths("CPCA1B0", &[mfs, over])),
        Err(e) => Reply::errno(e, mfs),
    }
}

/// `UNMOUNT TYPE(*UDFS) MFS|MNTOVRDIR`.
pub fn unmount(c: &dyn IfsClient, ty: &str, mfs: &str, over: &str) -> Reply {
    if !udfs_type(ty) {
        return Reply::fail(Msg::path("CPFA0AD", if mfs.is_empty() { over } else { mfs }));
    }
    let which = if over.is_empty() || over.eq_ignore_ascii_case("*MFS") {
        mfs
    } else {
        over
    };
    if which.is_empty() {
        return Reply::fail(Msg::new("CPFA094"));
    }
    match c.unmount(which) {
        Ok(()) => Reply::done(Msg::path("CPCA1B1", which)),
        Err(e) => Reply::errno(e, which),
    }
}

/// `DSPUDFS`'s text.
pub fn dspudfs(c: &dyn IfsClient, udfs: &str) -> Reply {
    let i = match c.dspudfs(udfs) {
        Ok(i) => i,
        Err(e) => return Reply::errno(e, udfs),
    };
    let mut t = String::new();
    let line = |t: &mut String, label: &str, v: &str| {
        let dots = ". ".repeat(20);
        let lead = format!(" {label} {dots}");
        let lead: String = lead.chars().take(40).collect();
        let _ = writeln!(t, "{lead}:   {v}");
    };
    let _ = writeln!(t, "{:^80}", "User-defined file system");
    line(&mut t, "User-defined file system", &i.path);
    line(&mut t, "Owner", &i.owner);
    line(
        &mut t,
        "Case sensitivity",
        if i.case_sensitive { "*MIXED" } else { "*MONO" },
    );
    line(
        &mut t,
        "Create object auditing value",
        match i.crtobjaud {
            CrtObjAud::SysVal => "*SYSVAL",
            CrtObjAud::Value(ObjAudit::None) => "*NONE",
            CrtObjAud::Value(ObjAudit::UsrPrf) => "*USRPRF",
            CrtObjAud::Value(ObjAudit::Change) => "*CHANGE",
            CrtObjAud::Value(ObjAudit::All) => "*ALL",
        },
    );
    line(&mut t, "Default file format", "*TYPE2");
    line(&mut t, "Creation date/time", &date_time(i.created));
    if i.mounted_at.is_empty() {
        line(&mut t, "Mounted", "No");
    } else {
        for m in &i.mounted_at {
            line(&mut t, "Mounted over", m);
        }
    }
    line(&mut t, "Text description", &i.text);
    Reply::text(t)
}
