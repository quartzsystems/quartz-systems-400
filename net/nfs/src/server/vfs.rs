// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`Vfs`]: what the protocols ask of a file system. Every call names
//! the request ([`Req`]: the caller already mapped by [`super::cred`],
//! the export already checked by [`super::gate`], the host's options)
//! and the objects by [`FileHandle`]. Names cross it as the client's
//! bytes, in the host's path name CCSID; data as the client's bytes, in
//! its data CCSID.

use std::net::IpAddr;
use std::sync::Arc;

use ifs::client::Caller;

use super::error::VResult;
use super::export::{ExportView, HostOpts};
use super::handle::FileHandle;

/// One request: who, through which export, from where.
#[derive(Debug, Clone)]
pub struct Req {
    /// Whom the call is made for.
    pub caller: Caller,
    /// The export the handles are in.
    pub export: Arc<dyn ExportView>,
    /// The client's address.
    pub peer: IpAddr,
    /// The host's options on the export.
    pub opts: HostOpts,
}

/// An object's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileType {
    /// A regular file (also a member, a save file, a user space and any
    /// other `/QSYS.LIB` object that is not a directory).
    Reg,
    /// A directory (also a library and a database file).
    Dir,
    /// A block special file.
    Blk,
    /// A character special file.
    Chr,
    /// A symbolic link.
    Lnk,
    /// A socket.
    Sock,
    /// A FIFO.
    Fifo,
}

/// A time: seconds and nanoseconds since 1970.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Time {
    /// Seconds.
    pub secs: i64,
    /// Nanoseconds.
    pub nsecs: u32,
}

impl Time {
    /// From nanoseconds.
    #[must_use]
    pub fn from_nanos(n: i64) -> Self {
        Self {
            secs: n.div_euclid(1_000_000_000),
            nsecs: u32::try_from(n.rem_euclid(1_000_000_000)).unwrap_or(0),
        }
    }

    /// As nanoseconds.
    #[must_use]
    pub fn nanos(self) -> i64 {
        self.secs
            .saturating_mul(1_000_000_000)
            .saturating_add(i64::from(self.nsecs))
    }
}

/// An object's attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    /// The type.
    pub ftype: FileType,
    /// The permission bits (`0o7777`).
    pub mode: u32,
    /// Links.
    pub nlink: u32,
    /// The owner's UID.
    pub uid: u32,
    /// The primary group's GID.
    pub gid: u32,
    /// The size as the client sees it (converted).
    pub size: u64,
    /// Bytes allocated.
    pub used: u64,
    /// A device's major and minor numbers.
    pub rdev: (u32, u32),
    /// The file system as the client sees it.
    pub fsid: u64,
    /// The object's number.
    pub fileid: u64,
    /// Last access.
    pub atime: Time,
    /// Last change of data.
    pub mtime: Time,
    /// Last change of data or attributes.
    pub ctime: Time,
    /// The change counter (`NFSv4` `change`).
    pub change: u64,
    /// The owner profile.
    pub owner: String,
    /// The primary group profile.
    pub group: Option<String>,
    /// The data's CCSID as stored.
    pub ccsid: u16,
}

/// An object found or made: its handle and attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The handle.
    pub fh: FileHandle,
    /// The attributes.
    pub attr: Attr,
}

/// What `wcc_data` says before a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WccAttr {
    /// Size.
    pub size: u64,
    /// Modification time.
    pub mtime: Time,
    /// Change time.
    pub ctime: Time,
}

/// An object before and after a change (either may be unknown).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Wcc {
    /// Before.
    pub before: Option<WccAttr>,
    /// After.
    pub after: Option<Attr>,
}

/// How a time is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SetTime {
    /// Left as it is.
    #[default]
    Keep,
    /// The server's time.
    Server,
    /// The client's.
    Client(Time),
}

/// Attributes to set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SetAttr {
    /// Permission bits.
    pub mode: Option<u32>,
    /// Owner UID.
    pub uid: Option<u32>,
    /// Group GID.
    pub gid: Option<u32>,
    /// Size.
    pub size: Option<u64>,
    /// Access time.
    pub atime: SetTime,
    /// Modification time.
    pub mtime: SetTime,
}

/// How `create` makes a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateHow {
    /// Made, or the existing file taken (and its attributes set).
    Unchecked(SetAttr),
    /// Made; an existing file is `Exist`.
    Guarded(SetAttr),
    /// Made once: a retransmission with the same verifier finds it.
    Exclusive([u8; 8]),
}

/// What a write asks to be stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stable {
    /// May be lost on a crash until committed.
    Unstable,
    /// Data on disk.
    DataSync,
    /// Data and attributes on disk.
    FileSync,
}

/// A write's result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// Bytes taken.
    pub count: u32,
    /// How stable they are.
    pub committed: Stable,
    /// The server instance's write verifier.
    pub verf: [u8; 8],
    /// The file before and after.
    pub wcc: Wcc,
}

/// A read's result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadData {
    /// The bytes (converted).
    pub data: Vec<u8>,
    /// Whether the end of file was reached.
    pub eof: bool,
    /// The file's attributes.
    pub attr: Option<Attr>,
}

/// An object made in a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Made {
    /// The new object.
    pub node: Option<Node>,
    /// The directory before and after.
    pub dir: Wcc,
}

/// A special file `mknod` makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Special {
    /// A FIFO.
    Fifo,
    /// A socket.
    Sock,
    /// A character device.
    Chr(u32, u32),
    /// A block device.
    Blk(u32, u32),
}

/// One directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// The name, in the host's path name CCSID.
    pub name: Vec<u8>,
    /// The object's number.
    pub fileid: u64,
    /// Where the next page starts after it.
    pub cookie: u64,
    /// With `plus`: its handle and attributes.
    pub node: Option<Node>,
}

/// A page of a directory: entries with cookies above the one asked
/// for, in cookie order, without `.` and `..`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirPage {
    /// The cookie verifier.
    pub verf: [u8; 8],
    /// The entries.
    pub entries: Vec<DirEntry>,
    /// Whether these are the last.
    pub eof: bool,
    /// The directory's attributes.
    pub dir: Option<Attr>,
}

/// Space and objects (`FSSTAT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FsStat {
    /// Total bytes.
    pub tbytes: u64,
    /// Free bytes.
    pub fbytes: u64,
    /// Bytes free to the caller.
    pub abytes: u64,
    /// Total objects.
    pub tfiles: u64,
    /// Free objects.
    pub ffiles: u64,
    /// Objects free to the caller.
    pub afiles: u64,
}

/// What `PATHCONF` answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathConf {
    /// Most links.
    pub linkmax: u32,
    /// Longest name.
    pub name_max: u32,
    /// Long names are refused, not cut.
    pub no_trunc: bool,
    /// Only a privileged user changes owners.
    pub chown_restricted: bool,
    /// Names are matched without regard to case.
    pub case_insensitive: bool,
    /// Names keep their case.
    pub case_preserving: bool,
}

/// Open access (`NFSv4` `OPEN`, NLM `SHARE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShareAccess {
    /// Read.
    Read,
    /// Write.
    Write,
    /// Both.
    Both,
}

/// What an open denies others.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShareDeny {
    /// Nothing.
    None,
    /// Reading.
    Read,
    /// Writing.
    Write,
    /// Both.
    Both,
}

/// A byte-range lock's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LockKind {
    /// Shared.
    Read,
    /// Exclusive.
    Write,
}

/// A lock in the way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conflict {
    /// The holder's job (the IFS owner).
    pub owner: u64,
    /// The holder within it (0: the job itself).
    pub sub: u64,
    /// Its kind.
    pub kind: LockKind,
    /// Its first byte.
    pub start: u64,
    /// Its length, 0 to the end of file and beyond.
    pub len: u64,
}

/// What a lock request got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockOutcome {
    /// Held now.
    Granted,
    /// Refused: this lock is in the way.
    Denied(Conflict),
    /// Waiting: the token is named by the grant poll once held.
    Queued(u64),
}

/// The file system the protocols serve. Locks and opens are held for
/// sub-owners (`sub`, a number the protocol gives each remote owner;
/// 0 is never one): they conflict with each other and with local jobs.
#[allow(clippy::missing_errors_doc, clippy::too_many_arguments)]
pub trait Vfs {
    /// The export's root.
    fn root(&self, r: &Req) -> VResult<Node>;
    /// The object at `path` (the client's bytes) under the export
    /// (MOUNT): a directory.
    fn lookup_path(&self, r: &Req, path: &[u8]) -> VResult<Node>;
    /// Attributes.
    fn getattr(&self, r: &Req, fh: &FileHandle) -> VResult<Attr>;
    /// Sets attributes; with `guard`, only when the change time is it
    /// (`NotSync`).
    fn setattr(&self, r: &Req, fh: &FileHandle, a: &SetAttr, guard: Option<Time>)
    -> VResult<Wcc>;
    /// The object `name` in `dir` (`.` and `..` too; `..` of the
    /// export's root is the root), and the directory's attributes.
    fn lookup(&self, r: &Req, dir: &FileHandle, name: &[u8]) -> VResult<(Node, Option<Attr>)>;
    /// Which of the `ACCESS3_*` bits `want` the caller has.
    fn access(&self, r: &Req, fh: &FileHandle, want: u32) -> VResult<(u32, Option<Attr>)>;
    /// A symbolic link's target (the client's bytes).
    fn readlink(&self, r: &Req, fh: &FileHandle) -> VResult<(Vec<u8>, Option<Attr>)>;
    /// Up to `count` bytes at `off`.
    fn read(&self, r: &Req, fh: &FileHandle, off: u64, count: u32) -> VResult<ReadData>;
    /// Writes `data` at `off`.
    fn write(&self, r: &Req, fh: &FileHandle, off: u64, data: &[u8], stable: Stable)
    -> VResult<Written>;
    /// Makes earlier unstable writes stable: the write verifier.
    fn commit(&self, r: &Req, fh: &FileHandle, off: u64, count: u32) -> VResult<([u8; 8], Wcc)>;
    /// Makes a regular file.
    fn create(&self, r: &Req, dir: &FileHandle, name: &[u8], how: CreateHow) -> VResult<Made>;
    /// Makes a directory.
    fn mkdir(&self, r: &Req, dir: &FileHandle, name: &[u8], a: &SetAttr) -> VResult<Made>;
    /// Makes a symbolic link to `target` (the client's bytes).
    fn symlink(&self, r: &Req, dir: &FileHandle, name: &[u8], target: &[u8], a: &SetAttr)
    -> VResult<Made>;
    /// Makes a special file (FIFOs only: the others are `NotSupp`).
    fn mknod(&self, r: &Req, dir: &FileHandle, name: &[u8], what: Special, a: &SetAttr)
    -> VResult<Made>;
    /// Removes a non-directory.
    fn remove(&self, r: &Req, dir: &FileHandle, name: &[u8]) -> VResult<Wcc>;
    /// Removes an empty directory.
    fn rmdir(&self, r: &Req, dir: &FileHandle, name: &[u8]) -> VResult<Wcc>;
    /// Renames, replacing what `to`/`to_name` names.
    fn rename(&self, r: &Req, from: &FileHandle, from_name: &[u8], to: &FileHandle, to_name: &[u8])
    -> VResult<(Wcc, Wcc)>;
    /// Links `fh` as `name` in `dir`.
    fn link(&self, r: &Req, fh: &FileHandle, dir: &FileHandle, name: &[u8])
    -> VResult<(Option<Attr>, Wcc)>;
    /// Entries after `cookie` (0: the first), at most `max`; a cookie
    /// other than 0 needs the page's `verf` (`BadCookie`). With `plus`,
    /// each entry's handle and attributes.
    fn readdir(&self, r: &Req, dir: &FileHandle, cookie: u64, verf: [u8; 8], max: usize, plus: bool)
    -> VResult<DirPage>;
    /// Space and objects.
    fn fsstat(&self, r: &Req, fh: &FileHandle) -> VResult<(FsStat, Option<Attr>)>;
    /// Names and links.
    fn pathconf(&self, r: &Req, fh: &FileHandle) -> VResult<(PathConf, Option<Attr>)>;
    /// Opens with a share reservation: the open's number.
    fn open(&self, r: &Req, fh: &FileHandle, access: ShareAccess, deny: ShareDeny) -> VResult<u64>;
    /// Ends an open of [`Vfs::open`].
    fn close(&self, open: u64) -> VResult<()>;
    /// Locks a range for `sub`; with `wait`, a lock that cannot be had
    /// now is queued.
    fn lock(&self, r: &Req, fh: &FileHandle, sub: u64, kind: LockKind, start: u64, len: u64, wait: bool)
    -> VResult<LockOutcome>;
    /// The lock that would refuse this one.
    fn test_lock(&self, r: &Req, fh: &FileHandle, sub: u64, kind: LockKind, start: u64, len: u64)
    -> VResult<Option<Conflict>>;
    /// Unlocks a range of `sub`'s.
    fn unlock(&self, r: &Req, fh: &FileHandle, sub: u64, start: u64, len: u64) -> VResult<()>;
    /// Takes back a queued lock: whether it was still queued.
    fn cancel_wait(&self, token: u64) -> VResult<bool>;
    /// Drops every lock and queued request of `sub`: how many locks.
    fn release_sub(&self, sub: u64) -> VResult<u32>;
    /// Reserves the lease `key` in state `want` (the IFS lease states)
    /// on the file, known to the caller as `tag`: the state granted.
    fn lease(&self, r: &Req, fh: &FileHandle, tag: u64, key: [u8; 32], want: u8) -> VResult<u8>;
    /// The leases still held on `tag`.
    fn lease_report(&self, tag: u64, leases: &[([u8; 32], u8)]) -> VResult<()>;
}
