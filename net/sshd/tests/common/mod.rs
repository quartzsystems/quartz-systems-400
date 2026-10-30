// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A system in memory for the server: profiles in a security store
//! (sign-on with `QMAXSIGN` and `PW` audit), an IFS with `/QSYS.LIB`,
//! QSH over a `MemSys` per session, and a client wired to the server
//! through an in-memory byte stream.

#![allow(dead_code, clippy::match_same_arms)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use blockdev::{BlockDev, MemBlockDev};
use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::client::{IfsClient, LocalIfs};
use ifs::ops::audit::MemAudit;
use ifs::ops::{Config as IfsConfig, Ctx, Ifs, Owner};
use ifs::vfs::qsys::{MemQsys, MemberData};
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::cmd::{Env, Keywords, usrprf};
use sec::profile::spcaut::SpecialAuth;
use sec::profile::{objects, shipped};
use sec::signon::{Attempt, Values};
use sshd::auth::{Account, Accounts, PasswordCheck, Peer};
use sshd::client::{Client, Options};
use sshd::config::paths;
use sshd::io::FileSystem;
use sshd::io::pipe::{PipeReader, PipeWriter, duplex};
use sshd::session::qsh_host::QshHost;
use sshd::session::transfer::FileTransfer;
use sshd::session::SessionRequest;
use sshd::{Deps, Server};
use store::obj::{ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};

/// Every test profile's password.
pub const PASSWORD: &str = "Secret123";
pub const NOW: u64 = 1_767_225_600;

fn sysval(n: &str) -> String {
    match n {
        "QSECURITY" => "40",
        "QPWDLVL" => "3",
        "QPWDMINLEN" => "6",
        "QPWDMAXLEN" => "128",
        "QPWDRULES" => "*PWDSYSVAL",
        "QPWDEXPITV" => "*NOMAX",
        "QMAXSIGN" => "3",
        "QMAXSGNACN" => "2",
        "QLMTSECOFR" => "0",
        "QDSPSGNINF" => "0",
        "QLMTDEVSSN" => "0",
        _ => "",
    }
    .into()
}

/// Profiles in a store, signed on through `sec::signon`.
pub struct StoreAccounts {
    store: Mutex<(Store, ObjectId)>,
    /// `PW` audit entries written so far.
    pub audit: Mutex<Vec<sec::audit::Event>>,
}

impl StoreAccounts {
    fn new(users: &[&str]) -> Self {
        let d = Arc::new(MemBlockDev::with_seed(16384, 7));
        let params = FormatParams {
            asp_number: 1,
            asp_uuid: [4; 16],
            unit_uuids: vec![[5; 16]],
            unit_serials: Vec::new(),
            wal_pages: 128,
            now: NOW,
        };
        let mut store = Store::format(
            vec![Box::new(d) as Box<dyn BlockDev>],
            &params,
            StoreConfig::default(),
        )
        .unwrap();
        let mut t = store.begin().unwrap();
        let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
        for lib in ["QGPL", "QUSRSYS"] {
            ops::create(
                &mut t,
                &store::obj::CreateParams {
                    library: qsys,
                    name: qsname::Name::parse(lib).unwrap(),
                    type_code: types::LIB,
                    attribute: String::new(),
                    text: String::new(),
                    owner: "QSECOFR".into(),
                    system: "QS400".into(),
                    now: NOW,
                    by_system: true,
                },
            )
            .unwrap();
        }
        shipped::install(&mut t, qsys, 40, 3, NOW).unwrap();
        let secofr = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
        let env = Env {
            qsys,
            actor: &secofr,
            now: NOW,
            sysval: &sysval,
        };
        usrprf::crtusrprf(
            &mut t,
            &env,
            &Keywords::new(&[("USRPRF", "QSTESTGRP"), ("PASSWORD", "*NONE"), ("GID", "*GEN")]),
        )
        .unwrap();
        for u in users {
            usrprf::crtusrprf(
                &mut t,
                &env,
                &Keywords::new(&[("USRPRF", u), ("PASSWORD", PASSWORD), ("GRPPRF", "QSTESTGRP")]),
            )
            .unwrap();
        }
        usrprf::chgusrprf(
            &mut t,
            &env,
            &Keywords::new(&[("USRPRF", "QSECOFR"), ("PASSWORD", PASSWORD), ("PWDEXP", "*NO")]),
        )
        .unwrap();
        t.commit().unwrap();
        let _ = sec::audit::take();
        Self {
            store: Mutex::new((store, qsys)),
            audit: Mutex::new(Vec::new()),
        }
    }

    /// Whether a profile is disabled now.
    pub fn disabled(&self, name: &str) -> bool {
        let mut g = self.store.lock().unwrap();
        let qsys = g.1;
        let mut t = g.0.begin().unwrap();
        objects::read(&mut t, qsys, name).unwrap().1.disabled
    }
}

impl Accounts for StoreAccounts {
    fn lookup(&self, name: &str) -> Option<Account> {
        let mut g = self.store.lock().unwrap();
        let qsys = g.1;
        let mut t = g.0.begin().ok()?;
        let (_, p) = objects::read(&mut t, qsys, name).ok()?;
        let mut groups: Vec<String> = Vec::new();
        if !p.grpprf.is_empty() && !p.grpprf.starts_with('*') {
            groups.push(p.grpprf.clone());
        }
        groups.extend(p.supgrpprf.iter().cloned());
        Some(Account {
            name: p.name.clone(),
            home: p.homedir.clone(),
            groups,
            uid: p.uid,
        })
    }

    fn check_password(&self, name: &str, password: &str, _peer: &Peer) -> PasswordCheck {
        let mut g = self.store.lock().unwrap();
        let qsys = g.1;
        let mut t = g.0.begin().unwrap();
        let attempt = Attempt {
            user: name.into(),
            password: password.into(),
            device: "QSSHD".into(),
            console: false,
            sessions: 0,
            now: NOW,
            proof: None,
        };
        let v = Values::from_sysvals(&sysval);
        let none = |_: &mut store::txn::Txn<'_>, _: &str| -> Option<Auth> { None };
        let r = sec::signon::sign_on(&mut t, qsys, &attempt, &v, &none).unwrap();
        t.commit().unwrap();
        self.audit.lock().unwrap().extend(sec::audit::take());
        match r {
            Ok(s) if s.next == sec::signon::Next::Start => PasswordCheck::Accepted,
            Ok(_) => PasswordCheck::Expired,
            Err(r) => PasswordCheck::Refused {
                msgid: r.msgid,
                text: r.text,
            },
        }
    }
}

fn principal(user: &str) -> Principal {
    let mut p = Principal {
        user: user.into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    };
    if user == "QSECOFR" {
        p.spcaut = SpecialAuth::ALLOBJ.0
            | SpecialAuth::IOSYSCFG.0
            | SpecialAuth::AUDIT.0
            | SpecialAuth::SECADM.0;
    }
    p
}

/// The IFS: `/home/<user>` for each test user, `/QOpenSys/qstest`, a
/// `/QSYS.LIB` with `QGPL/QCLSRC(HELLO)` and `QSTEST/QRPGLESRC`.
pub struct MemFs {
    pub ifs: Arc<Ifs<MemBackend>>,
    env: Arc<MemEnv>,
    audit: Arc<MemAudit>,
    owner: AtomicU64,
}

fn source(lines: &[&str], len: usize) -> MemberData {
    MemberData {
        records: lines
            .iter()
            .map(|l| ccsid::to_ebcdic(&format!("{l:<len$}")).unwrap_or_default())
            .collect(),
        record_len: len,
        source: true,
        ccsid: 37,
    }
}

impl MemFs {
    fn new(users: &[&str]) -> Self {
        let q = MemQsys::new(Auth::USE);
        q.add_lib("QGPL");
        q.add_lib("QSTEST");
        q.add_obj("QGPL", "QCLSRC", "*FILE", "PF-SRC", "QSYS");
        q.add_member("QGPL", "QCLSRC", "HELLO", source(&["PGM", "ENDPGM"], 80));
        q.add_obj("QSTEST", "QRPGLESRC", "*FILE", "PF-SRC", "QSTEST");
        let mut env = MemEnv::default()
            .user("QSYS", 0)
            .user("QSECOFR", 1)
            .user("QDFTOWN", 3)
            .group("QSTESTGRP", 111, 600);
        for (i, u) in users.iter().enumerate() {
            env = env.user(u, 110 + u32::try_from(i).unwrap() * 10);
        }
        let ifs = Ifs::new(MemBackend::new(), Box::new(q), IfsConfig::default());
        ifs.ensure_layout(&env).unwrap();
        let audit = MemAudit::default();
        let sec = principal("QSECOFR");
        let cx = Ctx {
            owner: Owner(u64::MAX),
            principal: &sec,
            env: &env,
            audit: &audit,
            job_ccsid: 37,
        };
        for u in users {
            let home = format!("/home/{u}");
            ifs.mkdir(&cx, &home, 0o755).unwrap();
            ifs.chgown(&cx, &home, u, true).unwrap();
        }
        if !users.is_empty() {
            ifs.mkdir(&cx, "/QOpenSys/qstest", 0o755).unwrap();
            ifs.chgown(&cx, "/QOpenSys/qstest", users[0], true).unwrap();
        }
        ifs.release_owner(Owner(u64::MAX));
        Self {
            ifs: Arc::new(ifs),
            env: Arc::new(env),
            audit: Arc::new(audit),
            owner: AtomicU64::new(1),
        }
    }

    /// A client as `user`.
    pub fn client(&self, user: &str) -> LocalIfs<MemBackend> {
        LocalIfs {
            ifs: self.ifs.clone(),
            env: self.env.clone(),
            audit: self.audit.clone(),
            principal: principal(user),
            owner: Owner(self.owner.fetch_add(1, Ordering::SeqCst)),
            job_ccsid: 37,
        }
    }

    /// Write a file as `QSECOFR`, made `user`'s.
    pub fn put(&self, path: &str, data: &[u8], owner: &str) {
        let c = self.client("QSECOFR");
        if let Some((dir, _)) = path.rsplit_once('/') {
            sshd::io::files::mkdir_p(&c, dir, 0o755).unwrap();
        }
        sshd::io::files::write(&c, path, data, 0o644).unwrap();
        let _ = c.chgown(path, owner, true);
        c.release();
    }

    /// Read a file as `QSECOFR`.
    pub fn get(&self, path: &str) -> Option<Vec<u8>> {
        let c = self.client("QSECOFR");
        let r = sshd::io::files::read(&c, path).ok();
        c.release();
        r
    }
}

struct Fs(Arc<MemFs>);

impl FileSystem for Fs {
    fn system(&self) -> Box<dyn IfsClient> {
        Box::new(self.0.client("QSECOFR"))
    }
    fn user(&self, account: &Account) -> Option<Box<dyn IfsClient>> {
        Some(Box::new(self.0.client(&account.name)))
    }
}

/// QSH over a fresh `MemSys` per session, with the session's
/// environment and home directory.
fn job_sys(req: &SessionRequest) -> Arc<dyn qsh::Sys> {
    let s = qsh::MemSys::new();
    s.mkdir_p(&req.account.home);
    for (k, v) in req.environment() {
        s.set_env(&k, &v);
    }
    s.set_cwd(&req.account.home);
    Arc::new(s)
}

/// Everything a test needs.
pub struct World {
    pub server: Arc<Server>,
    pub fs: Arc<MemFs>,
    pub accounts: Arc<StoreAccounts>,
}

/// The test users.
pub const USERS: &[&str] = &["QSTEST", "QSTEST2", "QSTEST3"];

/// A server started from `config` (written at the IBM i path first).
pub fn world(config: &str) -> World {
    world_with(config, |d| d)
}

/// The same, with the services adjusted.
pub fn world_with(config: &str, adjust: impl FnOnce(Deps) -> Deps) -> World {
    let fs = Arc::new(MemFs::new(USERS));
    let accounts = Arc::new(StoreAccounts::new(USERS));
    fs.put(paths::CONFIG_FILE, config.as_bytes(), "QSYS");
    let server = start(&fs, &accounts, adjust);
    World {
        server,
        fs,
        accounts,
    }
}

/// Start a server over existing parts.
pub fn start(
    fs: &Arc<MemFs>,
    accounts: &Arc<StoreAccounts>,
    adjust: impl FnOnce(Deps) -> Deps,
) -> Arc<Server> {
    let files: Arc<dyn FileSystem> = Arc::new(Fs(fs.clone()));
    let host = FileTransfer::new(QshHost::new(job_sys), files.clone());
    let deps = Deps::new(accounts.clone(), Arc::new(host), files);
    Arc::new(Server::start(paths::CONFIG_FILE, adjust(deps)).unwrap())
}

/// A connected client.
pub type TestClient = Client<PipeReader, PipeWriter>;

/// Connect a client with `opts`; the server runs on its own thread.
pub fn connect_with(server: &Arc<Server>, opts: &Options) -> TestClient {
    let ((sr, sw), (cr, cw)) = duplex();
    let srv = server.clone();
    std::thread::spawn(move || {
        let peer = Peer {
            addr: "10.1.2.3".into(),
            port: 50_000,
        };
        let local = Peer {
            addr: "10.0.0.1".into(),
            port: 22,
        };
        let _ = srv.serve(sr, sw, peer, local);
    });
    Client::connect(cr, cw, opts).unwrap()
}

/// Connect with the default options.
pub fn connect(server: &Arc<Server>) -> TestClient {
    connect_with(server, &Options::default())
}

/// Connect and sign on as `user` with the password.
pub fn signed_on(server: &Arc<Server>, user: &str) -> TestClient {
    let mut c = connect(server);
    assert!(c.auth_password(user, PASSWORD).unwrap(), "sign-on as {user}");
    c
}
