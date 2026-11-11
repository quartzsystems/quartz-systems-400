// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! 8.3 short names over the IFS, which stores none: each directory's are
//! made from its entries' file numbers by the library's
//! [`smb_server::short_name::assign`] (Windows' hash form, `XXHHHH~1.EXT`),
//! so every listing, query and job gives a file the same one while its
//! directory keeps it unique. A directory's file system decides the case
//! rule: in a case-sensitive one (`/QOpenSys`) a lower-case 8.3 name gets
//! a short name too. A path component that names nothing and could be a
//! short name is looked up as one.

use ifs::client::IfsClient;
use ifs::ops::DirEnt;
use smb_server::short_name::{assign, could_be_short, resolve};

/// A directory's entries without `.` and `..`, and whether its file system
/// tells case apart.
pub(super) fn entries(c: &dyn IfsClient, dir: &str) -> ifs::Result<(Vec<DirEnt>, bool)> {
    let mut v = c.read_dir(dir)?;
    v.retain(|e| e.name != "." && e.name != "..");
    let cs = c.statvfs(dir).is_ok_and(|s| s.case_sensitive);
    Ok((v, cs))
}

/// The short names of `ents`, in their order.
pub(super) fn of(ents: &[DirEnt], case_sensitive: bool) -> Vec<Option<String>> {
    let keyed: Vec<(&str, u64)> = ents.iter().map(|e| (e.name.as_str(), e.ino)).collect();
    assign(&keyed, case_sensitive)
}

/// The short name of the object at `path` (not the share root `root`).
pub(super) fn of_path(c: &dyn IfsClient, path: &str, root: &str) -> Option<String> {
    if path == root {
        return None;
    }
    let (dir, leaf) = path.rsplit_once('/')?;
    let dir = if dir.is_empty() { "/" } else { dir };
    let (ents, cs) = entries(c, dir).ok()?;
    let at = ents.iter().position(|e| e.name == leaf)?;
    of(&ents, cs).swap_remove(at)
}

/// `path` (below `root`) with each component that names nothing but is
/// an entry's short name replaced by the entry's name.
pub(super) fn long_path(c: &dyn IfsClient, root: &str, path: &str) -> String {
    let Some(rest) = path.strip_prefix(root) else {
        return path.to_owned();
    };
    if !rest.split('/').any(could_be_short) || c.stat(path, true).is_ok() {
        return path.to_owned();
    }
    let mut out = root.to_owned();
    for part in rest.split('/').filter(|p| !p.is_empty()) {
        let dir = out.clone();
        if !out.ends_with('/') {
            out.push('/');
        }
        if could_be_short(part)
            && c.stat(&format!("{out}{part}"), true).is_err()
            && let Ok((ents, cs)) = entries(c, &dir)
        {
            let keyed: Vec<(&str, u64)> = ents.iter().map(|e| (e.name.as_str(), e.ino)).collect();
            if let Some(i) = resolve(&keyed, cs, part) {
                out.push_str(&ents[i].name);
                continue;
            }
        }
        out.push_str(part);
    }
    out
}
