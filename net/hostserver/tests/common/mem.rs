// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Backends in memory: data queues, spooled files, the IFS, an SQL
//! session over a store, and a system with one user.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use hostserver::dtaq::{DataQueues, Entry, Fail, QueueAttrs};
use hostserver::netprt::{Splf, SplfAction, Spool};
use hostserver::session::Proof;
use hostserver::signon::{Change, Info, Refusal, SignonHost};
use hostserver::Auth;
use hostsvr::common::rc;
use hostsvr::crypt::{Level, substitute};
use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::client::LocalIfs;
use ifs::ops::audit::MemAudit;
use ifs::ops::{Config, Ifs, Owner};
use ifs::vfs::qsys::MemQsys;
use qsname::Name;
use sec::auth::bits::Auth as Bits;
use sec::auth::principal::Principal;
use sec::profile::spcaut::SpecialAuth;
use sql::embed::Session;
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

/// Queues in memory.
#[derive(Default)]
pub struct MemQueues {
    queues: HashMap<(String, String), (QueueAttrs, Vec<Entry>)>,
}

fn missing(name: &str, lib: &str) -> Fail {
    ("CPF9801".into(), format!("{lib}/{name} was not found."))
}

impl DataQueues for MemQueues {
    fn create(&mut self, name: &str, lib: &str, a: &QueueAttrs) -> Result<(), Fail> {
        let k = (lib.into(), name.into());
        if self.queues.contains_key(&k) {
            return Err(("CPF9870".into(), "It exists.".into()));
        }
        self.queues.insert(k, (a.clone(), Vec::new()));
        Ok(())
    }
    fn delete(&mut self, name: &str, lib: &str) -> Result<(), Fail> {
        self.queues.remove(&(lib.into(), name.into())).map(|_| ()).ok_or_else(|| missing(name, lib))
    }
    fn attributes(&mut self, name: &str, lib: &str) -> Result<QueueAttrs, Fail> {
        self.queues.get(&(lib.into(), name.into())).map(|q| q.0.clone()).ok_or_else(|| missing(name, lib))
    }
    fn write(&mut self, name: &str, lib: &str, key: &[u8], data: &[u8]) -> Result<(), Fail> {
        let q = self.queues.get_mut(&(lib.into(), name.into())).ok_or_else(|| missing(name, lib))?;
        q.1.push(Entry {
            data: data.to_vec(),
            key: key.to_vec(),
            sender: Vec::new(),
        });
        Ok(())
    }
    fn read(&mut self, name: &str, lib: &str, order: &str, key: &[u8], _wait: i32, peek: bool) -> Result<Option<Entry>, Fail> {
        let q = self.queues.get_mut(&(lib.into(), name.into())).ok_or_else(|| missing(name, lib))?;
        let at = if order.is_empty() {
            if q.0.sequence == 'L' { q.1.len().checked_sub(1) } else { (!q.1.is_empty()).then_some(0) }
        } else {
            q.1.iter().position(|e| e.key == key)
        };
        Ok(at.map(|i| if peek { q.1[i].clone() } else { q.1.remove(i) }))
    }
    fn clear(&mut self, name: &str, lib: &str, _order: &str, key: &[u8]) -> Result<(), Fail> {
        let q = self.queues.get_mut(&(lib.into(), name.into())).ok_or_else(|| missing(name, lib))?;
        q.1.retain(|e| !key.is_empty() && e.key != key);
        Ok(())
    }
}


/// The IFS with `/home/QSTEST` and four directories in it.
pub fn ifs() -> LocalIfs<MemBackend> {
    let env = MemEnv::default().user("QSYS", 0).user("QSECOFR", 1).user("MASKUSER", 100).user("QDFTOWN", 3);
    let ifs = Ifs::new(MemBackend::new(), Box::new(MemQsys::new(Bits::USE)), Config::default());
    ifs.ensure_layout(&env).unwrap();
    let p = Principal {
        user: "QSECOFR".into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        spcaut: SpecialAuth::ALLOBJ.0 | SpecialAuth::SECADM.0,
        ..Principal::default()
    };
    let l = LocalIfs {
        ifs: Arc::new(ifs),
        env: Arc::new(env),
        audit: Arc::new(MemAudit::default()),
        principal: p,
        owner: Owner(1),
        job_ccsid: 37,
    };
    l.with(|c| {
        let _ = c.mkdir("/home", 0o755);
        c.mkdir("/home/QSTEST", 0o777).unwrap();
        for d in ["dds", "data", "oracle", "out"] {
            c.mkdir(&format!("/home/QSTEST/{d}"), 0o777).unwrap();
        }
    });
    l
}


const NOW: u64 = 1_792_000_000;

/// An SQL session over a store with `QSYS2`, `SYSIBM` and `QSTEST`.
pub fn session() -> Session {
    let d = Arc::new(MemBlockDev::with_seed(16384, 21));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
        now: NOW,
    };
    let mut store = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &params, StoreConfig::default()).unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "S062B175").unwrap();
    for l in ["QSYS2", "SYSIBM"] {
        ops::create(
            &mut t,
            &CreateParams {
                library: qsys,
                name: Name::parse(l).unwrap(),
                type_code: types::LIB,
                attribute: String::new(),
                text: String::new(),
                owner: "QSYS".into(),
                system: "S062B175".into(),
                now: NOW,
                by_system: true,
            },
        )
        .unwrap();
    }
    let lib = ops::create(
        &mut t,
        &CreateParams {
            library: qsys,
            name: Name::parse("QSTEST").unwrap(),
            type_code: types::LIB,
            attribute: String::new(),
            text: "Test library".into(),
            owner: "QSECOFR".into(),
            system: "S062B175".into(),
            now: NOW,
            by_system: false,
        },
    )
    .unwrap();
    t.commit().unwrap();
    let names = NameContext {
        curlib: Some(lib),
        libl: vec![lib],
    };
    Session::new(Box::new(Rc::new(RefCell::new(store))), qsys, JobHandle(1), names)
}


/// Spooled files in memory.
#[derive(Clone)]
pub struct MemSpool(pub Vec<Splf>);

impl Spool for MemSpool {
    fn system(&mut self) -> String {
        "S062B175".into()
    }
    fn list(&mut self, _user: &str, _outq: Option<(String, String)>) -> Vec<Splf> {
        self.0.clone()
    }
    fn act(&mut self, _s: &Splf, _what: SplfAction) -> Result<(), String> {
        Ok(())
    }
    fn data(&mut self, _s: &Splf) -> Result<Vec<u8>, String> {
        Ok(Vec::new())
    }
    fn outqs(&mut self) -> Vec<(String, String)> {
        Vec::new()
    }
    fn writers(&mut self) -> Vec<(String, String, String)> {
        Vec::new()
    }
}


/// A system with one user that checks substitutes at its level.
#[derive(Clone)]
pub struct Host {
    /// `QPWDLVL`.
    pub level: u8,
    /// The user.
    pub user: String,
    /// Its password.
    pub password: String,
    /// The seed the server sends.
    pub seed: [u8; 8],
}

impl Host {
    /// A system.
    pub fn new(level: u8, user: &str, password: &str) -> Self {
        Self {
            level,
            user: user.into(),
            password: password.into(),
            seed: [9, 8, 7, 6, 5, 4, 3, 2],
        }
    }

    fn check(&self, user: &str, proof: Proof<'_>) -> Result<(), u32> {
        if user != self.user {
            return Err(rc::USER_UNKNOWN);
        }
        match proof {
            Proof::Substitute { seeds, bytes } => {
                let want = substitute(Level::from_qpwdlvl(self.level), user, &self.password, &seeds).unwrap();
                if want == bytes { Ok(()) } else { Err(rc::PASSWORD_INCORRECT) }
            }
            Proof::Token(t) => if t == [0x5A; 32] { Ok(()) } else { Err(rc::TOKEN_LENGTH) },
        }
    }
}

impl Auth for Host {
    fn password_level(&mut self) -> u8 {
        self.level
    }
    fn seed(&mut self) -> [u8; 8] {
        self.seed
    }
    fn start(&mut self, user: &str, proof: Proof<'_>) -> Result<(), u32> {
        self.check(user, proof)
    }
    fn job_name(&mut self) -> String {
        "000123/QUSER/QZSOSIGN".into()
    }
}

impl SignonHost for Host {
    fn verify(&mut self, user: &str, proof: Proof<'_>) -> Result<Info, Refusal> {
        self.check(user, proof).map_err(|c| (c, Vec::new()))?;
        Ok(Info {
            current: Some((2026, 10, 29, 12, 0, 0)),
            last: Some((2026, 10, 28, 9, 30, 0)),
            expires: Some((2027, 1, 1, 0, 0, 0)),
            warning_days: 7,
            ccsid: 37,
            messages: Vec::new(),
        })
    }
    fn token(&mut self, user: &str, proof: Proof<'_>, _kind: u8, _timeout: u32) -> Result<Vec<u8>, u32> {
        self.check(user, proof)?;
        Ok(vec![0x5A; 32])
    }
    fn change_password(&mut self, user: &str, c: &Change) -> Result<(), Refusal> {
        self.check(
            user,
            Proof::Substitute {
                seeds: c.seeds,
                bytes: &c.substitute,
            },
        )
        .map_err(|e| (e, Vec::new()))?;
        let new = hostsvr::crypt::recover_new_password(
            Level::from_qpwdlvl(self.level),
            user,
            &self.password,
            &c.seeds,
            &c.new_protected,
            c.new_len,
        )
        .map_err(|_| (rc::NEW_PASSWORD_NOT_VALID, Vec::new()))?;
        self.password = new;
        Ok(())
    }
}

