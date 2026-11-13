// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every call of the remote file system as NFS version 3 procedures.
//! `JUKEBOX` is waited out (a soft mount gives up after one major
//! timeout); attributes a reply leaves out are asked for.

use std::sync::atomic::Ordering;
use std::time::Duration;

use ifs::Errno;
use ifs::remote::{
    CreateMode, Cred, DirPage, FsStat, LockConflict, LockKind, LockOwner, OpenToken, PathConf,
    RAttr, REntry, RHandle, RKind, RResult, RemoteError, RemoteFs, SetAttr, Share, Stable,
    WriteReply,
};

use super::V3;
use super::attr::{rattr, sattr, time3};
use crate::rpc::Credential;
use crate::types::nfs3::NFS_PROGRAM::NFS_V3::{Args, Res};
use crate::types::nfs3::{
    ACCESS3args, ACCESS3res, COMMIT3args, COMMIT3res, CREATE3args, CREATE3res, FSSTAT3args,
    FSSTAT3res, GETATTR3args, GETATTR3res, LINK3args, LINK3res, LOOKUP3args, LOOKUP3res,
    MKDIR3args, MKDIR3res, MKNOD3args, MKNOD3res, PATHCONF3args, PATHCONF3res, READ3args, READ3res,
    READDIR3args, READDIR3res, READDIRPLUS3args, READDIRPLUS3res, READLINK3args, READLINK3res,
    REMOVE3args, REMOVE3res, RENAME3args, RENAME3res, RMDIR3args, RMDIR3res, SETATTR3args,
    SETATTR3res, SYMLINK3args, SYMLINK3res, WRITE3args, WRITE3res, createhow3, diropargs3, fattr3,
    filename3, mknoddata3, nfs_fh3, nfspath3, nfsstat3, post_op_attr, post_op_fh3, stable_how,
    symlinkdata3,
};

/// Picks a procedure's `resok`, or its status.
macro_rules! pick {
    ($v:ident, $r:ident) => {
        |res: Res| match res {
            Res::$v($r::NFS3_OK(ok)) => Ok(ok),
            Res::$v($r::default(s, ..)) => Err(s),
            _ => Err(nfsstat3::NFS3ERR_SERVERFAULT),
        }
    };
}

fn fh(h: &RHandle) -> nfs_fh3 {
    nfs_fh3 { data: h.0.clone() }
}

fn dirop(dir: &RHandle, name: &[u8]) -> diropargs3 {
    diropargs3 {
        dir: fh(dir),
        name: filename3(name.to_vec()),
    }
}

fn eio() -> RemoteError {
    RemoteError::Errno(Errno::EIO)
}

impl V3 {
    /// Calls `args` and picks its `resok`; `JUKEBOX` waits and asks
    /// again.
    fn call<T>(
        &self,
        cred: &Credential,
        args: &Args,
        pick: impl Fn(Res) -> Result<T, nfsstat3>,
    ) -> RResult<T> {
        let mut n = 0u32;
        loop {
            match pick(self.nfs.call::<Args, Res>(cred, args)?) {
                Ok(t) => return Ok(t),
                Err(nfsstat3::NFS3ERR_JUKEBOX) => {
                    n += 1;
                    let wait = Duration::from_millis(100)
                        .saturating_mul(1 << n.min(6))
                        .min(Duration::from_secs(5));
                    if self.policy.soft && wait.saturating_mul(n) > self.policy.major() {
                        return Err(RemoteError::TimedOut);
                    }
                    std::thread::sleep(wait);
                }
                Err(s) => return Err(super::super::status::nfs3(s)),
            }
        }
    }

    fn attrs(&self, c: &Credential, h: &RHandle, a: &post_op_attr) -> RResult<RAttr> {
        match a {
            Some(a) => Ok(rattr(a)),
            None => self.get(c, h),
        }
    }

    fn get(&self, c: &Credential, h: &RHandle) -> RResult<RAttr> {
        self.call(
            c,
            &Args::NFSPROC3_GETATTR(GETATTR3args { object: fh(h) }),
            |r| match r {
                Res::NFSPROC3_GETATTR(GETATTR3res::NFS3_OK(ok)) => Ok(ok.obj_attributes),
                Res::NFSPROC3_GETATTR(GETATTR3res::default(s)) => Err(s),
                _ => Err(nfsstat3::NFS3ERR_SERVERFAULT),
            },
        )
        .map(|a: fattr3| rattr(&a))
    }

    /// What a creating call answered: the handle (looked up when the
    /// reply left it out) and its attributes.
    fn made(
        &self,
        c: &Credential,
        dir: &RHandle,
        name: &[u8],
        obj: post_op_fh3,
        a: &post_op_attr,
    ) -> RResult<(RHandle, RAttr)> {
        match obj {
            Some(h) => {
                let h = RHandle(h.data);
                let a = self.attrs(c, &h, a)?;
                Ok((h, a))
            }
            None => self.lookup_wire(c, dir, name),
        }
    }

    fn lookup_wire(&self, c: &Credential, dir: &RHandle, name: &[u8]) -> RResult<(RHandle, RAttr)> {
        let ok = self.call(
            c,
            &Args::NFSPROC3_LOOKUP(LOOKUP3args {
                what: dirop(dir, name),
            }),
            pick!(NFSPROC3_LOOKUP, LOOKUP3res),
        )?;
        let h = RHandle(ok.object.data);
        let a = self.attrs(c, &h, &ok.obj_attributes)?;
        Ok((h, a))
    }

    fn page_plus(
        &self,
        c: &Credential,
        dir: &RHandle,
        cookie: u64,
        verf: u64,
        max: u32,
    ) -> RResult<Option<DirPage>> {
        let args = Args::NFSPROC3_READDIRPLUS(READDIRPLUS3args {
            dir: fh(dir),
            cookie,
            cookieverf: verf.to_be_bytes(),
            dircount: max.max(1024),
            maxcount: max.saturating_mul(8).clamp(8192, self.rsize.max(8192)),
        });
        match self.call(c, &args, pick!(NFSPROC3_READDIRPLUS, READDIRPLUS3res)) {
            Ok(ok) => Ok(Some(DirPage {
                verifier: u64::from_be_bytes(ok.cookieverf),
                eof: ok.reply.eof,
                entries: ok
                    .reply
                    .entries
                    .into_iter()
                    .map(|e| REntry {
                        name: e.name.0,
                        cookie: e.cookie,
                        fileid: e.fileid,
                        attr: e.name_attributes.as_ref().map(rattr),
                        handle: e.name_handle.map(|h| RHandle(h.data)),
                    })
                    .collect(),
            })),
            Err(RemoteError::Errno(Errno::ENOTSUP)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn page_plain(
        &self,
        c: &Credential,
        dir: &RHandle,
        cookie: u64,
        verf: u64,
        max: u32,
    ) -> RResult<DirPage> {
        let ok = self.call(
            c,
            &Args::NFSPROC3_READDIR(READDIR3args {
                dir: fh(dir),
                cookie,
                cookieverf: verf.to_be_bytes(),
                count: max.max(1024),
            }),
            pick!(NFSPROC3_READDIR, READDIR3res),
        )?;
        Ok(DirPage {
            verifier: u64::from_be_bytes(ok.cookieverf),
            eof: ok.reply.eof,
            entries: ok
                .reply
                .entries
                .into_iter()
                .map(|e| REntry {
                    name: e.name.0,
                    cookie: e.cookie,
                    fileid: e.fileid,
                    attr: None,
                    handle: None,
                })
                .collect(),
        })
    }

    fn lockd(&self) -> RResult<&std::sync::Arc<super::super::nlm::Locker>> {
        self.locker
            .as_ref()
            .ok_or(RemoteError::Errno(Errno::ENOLCK))
    }
}

impl RemoteFs for V3 {
    fn root(&self) -> RResult<(RHandle, RAttr)> {
        let a = self.get(&self.owner, &self.root)?;
        if a.kind != RKind::Dir {
            return Err(RemoteError::Errno(Errno::ENOTDIR));
        }
        Ok((self.root.clone(), a))
    }

    fn getattr(&self, cred: &Cred, h: &RHandle) -> RResult<RAttr> {
        self.get(&self.cred(cred), h)
    }

    fn setattr(
        &self,
        cred: &Cred,
        h: &RHandle,
        set: &SetAttr,
        guard: Option<i64>,
    ) -> RResult<RAttr> {
        let c = self.cred(cred);
        let ok = self.call(
            &c,
            &Args::NFSPROC3_SETATTR(SETATTR3args {
                object: fh(h),
                new_attributes: sattr(set),
                guard: guard.map(time3),
            }),
            pick!(NFSPROC3_SETATTR, SETATTR3res),
        )?;
        self.attrs(&c, h, &ok.obj_wcc.after)
    }

    fn lookup(&self, cred: &Cred, dir: &RHandle, name: &[u8]) -> RResult<(RHandle, RAttr)> {
        self.lookup_wire(&self.cred(cred), dir, name)
    }

    fn access(&self, cred: &Cred, h: &RHandle, mask: u32) -> RResult<u32> {
        // The IFS's bits are ACCESS3's.
        self.call(
            &self.cred(cred),
            &Args::NFSPROC3_ACCESS(ACCESS3args {
                object: fh(h),
                access: mask & 0x3f,
            }),
            pick!(NFSPROC3_ACCESS, ACCESS3res),
        )
        .map(|ok| ok.access & mask)
    }

    fn readlink(&self, cred: &Cred, h: &RHandle) -> RResult<Vec<u8>> {
        self.call(
            &self.cred(cred),
            &Args::NFSPROC3_READLINK(READLINK3args { symlink: fh(h) }),
            pick!(NFSPROC3_READLINK, READLINK3res),
        )
        .map(|ok| ok.data.0)
    }

    fn read(&self, cred: &Cred, h: &RHandle, off: u64, len: u32) -> RResult<(Vec<u8>, bool)> {
        let ok = self.call(
            &self.cred(cred),
            &Args::NFSPROC3_READ(READ3args {
                file: fh(h),
                offset: off,
                count: len.min(self.rsize),
            }),
            pick!(NFSPROC3_READ, READ3res),
        )?;
        if ok.data.len() > len as usize {
            return Err(eio());
        }
        Ok((ok.data, ok.eof))
    }

    fn write(
        &self,
        cred: &Cred,
        h: &RHandle,
        off: u64,
        data: &[u8],
        stable: Stable,
    ) -> RResult<WriteReply> {
        let how = match stable {
            Stable::Unstable => stable_how::UNSTABLE,
            Stable::DataSync => stable_how::DATA_SYNC,
            Stable::FileSync => stable_how::FILE_SYNC,
        };
        let ok = self.call(
            &self.cred(cred),
            &Args::NFSPROC3_WRITE(WRITE3args {
                file: fh(h),
                offset: off,
                count: u32::try_from(data.len()).map_err(|_| RemoteError::Errno(Errno::EFBIG))?,
                stable: how,
                data: data.to_vec(),
            }),
            pick!(NFSPROC3_WRITE, WRITE3res),
        )?;
        if ok.count as usize > data.len() {
            return Err(eio());
        }
        Ok(WriteReply {
            count: ok.count,
            committed: match ok.committed {
                stable_how::UNSTABLE => Stable::Unstable,
                stable_how::DATA_SYNC => Stable::DataSync,
                stable_how::FILE_SYNC => Stable::FileSync,
            },
            verifier: u64::from_be_bytes(ok.verf),
        })
    }

    fn commit(&self, cred: &Cred, h: &RHandle, off: u64, len: u32) -> RResult<u64> {
        self.call(
            &self.cred(cred),
            &Args::NFSPROC3_COMMIT(COMMIT3args {
                file: fh(h),
                offset: off,
                count: len,
            }),
            pick!(NFSPROC3_COMMIT, COMMIT3res),
        )
        .map(|ok| u64::from_be_bytes(ok.verf))
    }

    fn create(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        how: CreateMode,
    ) -> RResult<(RHandle, RAttr)> {
        let c = self.cred(cred);
        let how = match how {
            CreateMode::Unchecked(s) => createhow3::UNCHECKED(sattr(&s)),
            CreateMode::Guarded(s) => createhow3::GUARDED(sattr(&s)),
            CreateMode::Exclusive(v) => createhow3::EXCLUSIVE(v.to_be_bytes()),
        };
        let ok = self.call(
            &c,
            &Args::NFSPROC3_CREATE(CREATE3args {
                r#where: dirop(dir, name),
                how,
            }),
            pick!(NFSPROC3_CREATE, CREATE3res),
        )?;
        self.made(&c, dir, name, ok.obj, &ok.obj_attributes)
    }

    fn mkdir(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)> {
        let c = self.cred(cred);
        let ok = self.call(
            &c,
            &Args::NFSPROC3_MKDIR(MKDIR3args {
                r#where: dirop(dir, name),
                attributes: sattr(set),
            }),
            pick!(NFSPROC3_MKDIR, MKDIR3res),
        )?;
        self.made(&c, dir, name, ok.obj, &ok.obj_attributes)
    }

    fn symlink(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        target: &[u8],
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)> {
        let c = self.cred(cred);
        let ok = self.call(
            &c,
            &Args::NFSPROC3_SYMLINK(SYMLINK3args {
                r#where: dirop(dir, name),
                symlink: symlinkdata3 {
                    symlink_attributes: sattr(set),
                    symlink_data: nfspath3(target.to_vec()),
                },
            }),
            pick!(NFSPROC3_SYMLINK, SYMLINK3res),
        )?;
        self.made(&c, dir, name, ok.obj, &ok.obj_attributes)
    }

    fn mknod(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        kind: RKind,
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)> {
        if kind != RKind::Fifo {
            return Err(RemoteError::Errno(Errno::ENOTSUP));
        }
        let c = self.cred(cred);
        let ok = self.call(
            &c,
            &Args::NFSPROC3_MKNOD(MKNOD3args {
                r#where: dirop(dir, name),
                what: mknoddata3::NF3FIFO(sattr(set)),
            }),
            pick!(NFSPROC3_MKNOD, MKNOD3res),
        )?;
        self.made(&c, dir, name, ok.obj, &ok.obj_attributes)
    }

    fn remove(&self, cred: &Cred, dir: &RHandle, name: &[u8]) -> RResult<()> {
        self.call(
            &self.cred(cred),
            &Args::NFSPROC3_REMOVE(REMOVE3args {
                object: dirop(dir, name),
            }),
            pick!(NFSPROC3_REMOVE, REMOVE3res),
        )
        .map(|_| ())
    }

    fn rmdir(&self, cred: &Cred, dir: &RHandle, name: &[u8]) -> RResult<()> {
        self.call(
            &self.cred(cred),
            &Args::NFSPROC3_RMDIR(RMDIR3args {
                object: dirop(dir, name),
            }),
            pick!(NFSPROC3_RMDIR, RMDIR3res),
        )
        .map(|_| ())
    }

    fn rename(
        &self,
        cred: &Cred,
        from_dir: &RHandle,
        from: &[u8],
        to_dir: &RHandle,
        to: &[u8],
    ) -> RResult<()> {
        self.call(
            &self.cred(cred),
            &Args::NFSPROC3_RENAME(RENAME3args {
                from: dirop(from_dir, from),
                to: dirop(to_dir, to),
            }),
            pick!(NFSPROC3_RENAME, RENAME3res),
        )
        .map(|_| ())
    }

    fn link(&self, cred: &Cred, h: &RHandle, dir: &RHandle, name: &[u8]) -> RResult<()> {
        self.call(
            &self.cred(cred),
            &Args::NFSPROC3_LINK(LINK3args {
                file: fh(h),
                link: dirop(dir, name),
            }),
            pick!(NFSPROC3_LINK, LINK3res),
        )
        .map(|_| ())
    }

    fn readdir(
        &self,
        cred: &Cred,
        dir: &RHandle,
        cookie: u64,
        verifier: u64,
        max: u32,
    ) -> RResult<DirPage> {
        let c = self.cred(cred);
        let (mut cookie, mut verifier) = (cookie, verifier);
        // A page of `.` and `..` alone goes on to the next.
        for _ in 0..64 {
            let page = if self.plus.load(Ordering::Relaxed) {
                if let Some(p) = self.page_plus(&c, dir, cookie, verifier, max)? {
                    p
                } else {
                    // The server has no READDIRPLUS: READDIR from now on.
                    self.plus.store(false, Ordering::Relaxed);
                    self.page_plain(&c, dir, cookie, verifier, max)?
                }
            } else {
                self.page_plain(&c, dir, cookie, verifier, max)?
            };
            let last = page.entries.last().map(|e| e.cookie);
            let mut p = page;
            p.entries.retain(|e| e.name != b"." && e.name != b"..");
            if !p.entries.is_empty() || p.eof {
                return Ok(p);
            }
            match last {
                Some(k) => {
                    cookie = k;
                    verifier = p.verifier;
                }
                None => return Ok(p),
            }
        }
        Err(eio())
    }

    fn statfs(&self, cred: &Cred, h: &RHandle) -> RResult<FsStat> {
        self.call(
            &self.cred(cred),
            &Args::NFSPROC3_FSSTAT(FSSTAT3args { fsroot: fh(h) }),
            pick!(NFSPROC3_FSSTAT, FSSTAT3res),
        )
        .map(|ok| FsStat {
            total_bytes: ok.tbytes,
            free_bytes: ok.fbytes,
            avail_bytes: ok.abytes,
            total_files: ok.tfiles,
            free_files: ok.ffiles,
            avail_files: ok.afiles,
        })
    }

    fn pathconf(&self, cred: &Cred, h: &RHandle) -> RResult<PathConf> {
        self.call(
            &self.cred(cred),
            &Args::NFSPROC3_PATHCONF(PATHCONF3args { object: fh(h) }),
            pick!(NFSPROC3_PATHCONF, PATHCONF3res),
        )
        .map(|ok| PathConf {
            link_max: ok.linkmax,
            name_max: ok.name_max,
            no_trunc: ok.no_trunc,
            chown_restricted: ok.chown_restricted,
            case_insensitive: ok.case_insensitive,
            case_preserving: ok.case_preserving,
        })
    }

    fn open(
        &self,
        _cred: &Cred,
        _h: &RHandle,
        _share: Share,
        _owner: LockOwner,
    ) -> RResult<OpenToken> {
        // Version 3 has no opens: share modes are the IFS's among local
        // jobs.
        Ok(OpenToken::default())
    }

    fn close(&self, _cred: &Cred, _h: &RHandle, _token: &OpenToken) -> RResult<()> {
        Ok(())
    }

    fn lock(
        &self,
        _cred: &Cred,
        h: &RHandle,
        owner: LockOwner,
        kind: LockKind,
        off: u64,
        len: u64,
        _token: &OpenToken,
    ) -> RResult<()> {
        self.lockd()?.lock(&h.0, owner, kind, off, len)
    }

    fn unlock(
        &self,
        _cred: &Cred,
        h: &RHandle,
        owner: LockOwner,
        off: u64,
        len: u64,
    ) -> RResult<()> {
        self.lockd()?.unlock(&h.0, owner, off, len)
    }

    fn test(
        &self,
        _cred: &Cred,
        h: &RHandle,
        owner: LockOwner,
        kind: LockKind,
        off: u64,
        len: u64,
    ) -> RResult<Option<LockConflict>> {
        self.lockd()?.test(&h.0, owner, kind, off, len)
    }

    fn unmount(&self) -> RResult<()> {
        self.end()
    }
}
