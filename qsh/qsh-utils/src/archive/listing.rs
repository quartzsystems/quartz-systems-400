// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The verbose table of contents: one `ls -l` style line per member.

use qsh::sys::{Sys, UtilCtx};

use super::member::{Kind, Member};
use crate::common::mode;
use crate::common::time::{Zone, ls_date};

/// Formats listing lines with the utility's clock and zone.
pub struct Lister<'a> {
    now: i64,
    zone: Zone,
    sys: &'a dyn Sys,
}

impl<'a> Lister<'a> {
    /// A lister for a utility.
    pub fn new(cx: &UtilCtx<'a>) -> Self {
        Self {
            sys: cx.sys,
            now: i64::try_from(cx.sys.now().as_secs()).unwrap_or(0),
            zone: Zone::of(cx),
        }
    }

    /// `mode links owner group size date name` (` -> target` for a
    /// symbolic link, ` == name` for a hard link). Owners without a name
    /// in the archive (cpio) are named by the system when it can.
    pub fn line(&self, m: &Member, name: &str) -> String {
        let owner = if m.uname.is_empty() {
            self.sys.user_name(m.uid).unwrap_or_else(|| m.uid.to_string())
        } else {
            m.uname.clone()
        };
        let group = if m.gname.is_empty() {
            self.sys.group_name(m.gid).unwrap_or_else(|| m.gid.to_string())
        } else {
            m.gname.clone()
        };
        let size = match m.kind {
            Kind::Char | Kind::Block => format!("{}, {}", m.rdev.0, m.rdev.1),
            _ => m.size().to_string(),
        };
        let mut s = format!(
            "{} {:>2} {:<8} {:<8} {:>8} {} {}",
            mode::string(&m.stat()),
            m.nlink.max(1),
            owner,
            group,
            size,
            ls_date(m.mtime, self.now, &self.zone),
            name
        );
        match m.kind {
            Kind::Symlink => {
                s.push_str(" -> ");
                s.push_str(&m.link);
            }
            Kind::Hardlink => {
                s.push_str(" == ");
                s.push_str(&m.link);
            }
            _ => {}
        }
        s
    }
}
