// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the server tests share: the IFS over a `MemBackend` with
//! profiles, exports of it, the server's programs over `IfsVfs`, and a
//! client that sends real RPC messages through a dispatcher in memory.

#![allow(dead_code)]

use std::cell::Cell;
use std::net::SocketAddr;
use std::sync::Arc;

use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::client::{IfsClient, LocalIfs};
use ifs::ops::audit::{FileId, MemAudit};
use ifs::ops::{Config, Ifs, Owner};
use ifs::vfs::qsys::{MemQsys, MemberData};
use nfs::mount::Mountd;
use nfs::mount::list::{MemMountList, MountList};
use nfs::rpc::msg::encode_call;
use nfs::rpc::{Credential, Dispatcher, LocalProgram, Transport, auth};
use nfs::server::ifsvfs::{self, IfsVfs};
use nfs::server::{Gate, Instance, MemExport, MemExports};
use nfs::types::nfs3::{self as t3, NFS_PROGRAM::NFS_V3};
use nfs::types::rpc::call_body;
use nfs::v3::Nfs3;
use nfs::xdr::{Decoder, Encoder, ProcBody};
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::profile::spcaut::SpecialAuth;

/// `QNFSANON`'s UID, as IBM's.
pub const ANON_UID: u32 = 4_294_770_710;

/// A profile's principal.
pub fn user(name: &str) -> Principal {
    let mut p = Principal {
        user: name.into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    };
    match name {
        "QSECOFR" => {
            p.spcaut = SpecialAuth::ALLOBJ.0
                | SpecialAuth::IOSYSCFG.0
                | SpecialAuth::AUDIT.0
                | SpecialAuth::SECADM.0;
        }
        "ANN" => p.groups = vec!["DEV".into()],
        _ => {}
    }
    p
}

/// `QSECOFR` 0, `BOB` 100, `ANN` 101 (in `DEV`), `ZED` 102, `QNFSANON`.
pub fn env() -> MemEnv {
    MemEnv::default()
        .user("QSECOFR", 0)
        .user("QSYS", 101)
        .user("BOB", 100)
        .user("ANN", 103)
        .user("ZED", 102)
        .user("QNFSANON", ANON_UID)
        .user("QDFTOWN", 104)
        .group("DEV", 200, 500)
        .principal(user("QSECOFR"))
        .principal(user("ANN"))
}

/// A `/QSYS.LIB` with a library, a source file of one member and a user space.
pub fn qsys() -> MemQsys {
    let q = MemQsys::new(Auth::USE);
    q.add_lib("QGPL");
    q.add_obj("QGPL", "QCLSRC", "*FILE", "PF-SRC", "QSYS");
    q.add_member(
        "QGPL",
        "QCLSRC",
        "HELLO",
        MemberData {
            records: vec![
                ccsid::to_ebcdic(&format!("{:<20}", "PGM")).unwrap(),
                ccsid::to_ebcdic(&format!("{:<20}", "ENDPGM")).unwrap(),
            ],
            record_len: 20,
            source: true,
            ccsid: 37,
        },
    );
    q.add_obj("QGPL", "SPACE", "*USRSPC", "", "QSYS");
    q
}

/// The system the server serves.
pub struct World {
    /// The volume (kept for a restart).
    pub backend: MemBackend,
    /// The file system.
    pub ifs: Arc<Ifs<MemBackend>>,
    /// The profiles.
    pub env: MemEnv,
    /// The exports.
    pub exports: Arc<MemExports>,
    /// The server's start.
    pub instance: Arc<Instance>,
    /// The mount list.
    pub mounts: Arc<MemMountList>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// A fresh system.
    pub fn new() -> Self {
        let backend = MemBackend::new();
        let env = env();
        let ifs = Ifs::new(backend.clone(), Box::new(qsys()), Config::default());
        ifs.ensure_layout(&env).unwrap();
        Self {
            backend,
            ifs: Arc::new(ifs),
            env,
            exports: Arc::new(MemExports::new()),
            instance: Arc::new(Instance::new()),
            mounts: Arc::new(MemMountList::new()),
        }
    }

    /// The same volume after a restart of the server and the system:
    /// the exports kept, a new start.
    pub fn restarted(&self) -> Self {
        let ifs = Ifs::new(self.backend.clone(), Box::new(qsys()), Config::default());
        Self {
            backend: self.backend.clone(),
            ifs: Arc::new(ifs),
            env: self.env.clone(),
            exports: Arc::clone(&self.exports),
            instance: Arc::new(Instance::new()),
            mounts: Arc::new(MemMountList::new()),
        }
    }

    /// A job's client: `QSECOFR`, owner `owner`.
    pub fn job(&self, owner: u64) -> LocalIfs<MemBackend> {
        LocalIfs {
            ifs: Arc::clone(&self.ifs),
            env: Arc::new(self.env.clone()),
            audit: Arc::new(MemAudit::default()),
            principal: user("QSECOFR"),
            owner: Owner(owner),
            job_ccsid: 37,
        }
    }

    /// The server job's file system (owner 1).
    pub fn vfs(&self) -> IfsVfs<LocalIfs<MemBackend>> {
        IfsVfs::new(self.job(1), Arc::clone(&self.instance), ifsvfs::Config::default())
    }

    /// The file ID of `path`.
    pub fn fid(&self, path: &str) -> FileId {
        let s = self.job(9).stat(path, false).unwrap();
        FileId {
            fs: s.vfs,
            ino: s.ino,
            generation: s.ino_gen,
        }
    }

    /// An export of `path` (made, `*PUBLIC *RWX`, when missing) with ID `id`.
    pub fn export(&self, id: u32, path: &str) -> MemExport {
        let j = self.job(9);
        if j.stat(path, false).is_err() {
            j.mkdir(path, 0o777).unwrap();
            j.chmod(path, 0o777).unwrap();
        }
        MemExport::new(id, path, self.fid(path))
    }

    /// Puts `e` in the table.
    pub fn put(&self, e: MemExport) {
        self.exports.put(e);
    }

    /// The gate over the table.
    pub fn gate(&self) -> Gate {
        Gate::new(Arc::clone(&self.exports) as Arc<dyn nfs::server::ExportTable>)
    }

    /// NFS version 3 over a fresh `IfsVfs`.
    pub fn nfs3(&self) -> Nfs3<IfsVfs<LocalIfs<MemBackend>>> {
        Nfs3::new(self.vfs(), self.gate())
    }

    /// MOUNT over a fresh `IfsVfs`.
    pub fn mountd(&self) -> Mountd<IfsVfs<LocalIfs<MemBackend>>> {
        Mountd::new(
            self.vfs(),
            self.gate(),
            Arc::clone(&self.mounts) as Arc<dyn MountList>,
        )
    }

    /// The handle of `path` in export `e` (as made by the server).
    pub fn handle(&self, e: u32, path: &str) -> Vec<u8> {
        let x = self.exports.by_id_mem(e);
        nfs::server::FileHandle {
            export: e,
            fid: self.fid(path),
            root_ino: x.root.ino,
        }
        .to_bytes()
        .to_vec()
    }

    /// Writes a whole file as `QSECOFR`.
    pub fn write_file(&self, path: &str, data: &[u8]) {
        let j = self.job(9);
        let fd = j
            .open(
                path,
                ifs::ops::OpenArgs::new(
                    ifs::ops::flags::O_WRONLY | ifs::ops::flags::O_CREAT | ifs::ops::flags::O_TRUNC,
                )
                .mode(0o666),
            )
            .unwrap();
        ifs::client::write_all(&j, fd, data).unwrap();
        j.close(fd).unwrap();
    }

    /// Reads a whole file as `QSECOFR`.
    pub fn read_file(&self, path: &str) -> Vec<u8> {
        let j = self.job(9);
        let fd = j
            .open(path, ifs::ops::OpenArgs::new(ifs::ops::flags::O_RDONLY))
            .unwrap();
        let d = ifs::client::read_all(&j, fd).unwrap();
        j.close(fd).unwrap();
        d
    }
}

/// Lookups in the table by ID, for tests.
pub trait ById {
    /// The export with ID `id`.
    fn by_id_mem(&self, id: u32) -> MemExport;
}

impl ById for MemExports {
    fn by_id_mem(&self, id: u32) -> MemExport {
        use nfs::server::ExportTable;
        let e = self.by_id(id).unwrap();
        MemExport::new(e.id(), e.path(), e.root_id())
    }
}

/// A client sending RPC messages through a dispatcher in memory.
pub struct Client {
    /// The dispatcher (with the cache the daemon would set).
    pub d: Dispatcher,
    /// The client's address.
    pub peer: SocketAddr,
    /// Its credential.
    pub cred: Credential,
    /// How it arrives.
    pub transport: Transport,
    xid: Cell<u32>,
}

impl Client {
    /// A client at `peer` calling as `uid` (`AUTH_SYS`), through a
    /// dispatcher with NFS version 3's cache.
    pub fn new(peer: &str, uid: u32) -> Self {
        let mut d = Dispatcher::new();
        d.set_cache(nfs::server::run::nfs3_cache());
        Self {
            d,
            peer: peer.parse().unwrap(),
            cred: Credential::sys(b"client1", uid, 100, &[100]),
            transport: Transport::Udp,
            xid: Cell::new(1000),
        }
    }

    /// The next XID.
    pub fn next_xid(&self) -> u32 {
        let x = self.xid.get() + 1;
        self.xid.set(x);
        x
    }

    /// The call message of `args`, with XID `xid`.
    pub fn message<A: ProcBody>(&self, xid: u32, args: &A) -> Vec<u8> {
        let mut e = Encoder::new();
        args.encode_body(&mut e).unwrap();
        let body = call_body {
            rpcvers: 2,
            prog: A::PROGRAM,
            vers: A::VERSION,
            proc: args.procedure(),
            cred: self.cred.to_opaque().unwrap_or_else(|_| auth::none()),
            verf: auth::none(),
        };
        encode_call(xid, &body, e.as_bytes()).unwrap()
    }

    /// Sends `msg` to `p`: the reply.
    pub fn send(&self, p: &dyn LocalProgram, msg: &[u8]) -> Option<Vec<u8>> {
        self.d.handle_with(msg, self.peer, self.transport, &[p])
    }

    /// Calls `args` with XID `xid`, decoding `R`.
    pub fn call_xid<A: ProcBody, R: ProcBody>(&self, p: &dyn LocalProgram, xid: u32, args: &A) -> R {
        let reply = self.send(p, &self.message(xid, args)).expect("a reply");
        let res = nfs::rpc::client::results(&reply).expect("accepted");
        let mut d = Decoder::new(&res);
        let r = R::decode_body(args.procedure(), &mut d).unwrap().unwrap();
        assert!(d.is_empty(), "bytes left in the results");
        r
    }

    /// Calls `args`, decoding `R`.
    pub fn call<A: ProcBody, R: ProcBody>(&self, p: &dyn LocalProgram, args: &A) -> R {
        self.call_xid(p, self.next_xid(), args)
    }

    /// Calls an NFS version 3 procedure.
    pub fn v3(&self, p: &dyn LocalProgram, args: &NFS_V3::Args) -> NFS_V3::Res {
        self.call(p, args)
    }
}

/// `nfs_fh3` of bytes.
pub fn fh(b: &[u8]) -> t3::nfs_fh3 {
    t3::nfs_fh3 { data: b.to_vec() }
}

/// `diropargs3`.
pub fn dirop(dir: &[u8], name: &str) -> t3::diropargs3 {
    t3::diropargs3 {
        dir: fh(dir),
        name: t3::filename3(name.as_bytes().to_vec()),
    }
}

/// An empty `sattr3`.
pub fn no_sattr() -> t3::sattr3 {
    t3::sattr3 {
        mode: None,
        uid: None,
        gid: None,
        size: None,
        atime: t3::set_atime::default(t3::time_how::DONT_CHANGE),
        mtime: t3::set_mtime::default(t3::time_how::DONT_CHANGE),
    }
}

/// A `sattr3` with a mode.
pub fn mode(m: u32) -> t3::sattr3 {
    t3::sattr3 {
        mode: Some(m),
        ..no_sattr()
    }
}

/// NFS version 3 calls made short.
pub struct V3<'a> {
    /// The program.
    pub p: &'a dyn LocalProgram,
    /// The client.
    pub c: &'a Client,
}

impl V3<'_> {
    /// `LOOKUP`: the handle, or the status.
    pub fn lookup(&self, dir: &[u8], name: &str) -> Result<Vec<u8>, t3::nfsstat3> {
        match self.c.v3(self.p, &NFS_V3::Args::NFSPROC3_LOOKUP(t3::LOOKUP3args { what: dirop(dir, name) })) {
            NFS_V3::Res::NFSPROC3_LOOKUP(t3::LOOKUP3res::NFS3_OK(ok)) => Ok(ok.object.data),
            NFS_V3::Res::NFSPROC3_LOOKUP(t3::LOOKUP3res::default(s, _)) => Err(s),
            r => panic!("{r:?}"),
        }
    }

    /// `GETATTR`.
    pub fn getattr(&self, f: &[u8]) -> Result<t3::fattr3, t3::nfsstat3> {
        match self.c.v3(self.p, &NFS_V3::Args::NFSPROC3_GETATTR(t3::GETATTR3args { object: fh(f) })) {
            NFS_V3::Res::NFSPROC3_GETATTR(t3::GETATTR3res::NFS3_OK(ok)) => Ok(ok.obj_attributes),
            NFS_V3::Res::NFSPROC3_GETATTR(t3::GETATTR3res::default(s)) => Err(s),
            r => panic!("{r:?}"),
        }
    }

    /// `CREATE` (`how`): the handle.
    pub fn create_how(&self, dir: &[u8], name: &str, how: t3::createhow3) -> Result<Vec<u8>, t3::nfsstat3> {
        let a = t3::CREATE3args {
            r#where: dirop(dir, name),
            how,
        };
        match self.c.v3(self.p, &NFS_V3::Args::NFSPROC3_CREATE(a)) {
            NFS_V3::Res::NFSPROC3_CREATE(t3::CREATE3res::NFS3_OK(ok)) => Ok(ok.obj.unwrap().data),
            NFS_V3::Res::NFSPROC3_CREATE(t3::CREATE3res::default(s, _)) => Err(s),
            r => panic!("{r:?}"),
        }
    }

    /// `CREATE` unchecked, mode 0644.
    pub fn create(&self, dir: &[u8], name: &str) -> Result<Vec<u8>, t3::nfsstat3> {
        self.create_how(dir, name, t3::createhow3::UNCHECKED(mode(0o644)))
    }

    /// `MKDIR`.
    pub fn mkdir(&self, dir: &[u8], name: &str) -> Result<Vec<u8>, t3::nfsstat3> {
        let a = t3::MKDIR3args {
            r#where: dirop(dir, name),
            attributes: mode(0o755),
        };
        match self.c.v3(self.p, &NFS_V3::Args::NFSPROC3_MKDIR(a)) {
            NFS_V3::Res::NFSPROC3_MKDIR(t3::MKDIR3res::NFS3_OK(ok)) => Ok(ok.obj.unwrap().data),
            NFS_V3::Res::NFSPROC3_MKDIR(t3::MKDIR3res::default(s, _)) => Err(s),
            r => panic!("{r:?}"),
        }
    }

    /// `WRITE`: count, committed, verifier.
    pub fn write(&self, f: &[u8], off: u64, data: &[u8], stable: t3::stable_how) -> Result<t3::WRITE3resok, t3::nfsstat3> {
        let a = t3::WRITE3args {
            file: fh(f),
            offset: off,
            count: u32::try_from(data.len()).unwrap(),
            stable,
            data: data.to_vec(),
        };
        match self.c.v3(self.p, &NFS_V3::Args::NFSPROC3_WRITE(a)) {
            NFS_V3::Res::NFSPROC3_WRITE(t3::WRITE3res::NFS3_OK(ok)) => Ok(ok),
            NFS_V3::Res::NFSPROC3_WRITE(t3::WRITE3res::default(s, _)) => Err(s),
            r => panic!("{r:?}"),
        }
    }

    /// `READ`: data and eof.
    pub fn read(&self, f: &[u8], off: u64, count: u32) -> Result<(Vec<u8>, bool), t3::nfsstat3> {
        let a = t3::READ3args {
            file: fh(f),
            offset: off,
            count,
        };
        match self.c.v3(self.p, &NFS_V3::Args::NFSPROC3_READ(a)) {
            NFS_V3::Res::NFSPROC3_READ(t3::READ3res::NFS3_OK(ok)) => Ok((ok.data, ok.eof)),
            NFS_V3::Res::NFSPROC3_READ(t3::READ3res::default(s, _)) => Err(s),
            r => panic!("{r:?}"),
        }
    }

    /// `REMOVE`.
    pub fn remove(&self, dir: &[u8], name: &str) -> Result<(), t3::nfsstat3> {
        match self.c.v3(self.p, &NFS_V3::Args::NFSPROC3_REMOVE(t3::REMOVE3args { object: dirop(dir, name) })) {
            NFS_V3::Res::NFSPROC3_REMOVE(t3::REMOVE3res::NFS3_OK(_)) => Ok(()),
            NFS_V3::Res::NFSPROC3_REMOVE(t3::REMOVE3res::default(s, _)) => Err(s),
            r => panic!("{r:?}"),
        }
    }

    /// `SETATTR` without a guard.
    pub fn setattr(&self, f: &[u8], s: t3::sattr3) -> Result<(), t3::nfsstat3> {
        let a = t3::SETATTR3args {
            object: fh(f),
            new_attributes: s,
            guard: None,
        };
        match self.c.v3(self.p, &NFS_V3::Args::NFSPROC3_SETATTR(a)) {
            NFS_V3::Res::NFSPROC3_SETATTR(t3::SETATTR3res::NFS3_OK(_)) => Ok(()),
            NFS_V3::Res::NFSPROC3_SETATTR(t3::SETATTR3res::default(s, _)) => Err(s),
            r => panic!("{r:?}"),
        }
    }
}
