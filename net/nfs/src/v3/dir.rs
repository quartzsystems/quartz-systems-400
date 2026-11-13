// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `READDIR` and `READDIRPLUS`. A page starts with `.` (cookie 1) and
//! `..` (cookie 2) when asked from the start, then the file system's
//! entries (cookies from 3, [`crate::server::Vfs::readdir`]). The reply
//! is cut to `count` (`READDIR`) or to `dircount` bytes of names and
//! `maxcount` bytes in all (`READDIRPLUS`), each counted as XDR
//! encodes it; never between two entries of one cookie. A page that
//! cannot hold one entry is `NFS3ERR_TOOSMALL`.

use super::conv::{fattr, fh3, post, status};
use super::proc::Procs;
use crate::server::vfs::DirEntry;
use crate::server::{Node, Req, Vfs, VfsError};
use crate::types::nfs3::{
    READDIR3args, READDIR3res, READDIR3resfail, READDIR3resok, READDIRPLUS3args, READDIRPLUS3res,
    READDIRPLUS3resfail, READDIRPLUS3resok, dirlist3, dirlistplus3, entry3, entryplus3, filename3,
};

/// Bytes of a status, `post_op_attr` with attributes, the verifier,
/// the list's end and `eof`.
const HEAD: usize = 4 + 88 + 8 + 4 + 4;

fn padded(n: usize) -> usize {
    4 + n.div_ceil(4) * 4
}

/// What one entry costs: its directory information and its whole size.
fn cost(e: &DirEntry, plus: bool) -> (usize, usize) {
    let info = 8 + padded(e.name.len()) + 8;
    let mut all = 4 + info;
    if plus {
        all += 4 + 4;
        if e.node.is_some() {
            all += 84 + 4 + 32;
        }
    }
    (info, all)
}

/// The entries that fit: `.` and `..` first, then as many as `dirmax`
/// and `max` allow; whether all did.
fn fit(
    mut list: Vec<DirEntry>,
    plus: bool,
    dirmax: usize,
    max: usize,
) -> Result<(Vec<DirEntry>, bool), VfsError> {
    let (mut info, mut all) = (0usize, HEAD);
    let mut n = 0;
    for e in &list {
        let (i, a) = cost(e, plus);
        if info + i > dirmax || all + a > max {
            break;
        }
        info += i;
        all += a;
        n += 1;
    }
    let whole = n == list.len();
    // Two entries of one cookie stay together.
    while n > 0 && n < list.len() && list[n].cookie == list[n - 1].cookie {
        n -= 1;
    }
    if n == 0 && !list.is_empty() {
        return Err(VfsError::TooSmall);
    }
    list.truncate(n);
    Ok((list, whole))
}

impl<V: Vfs + ?Sized> Procs<'_, V> {
    /// `.` and `..` of a page from `cookie`.
    fn dots(
        &self,
        r: &Req,
        dir: &crate::server::FileHandle,
        cookie: u64,
        plus: bool,
    ) -> Result<Vec<DirEntry>, VfsError> {
        let mut out = Vec::new();
        if cookie == 0 {
            let at = self.vfs.getattr(r, dir)?;
            out.push(DirEntry {
                name: b".".to_vec(),
                fileid: at.fileid,
                cookie: 1,
                node: plus.then_some(Node { fh: *dir, attr: at }),
            });
        }
        if cookie <= 1 {
            let (n, _) = self.vfs.lookup(r, dir, b"..")?;
            out.push(DirEntry {
                name: b"..".to_vec(),
                fileid: n.attr.fileid,
                cookie: 2,
                node: plus.then_some(n),
            });
        }
        Ok(out)
    }

    /// A page: the entries, the verifier, eof and the directory's attributes.
    #[allow(clippy::type_complexity, clippy::result_large_err)]
    fn page(
        &self,
        dirfh: &[u8],
        cookie: u64,
        verf: [u8; 8],
        dirmax: usize,
        max: usize,
        plus: bool,
    ) -> Result<
        (Vec<DirEntry>, [u8; 8], bool, Option<crate::server::Attr>),
        (VfsError, Option<crate::server::Attr>),
    > {
        let (r, h) = self.open(dirfh).map_err(|e| (e, None))?;
        let fail = |e: VfsError| (e, self.vfs.getattr(&r, &h).ok());
        let mut list = if cookie <= 2 {
            self.dots(&r, &h, cookie, plus).map_err(fail)?
        } else {
            Vec::new()
        };
        let per = if plus { 160 } else { 24 };
        let want = (max.min(dirmax.max(per)) / per).clamp(1, 8192);
        let p = self
            .vfs
            .readdir(&r, &h, cookie.max(2), verf, want, plus)
            .map_err(fail)?;
        list.extend(p.entries);
        let (list, whole) = fit(list, plus, dirmax, max).map_err(fail)?;
        Ok((list, p.verf, whole && p.eof, p.dir))
    }

    pub(super) fn readdir(&self, a: &READDIR3args) -> READDIR3res {
        let max = usize::try_from(a.count).unwrap_or(usize::MAX);
        match self.page(&a.dir.data, a.cookie, a.cookieverf, max, max, false) {
            Ok((list, verf, eof, dir)) => READDIR3res::NFS3_OK(READDIR3resok {
                dir_attributes: post(dir.as_ref()),
                cookieverf: verf,
                reply: dirlist3 {
                    entries: list
                        .into_iter()
                        .map(|e| entry3 {
                            fileid: e.fileid,
                            name: filename3(e.name),
                            cookie: e.cookie,
                        })
                        .collect(),
                    eof,
                },
            }),
            Err((e, dir)) => READDIR3res::default(
                status(e),
                READDIR3resfail {
                    dir_attributes: post(dir.as_ref()),
                },
            ),
        }
    }

    pub(super) fn readdirplus(&self, a: &READDIRPLUS3args) -> READDIRPLUS3res {
        let dirmax = usize::try_from(a.dircount).unwrap_or(usize::MAX);
        let max = usize::try_from(a.maxcount).unwrap_or(usize::MAX);
        match self.page(&a.dir.data, a.cookie, a.cookieverf, dirmax, max, true) {
            Ok((list, verf, eof, dir)) => READDIRPLUS3res::NFS3_OK(READDIRPLUS3resok {
                dir_attributes: post(dir.as_ref()),
                cookieverf: verf,
                reply: dirlistplus3 {
                    entries: list
                        .into_iter()
                        .map(|e| entryplus3 {
                            fileid: e.fileid,
                            name: filename3(e.name),
                            cookie: e.cookie,
                            name_attributes: e.node.as_ref().map(|n| fattr(&n.attr)),
                            name_handle: e.node.as_ref().map(|n| fh3(&n.fh)),
                        })
                        .collect(),
                    eof,
                },
            }),
            Err((e, dir)) => READDIRPLUS3res::default(
                status(e),
                READDIRPLUS3resfail {
                    dir_attributes: post(dir.as_ref()),
                },
            ),
        }
    }
}
