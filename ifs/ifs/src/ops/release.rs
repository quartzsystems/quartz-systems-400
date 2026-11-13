// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `RLSIFSLCK OBJ()`: the byte-range locks local owners hold on an
//! object, or on every object of its file system, released.

use super::{Ctx, Ifs};
use crate::Result;
use crate::backend::Backend;

impl<B: Backend> Ifs<B> {
    /// Release the byte-range locks on the object at `path` (with
    /// `whole_fs`, on every object of its file system); how many went.
    /// The caller resolves the path as itself.
    pub fn release_ranges(&self, cx: &Ctx<'_>, path: &str, whole_fs: bool) -> Result<usize> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, rest) = self.split(&mut st);
        let r = sys.resolve(&cwd, path, true, Some(cx))?;
        let key = r.node.key();
        if matches!(r.node, crate::resolve::Node::Remote { .. }) {
            // A remote file's locks are its server's: unlocked there.
            return Ok(Self::remote_release(rest, key, whole_fs));
        }
        Ok(if whole_fs {
            rest.locks.release_ranges_on(key.fs)
        } else {
            rest.locks.release_ranges(key)
        })
    }
}
