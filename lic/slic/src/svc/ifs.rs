// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `svc.ifs`: the integrated file system from the service console, as
//! the job whose line it is (the system when there is none). The bare
//! word reports the mount; `ls`, `cat`, `put`, `mkdir`, `rm`, `stat`
//! and `df` go through the storage service's `Ifs` exactly as a job's
//! requests do.

use std::fmt::Write as _;

use ifs::Errno;
use ifs::client::{IfsClient, read_all, write_all};
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_CCSID, O_CREAT, O_RDONLY, O_TEXTDATA, O_TRUNC, O_WRONLY};
use sec::auth::principal::Principal;
use sec::profile::spcaut::SpecialAuth;

use crate::ipl::system::System;
use crate::stgsvc::ifs as service;

/// The help lines.
pub const HELP: &str = "\
ifs                  the IFS mount, capacity and the service's file system
ifs ls [PATH] | cat PATH | put PATH TEXT | mkdir PATH | rm PATH | stat PATH | df [PATH]
                     the IFS as the job sees it (put: TEXT in UTF-8, \\n for a line end)
";

/// Who the line runs as: the job's principal, else the system.
pub(crate) fn principal() -> Principal {
    sec::guard::current().map_or_else(
        || Principal {
            user: "QSECOFR".into(),
            spcaut: SpecialAuth::ALLOBJ.0
                | SpecialAuth::SECADM.0
                | SpecialAuth::AUDIT.0
                | SpecialAuth::IOSYSCFG.0,
            qsecurity: 40,
            objaud: "*NONE".into(),
            ..Principal::default()
        },
        |s| s.borrow().principal.clone(),
    )
}

/// One error as the console shows it.
fn fail(path: &str, e: Errno) -> String {
    format!("{path}: {} ({}, {})\n", e.text(), e.name(), e.message_id())
}

/// `svc.ifs [WORD ARGS]`.
pub fn command(system: &mut System, args: &[&str], line: &str) -> String {
    let Some((&word, rest)) = args.split_first() else {
        return format!("{}{}", crate::ipl::ifs::report(), report());
    };
    let p = principal();
    let number = system.current_job;
    let handle = u64::from(number);
    // Its own owner, so the job's descriptors are not touched.
    let owner = service::console_owner(handle);
    let ccsid = service::job_ccsid(system, number);
    let text = line
        .split_whitespace()
        .skip(3)
        .collect::<Vec<_>>()
        .join(" ");
    let out = service::with_client(system, handle, owner, &p, ccsid, |c| match (word, rest) {
        ("ls", []) => ls(c, "."),
        ("ls", [path]) => ls(c, path),
        ("cat", [path]) => cat(c, path),
        ("put", [path, ..]) => put(c, path, &text),
        ("mkdir", [path]) => c
            .mkdir(path, 0o755)
            .map_or_else(|e| fail(path, e), |()| format!("{path} made\n")),
        ("rm", [path]) => rm(c, path),
        ("stat", [path]) => stat(c, path),
        ("df", []) => df(c, "/"),
        ("df", [path]) => df(c, path),
        _ => HELP.to_string(),
    });
    // The console's calls are not a connection: nothing they opened stays.
    service::release_owner(system, handle, owner);
    out
}

/// The service's side of the bare word, after the mount report.
fn report() -> String {
    let mut out = String::new();
    match service::why_not() {
        None => {
            let _ = writeln!(out, "IFS root at {}", service::ROOT);
            if let Some((size, used)) = service::capacity() {
                let _ = writeln!(
                    out,
                    "capacity {size} bytes ({} MiB), used {used} ({} MiB)",
                    size >> 20,
                    used >> 20
                );
            }
        }
        Some(why) => {
            let _ = writeln!(out, "IFS not available: {why}");
        }
    }
    out
}

fn ls(c: &dyn IfsClient, path: &str) -> String {
    let mut entries = match c.read_dir(path) {
        Ok(v) => v,
        Err(Errno::ENOTDIR) => return stat_line(c, path, path),
        Err(e) => return fail(path, e),
    };
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let mut out = String::new();
    for e in entries {
        let full = if path.ends_with('/') {
            format!("{path}{}", e.name)
        } else {
            format!("{path}/{}", e.name)
        };
        out.push_str(&stat_line(c, &full, &e.name));
    }
    if out.is_empty() {
        out.push_str("(empty)\n");
    }
    out
}

fn stat_line(c: &dyn IfsClient, path: &str, name: &str) -> String {
    match c.stat(path, false) {
        Ok(s) => format!(
            "{:<8} {:<10} {:>5} {:>12}  {name}\n",
            s.objtype, s.owner, s.ccsid, s.size
        ),
        Err(e) => fail(path, e),
    }
}

fn cat(c: &dyn IfsClient, path: &str) -> String {
    let text = OpenArgs::new(O_RDONLY | O_TEXTDATA | O_CCSID).conv(1208);
    let fd = match c.open(path, text) {
        Ok(fd) => fd,
        // A binary file has no text form: its bytes as they are.
        Err(Errno::ECONVERT) => match c.open(path, OpenArgs::new(O_RDONLY)) {
            Ok(fd) => fd,
            Err(e) => return fail(path, e),
        },
        Err(e) => return fail(path, e),
    };
    let r = read_all(c, fd);
    let _ = c.close(fd);
    match r {
        Ok(b) => {
            let mut s = String::from_utf8_lossy(&b).into_owned();
            if !s.is_empty() && !s.ends_with('\n') {
                s.push('\n');
            }
            s
        }
        Err(e) => fail(path, e),
    }
}

fn put(c: &dyn IfsClient, path: &str, text: &str) -> String {
    let data = format!("{}\n", text.replace("\\n", "\n"));
    let args = OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC | O_CCSID)
        .mode(0o644)
        .conv(1208);
    let fd = match c.open(path, args) {
        Ok(fd) => fd,
        Err(e) => return fail(path, e),
    };
    let r = write_all(c, fd, data.as_bytes()).and_then(|()| c.close(fd));
    match r {
        Ok(()) => format!("{path}: {} bytes\n", data.len()),
        Err(e) => {
            let _ = c.close(fd);
            fail(path, e)
        }
    }
}

fn rm(c: &dyn IfsClient, path: &str) -> String {
    let r = match c.unlink(path) {
        Err(Errno::EISDIR) => c.rmdir(path),
        r => r,
    };
    r.map_or_else(|e| fail(path, e), |()| format!("{path} removed\n"))
}

fn stat(c: &dyn IfsClient, path: &str) -> String {
    let s = match c.stat(path, false) {
        Ok(s) => s,
        Err(e) => return fail(path, e),
    };
    let mut out = String::new();
    let _ = writeln!(out, "Path       {path}");
    let _ = writeln!(out, "Type       {}", s.objtype);
    let _ = writeln!(
        out,
        "Owner      {} (uid {}), group {} (gid {})",
        s.owner,
        s.uid,
        s.group.as_deref().unwrap_or("*NONE"),
        s.gid
    );
    let _ = writeln!(out, "Mode       {:o}", s.mode);
    let _ = writeln!(out, "Size       {} ({} allocated)", s.size, s.allocsize);
    let _ = writeln!(out, "CCSID      {}", s.ccsid);
    let _ = writeln!(out, "Links      {}", s.nlink);
    let _ = writeln!(out, "File ID    {}/{}/{}", s.vfs, s.ino, s.ino_gen);
    if let Ok(a) = c.dspaut(path) {
        for e in a.entries {
            let _ = writeln!(
                out,
                "Authority  {:<10} {:<8} object {:08X}",
                e.who, e.data, e.objects.0
            );
        }
    }
    out
}

fn df(c: &dyn IfsClient, path: &str) -> String {
    match c.statvfs(path) {
        Ok(v) => {
            let size = v.blocks.saturating_mul(v.frsize);
            let free = v.bfree.saturating_mul(v.frsize);
            format!(
                "{path}: {} file system, {size} bytes ({} MiB), {free} free ({} MiB), \
                 {} of {} file IDs free\n",
                v.name,
                size >> 20,
                free >> 20,
                v.ffree,
                v.files
            )
        }
        Err(e) => fail(path, e),
    }
}
