// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Directory pages. A cookie is a number the entry's name hashes to
//! (FNV-1a, 62 bits, plus 3: 1 and 2 are left for `.` and `..`), and a
//! page is the entries above the cookie asked for in cookie order. So
//! a cookie stays good while the directory changes: an entry removed is
//! not seen, one added may be, and none is seen twice or skipped. Two
//! names with one number stay on one page ([`DirPage`] never ends
//! between them). The cookie verifier is the directory's file ID
//! hashed: a cookie from another directory's page is `BadCookie`.

use ifs::client::IfsClient;
use ifs::ops::audit::FileId;

use super::attr::{attr_of, is_dir};
use super::{Fail, IfsVfs, join, node_of};
use crate::server::error::{VResult, VfsError};
use crate::server::handle::FileHandle;
use crate::server::text;
use crate::server::vfs::{DirEntry, DirPage, Req};

/// The cookie of an entry named `name`.
#[must_use]
pub fn cookie_of(name: &str) -> u64 {
    let h = name.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    });
    (h >> 2) + 3
}

/// The cookie verifier of a directory.
#[must_use]
pub fn verf_of(fid: FileId) -> [u8; 8] {
    let h = fid
        .to_bytes()
        .iter()
        .fold(0x6e66_7333_7665_7266_u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100_0000_01b3)
        });
    h.to_be_bytes()
}

impl<C: IfsClient> IfsVfs<C> {
    pub(crate) fn do_readdir(
        &self,
        r: &Req,
        dir: &FileHandle,
        cookie: u64,
        verf: [u8; 8],
        max: usize,
        plus: bool,
    ) -> VResult<DirPage> {
        let want = verf_of(dir.fid);
        if cookie != 0 && verf != want && verf != [0; 8] {
            return Err(VfsError::BadCookie);
        }
        self.run(r, |a| {
            let (path, st) = self.locate(a, r, dir)?;
            if !is_dir(&st) {
                return Err(Fail::V(VfsError::NotDir));
            }
            let mut all: Vec<(u64, String, u64)> =
                a.c.read_dir(&path)?
                    .into_iter()
                    .map(|e| (cookie_of(&e.name), e.name, e.ino))
                    .filter(|(c, _, _)| *c > cookie)
                    .collect();
            all.sort();
            let mut n = all.len().min(max.max(1));
            // Never end a page between two entries of one cookie.
            while n < all.len() && n > 0 && all[n].0 == all[n - 1].0 {
                n += 1;
            }
            let eof = n >= all.len();
            all.truncate(n);
            let mut entries = Vec::with_capacity(all.len());
            for (c, name, ino) in all {
                let node = if plus {
                    a.c.stat(&join(&path, &name), false)
                        .ok()
                        .map(|s| node_of(r, &s))
                } else {
                    None
                };
                entries.push(DirEntry {
                    name: text::to_client(r.opts.path_ccsid, &name)?,
                    fileid: ino,
                    cookie: c,
                    node,
                });
            }
            Ok(DirPage {
                verf: want,
                entries,
                eof,
                dir: Some(attr_of(r, &st)),
            })
        })
    }
}
