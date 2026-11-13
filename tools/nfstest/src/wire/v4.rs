// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFS version 4.0 (RFC 7530, 7531) and 4.1 (RFC 8881, 5662) `COMPOUND`,
//! by hand: the operations this client sends and their results.

use super::v4attr::{self, Attrs, get_bitmap, get_fattr, put_bitmap};
use super::xdr::{R, W};
use crate::Error;

/// `NFS4_PROGRAM` procedure 1.
pub const COMPOUND: u32 = 1;

/// Operation numbers.
pub mod op {
    /// `OP_ACCESS`.
    pub const ACCESS: u32 = 3;
    /// `OP_CLOSE`.
    pub const CLOSE: u32 = 4;
    /// `OP_COMMIT`.
    pub const COMMIT: u32 = 5;
    /// `OP_CREATE`.
    pub const CREATE: u32 = 6;
    /// `OP_GETATTR`.
    pub const GETATTR: u32 = 9;
    /// `OP_GETFH`.
    pub const GETFH: u32 = 10;
    /// `OP_LOCK`.
    pub const LOCK: u32 = 12;
    /// `OP_LOCKT`.
    pub const LOCKT: u32 = 13;
    /// `OP_LOCKU`.
    pub const LOCKU: u32 = 14;
    /// `OP_LOOKUP`.
    pub const LOOKUP: u32 = 15;
    /// `OP_OPEN`.
    pub const OPEN: u32 = 18;
    /// `OP_OPEN_CONFIRM`.
    pub const OPEN_CONFIRM: u32 = 20;
    /// `OP_PUTFH`.
    pub const PUTFH: u32 = 22;
    /// `OP_PUTROOTFH`.
    pub const PUTROOTFH: u32 = 24;
    /// `OP_READ`.
    pub const READ: u32 = 25;
    /// `OP_READDIR`.
    pub const READDIR: u32 = 26;
    /// `OP_REMOVE`.
    pub const REMOVE: u32 = 28;
    /// `OP_RENAME`.
    pub const RENAME: u32 = 29;
    /// `OP_RENEW`.
    pub const RENEW: u32 = 30;
    /// `OP_RESTOREFH`.
    pub const RESTOREFH: u32 = 31;
    /// `OP_SAVEFH`.
    pub const SAVEFH: u32 = 32;
    /// `OP_SETATTR`.
    pub const SETATTR: u32 = 34;
    /// `OP_SETCLIENTID`.
    pub const SETCLIENTID: u32 = 35;
    /// `OP_SETCLIENTID_CONFIRM`.
    pub const SETCLIENTID_CONFIRM: u32 = 36;
    /// `OP_WRITE`.
    pub const WRITE: u32 = 38;
    /// `OP_EXCHANGE_ID`.
    pub const EXCHANGE_ID: u32 = 42;
    /// `OP_CREATE_SESSION`.
    pub const CREATE_SESSION: u32 = 43;
    /// `OP_DESTROY_SESSION`.
    pub const DESTROY_SESSION: u32 = 44;
    /// `OP_LAYOUTGET`.
    pub const LAYOUTGET: u32 = 50;
    /// `OP_SEQUENCE`.
    pub const SEQUENCE: u32 = 53;
    /// `OP_DESTROY_CLIENTID`.
    pub const DESTROY_CLIENTID: u32 = 57;
    /// `OP_RECLAIM_COMPLETE`.
    pub const RECLAIM_COMPLETE: u32 = 58;
    /// `OP_ILLEGAL`.
    pub const ILLEGAL: u32 = 10044;
}

/// `nfsstat4` values the scenarios name.
pub mod st {
    /// `NFS4_OK`.
    pub const OK: u32 = 0;
    /// `NFS4ERR_NOENT`.
    pub const NOENT: u32 = 2;
    /// `NFS4ERR_EXIST`.
    pub const EXIST: u32 = 17;
    /// `NFS4ERR_NOTSUPP`.
    pub const NOTSUPP: u32 = 10004;
    /// `NFS4ERR_DENIED`.
    pub const DENIED: u32 = 10010;
    /// `NFS4ERR_GRACE`.
    pub const GRACE: u32 = 10013;
    /// `NFS4ERR_SHARE_DENIED`.
    pub const SHARE_DENIED: u32 = 10015;
    /// `NFS4ERR_MINOR_VERS_MISMATCH`.
    pub const MINOR_VERS_MISMATCH: u32 = 10021;
    /// `NFS4ERR_OP_ILLEGAL`.
    pub const OP_ILLEGAL: u32 = 10044;
    /// `NFS4ERR_SEQ_MISORDERED`.
    pub const SEQ_MISORDERED: u32 = 10063;
    /// `NFS4ERR_OP_NOT_IN_SESSION`.
    pub const OP_NOT_IN_SESSION: u32 = 10071;
}

/// The name of an `nfsstat4`.
pub fn status_name(s: u32) -> String {
    let n = match s {
        0 => "NFS4_OK",
        1 => "NFS4ERR_PERM",
        2 => "NFS4ERR_NOENT",
        5 => "NFS4ERR_IO",
        13 => "NFS4ERR_ACCESS",
        17 => "NFS4ERR_EXIST",
        18 => "NFS4ERR_XDEV",
        20 => "NFS4ERR_NOTDIR",
        21 => "NFS4ERR_ISDIR",
        22 => "NFS4ERR_INVAL",
        63 => "NFS4ERR_NAMETOOLONG",
        66 => "NFS4ERR_NOTEMPTY",
        70 => "NFS4ERR_STALE",
        10001 => "NFS4ERR_BADHANDLE",
        10004 => "NFS4ERR_NOTSUPP",
        10006 => "NFS4ERR_SERVERFAULT",
        10008 => "NFS4ERR_DELAY",
        10010 => "NFS4ERR_DENIED",
        10011 => "NFS4ERR_EXPIRED",
        10013 => "NFS4ERR_GRACE",
        10015 => "NFS4ERR_SHARE_DENIED",
        10017 => "NFS4ERR_CLID_INUSE",
        10020 => "NFS4ERR_NOFILEHANDLE",
        10021 => "NFS4ERR_MINOR_VERS_MISMATCH",
        10022 => "NFS4ERR_STALE_CLIENTID",
        10023 => "NFS4ERR_STALE_STATEID",
        10024 => "NFS4ERR_OLD_STATEID",
        10025 => "NFS4ERR_BAD_STATEID",
        10026 => "NFS4ERR_BAD_SEQID",
        10032 => "NFS4ERR_ATTRNOTSUPP",
        10036 => "NFS4ERR_BADXDR",
        10038 => "NFS4ERR_OPENMODE",
        10044 => "NFS4ERR_OP_ILLEGAL",
        10052 => "NFS4ERR_BADSESSION",
        10053 => "NFS4ERR_BADSLOT",
        10063 => "NFS4ERR_SEQ_MISORDERED",
        10068 => "NFS4ERR_RETRY_UNCACHED_REP",
        10071 => "NFS4ERR_OP_NOT_IN_SESSION",
        10076 => "NFS4ERR_SEQ_FALSE_RETRY",
        _ => return format!("nfsstat4 {s}"),
    };
    n.to_string()
}

/// `stateid4`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stateid {
    /// `seqid`.
    pub seqid: u32,
    /// `other`.
    pub other: [u8; 12],
}

impl Stateid {
    fn put(&self, w: &mut W) {
        w.u32(self.seqid).fixed(&self.other);
    }

    fn get(r: &mut R<'_>) -> Result<Stateid, Error> {
        let seqid = r.u32()?;
        let other = r.fixed(12)?.try_into().expect("12");
        Ok(Stateid { seqid, other })
    }
}

/// How an `OPEN` names its file.
#[derive(Clone, Debug)]
pub enum Claim {
    /// `CLAIM_NULL`: a name in the current directory.
    Null(Vec<u8>),
    /// `CLAIM_FH` (4.1): the current file.
    Fh,
}

/// `OPEN`'s arguments.
#[derive(Clone, Debug)]
pub struct Open {
    /// `seqid` (0 in 4.1).
    pub seqid: u32,
    /// `share_access` (1 read, 2 write, 3 both).
    pub access: u32,
    /// `share_deny` (0 none, 1 read, 2 write, 3 both).
    pub deny: u32,
    /// `open_owner4.clientid`.
    pub clientid: u64,
    /// `open_owner4.owner`.
    pub owner: Vec<u8>,
    /// Create (`UNCHECKED4` with this mode) or not.
    pub create: Option<u32>,
    /// The claim.
    pub claim: Claim,
}

/// `locker4`.
#[derive(Clone, Debug)]
pub enum Locker {
    /// A new lock owner from an open.
    New {
        /// `open_seqid`.
        open_seqid: u32,
        /// `open_stateid`.
        open_stateid: Stateid,
        /// `lock_seqid`.
        lock_seqid: u32,
        /// `lock_owner4.clientid`.
        clientid: u64,
        /// `lock_owner4.owner`.
        owner: Vec<u8>,
    },
    /// A known lock owner.
    Exist {
        /// `lock_stateid`.
        stateid: Stateid,
        /// `lock_seqid`.
        seqid: u32,
    },
}

/// One operation.
#[derive(Clone, Debug)]
pub enum Op {
    /// `PUTROOTFH`.
    PutRootFh,
    /// `PUTFH`.
    PutFh(Vec<u8>),
    /// `GETFH`.
    GetFh,
    /// `LOOKUP`.
    Lookup(Vec<u8>),
    /// `CREATE` of a directory with this mode.
    Mkdir(Vec<u8>, u32),
    /// `GETATTR` of these attribute numbers.
    GetAttr(Vec<u32>),
    /// `SAVEFH`.
    SaveFh,
    /// `RESTOREFH`.
    RestoreFh,
    /// `SETCLIENTID`: verifier, id, callback program, netid, address, ident.
    SetClientId([u8; 8], Vec<u8>, u32, String, String, u32),
    /// `SETCLIENTID_CONFIRM`.
    SetClientIdConfirm(u64, [u8; 8]),
    /// `RENEW`.
    Renew(u64),
    /// `OPEN`.
    Open(Open),
    /// `OPEN_CONFIRM`.
    OpenConfirm(Stateid, u32),
    /// `CLOSE`: seqid, stateid.
    Close(u32, Stateid),
    /// `WRITE`: stateid, offset, stable (0 unstable, 2 file sync), data.
    Write(Stateid, u64, u32, Vec<u8>),
    /// `READ`: stateid, offset, count.
    Read(Stateid, u64, u32),
    /// `COMMIT`.
    Commit(u64, u32),
    /// `READDIR`: cookie, verifier, dircount, maxcount, attributes.
    ReadDir(u64, [u8; 8], u32, u32, Vec<u32>),
    /// `REMOVE`.
    Remove(Vec<u8>),
    /// `RENAME` (saved directory, then current): old, new.
    Rename(Vec<u8>, Vec<u8>),
    /// `SETATTR`: stateid, mode, size.
    SetAttr(Stateid, Option<u32>, Option<u64>),
    /// `LOCK`: type (1 read, 2 write), reclaim, offset, length, locker.
    Lock(u32, bool, u64, u64, Locker),
    /// `LOCKT`: type, offset, length, clientid, owner.
    LockT(u32, u64, u64, u64, Vec<u8>),
    /// `LOCKU`: type, seqid, stateid, offset, length.
    LockU(u32, u32, Stateid, u64, u64),
    /// `EXCHANGE_ID`: verifier, owner, flags.
    ExchangeId([u8; 8], Vec<u8>, u32),
    /// `CREATE_SESSION`: clientid, sequence.
    CreateSession(u64, u32),
    /// `SEQUENCE`: session, sequence ID, slot, highest slot, cache this.
    Sequence([u8; 16], u32, u32, u32, bool),
    /// `RECLAIM_COMPLETE`.
    ReclaimComplete(bool),
    /// `DESTROY_SESSION`.
    DestroySession([u8; 16]),
    /// `DESTROY_CLIENTID`.
    DestroyClientId(u64),
    /// `LAYOUTGET` of a whole file (`LAYOUT4_NFSV4_1_FILES`, read).
    LayoutGet(Stateid),
    /// An operation number with no arguments (for `OP_ILLEGAL`).
    Raw(u32),
}

fn channel_attrs(w: &mut W) {
    // headerpadsize, maxrequestsize, maxresponsesize, maxresponsesize_cached,
    // maxoperations, maxrequests, rdma_ird<1>.
    w.u32(0)
        .u32(1_049_620)
        .u32(1_049_480)
        .u32(8192)
        .u32(16)
        .u32(8)
        .u32(0);
}

impl Op {
    /// The operation number.
    pub fn number(&self) -> u32 {
        match self {
            Op::PutRootFh => op::PUTROOTFH,
            Op::PutFh(_) => op::PUTFH,
            Op::GetFh => op::GETFH,
            Op::Lookup(_) => op::LOOKUP,
            Op::Mkdir(..) => op::CREATE,
            Op::GetAttr(_) => op::GETATTR,
            Op::SaveFh => op::SAVEFH,
            Op::RestoreFh => op::RESTOREFH,
            Op::SetClientId(..) => op::SETCLIENTID,
            Op::SetClientIdConfirm(..) => op::SETCLIENTID_CONFIRM,
            Op::Renew(_) => op::RENEW,
            Op::Open(_) => op::OPEN,
            Op::OpenConfirm(..) => op::OPEN_CONFIRM,
            Op::Close(..) => op::CLOSE,
            Op::Write(..) => op::WRITE,
            Op::Read(..) => op::READ,
            Op::Commit(..) => op::COMMIT,
            Op::ReadDir(..) => op::READDIR,
            Op::Remove(_) => op::REMOVE,
            Op::Rename(..) => op::RENAME,
            Op::SetAttr(..) => op::SETATTR,
            Op::Lock(..) => op::LOCK,
            Op::LockT(..) => op::LOCKT,
            Op::LockU(..) => op::LOCKU,
            Op::ExchangeId(..) => op::EXCHANGE_ID,
            Op::CreateSession(..) => op::CREATE_SESSION,
            Op::Sequence(..) => op::SEQUENCE,
            Op::ReclaimComplete(_) => op::RECLAIM_COMPLETE,
            Op::DestroySession(_) => op::DESTROY_SESSION,
            Op::DestroyClientId(_) => op::DESTROY_CLIENTID,
            Op::LayoutGet(_) => op::LAYOUTGET,
            Op::Raw(n) => *n,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn put(&self, w: &mut W) {
        w.u32(self.number());
        match self {
            Op::PutRootFh | Op::GetFh | Op::SaveFh | Op::RestoreFh | Op::Raw(_) => {}
            Op::PutFh(h) => {
                w.opaque(h);
            }
            Op::Lookup(n) | Op::Remove(n) => {
                w.opaque(n);
            }
            Op::GetAttr(attrs) => put_bitmap(w, &v4attr::bitmap(attrs)),
            Op::Mkdir(name, mode) => {
                // createtype4 NF4DIR, the name, the attributes.
                w.u32(2).opaque(name);
                v4attr::put_fattr_set(w, Some(*mode), None);
            }
            Op::SetClientId(verf, id, prog, netid, addr, ident) => {
                w.fixed(verf)
                    .opaque(id)
                    .u32(*prog)
                    .opaque(netid.as_bytes())
                    .opaque(addr.as_bytes())
                    .u32(*ident);
            }
            Op::SetClientIdConfirm(id, verf) => {
                w.u64(*id).fixed(verf);
            }
            Op::Renew(id) | Op::DestroyClientId(id) => {
                w.u64(*id);
            }
            Op::Open(o) => {
                w.u32(o.seqid)
                    .u32(o.access)
                    .u32(o.deny)
                    .u64(o.clientid)
                    .opaque(&o.owner);
                match o.create {
                    None => {
                        w.u32(0);
                    }
                    Some(mode) => {
                        // OPEN4_CREATE, UNCHECKED4, the mode.
                        w.u32(1).u32(0);
                        v4attr::put_fattr_set(w, Some(mode), None);
                    }
                }
                match &o.claim {
                    Claim::Null(name) => {
                        w.u32(0).opaque(name);
                    }
                    Claim::Fh => {
                        w.u32(4);
                    }
                }
            }
            Op::OpenConfirm(s, seqid) => {
                s.put(w);
                w.u32(*seqid);
            }
            Op::Close(seqid, s) => {
                w.u32(*seqid);
                s.put(w);
            }
            Op::Write(s, off, stable, data) => {
                s.put(w);
                w.u64(*off).u32(*stable).opaque(data);
            }
            Op::Read(s, off, count) => {
                s.put(w);
                w.u64(*off).u32(*count);
            }
            Op::Commit(off, count) => {
                w.u64(*off).u32(*count);
            }
            Op::ReadDir(cookie, verf, dircount, maxcount, attrs) => {
                w.u64(*cookie).fixed(verf).u32(*dircount).u32(*maxcount);
                put_bitmap(w, &v4attr::bitmap(attrs));
            }
            Op::Rename(old, new) => {
                w.opaque(old).opaque(new);
            }
            Op::SetAttr(s, mode, size) => {
                s.put(w);
                v4attr::put_fattr_set(w, *mode, *size);
            }
            Op::Lock(ty, reclaim, off, len, locker) => {
                w.u32(*ty).bool(*reclaim).u64(*off).u64(*len);
                match locker {
                    Locker::New {
                        open_seqid,
                        open_stateid,
                        lock_seqid,
                        clientid,
                        owner,
                    } => {
                        w.bool(true).u32(*open_seqid);
                        open_stateid.put(w);
                        w.u32(*lock_seqid).u64(*clientid).opaque(owner);
                    }
                    Locker::Exist { stateid, seqid } => {
                        w.bool(false);
                        stateid.put(w);
                        w.u32(*seqid);
                    }
                }
            }
            Op::LockT(ty, off, len, clientid, owner) => {
                w.u32(*ty).u64(*off).u64(*len).u64(*clientid).opaque(owner);
            }
            Op::LockU(ty, seqid, s, off, len) => {
                w.u32(*ty).u32(*seqid);
                s.put(w);
                w.u64(*off).u64(*len);
            }
            Op::ExchangeId(verf, owner, flags) => {
                // client_owner4, flags, SP4_NONE, no implementation ID.
                w.fixed(verf).opaque(owner).u32(*flags).u32(0).u32(0);
            }
            Op::CreateSession(id, seq) => {
                w.u64(*id).u32(*seq).u32(0);
                channel_attrs(w);
                channel_attrs(w);
                // cb_program, one callback_sec_parms4: AUTH_NONE.
                w.u32(0x4000_0000).u32(1).u32(0);
            }
            Op::Sequence(sid, seq, slot, high, cache) => {
                w.fixed(sid).u32(*seq).u32(*slot).u32(*high).bool(*cache);
            }
            Op::ReclaimComplete(one_fs) => {
                w.bool(*one_fs);
            }
            Op::DestroySession(sid) => {
                w.fixed(sid);
            }
            Op::LayoutGet(s) => {
                // signal_layout_avail, LAYOUT4_NFSV4_1_FILES, LAYOUTIOMODE4_READ,
                // offset, length, minlength, stateid, maxcount.
                w.bool(false).u32(1).u32(1).u64(0).u64(u64::MAX).u64(0);
                s.put(w);
                w.u32(65536);
            }
        }
    }
}

/// `COMPOUND4args`.
pub fn compound(tag: &str, minor: u32, ops: &[Op]) -> Vec<u8> {
    let mut w = W::new();
    w.opaque(tag.as_bytes()).u32(minor).u32(ops.len() as u32);
    for o in ops {
        o.put(&mut w);
    }
    w.done()
}

/// A directory entry from `READDIR`.
#[derive(Clone, Debug)]
pub struct Entry {
    /// `cookie`.
    pub cookie: u64,
    /// `name`.
    pub name: Vec<u8>,
    /// `attrs`.
    pub attrs: Attrs,
}

/// What one operation returned (when `NFS4_OK`, else `None`).
#[derive(Clone, Debug)]
pub enum Body {
    /// No results.
    None,
    /// `GETFH`.
    Fh(Vec<u8>),
    /// `GETATTR`.
    Attrs(Box<Attrs>),
    /// `SETCLIENTID`: clientid, confirm verifier.
    ClientId(u64, [u8; 8]),
    /// `OPEN`: stateid, rflags.
    Open(Stateid, u32),
    /// `OPEN_CONFIRM`, `CLOSE`, `LOCK`, `LOCKU`: a stateid.
    Stateid(Stateid),
    /// `WRITE`: count, committed, verifier.
    Write(u32, u32, [u8; 8]),
    /// `READ`: eof, data.
    Read(bool, Vec<u8>),
    /// `COMMIT`: verifier.
    Commit([u8; 8]),
    /// `READDIR`: verifier, entries, eof.
    ReadDir([u8; 8], Vec<Entry>, bool),
    /// `SETATTR`: the attributes set.
    SetAttr(Vec<u32>),
    /// `LOCK`, `LOCKT` denied: the holder's offset, length, type.
    Denied(u64, u64, u32),
    /// `EXCHANGE_ID`: clientid, sequence ID, flags.
    ExchangeId(u64, u32, u32),
    /// `CREATE_SESSION`: session, sequence.
    Session([u8; 16], u32),
    /// `SEQUENCE`: session, sequence ID, slot, status flags.
    Sequence([u8; 16], u32, u32, u32),
}

/// One operation's result.
#[derive(Clone, Debug)]
pub struct Res {
    /// The operation number the server answered with.
    pub op: u32,
    /// Its status.
    pub status: u32,
    /// Its results.
    pub body: Body,
}

/// `COMPOUND4res`.
#[derive(Clone, Debug)]
pub struct Reply {
    /// The compound's status (the last operation's).
    pub status: u32,
    /// The tag.
    pub tag: Vec<u8>,
    /// The results, up to the first that failed.
    pub res: Vec<Res>,
}

impl Reply {
    /// The result of operation `n` (the first one).
    pub fn of(&self, n: u32) -> Option<&Res> {
        self.res.iter().find(|r| r.op == n)
    }
}

fn change_info(r: &mut R<'_>) -> Result<(), Error> {
    r.bool()?;
    r.u64()?;
    r.u64()?;
    Ok(())
}

fn ace(r: &mut R<'_>) -> Result<(), Error> {
    r.u32()?;
    r.u32()?;
    r.u32()?;
    r.opaque(1024)?;
    Ok(())
}

fn delegation(r: &mut R<'_>) -> Result<(), Error> {
    match r.u32()? {
        0 => {}
        1 => {
            Stateid::get(r)?;
            r.bool()?;
            ace(r)?;
        }
        2 => {
            Stateid::get(r)?;
            r.bool()?;
            match r.u32()? {
                1 => {
                    r.u64()?;
                }
                2 => {
                    r.u32()?;
                    r.u32()?;
                }
                l => return Err(Error::Decode(format!("limitby {l}"))),
            }
            ace(r)?;
        }
        3 => {
            if matches!(r.u32()?, 1 | 2) {
                r.bool()?;
            }
        }
        d => return Err(Error::Decode(format!("open_delegation_type4 {d}"))),
    }
    Ok(())
}

fn channel(r: &mut R<'_>) -> Result<(), Error> {
    for _ in 0..6 {
        r.u32()?;
    }
    let n = r.count(1)?;
    for _ in 0..n {
        r.u32()?;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn body(n: u32, status: u32, r: &mut R<'_>) -> Result<Body, Error> {
    if status != st::OK {
        // The few results with a body on failure.
        match (n, status) {
            (op::LOCK | op::LOCKT, st::DENIED) => {
                let (off, len, ty) = (r.u64()?, r.u64()?, r.u32()?);
                r.u64()?;
                r.opaque(1024)?;
                return Ok(Body::Denied(off, len, ty));
            }
            (op::SETATTR, _) => return Ok(Body::SetAttr(get_bitmap(r)?)),
            (op::SETCLIENTID, 10017) => {
                r.opaque(1024)?;
                r.opaque(1024)?;
            }
            (op::LAYOUTGET, 10058) => {
                r.bool()?;
            }
            _ => {}
        }
        return Ok(Body::None);
    }
    Ok(match n {
        op::PUTROOTFH
        | op::PUTFH
        | op::LOOKUP
        | op::SAVEFH
        | op::RESTOREFH
        | op::SETCLIENTID_CONFIRM
        | op::RENEW
        | op::RECLAIM_COMPLETE
        | op::DESTROY_SESSION
        | op::DESTROY_CLIENTID
        | op::LOCKT => Body::None,
        op::GETFH => Body::Fh(r.opaque(128)?.to_vec()),
        op::GETATTR => Body::Attrs(Box::new(get_fattr(r)?)),
        op::SETCLIENTID => Body::ClientId(r.u64()?, r.fixed(8)?.try_into().expect("8")),
        op::OPEN => {
            let s = Stateid::get(r)?;
            change_info(r)?;
            let rflags = r.u32()?;
            get_bitmap(r)?;
            delegation(r)?;
            Body::Open(s, rflags)
        }
        op::OPEN_CONFIRM | op::CLOSE | op::LOCK | op::LOCKU => Body::Stateid(Stateid::get(r)?),
        op::WRITE => Body::Write(r.u32()?, r.u32()?, r.fixed(8)?.try_into().expect("8")),
        op::READ => Body::Read(r.bool()?, r.opaque(1 << 22)?.to_vec()),
        op::COMMIT => Body::Commit(r.fixed(8)?.try_into().expect("8")),
        op::READDIR => {
            let verf = r.fixed(8)?.try_into().expect("8");
            let mut entries = Vec::new();
            while r.bool()? {
                entries.push(Entry {
                    cookie: r.u64()?,
                    name: r.opaque(1024)?.to_vec(),
                    attrs: get_fattr(r)?,
                });
            }
            Body::ReadDir(verf, entries, r.bool()?)
        }
        op::REMOVE => {
            change_info(r)?;
            Body::None
        }
        op::RENAME => {
            change_info(r)?;
            change_info(r)?;
            Body::None
        }
        op::CREATE => {
            change_info(r)?;
            get_bitmap(r)?;
            Body::None
        }
        op::SETATTR => Body::SetAttr(get_bitmap(r)?),
        op::EXCHANGE_ID => {
            let (id, seq, flags) = (r.u64()?, r.u32()?, r.u32()?);
            if r.u32()? != 0 {
                return Err(Error::Decode("state protection other than SP4_NONE".into()));
            }
            r.u64()?;
            r.opaque(1024)?;
            r.opaque(1024)?;
            let n = r.count(1)?;
            for _ in 0..n {
                r.opaque(1024)?;
                r.opaque(1024)?;
                r.u64()?;
                r.u32()?;
            }
            Body::ExchangeId(id, seq, flags)
        }
        op::CREATE_SESSION => {
            let sid = r.fixed(16)?.try_into().expect("16");
            let seq = r.u32()?;
            r.u32()?;
            channel(r)?;
            channel(r)?;
            Body::Session(sid, seq)
        }
        op::SEQUENCE => {
            let sid = r.fixed(16)?.try_into().expect("16");
            let (seq, slot) = (r.u32()?, r.u32()?);
            r.u32()?;
            r.u32()?;
            Body::Sequence(sid, seq, slot, r.u32()?)
        }
        other => {
            return Err(Error::Decode(format!(
                "a result of operation {other}, which was not sent"
            )));
        }
    })
}

/// Decodes `COMPOUND4res`.
pub fn reply(b: &[u8]) -> Result<Reply, Error> {
    let mut r = R::new(b);
    let status = r.u32()?;
    let tag = r.opaque(1024)?.to_vec();
    let n = r.count(1024)?;
    let mut res = Vec::new();
    for _ in 0..n {
        let op = r.u32()?;
        let s = r.u32()?;
        let body = body(op, s, &mut r)?;
        res.push(Res {
            op,
            status: s,
            body,
        });
    }
    r.end()?;
    Ok(Reply { status, tag, res })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compound_header_and_ops() {
        let b = compound(
            "t",
            1,
            &[Op::PutRootFh, Op::Lookup(b"home".to_vec()), Op::GetFh],
        );
        let mut r = R::new(&b);
        assert_eq!(r.opaque(8).unwrap(), b"t");
        assert_eq!((r.u32().unwrap(), r.u32().unwrap()), (1, 3));
        assert_eq!(r.u32().unwrap(), op::PUTROOTFH);
        assert_eq!(r.u32().unwrap(), op::LOOKUP);
        assert_eq!(r.opaque(8).unwrap(), b"home");
        assert_eq!(r.u32().unwrap(), op::GETFH);
        r.end().unwrap();
    }

    #[test]
    fn replies_decode() {
        let b = W::new()
            .u32(0)
            .opaque(b"t")
            .u32(3)
            .u32(op::PUTROOTFH)
            .u32(0)
            .u32(op::GETFH)
            .u32(0)
            .opaque(&[7; 32])
            .u32(op::LOCK)
            .u32(st::DENIED)
            .u64(0)
            .u64(10)
            .u32(2)
            .u64(5)
            .opaque(b"o")
            .done();
        let rep = reply(&b).unwrap();
        assert_eq!(rep.res.len(), 3);
        assert!(matches!(&rep.of(op::GETFH).unwrap().body, Body::Fh(h) if h.len() == 32));
        assert!(matches!(
            rep.of(op::LOCK).unwrap().body,
            Body::Denied(0, 10, 2)
        ));
    }

    #[test]
    fn open_with_a_read_delegation() {
        let b = W::new()
            .u32(0)
            .opaque(b"")
            .u32(1)
            .u32(op::OPEN)
            .u32(0)
            .u32(1)
            .fixed(&[1; 12])
            .bool(true)
            .u64(1)
            .u64(2)
            .u32(4)
            .u32(0)
            .u32(1)
            .u32(2)
            .fixed(&[2; 12])
            .bool(false)
            .u32(0)
            .u32(0)
            .u32(0)
            .opaque(b"")
            .done();
        let rep = reply(&b).unwrap();
        assert!(matches!(rep.res[0].body, Body::Open(s, 4) if s.seqid == 1));
    }
}
