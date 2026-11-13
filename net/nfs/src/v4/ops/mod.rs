// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The operations, by concern: [`fh`] the filehandles and lookups,
//! [`attrs`] attributes and access, [`dir`] directory changes, [`io`]
//! reads and writes, [`open`] opens, [`lock`] byte-range locks,
//! [`client`] 4.0 client IDs and delegations, [`session`] 4.1's
//! sessions, [`refuse`] what is not done. This module dispatches and
//! holds what they share.

mod attrs;
mod client;
mod dir;
mod fh;
mod io;
mod lock;
mod open;
mod refuse;
mod session;

use super::Fh;
use super::attr::Bits;
use super::attr::get::{self, Env, Extra, View};
use super::compound::{Cx, Res};
use super::conv::status;
use super::pseudo::{self, Child, Tree};
use super::state::{Gone, OwnerKey, Replay, Tables, is_current, is_invalid, split_other};
use crate::server::vfs::{Attr, FileType, FsStat, PathConf, Time};
use crate::server::{FileHandle, Req, Vfs, VfsError};
use crate::types::nfs4::{fattr4, nfs_argop4, nfsstat4, stateid4};

type R<T> = Result<T, nfsstat4>;

/// The attribute sources of a real object.
struct Ex<'a> {
    vfs: &'a dyn Vfs,
    obj: Option<(&'a Req, &'a FileHandle)>,
}

impl Extra for Ex<'_> {
    fn fsstat(&mut self) -> R<FsStat> {
        match self.obj {
            Some((r, h)) => self.vfs.fsstat(r, h).map(|x| x.0).map_err(status),
            None => Ok(FsStat::default()),
        }
    }
    fn pathconf(&mut self) -> R<PathConf> {
        match self.obj {
            Some((r, h)) => self.vfs.pathconf(r, h).map(|x| x.0).map_err(status),
            None => Ok(PathConf {
                linkmax: 1,
                name_max: 255,
                no_trunc: true,
                chown_restricted: true,
                case_insensitive: false,
                case_preserving: true,
            }),
        }
    }
    fn created(&mut self) -> R<Time> {
        match self.obj {
            Some((r, h)) => self.vfs.created(r, h).map_err(status),
            None => Ok(Time::default()),
        }
    }
}

/// Which 4.0 owner a seqid belongs to.
#[derive(Debug, Clone)]
pub(super) enum Seq {
    Open(OwnerKey),
    Lock(OwnerKey),
}

/// What a seqid says of a request.
pub(super) enum SeqGo {
    New,
    Replay(Replay),
}

impl Cx<'_> {
    /// Runs one operation.
    pub(crate) fn op(&mut self, a: nfs_argop4) -> Res {
        use nfs_argop4 as A;
        let v41 = self.minor >= 1;
        match a {
            // 4.0's client ID and seqid operations are not 4.1's.
            A::OP_OPEN_CONFIRM(_)
            | A::OP_RENEW(_)
            | A::OP_SETCLIENTID(_)
            | A::OP_SETCLIENTID_CONFIRM(_)
            | A::OP_RELEASE_LOCKOWNER(_)
                if v41 =>
            {
                Res::Err(nfsstat4::NFS4ERR_NOTSUPP)
            }
            A::OP_ACCESS(x) => self.op_access(&x),
            A::OP_CLOSE(x) => self.op_close(&x),
            A::OP_COMMIT(x) => self.op_commit(&x),
            A::OP_CREATE(x) => self.op_create(&x),
            A::OP_DELEGPURGE(x) => Self::op_delegpurge(&x),
            A::OP_DELEGRETURN(x) => self.op_delegreturn(&x),
            A::OP_GETATTR(x) => self.op_getattr(&x),
            A::OP_GETFH => self.op_getfh(),
            A::OP_LINK(x) => self.op_link(&x),
            A::OP_LOCK(x) => self.op_lock(&x),
            A::OP_LOCKT(x) => self.op_lockt(&x),
            A::OP_LOCKU(x) => self.op_locku(&x),
            A::OP_LOOKUP(x) => self.op_lookup(&x),
            A::OP_LOOKUPP => self.op_lookupp(),
            A::OP_NVERIFY(x) => self.op_verify(&x.obj_attributes, false),
            A::OP_OPEN(x) => self.op_open(&x),
            A::OP_OPENATTR(x) => self.op_openattr(&x),
            A::OP_OPEN_CONFIRM(x) => self.op_open_confirm(&x),
            A::OP_OPEN_DOWNGRADE(x) => self.op_open_downgrade(&x),
            A::OP_PUTFH(x) => self.op_putfh(&x),
            A::OP_PUTPUBFH => self.op_putpubfh(),
            A::OP_PUTROOTFH => self.op_putrootfh(),
            A::OP_READ(x) => self.op_read(&x),
            A::OP_READDIR(x) => self.op_readdir(&x),
            A::OP_READLINK => self.op_readlink(),
            A::OP_REMOVE(x) => self.op_remove(&x),
            A::OP_RENAME(x) => self.op_rename(&x),
            A::OP_RENEW(x) => self.op_renew(&x),
            A::OP_RESTOREFH => self.op_restorefh(),
            A::OP_SAVEFH => self.op_savefh(),
            A::OP_SECINFO(x) => self.op_secinfo(&x),
            A::OP_SETATTR(x) => self.op_setattr(&x),
            A::OP_SETCLIENTID(x) => self.op_setclientid(&x),
            A::OP_SETCLIENTID_CONFIRM(x) => self.op_setclientid_confirm(&x),
            A::OP_VERIFY(x) => self.op_verify(&x.obj_attributes, true),
            A::OP_WRITE(x) => self.op_write(&x),
            A::OP_RELEASE_LOCKOWNER(x) => self.op_release_lockowner(&x),
            A::OP_BACKCHANNEL_CTL(x) => self.op_backchannel_ctl(&x),
            A::OP_BIND_CONN_TO_SESSION(x) => self.op_bind_conn(&x),
            A::OP_EXCHANGE_ID(x) => self.op_exchange_id(&x),
            A::OP_CREATE_SESSION(x) => self.op_create_session(&x),
            A::OP_DESTROY_SESSION(x) => self.op_destroy_session(&x),
            A::OP_FREE_STATEID(x) => self.op_free_stateid(&x),
            A::OP_SECINFO_NO_NAME(x) => self.op_secinfo_no_name(x),
            A::OP_SEQUENCE(x) => self.op_sequence(&x),
            A::OP_SET_SSV(x) => Self::op_set_ssv(&x),
            A::OP_TEST_STATEID(x) => self.op_test_stateid(&x),
            A::OP_DESTROY_CLIENTID(x) => self.op_destroy_clientid(&x),
            A::OP_RECLAIM_COMPLETE(x) => self.op_reclaim_complete(&x),
            A::OP_GET_DIR_DELEGATION(_)
            | A::OP_GETDEVICEINFO(_)
            | A::OP_GETDEVICELIST(_)
            | A::OP_LAYOUTCOMMIT(_)
            | A::OP_LAYOUTGET(_)
            | A::OP_LAYOUTRETURN(_)
            | A::OP_WANT_DELEGATION(_) => Res::Err(nfsstat4::NFS4ERR_NOTSUPP),
            A::OP_ILLEGAL => Res::Err(nfsstat4::NFS4ERR_OP_ILLEGAL),
        }
    }

    /// The pseudo file system this client sees.
    pub(super) fn tree(&mut self) -> &Tree {
        if self.tree.is_none() {
            self.tree = Some(Tree::build(&*self.srv.exports, self.ctx.peer.ip()));
        }
        self.tree.get_or_insert_with(unreachable_tree)
    }

    /// Sets the current filehandle (the current stateid goes).
    pub(super) fn set_cur(&mut self, fh: Fh) {
        self.cur = fh;
        self.cur_sid = None;
    }

    /// The current filehandle, set.
    pub(super) fn need(&self) -> R<Fh> {
        match &self.cur {
            Fh::None => Err(nfsstat4::NFS4ERR_NOFILEHANDLE),
            f => Ok(f.clone()),
        }
    }

    /// The current filehandle, an export's object; a pseudo directory
    /// is `pseudo`.
    pub(super) fn real(&self, pseudo: nfsstat4) -> R<(Req, FileHandle)> {
        match self.need()? {
            Fh::Real(r, h) => Ok((r, h)),
            _ => Err(pseudo),
        }
    }

    /// The handle's bytes.
    pub(super) fn fh_bytes(fh: &Fh) -> Vec<u8> {
        match fh {
            Fh::None => Vec::new(),
            Fh::Pseudo(id) => pseudo::handle(*id).to_vec(),
            Fh::Real(_, h) => h.to_bytes().to_vec(),
        }
    }

    /// A pseudo directory's attributes.
    pub(super) fn pseudo_attr(&self, id: u64) -> Attr {
        let t = self
            .srv
            .instance
            .started()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let at = Time {
            secs: i64::try_from(t.as_secs()).unwrap_or(0),
            nsecs: t.subsec_nanos(),
        };
        Attr {
            ftype: FileType::Dir,
            mode: 0o555,
            nlink: 2,
            uid: 0,
            gid: 0,
            size: 0,
            used: 0,
            rdev: (0, 0),
            fsid: 0,
            fileid: id,
            atime: at,
            mtime: at,
            ctime: at,
            change: u64::from(self.srv.boot),
            owner: "QSYS".into(),
            group: None,
            ccsid: 1208,
        }
    }

    /// The attributes of an object.
    pub(super) fn attr_of(&self, fh: &Fh) -> R<Attr> {
        match fh {
            Fh::None => Err(nfsstat4::NFS4ERR_NOFILEHANDLE),
            Fh::Pseudo(id) => Ok(self.pseudo_attr(*id)),
            Fh::Real(r, h) => self.vfs.getattr(r, h).map_err(status),
        }
    }

    /// The `want` attributes of `fh`, its attributes `a` known.
    pub(super) fn fattr(&mut self, fh: &Fh, a: &Attr, want: &Bits) -> R<fattr4> {
        let bytes = Self::fh_bytes(fh);
        let (fsid, mounted_on) = match fh {
            Fh::Real(r, h) => {
                let mut on = a.fileid;
                if h.fid == r.export.root_id() {
                    let id = r.export.id();
                    if let Some(p) = self.tree().place_of(id).filter(|p| *p != 0) {
                        on = p;
                    }
                }
                ((a.fsid >> 32, a.fsid & 0xffff_ffff), on)
            }
            _ => (pseudo::FSID, a.fileid),
        };
        let v = View {
            attr: a,
            fh: &bytes,
            fsid,
            mounted_on,
        };
        let env = Env {
            minor: self.minor,
            lease: u32::try_from(self.srv.cfg.lease.as_secs()).unwrap_or(90),
            max_io: u64::from(self.srv.cfg.max_io),
            reg: &self.srv.registry,
        };
        let mut x = Ex {
            vfs: self.vfs,
            obj: match fh {
                Fh::Real(r, h) => Some((r, h)),
                _ => None,
            },
        };
        get::encode(want, &v, &mut x, &env)
    }

    /// The root of export `e` as a filehandle.
    pub(super) fn export_root(&self, e: &std::sync::Arc<dyn crate::server::ExportView>) -> R<Fh> {
        let r = self
            .gate()
            .req(self.ctx, std::sync::Arc::clone(e), 4)
            .map_err(status)?;
        let n = self.vfs.root(&r).map_err(status)?;
        Ok(Fh::Real(r, n.fh))
    }

    pub(super) fn gate(&self) -> crate::server::Gate {
        crate::server::Gate::new(std::sync::Arc::clone(&self.srv.exports))
    }

    /// What a pseudo directory's entry is as a filehandle.
    pub(super) fn child_fh(&mut self, c: &Child) -> R<Fh> {
        match c {
            Child::Node(k) => Ok(Fh::Pseudo(self.tree().nodes[*k].id)),
            Child::Export(e) => self.export_root(e),
        }
    }

    /// The directory's change value.
    pub(super) fn change_of(&self, r: &Req, h: &FileHandle) -> u64 {
        self.vfs.getattr(r, h).map_or(0, |a| a.change)
    }

    /// 4.1's current stateid in place of the special one.
    pub(super) fn subst(&self, s: &stateid4) -> R<stateid4> {
        if self.minor >= 1 {
            if is_current(s) {
                return self.cur_sid.clone().ok_or(nfsstat4::NFS4ERR_BAD_STATEID);
            }
            if is_invalid(s) {
                return Err(nfsstat4::NFS4ERR_BAD_STATEID);
            }
        }
        Ok(s.clone())
    }

    /// The number of a stateid that is not special, checked: its start,
    /// whether it is gone, its client, its seqid. Its client's lease is
    /// renewed.
    pub(super) fn find_sid(&self, t: &mut Tables, s: &stateid4) -> R<u64> {
        let (boot, id) = split_other(&s.other);
        if boot != self.srv.boot {
            return Err(nfsstat4::NFS4ERR_STALE_STATEID);
        }
        if let Some((why, client)) = t.gone.get(&id) {
            if self.minor >= 1 && self.client != Some(*client) {
                return Err(nfsstat4::NFS4ERR_BAD_STATEID);
            }
            return Err(match why {
                Gone::DelegRevoked if self.minor >= 1 => nfsstat4::NFS4ERR_DELEG_REVOKED,
                Gone::Expired | Gone::DelegRevoked => nfsstat4::NFS4ERR_EXPIRED,
                Gone::AdminRevoked => nfsstat4::NFS4ERR_ADMIN_REVOKED,
            });
        }
        let Some(sid) = t.sids.get(&id) else {
            return Err(nfsstat4::NFS4ERR_BAD_STATEID);
        };
        if self.minor >= 1 && self.client != Some(sid.client) {
            return Err(nfsstat4::NFS4ERR_BAD_STATEID);
        }
        if !(self.minor >= 1 && s.seqid == 0) {
            if s.seqid > sid.seq {
                return Err(nfsstat4::NFS4ERR_BAD_STATEID);
            }
            if s.seqid < sid.seq {
                return Err(nfsstat4::NFS4ERR_OLD_STATEID);
            }
        }
        let client = sid.client;
        if let Some(c) = t.clients.get_mut(&client) {
            c.renewed = self.now;
        }
        Ok(id)
    }

    /// 4.0: what `seqid` is for `who` (4.1 ignores seqids).
    pub(super) fn seq_begin(&self, t: &Tables, who: &Seq, seqid: u32) -> R<SeqGo> {
        if self.minor >= 1 {
            return Ok(SeqGo::New);
        }
        let (seq, last) = match who {
            Seq::Open(k) => match t.open_owners.get(k) {
                Some(o) => (o.seq, o.last.clone()),
                None => return Ok(SeqGo::New),
            },
            Seq::Lock(k) => match t.lock_owners.get(k) {
                Some(o) => (o.seq, o.last.clone()),
                None => return Ok(SeqGo::New),
            },
        };
        match seq {
            None => Ok(SeqGo::New),
            Some(s) if seqid == s.wrapping_add(1) => Ok(SeqGo::New),
            Some(s) if seqid == s => match last {
                Some(r) if r.seqid == seqid => Ok(SeqGo::Replay(r)),
                _ => Err(nfsstat4::NFS4ERR_BAD_SEQID),
            },
            Some(_) => Err(nfsstat4::NFS4ERR_BAD_SEQID),
        }
    }

    /// 4.0: the seqid taken and the reply kept for a retransmission.
    pub(super) fn seq_end(
        &self,
        t: &mut Tables,
        who: &Seq,
        seqid: u32,
        st: nfsstat4,
        bytes: &[u8],
        fh: Option<FileHandle>,
    ) {
        if self.minor >= 1 || super::state::keeps_seqid(st) {
            return;
        }
        let r = Replay {
            seqid,
            bytes: bytes.to_vec(),
            fh,
        };
        match who {
            Seq::Open(k) => {
                if let Some(o) = t.open_owners.get_mut(k) {
                    o.seq = Some(seqid);
                    o.last = Some(r);
                    o.used = Some(self.now);
                }
            }
            Seq::Lock(k) => {
                if let Some(o) = t.lock_owners.get_mut(k) {
                    o.seq = Some(seqid);
                    o.last = Some(r);
                }
            }
        }
    }

    /// A replayed reply: the kept bytes, and the filehandle it left.
    pub(super) fn replay(&mut self, r: Replay) -> Res {
        if let Some(h) = r.fh
            && let Ok((req, h)) = self.gate().open(self.ctx, &h.to_bytes(), 4)
        {
            self.set_cur(Fh::Real(req, h));
        }
        let st = r
            .bytes
            .get(4..8)
            .and_then(|b| nfsstat4::from_value(i32::from_be_bytes([b[0], b[1], b[2], b[3]])))
            .unwrap_or(nfsstat4::NFS4_OK);
        Res::Raw(st, r.bytes)
    }

    /// Whether the export of `r` may be changed by this client.
    pub(super) fn writable(r: &Req) -> R<()> {
        if crate::server::gate::writable(r) {
            Ok(())
        } else {
            Err(nfsstat4::NFS4ERR_ROFS)
        }
    }

    /// A name checked for this host.
    pub(super) fn check_name(&self, r: Option<&Req>, b: &[u8]) -> R<()> {
        let utf8 = r.is_none_or(|r| r.opts.path_ccsid == ccsid::conv::UTF8);
        super::conv::name(b, utf8, self.minor)
    }
}

/// A file system error as a status.
pub(super) fn vs(e: VfsError) -> nfsstat4 {
    status(e)
}

fn unreachable_tree() -> Tree {
    Tree {
        root: pseudo::Root::Pseudo,
        nodes: Vec::new(),
        places: Vec::new(),
        public: None,
    }
}
