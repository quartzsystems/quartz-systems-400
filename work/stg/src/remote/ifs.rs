// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS from the job: every `IfsClient` method is one `ifs`
//! request. Descriptors and directory handles are the service's; they
//! end with the connection. Data moves in chunks of at most
//! `ifs::client::CHUNK` bytes, so a `write` may take less than it was
//! given, as `write()` may.

use ifs::Result;
use ifs::backend::Nanos;
use ifs::client::{CHUNK, IfsClient, WalkEntry};
use ifs::lock::range::{RangeKind, RangeLock};
use ifs::lock::{LockInfo, ObjKey};
use ifs::meta::reclaim::Report;
use ifs::meta::{CrtObjAud, Journal};
use ifs::ops::audit::FileId;
use ifs::ops::{
    Attr, AttrId, AuthList, ChgAut, CopyOpts, DirEnt, OpenArgs, RenameMode, Stat, StatVfs,
    UdfsInfo, WalkOpts,
};

use crate::Shared;
use crate::ifs::{IfsOp, IfsReply, reply_as};
use crate::proto::Client;

/// The IFS over a link.
pub struct RemoteIfs {
    link: Shared,
}

impl RemoteIfs {
    /// Over a link.
    #[must_use]
    pub fn new(link: Shared) -> Self {
        Self { link }
    }

    fn call(&self, op: IfsOp) -> IfsReply {
        self.link.borrow_mut().with(|l| l.ifs(op))
    }
}

fn chunk(n: usize) -> u32 {
    u32::try_from(n.min(CHUNK)).unwrap_or(u32::MAX)
}

impl IfsClient for RemoteIfs {
    fn open(&self, path: &str, args: OpenArgs) -> Result<i32> {
        reply_as!(
            self.call(IfsOp::Open {
                path: path.into(),
                args
            }),
            Fd
        )
    }
    fn openat(&self, dirfd: Option<i32>, path: &str, args: OpenArgs) -> Result<i32> {
        reply_as!(
            self.call(IfsOp::OpenAt {
                dirfd,
                path: path.into(),
                args
            }),
            Fd
        )
    }
    fn close(&self, fd: i32) -> Result<()> {
        reply_as!(self.call(IfsOp::Close { fd }), Unit)
    }
    fn read(&self, fd: i32, max: usize) -> Result<Vec<u8>> {
        reply_as!(
            self.call(IfsOp::Read {
                fd,
                max: chunk(max)
            }),
            Bytes
        )
    }
    fn pread(&self, fd: i32, max: usize, off: u64) -> Result<Vec<u8>> {
        reply_as!(
            self.call(IfsOp::PRead {
                fd,
                max: chunk(max),
                off
            }),
            Bytes
        )
    }
    fn write(&self, fd: i32, data: &[u8]) -> Result<usize> {
        let data = data[..data.len().min(CHUNK)].to_vec();
        reply_as!(self.call(IfsOp::Write { fd, data }), Size)
            .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
    }
    fn pwrite(&self, fd: i32, data: &[u8], off: u64) -> Result<usize> {
        let data = data[..data.len().min(CHUNK)].to_vec();
        reply_as!(self.call(IfsOp::PWrite { fd, data, off }), Size)
            .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
    }
    fn lseek(&self, fd: i32, off: i64, whence: i32) -> Result<u64> {
        reply_as!(self.call(IfsOp::Seek { fd, off, whence }), Size)
    }
    fn fsync(&self, fd: i32, data_only: bool) -> Result<()> {
        reply_as!(self.call(IfsOp::Fsync { fd, data_only }), Unit)
    }
    fn ftruncate(&self, fd: i32, len: u64) -> Result<()> {
        reply_as!(self.call(IfsOp::Ftruncate { fd, len }), Unit)
    }
    fn fstat(&self, fd: i32) -> Result<Stat> {
        reply_as!(self.call(IfsOp::Fstat { fd }), Stat)
    }
    fn stat(&self, path: &str, follow: bool) -> Result<Stat> {
        reply_as!(
            self.call(IfsOp::Stat {
                path: path.into(),
                follow
            }),
            Stat
        )
    }
    fn access(&self, path: &str, amode: i32) -> Result<()> {
        reply_as!(
            self.call(IfsOp::Access {
                path: path.into(),
                amode
            }),
            Unit
        )
    }
    fn chmod(&self, path: &str, mode: u32) -> Result<()> {
        reply_as!(
            self.call(IfsOp::Chmod {
                path: path.into(),
                mode
            }),
            Unit
        )
    }
    fn chown(&self, path: &str, uid: u32, gid: u32) -> Result<()> {
        reply_as!(
            self.call(IfsOp::Chown {
                path: path.into(),
                uid,
                gid
            }),
            Unit
        )
    }
    fn link(&self, existing: &str, new: &str) -> Result<()> {
        reply_as!(
            self.call(IfsOp::Link {
                existing: existing.into(),
                new: new.into()
            }),
            Unit
        )
    }
    fn unlink(&self, path: &str) -> Result<()> {
        reply_as!(self.call(IfsOp::Unlink { path: path.into() }), Unit)
    }
    fn rename(&self, old: &str, new: &str, mode: RenameMode) -> Result<()> {
        reply_as!(
            self.call(IfsOp::Rename {
                old: old.into(),
                new: new.into(),
                mode
            }),
            Unit
        )
    }
    fn symlink(&self, target: &str, path: &str) -> Result<()> {
        reply_as!(
            self.call(IfsOp::Symlink {
                target: target.into(),
                path: path.into()
            }),
            Unit
        )
    }
    fn readlink(&self, path: &str) -> Result<String> {
        reply_as!(self.call(IfsOp::Readlink { path: path.into() }), Text)
    }
    fn mkdir(&self, path: &str, mode: u32) -> Result<()> {
        reply_as!(
            self.call(IfsOp::Mkdir {
                path: path.into(),
                mode
            }),
            Unit
        )
    }
    fn mkfifo(&self, path: &str, mode: u32) -> Result<()> {
        reply_as!(
            self.call(IfsOp::Mkfifo {
                path: path.into(),
                mode
            }),
            Unit
        )
    }
    fn rmdir(&self, path: &str) -> Result<()> {
        reply_as!(self.call(IfsOp::Rmdir { path: path.into() }), Unit)
    }
    fn opendir(&self, path: &str) -> Result<u64> {
        reply_as!(self.call(IfsOp::Opendir { path: path.into() }), Handle)
    }
    fn readdir(&self, handle: u64, max: usize) -> Result<Vec<DirEnt>> {
        reply_as!(
            self.call(IfsOp::Readdir {
                handle,
                max: u32::try_from(max).unwrap_or(u32::MAX)
            }),
            Entries
        )
    }
    fn rewinddir(&self, handle: u64) -> Result<()> {
        reply_as!(self.call(IfsOp::Rewinddir { handle }), Unit)
    }
    fn closedir(&self, handle: u64) -> Result<()> {
        reply_as!(self.call(IfsOp::Closedir { handle }), Unit)
    }
    fn read_dir(&self, path: &str) -> Result<Vec<DirEnt>> {
        reply_as!(self.call(IfsOp::ReadDir { path: path.into() }), Entries)
    }
    fn chdir(&self, path: &str) -> Result<()> {
        reply_as!(self.call(IfsOp::Chdir { path: path.into() }), Unit)
    }
    fn fchdir(&self, fd: i32) -> Result<()> {
        reply_as!(self.call(IfsOp::Fchdir { fd }), Unit)
    }
    fn getcwd(&self) -> Result<String> {
        reply_as!(self.call(IfsOp::Getcwd), Text)
    }
    fn dup(&self, fd: i32) -> Result<i32> {
        reply_as!(self.call(IfsOp::Dup { fd }), Fd)
    }
    fn dup2(&self, fd: i32, new: i32) -> Result<i32> {
        reply_as!(self.call(IfsOp::Dup2 { fd, new }), Fd)
    }
    fn fcntl(&self, fd: i32, cmd: i32, arg: i32) -> Result<i32> {
        reply_as!(self.call(IfsOp::Fcntl { fd, cmd, arg }), Int)
    }
    fn lock_range(
        &self,
        fd: i32,
        cmd: i32,
        kind: RangeKind,
        whence: i32,
        start: i64,
        len: i64,
    ) -> Result<Option<RangeLock>> {
        reply_as!(
            self.call(IfsOp::LockRange {
                fd,
                cmd,
                kind,
                whence,
                start,
                len
            }),
            Lock
        )
    }
    fn fd_path(&self, fd: i32) -> Result<String> {
        reply_as!(self.call(IfsOp::FdPath { fd }), Text)
    }
    fn utime(&self, path: &str, times: Option<(Nanos, Nanos)>) -> Result<()> {
        reply_as!(
            self.call(IfsOp::Utime {
                path: path.into(),
                times
            }),
            Unit
        )
    }
    fn umask(&self, mask: u32) -> Result<u32> {
        reply_as!(self.call(IfsOp::Umask { mask }), Mask)
    }
    fn pathconf(&self, path: &str, name: i32) -> Result<Option<i64>> {
        reply_as!(
            self.call(IfsOp::Pathconf {
                path: path.into(),
                name
            }),
            Limit
        )
    }
    fn statvfs(&self, path: &str) -> Result<StatVfs> {
        reply_as!(self.call(IfsOp::Statvfs { path: path.into() }), StatVfs)
    }
    fn get_attrs(&self, path: &str, ids: &[AttrId], follow: bool) -> Result<Vec<Attr>> {
        reply_as!(
            self.call(IfsOp::GetAttrs {
                path: path.into(),
                ids: ids.to_vec(),
                follow
            }),
            Attrs
        )
    }
    fn set_attr(&self, path: &str, attr: &Attr, follow: bool) -> Result<()> {
        reply_as!(
            self.call(IfsOp::SetAttr {
                path: path.into(),
                attr: attr.clone(),
                follow
            }),
            Unit
        )
    }
    fn dspaut(&self, path: &str) -> Result<AuthList> {
        reply_as!(self.call(IfsOp::Dspaut { path: path.into() }), AuthList)
    }
    fn chgaut(&self, path: &str, c: &ChgAut) -> Result<()> {
        reply_as!(
            self.call(IfsOp::Chgaut {
                path: path.into(),
                c: c.clone()
            }),
            Unit
        )
    }
    fn rvkaut(&self, path: &str, user: &str) -> Result<()> {
        reply_as!(
            self.call(IfsOp::Rvkaut {
                path: path.into(),
                user: user.into()
            }),
            Unit
        )
    }
    fn chgown(&self, path: &str, new_owner: &str, revoke_old: bool) -> Result<()> {
        reply_as!(
            self.call(IfsOp::Chgown {
                path: path.into(),
                new_owner: new_owner.into(),
                revoke_old
            }),
            Unit
        )
    }
    fn chgpgp(&self, path: &str, new_group: Option<&str>, revoke_old: bool) -> Result<()> {
        reply_as!(
            self.call(IfsOp::Chgpgp {
                path: path.into(),
                new_group: new_group.map(str::to_string),
                revoke_old
            }),
            Unit
        )
    }
    fn check_out(&self, path: &str) -> Result<()> {
        reply_as!(self.call(IfsOp::CheckOut { path: path.into() }), Unit)
    }
    fn check_in(&self, path: &str) -> Result<()> {
        reply_as!(self.call(IfsOp::CheckIn { path: path.into() }), Unit)
    }
    fn start_journal(&self, path: &str, j: Journal) -> Result<()> {
        reply_as!(
            self.call(IfsOp::StartJournal {
                path: path.into(),
                j
            }),
            Unit
        )
    }
    fn end_journal(&self, path: &str) -> Result<()> {
        reply_as!(self.call(IfsOp::EndJournal { path: path.into() }), Unit)
    }
    fn copy(&self, from: &str, to: &str, opts: CopyOpts) -> Result<()> {
        reply_as!(
            self.call(IfsOp::Copy {
                from: from.into(),
                to: to.into(),
                opts
            }),
            Unit
        )
    }
    fn mov(&self, from: &str, to: &str, replace: bool) -> Result<()> {
        reply_as!(
            self.call(IfsOp::Mov {
                from: from.into(),
                to: to.into(),
                replace
            }),
            Unit
        )
    }
    fn remove_tree(&self, path: &str) -> Result<()> {
        reply_as!(self.call(IfsOp::RemoveTree { path: path.into() }), Unit)
    }
    fn walk(&self, path: &str, opts: WalkOpts, max: usize) -> Result<Vec<WalkEntry>> {
        reply_as!(
            self.call(IfsOp::Walk {
                path: path.into(),
                opts,
                max: u32::try_from(max).unwrap_or(u32::MAX)
            }),
            Walk
        )
    }
    fn path_from_file_id(&self, id: FileId) -> Result<String> {
        reply_as!(self.call(IfsOp::PathFromFileId { id }), Text)
    }
    fn crtudfs(
        &self,
        udfs: &str,
        case_sensitive: bool,
        crtobjaud: CrtObjAud,
        text: &str,
    ) -> Result<()> {
        reply_as!(
            self.call(IfsOp::Crtudfs {
                udfs: udfs.into(),
                case_sensitive,
                crtobjaud,
                text: text.into()
            }),
            Unit
        )
    }
    fn dltudfs(&self, udfs: &str) -> Result<()> {
        reply_as!(self.call(IfsOp::Dltudfs { udfs: udfs.into() }), Unit)
    }
    fn mount(&self, udfs: &str, over: &str, read_only: bool) -> Result<u32> {
        reply_as!(
            self.call(IfsOp::Mount {
                udfs: udfs.into(),
                over: over.into(),
                read_only
            }),
            Mask
        )
    }
    fn unmount(&self, path: &str) -> Result<()> {
        reply_as!(self.call(IfsOp::Unmount { path: path.into() }), Unit)
    }
    fn dspudfs(&self, udfs: &str) -> Result<UdfsInfo> {
        reply_as!(self.call(IfsOp::Dspudfs { udfs: udfs.into() }), Udfs)
    }
    fn rcllnk(&self, check_only: bool) -> Result<Report> {
        reply_as!(self.call(IfsOp::Rcllnk { check_only }), Report)
    }
    fn object_locks(&self, path: &str) -> Result<Vec<LockInfo>> {
        reply_as!(self.call(IfsOp::ObjectLocks { path: path.into() }), Locks)
    }
    fn owner_locks(&self) -> Result<Vec<(ObjKey, LockInfo)>> {
        reply_as!(self.call(IfsOp::OwnerLocks), OwnerLocks)
    }
}
