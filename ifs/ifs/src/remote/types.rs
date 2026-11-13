// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a remote file system and the IFS say to each other: handles,
//! attributes, credentials, the arguments of the calls, their errors,
//! and what a mount tells the IFS about itself ([`RemoteInfo`]).

use std::time::Duration;

use crate::backend::Nanos;
use crate::errno::Errno;

/// The longest handle a remote file system may give.
pub const MAX_HANDLE: usize = 128;

/// An object on the server: opaque bytes, at most [`MAX_HANDLE`].
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct RHandle(pub Vec<u8>);

impl std::fmt::Debug for RHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RHandle(")?;
        for b in &self.0 {
            write!(f, "{b:02x}")?;
        }
        f.write_str(")")
    }
}

/// What a remote object is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RKind {
    /// A regular file.
    #[default]
    File,
    /// A directory.
    Dir,
    /// A symbolic link.
    Symlink,
    /// A FIFO.
    Fifo,
    /// A socket.
    Socket,
    /// A character device.
    Chr,
    /// A block device.
    Blk,
}

/// A remote object's attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct RAttr {
    /// What it is.
    pub kind: RKind,
    /// Permission bits with set-user-ID, set-group-ID and sticky.
    pub mode: u32,
    /// Hard links.
    pub nlink: u32,
    /// Owner's UID as the server keeps it.
    pub uid: u32,
    /// Group's GID.
    pub gid: u32,
    /// Bytes of data.
    pub size: u64,
    /// Bytes allocated.
    pub used: u64,
    /// The server's file number.
    pub fileid: u64,
    /// Device number of a device node.
    pub rdev: u64,
    /// Last access, nanoseconds since the epoch.
    pub atime: Nanos,
    /// Last data change.
    pub mtime: Nanos,
    /// Last attribute change.
    pub ctime: Nanos,
    /// A value that moves on every change (the change attribute, or
    /// the modification time and size folded together when the
    /// protocol has none).
    pub change: u64,
}

/// Who a call is made as: the job user's UID, its primary group's
/// GID and the GIDs of all its groups.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Cred {
    /// UID.
    pub uid: u32,
    /// Primary GID.
    pub gid: u32,
    /// Every group's GID, the primary first.
    pub gids: Vec<u32>,
}

impl Cred {
    /// The UID and GID a user without one is sent as.
    pub const NOBODY: u32 = 65534;

    /// A caller with no identity.
    #[must_use]
    pub fn nobody() -> Self {
        Self {
            uid: Self::NOBODY,
            gid: Self::NOBODY,
            gids: Vec::new(),
        }
    }
}

/// A time `setattr` sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SetTime {
    /// The server's clock.
    Server,
    /// This time.
    At(Nanos),
}

/// The attributes `setattr` (and the creating calls) set; `None`
/// leaves one alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SetAttr {
    /// Permission bits.
    pub mode: Option<u32>,
    /// Owner.
    pub uid: Option<u32>,
    /// Group.
    pub gid: Option<u32>,
    /// Size.
    pub size: Option<u64>,
    /// Access time.
    pub atime: Option<SetTime>,
    /// Modification time.
    pub mtime: Option<SetTime>,
}

/// How `create` treats a name that exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CreateMode {
    /// Open what is there.
    Unchecked(SetAttr),
    /// Fail with `EEXIST`.
    Guarded(SetAttr),
    /// Fail with `EEXIST` unless this verifier made it (a retried
    /// exclusive create succeeds once).
    Exclusive(u64),
}

/// How far a write must reach before it answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Stable {
    /// The server's memory; `commit` makes it safe.
    #[default]
    Unstable,
    /// Data on stable storage.
    DataSync,
    /// Data and attributes on stable storage.
    FileSync,
}

/// What `write` answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteReply {
    /// Bytes taken.
    pub count: u32,
    /// How far they reached.
    pub committed: Stable,
    /// The server's write verifier: a different value later means it
    /// restarted and unstable data may be lost.
    pub verifier: u64,
}

/// One directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct REntry {
    /// The name as the server holds it.
    pub name: Vec<u8>,
    /// Where the next page starts after it.
    pub cookie: u64,
    /// The server's file number.
    pub fileid: u64,
    /// Its attributes, when the server sent them.
    pub attr: Option<RAttr>,
    /// Its handle, when the server sent it.
    pub handle: Option<RHandle>,
}

/// A page of a directory.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DirPage {
    /// The entries (without `.` and `..`).
    pub entries: Vec<REntry>,
    /// No more after these.
    pub eof: bool,
    /// The cookie verifier to send with the next page.
    pub verifier: u64,
}

/// Capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FsStat {
    /// Bytes in all.
    pub total_bytes: u64,
    /// Bytes free.
    pub free_bytes: u64,
    /// Bytes free to the caller.
    pub avail_bytes: u64,
    /// Objects in all.
    pub total_files: u64,
    /// Objects free.
    pub free_files: u64,
    /// Objects free to the caller.
    pub avail_files: u64,
}

/// Limits.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathConf {
    /// Most hard links.
    pub link_max: u32,
    /// Longest name in bytes.
    pub name_max: u32,
    /// A longer name is refused, not cut.
    pub no_trunc: bool,
    /// Only the superuser changes owners.
    pub chown_restricted: bool,
    /// Names compare without case.
    pub case_insensitive: bool,
    /// Names keep their case.
    pub case_preserving: bool,
}

impl Default for PathConf {
    fn default() -> Self {
        Self {
            link_max: 32_767,
            name_max: 255,
            no_trunc: true,
            chown_restricted: true,
            case_insensitive: false,
            case_preserving: true,
        }
    }
}

/// A stateful protocol's open (empty for a stateless one).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct OpenToken(pub Vec<u8>);

/// An open's share reservation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Share {
    /// Reads.
    pub read: bool,
    /// Writes.
    pub write: bool,
    /// Others may not read.
    pub deny_read: bool,
    /// Others may not write.
    pub deny_write: bool,
}

/// A byte-range lock's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LockKind {
    /// Shared.
    Read,
    /// Exclusive.
    Write,
}

/// Who holds a byte-range lock: one per IFS owner (job).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LockOwner(pub u64);

/// A lock that stands in the way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockConflict {
    /// Its kind.
    pub kind: LockKind,
    /// First byte.
    pub off: u64,
    /// Bytes (0: to the end and beyond).
    pub len: u64,
    /// The holder, as the server names it (opaque).
    pub holder: u64,
}

/// Why a remote call failed. The transport conditions the IFS treats
/// apart from an `errno` are their own variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RemoteError {
    /// The server answered with this error.
    #[error("{0:?}")]
    Errno(Errno),
    /// A soft mount gave up retrying.
    #[error("the server did not answer in time")]
    TimedOut,
    /// The call was taken back (`intr`).
    #[error("the call was interrupted")]
    Interrupted,
    /// The handle no longer names an object.
    #[error("the handle is stale")]
    Stale,
}

impl RemoteError {
    /// The `errno` a job sees.
    #[must_use]
    pub fn errno(self) -> Errno {
        match self {
            Self::Errno(e) => e,
            Self::TimedOut => Errno::ETIMEDOUT,
            Self::Interrupted => Errno::EINTR,
            Self::Stale => Errno::ESTALE,
        }
    }
}

impl From<Errno> for RemoteError {
    fn from(e: Errno) -> Self {
        match e {
            Errno::ESTALE => Self::Stale,
            Errno::ETIMEDOUT => Self::TimedOut,
            Errno::EINTR => Self::Interrupted,
            e => Self::Errno(e),
        }
    }
}

/// A remote call's result.
pub type RResult<T> = std::result::Result<T, RemoteError>;

/// How long attributes stay good without asking: regular files between
/// `reg_min` and `reg_max`, directories between `dir_min` and
/// `dir_max` (doubling while nothing changes). All zero is `noac`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttrTimes {
    /// `acregmin`.
    pub reg_min: Duration,
    /// `acregmax`.
    pub reg_max: Duration,
    /// `acdirmin`.
    pub dir_min: Duration,
    /// `acdirmax`.
    pub dir_max: Duration,
}

impl AttrTimes {
    /// `noac`: every use asks the server (once per request).
    pub const NONE: Self = Self {
        reg_min: Duration::ZERO,
        reg_max: Duration::ZERO,
        dir_min: Duration::ZERO,
        dir_max: Duration::ZERO,
    };
}

impl Default for AttrTimes {
    fn default() -> Self {
        Self {
            reg_min: Duration::from_secs(3),
            reg_max: Duration::from_secs(60),
            dir_min: Duration::from_secs(30),
            dir_max: Duration::from_secs(60),
        }
    }
}

/// What a mount tells the IFS about itself: what the IFS enforces or
/// uses, out of the options only the factory parses.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteInfo {
    /// The file system type shown by `DSPMFSINF` (`NFS` ...).
    pub name: String,
    /// What is mounted, as shown (`host:/path`).
    pub source: String,
    /// The options in effect, as shown.
    pub options: String,
    /// `ro`: nothing changes; refused locally before any call.
    pub read_only: bool,
    /// `nosuid`: set-ID bits are not honoured or set.
    pub nosuid: bool,
    /// The attribute cache.
    pub attr_times: AttrTimes,
    /// Bytes per read call.
    pub rsize: u32,
    /// Bytes per write call.
    pub wsize: u32,
    /// Calls give up (`ETIMEDOUT`) rather than retry for ever.
    pub soft: bool,
    /// A waiting job may be taken back (`EINTR`).
    pub intr: bool,
    /// Names compare exactly.
    pub case_sensitive: bool,
    /// The server keeps a removed file that is still open itself (no
    /// hidden rename by the IFS).
    pub keeps_open_unlinked: bool,
    /// `open` and `close` mean something (share reservations).
    pub stateful_open: bool,
    /// Every write is sent `FileSync`.
    pub sync_writes: bool,
    /// Byte-range locks work; without them they are `ENOLCK`.
    pub locks: bool,
    /// How long one call may run before the mount reports the server
    /// not responding.
    pub slow_after: Duration,
}

impl Default for RemoteInfo {
    fn default() -> Self {
        Self {
            name: String::new(),
            source: String::new(),
            options: String::new(),
            read_only: false,
            nosuid: false,
            attr_times: AttrTimes::default(),
            rsize: 32 * 1024,
            wsize: 32 * 1024,
            soft: false,
            intr: false,
            case_sensitive: true,
            keeps_open_unlinked: false,
            stateful_open: false,
            sync_writes: false,
            locks: true,
            slow_after: Duration::from_secs(15),
        }
    }
}

/// One half of `MOUNT CCSID(data path)` or `CODEPAGE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CcsidSel {
    /// `*BINARY`: no conversion (data only).
    #[default]
    Binary,
    /// `*ASCII`: the ASCII CCSID matching the job's.
    Ascii,
    /// `*JOBCCSID`: the job's.
    Job,
    /// A CCSID (a code page is given as its CCSID).
    Ccsid(u16),
}

/// `MOUNT`'s `CCSID` (or `CODEPAGE`, given as CCSIDs): what the data
/// of the mount's files is tagged with, and what its names are in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MountCcsid {
    /// Data.
    pub data: CcsidSel,
    /// Path names.
    pub path: CcsidSel,
}

impl Default for MountCcsid {
    fn default() -> Self {
        Self {
            data: CcsidSel::Binary,
            path: CcsidSel::Ascii,
        }
    }
}

/// One mounted file system, as `DSPMFSINF`, SQL and Navigator show it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MountInfo {
    /// Its id (`st_vfs`).
    pub id: u32,
    /// `*NFS`, `*UDFS`, or the file system's own name for the ones
    /// every system has (`root`, `QOpenSys`, `QSYS.LIB` ...).
    pub kind: String,
    /// Where it is.
    pub at: String,
    /// What is mounted (`host:/path`, the UDFS's block special file);
    /// empty for the standard ones.
    pub source: String,
    /// The options in effect.
    pub options: String,
    /// Nothing may change.
    pub read_only: bool,
    /// Names compare exactly.
    pub case_sensitive: bool,
    /// A remote mount's server answers (`None` for a local one).
    pub responding: Option<bool>,
    /// The data CCSID objects are tagged with (65535: binary).
    pub data_ccsid: u16,
    /// The CCSID path names are sent in.
    pub path_ccsid: u16,
}
