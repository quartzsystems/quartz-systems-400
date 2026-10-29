// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS over the link: one request (`ifs`, tag 440) carries an
//! [`IfsOp`], one variant per `IfsClient` method, and comes back as an
//! [`IfsReply`]. [`serve`] answers an operation against any
//! `IfsClient` (the service's `Direct` over its `Ifs`, or a test's),
//! so the mapping between the trait and the wire exists once. Data
//! moves in chunks of at most `ifs::client::CHUNK` bytes: the service
//! clamps what it reads and takes.

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
use ifs::{Errno, Result};
use ipc::wire_enum;

/// One IFS operation, as the job sends it.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub enum IfsOp {
    Open { path: String, args: OpenArgs },
    OpenAt { dirfd: Option<i32>, path: String, args: OpenArgs },
    Close { fd: i32 },
    Read { fd: i32, max: u32 },
    PRead { fd: i32, max: u32, off: u64 },
    Write { fd: i32, data: Vec<u8> },
    PWrite { fd: i32, data: Vec<u8>, off: u64 },
    Seek { fd: i32, off: i64, whence: i32 },
    Fsync { fd: i32, data_only: bool },
    Ftruncate { fd: i32, len: u64 },
    Fstat { fd: i32 },
    Stat { path: String, follow: bool },
    Access { path: String, amode: i32 },
    Chmod { path: String, mode: u32 },
    Chown { path: String, uid: u32, gid: u32 },
    Link { existing: String, new: String },
    Unlink { path: String },
    Rename { old: String, new: String, mode: RenameMode },
    Symlink { target: String, path: String },
    Readlink { path: String },
    Mkdir { path: String, mode: u32 },
    Mkfifo { path: String, mode: u32 },
    Rmdir { path: String },
    Opendir { path: String },
    Readdir { handle: u64, max: u32 },
    Rewinddir { handle: u64 },
    Closedir { handle: u64 },
    ReadDir { path: String },
    Chdir { path: String },
    Fchdir { fd: i32 },
    Getcwd,
    Dup { fd: i32 },
    Dup2 { fd: i32, new: i32 },
    Fcntl { fd: i32, cmd: i32, arg: i32 },
    LockRange { fd: i32, cmd: i32, kind: RangeKind, whence: i32, start: i64, len: i64 },
    FdPath { fd: i32 },
    Utime { path: String, times: Option<(Nanos, Nanos)> },
    Umask { mask: u32 },
    Pathconf { path: String, name: i32 },
    Statvfs { path: String },
    GetAttrs { path: String, ids: Vec<AttrId>, follow: bool },
    SetAttr { path: String, attr: Attr, follow: bool },
    Dspaut { path: String },
    Chgaut { path: String, c: ChgAut },
    Rvkaut { path: String, user: String },
    Chgown { path: String, new_owner: String, revoke_old: bool },
    Chgpgp { path: String, new_group: Option<String>, revoke_old: bool },
    CheckOut { path: String },
    CheckIn { path: String },
    StartJournal { path: String, j: Journal },
    EndJournal { path: String },
    Copy { from: String, to: String, opts: CopyOpts },
    Mov { from: String, to: String, replace: bool },
    RemoveTree { path: String },
    Walk { path: String, opts: WalkOpts, max: u32 },
    PathFromFileId { id: FileId },
    Crtudfs { udfs: String, case_sensitive: bool, crtobjaud: CrtObjAud, text: String },
    Dltudfs { udfs: String },
    Mount { udfs: String, over: String, read_only: bool },
    Unmount { path: String },
    Dspudfs { udfs: String },
    Rcllnk { check_only: bool },
    ObjectLocks { path: String },
    OwnerLocks,
}

wire_enum!(IfsOp {
    0 => Open { path, args },
    1 => OpenAt { dirfd, path, args },
    2 => Close { fd },
    3 => Read { fd, max },
    4 => PRead { fd, max, off },
    5 => Write { fd, data },
    6 => PWrite { fd, data, off },
    7 => Seek { fd, off, whence },
    8 => Fsync { fd, data_only },
    9 => Ftruncate { fd, len },
    10 => Fstat { fd },
    11 => Stat { path, follow },
    12 => Access { path, amode },
    13 => Chmod { path, mode },
    14 => Chown { path, uid, gid },
    15 => Link { existing, new },
    16 => Unlink { path },
    17 => Rename { old, new, mode },
    18 => Symlink { target, path },
    19 => Readlink { path },
    20 => Mkdir { path, mode },
    21 => Mkfifo { path, mode },
    22 => Rmdir { path },
    23 => Opendir { path },
    24 => Readdir { handle, max },
    25 => Rewinddir { handle },
    26 => Closedir { handle },
    27 => ReadDir { path },
    28 => Chdir { path },
    29 => Fchdir { fd },
    30 => Getcwd,
    31 => Dup { fd },
    32 => Dup2 { fd, new },
    33 => Fcntl { fd, cmd, arg },
    34 => LockRange { fd, cmd, kind, whence, start, len },
    35 => FdPath { fd },
    36 => Utime { path, times },
    37 => Umask { mask },
    38 => Pathconf { path, name },
    39 => Statvfs { path },
    40 => GetAttrs { path, ids, follow },
    41 => SetAttr { path, attr, follow },
    42 => Dspaut { path },
    43 => Chgaut { path, c },
    44 => Rvkaut { path, user },
    45 => Chgown { path, new_owner, revoke_old },
    46 => Chgpgp { path, new_group, revoke_old },
    47 => CheckOut { path },
    48 => CheckIn { path },
    49 => StartJournal { path, j },
    50 => EndJournal { path },
    51 => Copy { from, to, opts },
    52 => Mov { from, to, replace },
    53 => RemoveTree { path },
    54 => Walk { path, opts, max },
    55 => PathFromFileId { id },
    56 => Crtudfs { udfs, case_sensitive, crtobjaud, text },
    57 => Dltudfs { udfs },
    58 => Mount { udfs, over, read_only },
    59 => Unmount { path },
    60 => Dspudfs { udfs },
    61 => Rcllnk { check_only },
    62 => ObjectLocks { path },
    63 => OwnerLocks,
});

/// What an operation answers: the value its method returns, or the
/// IBM `errno`. The variant says which; a job that gets the wrong
/// one treats it as `EUNKNOWN`.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub enum IfsReply {
    Unit { r: Result<()> },
    Fd { r: Result<i32> },
    Int { r: Result<i32> },
    Bytes { r: Result<Vec<u8>> },
    Size { r: Result<u64> },
    Stat { r: Result<Stat> },
    Text { r: Result<String> },
    Handle { r: Result<u64> },
    Entries { r: Result<Vec<DirEnt>> },
    Lock { r: Result<Option<RangeLock>> },
    Mask { r: Result<u32> },
    Limit { r: Result<Option<i64>> },
    StatVfs { r: Result<StatVfs> },
    Attrs { r: Result<Vec<Attr>> },
    AuthList { r: Result<AuthList> },
    Walk { r: Result<Vec<WalkEntry>> },
    Udfs { r: Result<UdfsInfo> },
    Report { r: Result<Report> },
    Locks { r: Result<Vec<LockInfo>> },
    OwnerLocks { r: Result<Vec<(ObjKey, LockInfo)>> },
}

wire_enum!(IfsReply {
    0 => Unit { r },
    1 => Fd { r },
    2 => Int { r },
    3 => Bytes { r },
    4 => Size { r },
    5 => Stat { r },
    6 => Text { r },
    7 => Handle { r },
    8 => Entries { r },
    9 => Lock { r },
    10 => Mask { r },
    11 => Limit { r },
    12 => StatVfs { r },
    13 => Attrs { r },
    14 => AuthList { r },
    15 => Walk { r },
    16 => Udfs { r },
    17 => Report { r },
    18 => Locks { r },
    19 => OwnerLocks { r },
});

/// The answer `Err(EUNKNOWN)` for a reply of the wrong shape.
pub fn unknown<T>() -> Result<T> {
    Err(Errno::EUNKNOWN)
}

/// Answer `op` with `c`.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn serve(c: &dyn IfsClient, op: IfsOp) -> IfsReply {
    use IfsReply as R;
    let n = |max: u32| (max as usize).min(CHUNK);
    match op {
        IfsOp::Open { path, args } => R::Fd {
            r: c.open(&path, args),
        },
        IfsOp::OpenAt { dirfd, path, args } => R::Fd {
            r: c.openat(dirfd, &path, args),
        },
        IfsOp::Close { fd } => R::Unit { r: c.close(fd) },
        IfsOp::Read { fd, max } => R::Bytes {
            r: c.read(fd, n(max)),
        },
        IfsOp::PRead { fd, max, off } => R::Bytes {
            r: c.pread(fd, n(max), off),
        },
        IfsOp::Write { fd, data } => R::Size {
            r: c.write(fd, &data[..data.len().min(CHUNK)]).map(|n| n as u64),
        },
        IfsOp::PWrite { fd, data, off } => R::Size {
            r: c.pwrite(fd, &data[..data.len().min(CHUNK)], off)
                .map(|n| n as u64),
        },
        IfsOp::Seek { fd, off, whence } => R::Size {
            r: c.lseek(fd, off, whence),
        },
        IfsOp::Fsync { fd, data_only } => R::Unit {
            r: c.fsync(fd, data_only),
        },
        IfsOp::Ftruncate { fd, len } => R::Unit {
            r: c.ftruncate(fd, len),
        },
        IfsOp::Fstat { fd } => R::Stat { r: c.fstat(fd) },
        IfsOp::Stat { path, follow } => R::Stat {
            r: c.stat(&path, follow),
        },
        IfsOp::Access { path, amode } => R::Unit {
            r: c.access(&path, amode),
        },
        IfsOp::Chmod { path, mode } => R::Unit {
            r: c.chmod(&path, mode),
        },
        IfsOp::Chown { path, uid, gid } => R::Unit {
            r: c.chown(&path, uid, gid),
        },
        IfsOp::Link { existing, new } => R::Unit {
            r: c.link(&existing, &new),
        },
        IfsOp::Unlink { path } => R::Unit { r: c.unlink(&path) },
        IfsOp::Rename { old, new, mode } => R::Unit {
            r: c.rename(&old, &new, mode),
        },
        IfsOp::Symlink { target, path } => R::Unit {
            r: c.symlink(&target, &path),
        },
        IfsOp::Readlink { path } => R::Text {
            r: c.readlink(&path),
        },
        IfsOp::Mkdir { path, mode } => R::Unit {
            r: c.mkdir(&path, mode),
        },
        IfsOp::Mkfifo { path, mode } => R::Unit {
            r: c.mkfifo(&path, mode),
        },
        IfsOp::Rmdir { path } => R::Unit { r: c.rmdir(&path) },
        IfsOp::Opendir { path } => R::Handle {
            r: c.opendir(&path),
        },
        IfsOp::Readdir { handle, max } => R::Entries {
            r: c.readdir(handle, (max as usize).clamp(1, 4096)),
        },
        IfsOp::Rewinddir { handle } => R::Unit {
            r: c.rewinddir(handle),
        },
        IfsOp::Closedir { handle } => R::Unit {
            r: c.closedir(handle),
        },
        IfsOp::ReadDir { path } => R::Entries {
            r: c.read_dir(&path),
        },
        IfsOp::Chdir { path } => R::Unit { r: c.chdir(&path) },
        IfsOp::Fchdir { fd } => R::Unit { r: c.fchdir(fd) },
        IfsOp::Getcwd => R::Text { r: c.getcwd() },
        IfsOp::Dup { fd } => R::Fd { r: c.dup(fd) },
        IfsOp::Dup2 { fd, new } => R::Fd { r: c.dup2(fd, new) },
        IfsOp::Fcntl { fd, cmd, arg } => R::Int {
            r: c.fcntl(fd, cmd, arg),
        },
        IfsOp::LockRange {
            fd,
            cmd,
            kind,
            whence,
            start,
            len,
        } => R::Lock {
            r: c.lock_range(fd, cmd, kind, whence, start, len),
        },
        IfsOp::FdPath { fd } => R::Text { r: c.fd_path(fd) },
        IfsOp::Utime { path, times } => R::Unit {
            r: c.utime(&path, times),
        },
        IfsOp::Umask { mask } => R::Mask { r: c.umask(mask) },
        IfsOp::Pathconf { path, name } => R::Limit {
            r: c.pathconf(&path, name),
        },
        IfsOp::Statvfs { path } => R::StatVfs {
            r: c.statvfs(&path),
        },
        IfsOp::GetAttrs { path, ids, follow } => R::Attrs {
            r: c.get_attrs(&path, &ids, follow),
        },
        IfsOp::SetAttr { path, attr, follow } => R::Unit {
            r: c.set_attr(&path, &attr, follow),
        },
        IfsOp::Dspaut { path } => R::AuthList { r: c.dspaut(&path) },
        IfsOp::Chgaut { path, c: chg } => R::Unit {
            r: c.chgaut(&path, &chg),
        },
        IfsOp::Rvkaut { path, user } => R::Unit {
            r: c.rvkaut(&path, &user),
        },
        IfsOp::Chgown {
            path,
            new_owner,
            revoke_old,
        } => R::Unit {
            r: c.chgown(&path, &new_owner, revoke_old),
        },
        IfsOp::Chgpgp {
            path,
            new_group,
            revoke_old,
        } => R::Unit {
            r: c.chgpgp(&path, new_group.as_deref(), revoke_old),
        },
        IfsOp::CheckOut { path } => R::Unit {
            r: c.check_out(&path),
        },
        IfsOp::CheckIn { path } => R::Unit {
            r: c.check_in(&path),
        },
        IfsOp::StartJournal { path, j } => R::Unit {
            r: c.start_journal(&path, j),
        },
        IfsOp::EndJournal { path } => R::Unit {
            r: c.end_journal(&path),
        },
        IfsOp::Copy { from, to, opts } => R::Unit {
            r: c.copy(&from, &to, opts),
        },
        IfsOp::Mov { from, to, replace } => R::Unit {
            r: c.mov(&from, &to, replace),
        },
        IfsOp::RemoveTree { path } => R::Unit {
            r: c.remove_tree(&path),
        },
        IfsOp::Walk { path, opts, max } => R::Walk {
            r: c.walk(&path, opts, (max as usize).clamp(1, 100_000)),
        },
        IfsOp::PathFromFileId { id } => R::Text {
            r: c.path_from_file_id(id),
        },
        IfsOp::Crtudfs {
            udfs,
            case_sensitive,
            crtobjaud,
            text,
        } => R::Unit {
            r: c.crtudfs(&udfs, case_sensitive, crtobjaud, &text),
        },
        IfsOp::Dltudfs { udfs } => R::Unit { r: c.dltudfs(&udfs) },
        IfsOp::Mount {
            udfs,
            over,
            read_only,
        } => R::Mask {
            r: c.mount(&udfs, &over, read_only),
        },
        IfsOp::Unmount { path } => R::Unit { r: c.unmount(&path) },
        IfsOp::Dspudfs { udfs } => R::Udfs { r: c.dspudfs(&udfs) },
        IfsOp::Rcllnk { check_only } => R::Report {
            r: c.rcllnk(check_only),
        },
        IfsOp::ObjectLocks { path } => R::Locks {
            r: c.object_locks(&path),
        },
        IfsOp::OwnerLocks => R::OwnerLocks { r: c.owner_locks() },
    }
}

macro_rules! reply_as {
    ($reply:expr, $variant:ident) => {
        match $reply {
            IfsReply::$variant { r } => r,
            _ => $crate::ifs::unknown(),
        }
    };
}

pub(crate) use reply_as;
