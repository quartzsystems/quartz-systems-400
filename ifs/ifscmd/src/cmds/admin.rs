// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `RCLLNK` (the IFS reclaim) and `SAV` (the walk is here; the media
//! are Phase 11's, so the save ends with `CPF3700`).

use std::fmt::Write as _;

use ifs::client::IfsClient;
use ifs::meta::reclaim::{Problem, Repair};
use ifs::ops::WalkOpts;

use crate::msg::{Msg, Reply};

/// `RCLLNK`: the reclaim of the root, `/QOpenSys` and the user-defined
/// file systems (whatever `OBJ` names: the metadata is checked as one).
/// `check_only` reports without changing. Problems left uncorrected end
/// with `CPFA0F1` and the counts.
pub fn rcllnk(c: &dyn IfsClient, check_only: bool) -> Reply {
    let r = match c.rcllnk(check_only) {
        Ok(r) => r,
        Err(e) => return Reply::errno(e, "/"),
    };
    let mut t = String::new();
    for (p, rep) in &r.repaired {
        let what = match rep {
            Repair::MissingMeta => "attributes were missing; defaults written".to_string(),
            Repair::InvalidMeta => "attributes were damaged; defaults written".to_string(),
            Repair::Owner(o) => format!("owner {o} no longer exists; QDFTOWN owns it"),
            Repair::Group(g) => format!("primary group {g} no longer exists; removed"),
            Repair::Private(u) => format!("authority of deleted profile {u} removed"),
            Repair::Autl(l) => format!("authorization list {l} no longer exists; removed"),
            Repair::CheckOut(u) => format!("check-out by deleted profile {u} ended"),
            Repair::StrayAttr(a) => format!("stray attribute {a} removed"),
            Repair::Mirror => "mode, owner or group mirror rewritten".to_string(),
        };
        let _ = writeln!(t, "{p}: {what}");
    }
    for (p, prob) in &r.problems {
        let what = match prob {
            Problem::LinkCount { nlink, found } => {
                format!("link count {nlink}, {found} links found")
            }
            Problem::FoldCollision(other) => format!("name folds like {other}; only one is reachable"),
        };
        let _ = writeln!(t, "{p}: {what}");
    }
    let verb = if check_only { "checked" } else { "reclaimed" };
    let _ = writeln!(
        t,
        "{} object links {verb}; {} repairs, {} problems left.",
        r.objects,
        r.repaired.len(),
        r.problems.len()
    );
    let mut reply = Reply::text(t);
    if !r.problems.is_empty() {
        let total = (r.repaired.len() + r.problems.len()) as u64;
        reply.escape = Some(
            Msg::new("CPFA0F1")
                .u64(r.objects)
                .u64(r.objects)
                .u64(r.repaired.len() as u64)
                .u64(total),
        );
    }
    reply
}

/// `SAV`: every object the paths name is visited (`SUBTREE(*ALL)`
/// follows directories); nothing is written: the escape `CPF3700` says
/// save media arrive in Phase 11.
pub fn sav(c: &dyn IfsClient, objs: &[String], subtree_all: bool) -> Reply {
    let mut n = 0usize;
    for o in objs {
        let o = if o.trim().is_empty() || o.trim() == "*" {
            "."
        } else {
            o.as_str()
        };
        if subtree_all {
            match c.walk(o, WalkOpts::default(), 1 << 20) {
                Ok(v) => n += v.len(),
                Err(e) => return Reply::errno(e, o),
            }
        } else {
            match c.stat(o, false) {
                Ok(_) => n += 1,
                Err(e) => return Reply::errno(e, o),
            }
        }
    }
    let mut r = Reply::text(format!(
        "{n} objects found to save; saving to media comes with Phase 11. Nothing was saved.\n"
    ));
    r.escape = Some(Msg::new("CPF3700"));
    r
}
