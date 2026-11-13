// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the NFSv4 tests share: a server over the version 3 tests'
//! world, a clock moved by hand, stable records in memory, and clients
//! sending real `COMPOUND` messages through a dispatcher in memory.

#![allow(
    dead_code,
    clippy::doc_markdown,
    clippy::match_wildcard_for_single_variants
)]

use std::cell::Cell;
use std::sync::Arc;
use std::time::Duration;

use ifs::backend::mem::MemBackend;
use ifs::client::LocalIfs;
use nfs::registry::{MemIdMap, Registry};
use nfs::rpc::Credential;
use nfs::server::ifsvfs::IfsVfs;
use nfs::server::{Clock, ExportTable, ManualClock};
use nfs::types::nfs4::NFS4_PROGRAM::NFS_V4;
use nfs::types::nfs4::{self as t, nfs_argop4, nfs_resop4, nfsstat4, stateid4};
use nfs::v4::attr::Bits;
use nfs::v4::cb::{CbConnector, TcpConnector};
use nfs::v4::state::stable::MemStable;
use nfs::v4::{Config, Nfs34, Server};

use crate::srv3_common::{ANON_UID, Client, World};

/// The server job's file system.
pub type Fs = IfsVfs<LocalIfs<MemBackend>>;

/// A server and its world.
pub struct Env {
    /// The system.
    pub w: World,
    /// The clock.
    pub clock: Arc<ManualClock>,
    /// The stable records.
    pub stable: Arc<MemStable>,
    /// The server.
    pub srv: Arc<Server>,
    /// One thread's program.
    pub prog: Nfs34<Fs>,
    /// The callback connector.
    pub cb: Arc<dyn CbConnector>,
}

/// The identity map of the world's profiles.
pub fn ids() -> MemIdMap {
    MemIdMap {
        domain: "example.com".into(),
        users: vec![
            ("QSECOFR".into(), 0),
            ("BOB".into(), 100),
            ("ZED".into(), 102),
            ("ANN".into(), 103),
            ("QNFSANON".into(), ANON_UID),
        ],
        groups: vec![("DEV".into(), 500)],
    }
}

impl Env {
    /// A server over `w` (whose exports are already in place).
    pub fn new(w: World) -> Self {
        Self::with(
            w,
            Arc::new(ManualClock::default()),
            Arc::new(MemStable::new()),
            Config::default(),
        )
    }

    /// A server with these parts.
    pub fn with(w: World, clock: Arc<ManualClock>, stable: Arc<MemStable>, cfg: Config) -> Self {
        let cb: Arc<dyn CbConnector> = Arc::new(TcpConnector {
            cfg: nfs::rpc::ClientConfig {
                timeout: Duration::from_secs(2),
                retransmits: 0,
                ..nfs::rpc::ClientConfig::default()
            },
        });
        let reg = Arc::new(Registry::new(
            Arc::new(ids()),
            nfs::registry::TTL,
            clock.clone() as Arc<dyn Clock>,
        ));
        let srv = Server::new(
            cfg,
            Arc::clone(&w.exports) as Arc<dyn ExportTable>,
            Arc::clone(&w.instance),
            clock.clone() as Arc<dyn Clock>,
            reg,
            stable.clone(),
            Arc::clone(&cb),
        );
        let prog = srv.program(w.vfs());
        Self {
            w,
            clock,
            stable,
            srv,
            prog,
            cb,
        }
    }

    /// The server started again over the same volume and records.
    pub fn restart(&self) -> Self {
        Self::with(
            self.w.restarted(),
            Arc::new(ManualClock::default()),
            Arc::clone(&self.stable),
            self.srv.config().clone(),
        )
    }

    /// One round of the background work.
    pub fn tick(&self) {
        self.srv.tick(self.prog.vfs());
    }

    /// Moves the clock on.
    pub fn advance(&self, secs: u64) {
        self.clock.advance(Duration::from_secs(secs));
    }
}

/// A client: its RPC side, its minor version and, for 4.1, its session.
pub struct V4c {
    /// The RPC client.
    pub c: Client,
    /// 0 or 1.
    pub minor: u32,
    /// 4.0: the client ID; 4.1: too.
    pub clientid: Cell<u64>,
    /// 4.1: the session.
    pub session: Cell<[u8; 16]>,
    /// 4.1: slot 0's next sequence ID.
    pub seq: Cell<u32>,
    /// 4.0: the next open-owner seqid.
    pub oseq: Cell<u32>,
}

impl V4c {
    /// A client at `peer` calling as `uid`.
    pub fn new(peer: &str, uid: u32, minor: u32) -> Self {
        let mut c = Client::new(peer, uid);
        c.transport = nfs::rpc::Transport::Tcp;
        Self {
            c,
            minor,
            clientid: Cell::new(0),
            session: Cell::new([0; 16]),
            seq: Cell::new(1),
            oseq: Cell::new(1),
        }
    }

    /// A `COMPOUND` of `ops` at minor version `minor`, as it is.
    pub fn raw(&self, e: &Env, minor: u32, ops: Vec<nfs_argop4>) -> t::COMPOUND4res {
        let a = NFS_V4::Args::NFSPROC4_COMPOUND(t::COMPOUND4args {
            tag: t::utf8string(b"test".to_vec()),
            minorversion: minor,
            argarray: ops,
        });
        match self.c.call::<NFS_V4::Args, NFS_V4::Res>(&e.prog, &a) {
            NFS_V4::Res::NFSPROC4_COMPOUND(r) => r,
            r => panic!("{r:?}"),
        }
    }

    /// `ops` in a compound (4.1: after this client's `SEQUENCE`).
    pub fn run(&self, e: &Env, mut ops: Vec<nfs_argop4>) -> t::COMPOUND4res {
        if self.minor >= 1 {
            ops.insert(0, self.sequence(false));
        }
        let mut r = self.raw(e, self.minor, ops);
        if self.minor >= 1 {
            assert!(!r.resarray.is_empty());
            r.resarray.remove(0);
        }
        r
    }

    /// The next `SEQUENCE` on slot 0.
    pub fn sequence(&self, cachethis: bool) -> nfs_argop4 {
        let s = self.seq.get();
        self.seq.set(s + 1);
        nfs_argop4::OP_SEQUENCE(t::SEQUENCE4args {
            sa_sessionid: self.session.get(),
            sa_sequenceid: s,
            sa_slotid: 0,
            sa_highest_slotid: 0,
            sa_cachethis: cachethis,
        })
    }

    /// 4.0: `SETCLIENTID` and `SETCLIENTID_CONFIRM` as `id` with
    /// callback address `cb` (`tcp`).
    pub fn setclientid(&self, e: &Env, id: &str, cb: &str) -> u64 {
        let r = self.raw(
            e,
            0,
            vec![nfs_argop4::OP_SETCLIENTID(t::SETCLIENTID4args {
                client: t::nfs_client_id4 {
                    verifier: [1; 8],
                    id: id.as_bytes().to_vec(),
                },
                callback: t::cb_client4 {
                    cb_program: 0x4000_0001,
                    cb_location: t::netaddr4 {
                        na_r_netid: b"tcp".to_vec(),
                        na_r_addr: cb.as_bytes().to_vec(),
                    },
                },
                callback_ident: 7,
            })],
        );
        let nfs_resop4::OP_SETCLIENTID(t::SETCLIENTID4res::NFS4_OK(ok)) = &r.resarray[0] else {
            panic!("{r:?}");
        };
        let r = self.raw(
            e,
            0,
            vec![nfs_argop4::OP_SETCLIENTID_CONFIRM(
                t::SETCLIENTID_CONFIRM4args {
                    clientid: ok.clientid,
                    setclientid_confirm: ok.setclientid_confirm,
                },
            )],
        );
        assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
        self.clientid.set(ok.clientid);
        ok.clientid
    }

    /// 4.1: `EXCHANGE_ID` as `id` and `CREATE_SESSION` (`flags`).
    pub fn session(&self, e: &Env, id: &str, flags: u32) -> [u8; 16] {
        let r = self.raw(e, 1, vec![exchange_id(id, [2; 8], 0)]);
        let nfs_resop4::OP_EXCHANGE_ID(t::EXCHANGE_ID4res::NFS4_OK(ok)) = &r.resarray[0] else {
            panic!("{r:?}");
        };
        self.clientid.set(ok.eir_clientid);
        let r = self.raw(
            e,
            1,
            vec![create_session(ok.eir_clientid, ok.eir_sequenceid, flags)],
        );
        let nfs_resop4::OP_CREATE_SESSION(t::CREATE_SESSION4res::NFS4_OK(cs)) = &r.resarray[0]
        else {
            panic!("{r:?}");
        };
        self.session.set(cs.csr_sessionid);
        self.seq.set(1);
        cs.csr_sessionid
    }

    /// An open owner of this client.
    pub fn owner(&self, name: &str) -> t::open_owner4 {
        t::state_owner4 {
            clientid: self.clientid.get(),
            owner: name.as_bytes().to_vec(),
        }
    }

    /// The next open-owner seqid (4.0).
    pub fn next_oseq(&self) -> u32 {
        let s = self.oseq.get();
        self.oseq.set(s + 1);
        s
    }

    /// `OPEN` of `name` in the current directory; `create` unchecked
    /// with mode 0644.
    pub fn open(
        &self,
        owner: &str,
        name: &str,
        create: bool,
        access: u32,
        deny: u32,
    ) -> nfs_argop4 {
        open_args(
            self.next_oseq(),
            self.owner(owner),
            name,
            create,
            access,
            deny,
        )
    }
}

/// `EXCHANGE_ID`.
pub fn exchange_id(id: &str, verf: [u8; 8], flags: u32) -> nfs_argop4 {
    nfs_argop4::OP_EXCHANGE_ID(t::EXCHANGE_ID4args {
        eia_clientowner: t::client_owner4 {
            co_verifier: verf,
            co_ownerid: id.as_bytes().to_vec(),
        },
        eia_flags: flags,
        eia_state_protect: t::state_protect4_a::SP4_NONE,
        eia_client_impl_id: Vec::new(),
    })
}

/// Channel attributes a client asks for.
pub fn chan(reqs: u32) -> t::channel_attrs4 {
    t::channel_attrs4 {
        ca_headerpadsize: 0,
        ca_maxrequestsize: 1 << 20,
        ca_maxresponsesize: 1 << 20,
        ca_maxresponsesize_cached: 8192,
        ca_maxoperations: 16,
        ca_maxrequests: reqs,
        ca_rdma_ird: Vec::new(),
    }
}

/// `CREATE_SESSION`.
pub fn create_session(clientid: u64, seq: u32, flags: u32) -> nfs_argop4 {
    nfs_argop4::OP_CREATE_SESSION(t::CREATE_SESSION4args {
        csa_clientid: clientid,
        csa_sequence: seq,
        csa_flags: flags,
        csa_fore_chan_attrs: chan(4),
        csa_back_chan_attrs: chan(1),
        csa_cb_program: 0x4000_0002,
        csa_sec_parms: vec![t::callback_sec_parms4::AUTH_NONE],
    })
}

/// An empty `fattr4`.
pub fn no_attrs() -> t::fattr4 {
    t::fattr4 {
        attrmask: t::bitmap4(Vec::new()),
        attr_vals: t::attrlist4(Vec::new()),
    }
}

/// A `fattr4` with a mode.
pub fn mode_attrs(m: u32) -> t::fattr4 {
    t::fattr4 {
        attrmask: Bits::of(&[t::FATTR4_MODE]).to_bitmap(),
        attr_vals: t::attrlist4(m.to_be_bytes().to_vec()),
    }
}

/// `OPEN` arguments.
pub fn open_args(
    seqid: u32,
    owner: t::open_owner4,
    name: &str,
    create: bool,
    access: u32,
    deny: u32,
) -> nfs_argop4 {
    nfs_argop4::OP_OPEN(t::OPEN4args {
        seqid,
        share_access: access,
        share_deny: deny,
        owner,
        openhow: if create {
            t::openflag4::OPEN4_CREATE(t::createhow4::UNCHECKED4(mode_attrs(0o644)))
        } else {
            t::openflag4::default(t::opentype4::OPEN4_NOCREATE)
        },
        claim: t::open_claim4::CLAIM_NULL(t::utf8string(name.as_bytes().to_vec())),
    })
}

/// `PUTROOTFH`.
pub fn rootfh() -> nfs_argop4 {
    nfs_argop4::OP_PUTROOTFH
}

/// `PUTFH`.
pub fn putfh(b: &[u8]) -> nfs_argop4 {
    nfs_argop4::OP_PUTFH(t::PUTFH4args {
        object: t::nfs_fh4(b.to_vec()),
    })
}

/// `LOOKUP`.
pub fn lookup(n: &str) -> nfs_argop4 {
    nfs_argop4::OP_LOOKUP(t::LOOKUP4args {
        objname: t::utf8string(n.as_bytes().to_vec()),
    })
}

/// `GETFH`.
pub fn getfh() -> nfs_argop4 {
    nfs_argop4::OP_GETFH
}

/// `GETATTR` of `bits`.
pub fn getattr(bits: &[u32]) -> nfs_argop4 {
    nfs_argop4::OP_GETATTR(t::GETATTR4args {
        attr_request: Bits::of(bits).to_bitmap(),
    })
}

/// `READ`.
pub fn read(s: &stateid4, off: u64, count: u32) -> nfs_argop4 {
    nfs_argop4::OP_READ(t::READ4args {
        stateid: s.clone(),
        offset: off,
        count,
    })
}

/// `WRITE` (file sync).
pub fn write(s: &stateid4, off: u64, data: &[u8]) -> nfs_argop4 {
    nfs_argop4::OP_WRITE(t::WRITE4args {
        stateid: s.clone(),
        offset: off,
        stable: t::stable_how4::FILE_SYNC4,
        data: data.to_vec(),
    })
}

/// `CLOSE`.
pub fn close(seqid: u32, s: &stateid4) -> nfs_argop4 {
    nfs_argop4::OP_CLOSE(t::CLOSE4args {
        seqid,
        open_stateid: s.clone(),
    })
}

/// The anonymous stateid.
pub fn anon() -> stateid4 {
    stateid4 {
        seqid: 0,
        other: [0; 12],
    }
}

/// The status of each result.
pub fn statuses(r: &t::COMPOUND4res) -> Vec<nfsstat4> {
    r.resarray.iter().map(status_of).collect()
}

/// A result's status, as its encoding starts.
pub fn status_of(r: &nfs_resop4) -> nfsstat4 {
    let b = nfs::xdr::to_bytes(r).unwrap();
    nfsstat4::from_value(i32::from_be_bytes([b[4], b[5], b[6], b[7]])).unwrap()
}

/// The handle `GETFH` answered.
pub fn fh_of(r: &t::COMPOUND4res) -> Vec<u8> {
    r.resarray
        .iter()
        .find_map(|x| match x {
            nfs_resop4::OP_GETFH(t::GETFH4res::NFS4_OK(ok)) => Some(ok.object.0.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no handle in {r:?}"))
}

/// The open stateid `OPEN` answered, and its result.
pub fn opened(r: &t::COMPOUND4res) -> t::OPEN4resok {
    r.resarray
        .iter()
        .find_map(|x| match x {
            nfs_resop4::OP_OPEN(t::OPEN4res::NFS4_OK(ok)) => Some(ok.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no open in {r:?}"))
}

/// The attributes `GETATTR` answered.
pub fn attrs_of(r: &t::COMPOUND4res) -> t::fattr4 {
    r.resarray
        .iter()
        .find_map(|x| match x {
            nfs_resop4::OP_GETATTR(t::GETATTR4res::NFS4_OK(ok)) => Some(ok.obj_attributes.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no attributes in {r:?}"))
}

/// An `AUTH_SYS` credential of `uid`.
pub fn cred(uid: u32) -> Credential {
    Credential::sys(b"client", uid, 100, &[100])
}
