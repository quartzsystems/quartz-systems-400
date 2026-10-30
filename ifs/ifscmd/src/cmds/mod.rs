// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The commands, one function each, grouped by what they change:
//! directories and the current directory ([`dirs`]), links ([`links`]),
//! authority ([`auth`]), attributes, check-out and journaling
//! ([`attr`]), user-defined file systems ([`udfs`]), `RCLLNK` and `SAV`
//! ([`admin`]). Paths are taken as typed: the IFS resolves a relative
//! one against the caller's current directory.

pub mod admin;
pub mod attr;
pub mod auth;
pub mod dirs;
pub mod links;
pub mod udfs;

use ifs::Errno;
use ifs::client::IfsClient;
use ifs::ops::WalkOpts;

use crate::msg::{Msg, Reply};

/// `SUBTREE` for the commands that take `*NONE` or `*ALL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Subtree {
    /// The object alone.
    #[default]
    None,
    /// The object and, for a directory, everything under it.
    All,
}

impl Subtree {
    /// From `*NONE`/`*ALL`.
    #[must_use]
    pub fn of(word: &str) -> Self {
        if word.trim().eq_ignore_ascii_case("*ALL") {
            Self::All
        } else {
            Self::None
        }
    }
}

/// The objects a command with `SUBTREE` acts on: the path, then with
/// `*ALL` everything under it, parents first.
pub fn targets(c: &dyn IfsClient, path: &str, subtree: Subtree, follow: bool) -> Result<Vec<String>, Errno> {
    if subtree == Subtree::None {
        c.stat(path, follow)?;
        return Ok(vec![path.to_string()]);
    }
    let all = c.walk(
        path,
        WalkOpts {
            follow,
            one_fs: false,
        },
        1 << 20,
    )?;
    Ok(all.into_iter().map(|e| e.path).collect())
}

/// Apply `f` to each target. One object: its error is the escape. More:
/// each failure is a diagnostic, and the escape `CPFA0AB` names the
/// first that failed; `done` builds the completion from the count.
pub fn each(
    c: &dyn IfsClient,
    path: &str,
    subtree: Subtree,
    follow: bool,
    mut f: impl FnMut(&str) -> Result<(), Errno>,
    done: impl FnOnce(usize) -> Reply,
) -> Reply {
    let all = match targets(c, path, subtree, follow) {
        Ok(v) => v,
        Err(e) => return Reply::errno(e, path),
    };
    if all.len() == 1 && subtree == Subtree::None {
        return match f(&all[0]) {
            Ok(()) => done(1),
            Err(e) => Reply::errno(e, &all[0]),
        };
    }
    let mut failed: Vec<(String, Errno)> = Vec::new();
    let mut ok = 0;
    for p in &all {
        match f(p) {
            Ok(()) => ok += 1,
            Err(e) => failed.push((p.clone(), e)),
        }
    }
    if failed.is_empty() {
        return done(ok);
    }
    let mut r = Reply::fail(Msg::path("CPFA0AB", &failed[0].0));
    for (p, e) in &failed {
        r = r.diag(Msg::errno(*e, p));
    }
    r
}

/// A yes/no special value (`*YES`, or the definitions' 1).
#[must_use]
pub fn yes(word: &str) -> bool {
    let w = word.trim();
    w.eq_ignore_ascii_case("*YES") || w == "1"
}
