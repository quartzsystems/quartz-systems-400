// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CRTUDFS`, `DLTUDFS`, `MOUNT`/`ADDMFS` and `UNMOUNT`/`RMVMFS` for
//! `TYPE(*UDFS)` and `TYPE(*NFS)`, and `DSPUDFS`'s text. An NFS mount
//! goes to the IFS as given (`OPTIONS` is the mount's to parse); with
//! no NFS client registered it is `CPFA0AD`, as any other type.

use std::fmt::Write as _;

use ifs::Errno;
use ifs::client::IfsClient;
use ifs::meta::{CrtObjAud, ObjAudit};
use ifs::remote::{CcsidSel, MountCcsid};

use crate::display;
use crate::msg::{Msg, Reply};

/// `CRTUDFS UDFS CASE CRTOBJAUD TEXT`.
pub fn crtudfs(
    c: &dyn IfsClient,
    udfs: &str,
    case_mixed: bool,
    crtobjaud: &str,
    text: &str,
) -> Reply {
    let aud = match crtobjaud.trim().to_ascii_uppercase().as_str() {
        "" | "*SYSVAL" => CrtObjAud::SysVal,
        "*NONE" => CrtObjAud::Value(ObjAudit::None),
        "*USRPRF" => CrtObjAud::Value(ObjAudit::UsrPrf),
        "*CHANGE" => CrtObjAud::Value(ObjAudit::Change),
        "*ALL" => CrtObjAud::Value(ObjAudit::All),
        w => return Reply::fail(Msg::new("CPFA0CB").chars(w, 10)),
    };
    let text = if text.eq_ignore_ascii_case("*BLANK") {
        ""
    } else {
        text
    };
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

/// One half of `MOUNT CCSID(data path)` or `CODEPAGE(data path)`:
/// `*BINARY`, `*ASCII`, `*JOBCCSID` or a number (a code page given as
/// its CCSID); `None` when it is none of them.
#[must_use]
pub fn ccsid_sel(v: &str, codepage: bool) -> Option<CcsidSel> {
    match v.trim().to_ascii_uppercase().as_str() {
        "*BINARY" => Some(CcsidSel::Binary),
        "*ASCII" => Some(CcsidSel::Ascii),
        "*JOBCCSID" => Some(CcsidSel::Job),
        n => {
            let n: u16 = n.parse().ok().filter(|n| *n > 0 && *n < 65535)?;
            Some(CcsidSel::Ccsid(if codepage {
                ccsid::conv::ccsid_of_codepage(n)
            } else {
                n
            }))
        }
    }
}

/// `MOUNT`'s CCSIDs from its `CCSID` and `CODEPAGE` pairs (empty
/// elements take the defaults, `*BINARY` and `*ASCII`); `CODEPAGE`
/// counts where `CCSID` was left alone. The value in error when one is
/// not valid.
pub fn mount_ccsid(ccsid: [&str; 2], codepage: [&str; 2]) -> Result<MountCcsid, String> {
    let d = MountCcsid::default();
    let pick = |c: &str, p: &str, dft: CcsidSel| -> Result<CcsidSel, String> {
        let given = |v: &str| !v.trim().is_empty() && !v.trim().eq_ignore_ascii_case("*DFT");
        if given(c) {
            ccsid_sel(c, false).ok_or_else(|| c.trim().to_string())
        } else if given(p) {
            ccsid_sel(p, true).ok_or_else(|| p.trim().to_string())
        } else {
            Ok(dft)
        }
    };
    Ok(MountCcsid {
        data: pick(ccsid[0], codepage[0], d.data)?,
        path: pick(ccsid[1], codepage[1], d.path)?,
    })
}

/// `MOUNT TYPE(*UDFS) MFS MNTOVRDIR OPTIONS`: `ro` in the options
/// mounts read-only.
pub fn mount(c: &dyn IfsClient, ty: &str, mfs: &str, over: &str, options: &str) -> Reply {
    mount_with(c, ty, mfs, over, options, MountCcsid::default())
}

/// `MOUNT TYPE MFS MNTOVRDIR OPTIONS CCSID`: a UDFS, or a remote file
/// system through the IFS's registered client (`TYPE(*NFS)`, `MFS`
/// `host:/path`, `OPTIONS` as given).
pub fn mount_with(
    c: &dyn IfsClient,
    ty: &str,
    mfs: &str,
    over: &str,
    options: &str,
    ccsid: MountCcsid,
) -> Reply {
    let t = ty.trim().to_ascii_uppercase();
    if t == "*NFS" {
        return match c.mount_remote("*NFS", mfs, over, options, ccsid) {
            Ok(_) => Reply::done(Msg::paths("CPCA1B0", &[mfs, over])),
            Err(Errno::ENOTSUP) => Reply::fail(Msg::path("CPFA0AD", mfs)),
            Err(e) => Reply::errno(e, mfs),
        };
    }
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

/// `UNMOUNT TYPE(*UDFS) MFS|MNTOVRDIR`, `TYPE(*NFS) MNTOVRDIR|MFS`,
/// `TYPE(*ALL) MNTOVRDIR(*ALL)`: every mount of the type over the
/// directory (or everywhere for `*ALL`); an NFS mount named by its `MFS`
/// is found by what is mounted.
pub fn unmount(c: &dyn IfsClient, ty: &str, mfs: &str, over: &str) -> Reply {
    let t = ty.trim().to_ascii_uppercase();
    let by_mfs = over.is_empty() || over.eq_ignore_ascii_case("*MFS");
    if t == "*NFS" || t == "*ALL" {
        let dir = if by_mfs {
            if mfs.is_empty() {
                return Reply::fail(Msg::new("CPFA094"));
            }
            match c.mounts() {
                Ok(v) => match v.into_iter().find(|m| m.source == mfs) {
                    Some(m) => m.at,
                    None => return Reply::errno(Errno::EINVAL, mfs),
                },
                Err(e) => return Reply::errno(e, mfs),
            }
        } else {
            over.to_string()
        };
        return match c.unmount_kind(&t, &dir) {
            Ok(_) => Reply::done(Msg::path("CPCA1B1", &dir)),
            Err(Errno::ENOTSUP) => Reply::fail(Msg::path("CPFA0AD", &dir)),
            Err(e) => Reply::errno(e, &dir),
        };
    }
    if !udfs_type(ty) {
        return Reply::fail(Msg::path(
            "CPFA0AD",
            if mfs.is_empty() { over } else { mfs },
        ));
    }
    let which = if by_mfs { mfs } else { over };
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
    let v = match display::udfs(c, udfs) {
        Ok(v) => v,
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
    for (k, val) in &v.vars {
        let label = display::UDFS_LABELS
            .iter()
            .find(|(n, _)| n == k)
            .map_or("", |(_, l)| *l);
        line(&mut t, label, val);
        if *k == "MOUNTED" {
            for m in &v.mounts {
                line(&mut t, "Mounted over", m);
            }
        }
    }
    Reply::text(t)
}
