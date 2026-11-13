// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every call of the remote file system as a `COMPOUND`: `PUTFH`, the
//! operation, and `GETFH`/`GETATTR` where the IFS wants the result.
//! A regular file is made by `OPEN` with create (then closed); opens
//! are `OPEN` (`CLAIM_FH` in 4.1, `CLAIM_NULL` by the name the handle
//! was found by in 4.0); reads and writes carry an open's stateid.

use std::sync::PoisonError;

use ifs::Errno;
use ifs::remote::{
    CreateMode, Cred, DirPage, FsStat, LockConflict, LockKind, LockOwner, OpenToken, PathConf,
    RAttr, REntry, RHandle, RKind, RResult, RemoteError, RemoteFs, SetAttr, Share, Stable,
    WriteReply,
};

use super::V4;
use super::attr::{self, rattr};
use super::state::{self, Claim, Open};
use crate::client::ranges::Range;
use crate::rpc::Credential;
use crate::types::nfs4::{
    ACCESS4args, ACCESS4res, CLOSE4args, COMMIT4args, COMMIT4res, CREATE4args, GETATTR4args,
    GETATTR4res, GETFH4res, LINK4args, LOCK4denied, LOCKT4args, LOCKT4res, LOCKU4args, LOOKUP4args,
    OPEN4_SHARE_ACCESS_READ, OPEN4_SHARE_ACCESS_WRITE, PUTFH4args, READ4args, READ4res,
    READDIR4args, READDIR4res, READLINK4res, REMOVE4args, RENAME4args, SETATTR4args, VERIFY4args,
    WRITE4args, WRITE4res, createhow4, createtype4, creatverfattr, fattr4, nfs_argop4, nfs_fh4,
    nfs_lock_type4, nfs_resop4, nfsstat4, stable_how4, utf8string,
};

fn putfh(h: &RHandle) -> nfs_argop4 {
    nfs_argop4::OP_PUTFH(PUTFH4args {
        object: nfs_fh4(h.0.clone()),
    })
}

fn getattr(list: &[u32]) -> nfs_argop4 {
    nfs_argop4::OP_GETATTR(GETATTR4args {
        attr_request: attr::bitmap(list),
    })
}

fn name(n: &[u8]) -> utf8string {
    utf8string(n.to_vec())
}

fn eio() -> RemoteError {
    RemoteError::Errno(Errno::EIO)
}

/// The attributes `GETATTR` answered at `i`.
fn attrs_at(list: &[nfs_resop4], i: usize) -> RResult<attr::Attrs> {
    match list.get(i) {
        Some(nfs_resop4::OP_GETATTR(GETATTR4res::NFS4_OK(a))) => {
            attr::decode(&a.obj_attributes).map_err(|_| eio())
        }
        _ => Err(eio()),
    }
}

/// The handle `GETFH` answered at `i`.
fn fh_at(list: &[nfs_resop4], i: usize) -> RResult<RHandle> {
    match list.get(i) {
        Some(nfs_resop4::OP_GETFH(GETFH4res::NFS4_OK(f))) => Ok(RHandle(f.object.0.clone())),
        _ => Err(eio()),
    }
}

fn conflict(d: &LOCK4denied) -> LockConflict {
    let mut tail = [0u8; 8];
    let o = &d.owner.owner;
    let k = o.len().min(8);
    tail[8 - k..].copy_from_slice(&o[o.len() - k..]);
    LockConflict {
        kind: match d.locktype {
            nfs_lock_type4::READ_LT | nfs_lock_type4::READW_LT => LockKind::Read,
            nfs_lock_type4::WRITE_LT | nfs_lock_type4::WRITEW_LT => LockKind::Write,
        },
        off: d.offset,
        len: if d.length == u64::MAX { 0 } else { d.length },
        holder: u64::from_be_bytes(tail),
    }
}

impl V4 {
    fn cred(&self, c: &Cred) -> Credential {
        crate::client::wire_cred(&self.core.host, c)
    }

    fn run(
        &self,
        c: &Cred,
        build: &dyn Fn(&super::core::Core) -> Vec<nfs_argop4>,
    ) -> RResult<Vec<nfs_resop4>> {
        self.core.run(&self.cred(c), build)
    }

    fn remember(&self, dir: &RHandle, n: &[u8], h: &RHandle) {
        if self.core.minor == 0 {
            self.core
                .state()
                .names
                .insert(h.clone(), (dir.clone(), n.to_vec()));
        }
    }

    /// `CREATE` of a directory, link or FIFO.
    fn make(
        &self,
        cred: &Cred,
        dir: &RHandle,
        n: &[u8],
        t: &createtype4,
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)> {
        let list = self.run(cred, &|_| {
            vec![
                putfh(dir),
                nfs_argop4::OP_CREATE(CREATE4args {
                    objtype: t.clone(),
                    objname: name(n),
                    createattrs: attr::settable(set),
                }),
                nfs_argop4::OP_GETFH,
                getattr(attr::OBJECT),
            ]
        })?;
        let h = fh_at(&list, 2)?;
        self.remember(dir, n, &h);
        Ok((h, rattr(&attrs_at(&list, 3)?)))
    }

    /// The kind of `n` in `dir`.
    fn kind_of(&self, cred: &Cred, dir: &RHandle, n: &[u8]) -> RResult<RKind> {
        let list = self.run(cred, &|_| {
            vec![
                putfh(dir),
                nfs_argop4::OP_LOOKUP(LOOKUP4args { objname: name(n) }),
                getattr(&[crate::types::nfs4::FATTR4_TYPE]),
            ]
        })?;
        Ok(attr::kind(attrs_at(&list, 2)?.ftype.unwrap_or(1)))
    }

    fn close_wire(&self, cred: &Credential, o: &Open) -> RResult<()> {
        let seq = self.core.state().owner_seq(o.owner);
        let mut s = seq.lock().unwrap_or_else(PoisonError::into_inner);
        let s0 = *s;
        let (st, _) = self.core.send(
            cred,
            &|c: &super::core::Core| {
                let id = c
                    .state()
                    .opens
                    .values()
                    .find(|x| x.fh == o.fh && x.owner == o.owner)
                    .map_or_else(|| o.stateid.clone(), |x| x.stateid.clone());
                vec![
                    putfh(&o.fh),
                    nfs_argop4::OP_CLOSE(CLOSE4args {
                        seqid: s0,
                        open_stateid: id,
                    }),
                ]
            },
            true,
        )?;
        if self.core.minor == 0 && state::moves_seqid(st) {
            *s = s.wrapping_add(1);
        }
        match st {
            nfsstat4::NFS4_OK | nfsstat4::NFS4ERR_BAD_STATEID | nfsstat4::NFS4ERR_EXPIRED => Ok(()),
            s => Err(crate::client::status::nfs4(s)),
        }
    }

    fn page(
        &self,
        cred: &Cred,
        dir: &RHandle,
        cookie: u64,
        verf: u64,
        max: u32,
    ) -> RResult<DirPage> {
        let list = self.run(cred, &|_| {
            vec![
                putfh(dir),
                nfs_argop4::OP_READDIR(READDIR4args {
                    cookie,
                    cookieverf: verf.to_be_bytes(),
                    dircount: max.max(1024),
                    maxcount: max.saturating_mul(8).clamp(8192, self.rsize.max(8192)),
                    attr_request: attr::bitmap(attr::ENTRY),
                }),
            ]
        })?;
        let Some(nfs_resop4::OP_READDIR(READDIR4res::NFS4_OK(ok))) = list.get(1) else {
            return Err(eio());
        };
        let mut entries = Vec::with_capacity(ok.reply.entries.len());
        for e in &ok.reply.entries {
            let a = attr::decode(&e.attrs).map_err(|_| eio())?;
            let good = a.rdattr_error.is_none_or(|x| x == 0);
            let handle = a.filehandle.clone().filter(|_| good).map(RHandle);
            if let Some(h) = &handle {
                self.remember(dir, &e.name.0, h);
            }
            entries.push(REntry {
                name: e.name.0.clone(),
                cookie: e.cookie,
                fileid: a.fileid.unwrap_or(0),
                attr: (good && a.ftype.is_some()).then(|| rattr(&a)),
                handle,
            });
        }
        Ok(DirPage {
            entries,
            eof: ok.reply.eof,
            verifier: u64::from_be_bytes(ok.cookieverf),
        })
    }
}

impl RemoteFs for V4 {
    fn root(&self) -> RResult<(RHandle, RAttr)> {
        let owner = self.core.owner.clone();
        let list = self
            .core
            .run(&owner, &|_| vec![putfh(&self.root), getattr(attr::OBJECT)])?;
        Ok((self.root.clone(), rattr(&attrs_at(&list, 1)?)))
    }

    fn getattr(&self, cred: &Cred, h: &RHandle) -> RResult<RAttr> {
        let list = self.run(cred, &|_| vec![putfh(h), getattr(attr::OBJECT)])?;
        Ok(rattr(&attrs_at(&list, 1)?))
    }

    fn setattr(
        &self,
        cred: &Cred,
        h: &RHandle,
        set: &SetAttr,
        guard: Option<i64>,
    ) -> RResult<RAttr> {
        let list = self.run(cred, &|c| {
            let mut v = vec![putfh(h)];
            if let Some(g) = guard {
                v.push(nfs_argop4::OP_VERIFY(VERIFY4args {
                    obj_attributes: attr::ctime_is(g),
                }));
            }
            let stateid = if set.size.is_some() {
                c.state().io_stateid(h, OPEN4_SHARE_ACCESS_WRITE)
            } else {
                state::anonymous()
            };
            v.push(nfs_argop4::OP_SETATTR(SETATTR4args {
                stateid,
                obj_attributes: attr::settable(set),
            }));
            v.push(getattr(attr::OBJECT));
            v
        })?;
        Ok(rattr(&attrs_at(&list, list.len() - 1)?))
    }

    fn lookup(&self, cred: &Cred, dir: &RHandle, n: &[u8]) -> RResult<(RHandle, RAttr)> {
        let list = self.run(cred, &|_| {
            vec![
                putfh(dir),
                nfs_argop4::OP_LOOKUP(LOOKUP4args { objname: name(n) }),
                nfs_argop4::OP_GETFH,
                getattr(attr::OBJECT),
            ]
        })?;
        let h = fh_at(&list, 2)?;
        self.remember(dir, n, &h);
        Ok((h, rattr(&attrs_at(&list, 3)?)))
    }

    fn access(&self, cred: &Cred, h: &RHandle, mask: u32) -> RResult<u32> {
        let list = self.run(cred, &|_| {
            vec![
                putfh(h),
                nfs_argop4::OP_ACCESS(ACCESS4args {
                    access: mask & 0x3f,
                }),
            ]
        })?;
        match list.get(1) {
            Some(nfs_resop4::OP_ACCESS(ACCESS4res::NFS4_OK(ok))) => Ok(ok.access & mask),
            _ => Err(eio()),
        }
    }

    fn readlink(&self, cred: &Cred, h: &RHandle) -> RResult<Vec<u8>> {
        let list = self.run(cred, &|_| vec![putfh(h), nfs_argop4::OP_READLINK])?;
        match list.get(1) {
            Some(nfs_resop4::OP_READLINK(READLINK4res::NFS4_OK(ok))) => Ok(ok.link.0.clone()),
            _ => Err(eio()),
        }
    }

    fn read(&self, cred: &Cred, h: &RHandle, off: u64, len: u32) -> RResult<(Vec<u8>, bool)> {
        let count = len.min(self.rsize);
        let list = self.run(cred, &|c| {
            vec![
                putfh(h),
                nfs_argop4::OP_READ(READ4args {
                    stateid: c.state().io_stateid(h, OPEN4_SHARE_ACCESS_READ),
                    offset: off,
                    count,
                }),
            ]
        })?;
        match list.get(1) {
            Some(nfs_resop4::OP_READ(READ4res::NFS4_OK(ok))) if ok.data.len() <= len as usize => {
                Ok((ok.data.clone(), ok.eof))
            }
            _ => Err(eio()),
        }
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
            Stable::Unstable => stable_how4::UNSTABLE4,
            Stable::DataSync => stable_how4::DATA_SYNC4,
            Stable::FileSync => stable_how4::FILE_SYNC4,
        };
        let list = self.run(cred, &|c| {
            vec![
                putfh(h),
                nfs_argop4::OP_WRITE(WRITE4args {
                    stateid: c.state().io_stateid(h, OPEN4_SHARE_ACCESS_WRITE),
                    offset: off,
                    stable: how,
                    data: data.to_vec(),
                }),
            ]
        })?;
        match list.get(1) {
            Some(nfs_resop4::OP_WRITE(WRITE4res::NFS4_OK(ok)))
                if ok.count as usize <= data.len() =>
            {
                Ok(WriteReply {
                    count: ok.count,
                    committed: match ok.committed {
                        stable_how4::UNSTABLE4 => Stable::Unstable,
                        stable_how4::DATA_SYNC4 => Stable::DataSync,
                        stable_how4::FILE_SYNC4 => Stable::FileSync,
                    },
                    verifier: u64::from_be_bytes(ok.writeverf),
                })
            }
            _ => Err(eio()),
        }
    }

    fn commit(&self, cred: &Cred, h: &RHandle, off: u64, len: u32) -> RResult<u64> {
        let list = self.run(cred, &|_| {
            vec![
                putfh(h),
                nfs_argop4::OP_COMMIT(COMMIT4args {
                    offset: off,
                    count: len,
                }),
            ]
        })?;
        match list.get(1) {
            Some(nfs_resop4::OP_COMMIT(COMMIT4res::NFS4_OK(ok))) => {
                Ok(u64::from_be_bytes(ok.writeverf))
            }
            _ => Err(eio()),
        }
    }

    fn create(
        &self,
        cred: &Cred,
        dir: &RHandle,
        n: &[u8],
        how: CreateMode,
    ) -> RResult<(RHandle, RAttr)> {
        let c = self.cred(cred);
        let how = match how {
            CreateMode::Unchecked(s) => createhow4::UNCHECKED4(attr::settable(&s)),
            CreateMode::Guarded(s) => createhow4::GUARDED4(attr::settable(&s)),
            CreateMode::Exclusive(v) if self.core.minor == 1 => {
                createhow4::EXCLUSIVE4_1(creatverfattr {
                    cva_verf: v.to_be_bytes(),
                    cva_attrs: fattr4 {
                        attrmask: attr::bitmap(&[]),
                        attr_vals: crate::types::nfs4::attrlist4(Vec::new()),
                    },
                })
            }
            CreateMode::Exclusive(v) => createhow4::EXCLUSIVE4(v.to_be_bytes()),
        };
        // Files are made by opening them, then closed again.
        let op = state::open_wire(
            &self.core,
            &c,
            &Claim::Name(dir.clone(), n.to_vec()),
            0,
            OPEN4_SHARE_ACCESS_WRITE,
            0,
            Some(&how),
            true,
        )?;
        self.remember(dir, n, &op.fh);
        let o = Open {
            fh: op.fh.clone(),
            owner: 0,
            access: OPEN4_SHARE_ACCESS_WRITE,
            deny: 0,
            stateid: op.stateid,
        };
        self.close_wire(&c, &o)?;
        Ok((op.fh, rattr(&op.attrs)))
    }

    fn mkdir(
        &self,
        cred: &Cred,
        dir: &RHandle,
        n: &[u8],
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)> {
        self.make(cred, dir, n, &createtype4::NF4DIR, set)
    }

    fn symlink(
        &self,
        cred: &Cred,
        dir: &RHandle,
        n: &[u8],
        target: &[u8],
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)> {
        self.make(
            cred,
            dir,
            n,
            &createtype4::NF4LNK(utf8string(target.to_vec())),
            set,
        )
    }

    fn mknod(
        &self,
        cred: &Cred,
        dir: &RHandle,
        n: &[u8],
        kind: RKind,
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)> {
        if kind != RKind::Fifo {
            return Err(RemoteError::Errno(Errno::ENOTSUP));
        }
        self.make(cred, dir, n, &createtype4::NF4FIFO, set)
    }

    fn remove(&self, cred: &Cred, dir: &RHandle, n: &[u8]) -> RResult<()> {
        if self.kind_of(cred, dir, n)? == RKind::Dir {
            return Err(RemoteError::Errno(Errno::EISDIR));
        }
        self.run(cred, &|_| {
            vec![
                putfh(dir),
                nfs_argop4::OP_REMOVE(REMOVE4args { target: name(n) }),
            ]
        })
        .map(|_| ())
    }

    fn rmdir(&self, cred: &Cred, dir: &RHandle, n: &[u8]) -> RResult<()> {
        if self.kind_of(cred, dir, n)? != RKind::Dir {
            return Err(RemoteError::Errno(Errno::ENOTDIR));
        }
        self.run(cred, &|_| {
            vec![
                putfh(dir),
                nfs_argop4::OP_REMOVE(REMOVE4args { target: name(n) }),
            ]
        })
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
        self.run(cred, &|_| {
            vec![
                putfh(from_dir),
                nfs_argop4::OP_SAVEFH,
                putfh(to_dir),
                nfs_argop4::OP_RENAME(RENAME4args {
                    oldname: name(from),
                    newname: name(to),
                }),
            ]
        })?;
        if self.core.minor == 0 {
            let mut st = self.core.state();
            for v in st.names.values_mut() {
                if &v.0 == from_dir && v.1 == from {
                    *v = (to_dir.clone(), to.to_vec());
                }
            }
        }
        Ok(())
    }

    fn link(&self, cred: &Cred, h: &RHandle, dir: &RHandle, n: &[u8]) -> RResult<()> {
        self.run(cred, &|_| {
            vec![
                putfh(h),
                nfs_argop4::OP_SAVEFH,
                putfh(dir),
                nfs_argop4::OP_LINK(LINK4args { newname: name(n) }),
            ]
        })
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
        self.page(cred, dir, cookie, verifier, max)
    }

    fn statfs(&self, cred: &Cred, h: &RHandle) -> RResult<FsStat> {
        let list = self.run(cred, &|_| vec![putfh(h), getattr(attr::CAPACITY)])?;
        let a = attrs_at(&list, 1)?;
        Ok(FsStat {
            total_bytes: a.space[2].unwrap_or(0),
            free_bytes: a.space[1].unwrap_or(0),
            avail_bytes: a.space[0].unwrap_or(0),
            total_files: a.files[2].unwrap_or(0),
            free_files: a.files[1].unwrap_or(0),
            avail_files: a.files[0].unwrap_or(0),
        })
    }

    fn pathconf(&self, cred: &Cred, h: &RHandle) -> RResult<PathConf> {
        let list = self.run(cred, &|_| vec![putfh(h), getattr(attr::LIMITS)])?;
        let a = attrs_at(&list, 1)?;
        let d = PathConf::default();
        Ok(PathConf {
            link_max: a.maxlink.unwrap_or(d.link_max),
            name_max: a.maxname.unwrap_or(d.name_max),
            no_trunc: a.no_trunc.unwrap_or(d.no_trunc),
            chown_restricted: a.chown_restricted.unwrap_or(d.chown_restricted),
            case_insensitive: a.case_insensitive.unwrap_or(false),
            case_preserving: a.case_preserving.unwrap_or(true),
        })
    }

    fn open(&self, cred: &Cred, h: &RHandle, share: Share, owner: LockOwner) -> RResult<OpenToken> {
        let mut access = 0;
        if share.read {
            access |= OPEN4_SHARE_ACCESS_READ;
        }
        if share.write {
            access |= OPEN4_SHARE_ACCESS_WRITE;
        }
        if access == 0 {
            access = OPEN4_SHARE_ACCESS_READ;
        }
        let deny = u32::from(share.deny_read) | (u32::from(share.deny_write) << 1);
        let claim = if self.core.minor == 1 {
            Claim::Fh(h.clone())
        } else {
            // 4.0 opens by name: the one the handle was found by.
            let (d, n) = self
                .core
                .state()
                .names
                .get(h)
                .cloned()
                .ok_or(RemoteError::Stale)?;
            Claim::Name(d, n)
        };
        let op = state::open_wire(
            &self.core,
            &self.cred(cred),
            &claim,
            owner.0,
            access,
            deny,
            None,
            true,
        )?;
        let mut st = self.core.state();
        st.next += 1;
        let id = st.next;
        st.opens.insert(
            id,
            Open {
                fh: h.clone(),
                owner: owner.0,
                access,
                deny,
                stateid: op.stateid,
            },
        );
        Ok(OpenToken(id.to_be_bytes().to_vec()))
    }

    fn close(&self, cred: &Cred, _h: &RHandle, token: &OpenToken) -> RResult<()> {
        let id = <[u8; 8]>::try_from(token.0.as_slice())
            .map(u64::from_be_bytes)
            .map_err(|_| RemoteError::Errno(Errno::EBADF))?;
        let Some(o) = self.core.state().opens.remove(&id) else {
            return Ok(());
        };
        let left = self
            .core
            .state()
            .opens
            .values()
            .any(|x| x.fh == o.fh && x.owner == o.owner);
        if !left {
            self.core.state().locks.remove(&(o.fh.clone(), o.owner));
        }
        self.close_wire(&self.cred(cred), &o)
    }

    fn lock(
        &self,
        cred: &Cred,
        h: &RHandle,
        owner: LockOwner,
        kind: LockKind,
        off: u64,
        len: u64,
        token: &OpenToken,
    ) -> RResult<()> {
        let open = {
            let st = self.core.state();
            let by_token = <[u8; 8]>::try_from(token.0.as_slice())
                .ok()
                .map(u64::from_be_bytes)
                .and_then(|id| st.opens.get(&id).cloned());
            by_token.or_else(|| {
                st.opens
                    .values()
                    .find(|o| &o.fh == h && o.owner == owner.0)
                    .cloned()
            })
        };
        let r = Range {
            exclusive: kind == LockKind::Write,
            off,
            len,
        };
        match state::lock_wire(
            &self.core,
            &self.cred(cred),
            h,
            owner.0,
            r,
            open.as_ref(),
            false,
        )? {
            Ok(()) => Ok(()),
            Err(s) => Err(crate::client::status::nfs4(s)),
        }
    }

    fn unlock(
        &self,
        cred: &Cred,
        h: &RHandle,
        owner: LockOwner,
        off: u64,
        len: u64,
    ) -> RResult<()> {
        let Some(l) = self.core.state().locks.get(&(h.clone(), owner.0)).cloned() else {
            return Ok(());
        };
        let (st, _) = self.core.send(
            &self.cred(cred),
            &|_| {
                vec![
                    putfh(h),
                    nfs_argop4::OP_LOCKU(LOCKU4args {
                        locktype: nfs_lock_type4::WRITE_LT,
                        seqid: l.seqid,
                        lock_stateid: l.stateid.clone(),
                        offset: off,
                        length: if len == 0 { u64::MAX } else { len },
                    }),
                ]
            },
            true,
        )?;
        let mut s = self.core.state();
        if let Some(x) = s.locks.get_mut(&(h.clone(), owner.0)) {
            if state::moves_seqid(st) {
                x.seqid = x.seqid.wrapping_add(1);
            }
            if st == nfsstat4::NFS4_OK {
                crate::client::ranges::cut(&mut x.held, off, len);
            }
        }
        drop(s);
        match st {
            nfsstat4::NFS4_OK => Ok(()),
            s => Err(crate::client::status::nfs4(s)),
        }
    }

    fn test(
        &self,
        cred: &Cred,
        h: &RHandle,
        owner: LockOwner,
        kind: LockKind,
        off: u64,
        len: u64,
    ) -> RResult<Option<LockConflict>> {
        let (st, list) = self.core.send(
            &self.cred(cred),
            &|c| {
                vec![
                    putfh(h),
                    nfs_argop4::OP_LOCKT(LOCKT4args {
                        locktype: if kind == LockKind::Write {
                            nfs_lock_type4::WRITE_LT
                        } else {
                            nfs_lock_type4::READ_LT
                        },
                        offset: off,
                        length: if len == 0 { u64::MAX } else { len },
                        owner: state::lock_owner(c, owner.0),
                    }),
                ]
            },
            true,
        )?;
        match (st, list.get(1)) {
            (nfsstat4::NFS4_OK, _) => Ok(None),
            (_, Some(nfs_resop4::OP_LOCKT(LOCKT4res::NFS4ERR_DENIED(d)))) => Ok(Some(conflict(d))),
            (s, _) => Err(crate::client::status::nfs4(s)),
        }
    }

    fn unmount(&self) -> RResult<()> {
        let opens: Vec<Open> = self.core.state().opens.drain().map(|(_, o)| o).collect();
        let owner = self.core.owner.clone();
        for o in &opens {
            let _ = self.close_wire(&owner, o);
        }
        self.core.destroy();
        self.core
            .stopped
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.status.stop();
        if let Some(t) = self
            .renewer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            let _ = t.join();
        }
        Ok(())
    }
}
