// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A system with a remote mount: the IFS over a `MemBackend`, a
//! `MemServer` offered as `srv:/exp` by a `MemFactory`, and callers
//! that run as a job's requests do in the service (deferred and run
//! again as the same request).

#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::client::{IfsClient, LocalIfs};
use ifs::lease::defer::deferring;
use ifs::ops::audit::MemAudit;
use ifs::ops::{Config, Ifs, Owner};
use ifs::remote::mem::{MemFactory, MemServer};
use ifs::remote::request;
use ifs::remote::{AttrTimes, MountCcsid, RemoteConfig, RemoteInfo};
use ifs::vfs::qsys::MemQsys;
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::profile::spcaut::SpecialAuth;

/// The system.
pub struct Rig {
    pub ifs: Arc<Ifs<MemBackend>>,
    pub env: Arc<MemEnv>,
    pub server: Arc<MemServer>,
    pub factory: Arc<MemFactory>,
}

/// Profiles with UIDs the server sees: `QSECOFR` 0 (root to it, as on
/// IBM i), `BOB` 100, `ANN` 101 (group `DEV`, GID 500).
pub fn env() -> MemEnv {
    MemEnv::default()
        .user("QSYS", 2)
        .user("QSECOFR", 0)
        .user("BOB", 100)
        .user("ANN", 101)
        .user("QDFTOWN", 3)
        .group("DEV", 200, 500)
}

pub fn user(name: &str) -> Principal {
    let mut p = Principal {
        user: name.into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    };
    match name {
        "QSECOFR" => {
            p.spcaut = SpecialAuth::ALLOBJ.0 | SpecialAuth::IOSYSCFG.0;
        }
        "ANN" => p.groups = vec!["DEV".into()],
        _ => {}
    }
    p
}

/// What a mount says of itself in most tests: short attribute times,
/// small blocks so a few kilobytes span several.
pub fn info() -> RemoteInfo {
    RemoteInfo {
        name: "NFS".into(),
        attr_times: AttrTimes {
            reg_min: Duration::from_millis(300),
            reg_max: Duration::from_millis(600),
            dir_min: Duration::from_millis(300),
            dir_max: Duration::from_millis(600),
        },
        rsize: 1024,
        wsize: 1024,
        ..RemoteInfo::default()
    }
}

/// A system with `/mnt` and the server offered, not yet mounted.
pub fn rig_with(info: RemoteInfo, cfg: RemoteConfig) -> Rig {
    let env = Arc::new(env());
    let ifs = Ifs::new(
        MemBackend::new(),
        Box::new(MemQsys::new(Auth::USE)),
        Config::default(),
    );
    ifs.ensure_layout(&*env).unwrap();
    let ifs = Arc::new(ifs);
    let server = Arc::new(MemServer::new());
    let factory = Arc::new(MemFactory::new("*NFS"));
    factory.add("srv:/exp", Arc::clone(&server), info);
    ifs.set_remote_factory(Some(factory.clone()), cfg);
    let r = Rig {
        ifs,
        env,
        server,
        factory,
    };
    r.client("QSECOFR", 900).mkdir("/mnt", 0o777).unwrap();
    r
}

/// Mounted at `/mnt`.
pub fn mounted_with(info: RemoteInfo, cfg: RemoteConfig) -> Rig {
    let r = rig_with(info, cfg);
    r.client("QSECOFR", 900)
        .mount_remote("*NFS", "srv:/exp", "/mnt", "rw", MountCcsid::default())
        .unwrap();
    r
}

pub fn mounted() -> Rig {
    mounted_with(
        info(),
        RemoteConfig {
            threads: 2,
            readahead: 2,
            behind: 2,
            ..RemoteConfig::default()
        },
    )
}

impl Rig {
    /// A job's client (each call waits as the service would park it).
    pub fn client(&self, name: &str, owner: u64) -> LocalIfs<MemBackend> {
        LocalIfs {
            ifs: Arc::clone(&self.ifs),
            env: self.env.clone(),
            audit: Arc::new(MemAudit::default()),
            principal: user(name),
            owner: Owner(owner),
            job_ccsid: 37,
        }
    }

    /// The mount at `/mnt`.
    pub fn mount(&self) -> Arc<ifs::remote::RemoteMount> {
        let id = self
            .ifs
            .mount_infos()
            .into_iter()
            .find(|m| m.at == "/mnt")
            .expect("mounted")
            .id;
        self.ifs.remote_mount(id).expect("remote")
    }
}

/// Run `f` as a parked request is run: again and again with the same
/// token until it does not defer; the result and how many runs it took.
pub fn drive<T>(c: &LocalIfs<MemBackend>, f: impl Fn(&dyn IfsClient) -> T) -> (T, u32) {
    let mut token = None;
    let mut runs = 0;
    loop {
        runs += 1;
        if let Some(t) = token {
            request::rerun(t);
        }
        let (r, d) = deferring(true, || c.with(&f));
        let Some(d) = d else {
            return (r, runs);
        };
        token = request::last();
        if std::env::var_os("QS_DBG").is_some() {
            eprintln!("run {runs}: {d:?}");
        }
        assert!(runs < 1000, "no progress");
        c.ifs.wait_deferral(&d);
    }
}

/// Wait until `cond` holds (at most five seconds).
pub fn until(cond: impl Fn() -> bool) {
    for _ in 0..500 {
        if cond() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("condition never held");
}
