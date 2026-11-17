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
//!
//! The listing in cookie order is kept for the next page ([`Lists`]),
//! for the same caller while the directory's attributes are the same,
//! so a directory is read once however many pages it takes; a plus
//! page's attributes are one request ([`IfsClient::stat_many`]).

use std::time::{Duration, Instant};

use ifs::client::{Caller, IfsClient};
use ifs::ops::Stat;
use ifs::ops::audit::FileId;

use super::attr::{attr_of, is_dir};
use super::{Fail, IfsVfs, join, node_of};
use crate::server::error::{VResult, VfsError};
use crate::server::handle::FileHandle;
use crate::server::text;
use crate::server::vfs::{DirEntry, DirPage, Req};

/// Listings kept at most.
const KEPT: usize = 4;
/// How long one is kept.
const LIFE: Duration = Duration::from_secs(5);

/// A directory's entries in cookie order: cookie, name, file number.
type Ents = Vec<(u64, String, u64)>;

/// One listing kept.
struct Kept {
    fid: FileId,
    who: Caller,
    /// The directory's attributes it was read under.
    seen: (u64, i64, i64),
    at: Instant,
    ents: Ents,
}

/// The listings kept between pages.
#[derive(Default)]
pub(crate) struct Lists(Vec<Kept>);

impl Lists {
    fn get(&self, fid: FileId, who: &Caller, seen: (u64, i64, i64)) -> Option<&Ents> {
        self.0
            .iter()
            .find(|k| k.fid == fid && &k.who == who && k.seen == seen && k.at.elapsed() < LIFE)
            .map(|k| &k.ents)
    }

    fn put(&mut self, fid: FileId, who: &Caller, seen: (u64, i64, i64), ents: Ents) {
        self.0.retain(|k| k.fid != fid || &k.who != who);
        if self.0.len() >= KEPT {
            self.0.remove(0);
        }
        self.0.push(Kept {
            fid,
            who: who.clone(),
            seen,
            at: Instant::now(),
            ents,
        });
    }
}

/// What a directory's listing is good for.
fn seen_of(st: &Stat) -> (u64, i64, i64) {
    (st.change, st.mtime, st.ctime)
}

#[must_use]
pub fn cookie_of(name: &str) -> u64 {
    let h = name.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    });
    (h >> 2) + 3
}

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
            let seen = seen_of(&st);
            // The first page reads the directory again.
            let kept = if cookie == 0 {
                None
            } else {
                self.lists
                    .borrow()
                    .get(dir.fid, a.who, seen)
                    .map(|e| page_of(e, cookie, max))
            };
            let (all, eof) = if let Some(p) = kept {
                p
            } else {
                let mut ents: Ents =
                    a.c.read_dir(&path)?
                        .into_iter()
                        .map(|e| (cookie_of(&e.name), e.name, e.ino))
                        .collect();
                ents.sort();
                let p = page_of(&ents, cookie, max);
                self.lists.borrow_mut().put(dir.fid, a.who, seen, ents);
                p
            };
            let stats = if plus && !all.is_empty() {
                let paths: Vec<String> = all.iter().map(|(_, n, _)| join(&path, n)).collect();
                a.c.stat_many(&paths, false)?
            } else {
                Vec::new()
            };
            let mut stats = stats.into_iter();
            let mut entries = Vec::with_capacity(all.len());
            for (c, name, ino) in all {
                let node = if plus {
                    stats.next().and_then(Result::ok).map(|s| node_of(r, &s))
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

/// The entries above `cookie`, at most `max` (more to keep two of one
/// cookie together), and whether they reach the end.
fn page_of(ents: &Ents, cookie: u64, max: usize) -> (Ents, bool) {
    let from = ents.partition_point(|(c, _, _)| *c <= cookie);
    let rest = &ents[from..];
    let mut n = rest.len().min(max.max(1));
    // Never end a page between two entries of one cookie.
    while n < rest.len() && n > 0 && rest[n].0 == rest[n - 1].0 {
        n += 1;
    }
    (rest[..n].to_vec(), n >= rest.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_follow_the_cookies() {
        let ents: Ents = vec![
            (5, "a".into(), 1),
            (7, "b".into(), 2),
            (7, "c".into(), 3),
            (9, "d".into(), 4),
        ];
        assert_eq!(page_of(&ents, 0, 1).0.len(), 1);
        let (p, eof) = page_of(&ents, 5, 1);
        assert_eq!(p.len(), 2, "two of one cookie on one page");
        assert!(!eof);
        let (p, eof) = page_of(&ents, 7, 10);
        assert_eq!((p.len(), eof), (1, true));
        assert!(page_of(&ents, 9, 10).0.is_empty());
    }
}
