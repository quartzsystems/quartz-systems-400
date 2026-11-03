// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ListDirectory` and `GetInfo`: the directory read and each link's
//! attributes, as `WRKLNK` and `DSPLNK` read them (`*R` on the
//! directory, `*X` on the path), names matched and ordered as they
//! match and order them.

use ifs::client::IfsClient;
use ifs::ops::Stat;
use ifs::ops::flags::{S_IFDIR, S_IFLNK, S_IFMT, S_IFREG};
use ifscmd::display::{folds, join, matches, split};
use qap_proto::ops::ifs::{Entries, Entry, EntryKind, ListDirectory};

use super::{Done, Stop};

/// What a link is, by its mode.
fn kind_of(s: &Stat) -> EntryKind {
    match s.mode & S_IFMT {
        S_IFREG => EntryKind::File,
        S_IFDIR => EntryKind::Directory,
        S_IFLNK => EntryKind::Symlink,
        _ => EntryKind::Other,
    }
}

/// Milliseconds from nanoseconds.
fn ms(ns: i64) -> i64 {
    ns.div_euclid(1_000_000)
}

/// The entry for `path`, named `name`, not following a link.
pub(super) fn entry(c: &dyn IfsClient, path: &str, name: &str) -> Result<Entry, ifs::Errno> {
    let s = c.stat(path, false)?;
    let kind = kind_of(&s);
    let target = if kind == EntryKind::Symlink {
        c.readlink(path).ok()
    } else {
        None
    };
    Ok(Entry {
        name: name.to_string(),
        kind,
        size: s.size,
        modified: ms(s.mtime),
        owner: s.owner,
        ccsid: s.ccsid,
        target,
    })
}

/// `ListDirectory`: the names matching, ordered as the displays order
/// them (by name without case, then with it), from `start`.
pub(super) fn list(c: &dyn IfsClient, req: &ListDirectory) -> Done {
    let dir = req.path.as_str();
    let pattern = if req.pattern.is_empty() { "*" } else { req.pattern.as_str() };
    let fold = folds(dir);
    let mut names: Vec<String> = c
        .read_dir(dir)
        .map_err(|e| Stop::errno(e, dir))?
        .into_iter()
        .map(|e| e.name)
        .filter(|n| matches(pattern, n, fold))
        .collect();
    names.sort_by(|a, b| a.to_uppercase().cmp(&b.to_uppercase()).then_with(|| a.cmp(b)));
    let total = u32::try_from(names.len()).unwrap_or(u32::MAX);
    let start = usize::try_from(req.start).unwrap_or(usize::MAX);
    let max = match req.max {
        0 => usize::MAX,
        m => usize::try_from(m).unwrap_or(usize::MAX),
    };
    let entries = names
        .iter()
        .skip(start)
        .take(max)
        .map(|n| {
            // A link that cannot be read is still listed, by name.
            entry(c, &join(dir, n), n).unwrap_or_else(|_| Entry {
                name: n.clone(),
                kind: EntryKind::Other,
                ..Entry::default()
            })
        })
        .collect();
    Ok(ipc::encode(&Entries { total, entries }))
}

/// `GetInfo`: one link, not followed.
pub(super) fn info(c: &dyn IfsClient, path: &str) -> Done {
    let (_, name) = split(path);
    let e = entry(c, path, &name).map_err(|e| Stop::errno(e, path))?;
    Ok(ipc::encode(&e))
}
