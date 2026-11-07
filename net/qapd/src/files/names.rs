// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CreateDirectory`, `Delete`, `Rename` and `Copy`: the calls of `MD`,
//! `RMVLNK` and `RD`, `MOV`, and `CPY SUBTREE(*NODIR)`, with their
//! authority checks and audit entries.

use ifs::Errno;
use ifs::client::IfsClient;
use ifs::ops::CopyOpts;
use ifs::ops::flags::{S_IFDIR, S_IFMT};
use ifscmd::cmds::dirs::{Md, md};
use qap_proto::ops::ifs::{CreateDirectory, FromTo};

use super::{Done, Stop};

/// `MD DIR(path)` with every default.
fn make(c: &dyn IfsClient, path: &str) -> Result<(), Stop> {
    match md(c, &Md::new(path)).escape {
        Some(m) => Err(Stop::escape(m)),
        None => Ok(()),
    }
}

/// `CreateDirectory`: with `parents`, each missing directory above it
/// first, and one there already is no failure (as `mkdir -p`).
pub(super) fn create_directory(c: &dyn IfsClient, req: &CreateDirectory) -> Done {
    let path = req.path.trim_end_matches('/');
    if req.parents {
        let mut at = String::new();
        for comp in path.split('/').filter(|s| !s.is_empty()) {
            at.push('/');
            at.push_str(comp);
            match c.stat(&at, true) {
                Ok(s) if s.mode & S_IFMT == S_IFDIR => {}
                Ok(_) => return Err(Stop::errno(Errno::ENOTDIR, &at)),
                Err(Errno::ENOENT) => make(c, &at)?,
                Err(e) => return Err(Stop::errno(e, &at)),
            }
        }
        return Ok(Vec::new());
    }
    make(c, path)?;
    Ok(Vec::new())
}

/// `Delete`: a directory as `RD` removes it (empty only), anything else
/// as `RMVLNK` (a link, not what it points to).
pub(super) fn delete(c: &dyn IfsClient, path: &str) -> Done {
    let s = c.stat(path, false).map_err(|e| Stop::errno(e, path))?;
    let r = if s.mode & S_IFMT == S_IFDIR {
        c.rmdir(path)
    } else {
        c.unlink(path)
    };
    r.map_err(|e| Stop::errno(e, path))?;
    Ok(Vec::new())
}

/// `Rename`: as `MOV` moves (a rename, or a copy and delete across file
/// systems); `replace` replaces `to`.
pub(super) fn rename(c: &dyn IfsClient, req: &FromTo) -> Done {
    c.mov(&req.from, &req.to, req.replace)
        .map_err(|e| Stop::errno(e, &req.from))?;
    Ok(Vec::new())
}

/// `Copy`: a file as `CPY SUBTREE(*NODIR)` copies it, its CCSID and
/// data as stored, its owner the caller; a directory is refused.
pub(super) fn copy(c: &dyn IfsClient, req: &FromTo) -> Done {
    let from = req.from.as_str();
    let s = c.stat(from, true).map_err(|e| Stop::errno(e, from))?;
    if s.mode & S_IFMT == S_IFDIR {
        return Err(Stop::errno(Errno::EISDIR, from));
    }
    let opts = CopyOpts {
        subtree: false,
        replace: req.replace,
        to_ccsid: None,
        keep_owner: false,
    };
    c.copy(from, &req.to, opts)
        .map_err(|e| Stop::errno(e, from))?;
    Ok(Vec::new())
}
