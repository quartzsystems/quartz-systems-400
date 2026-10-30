// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An IFS in memory for tests and kits: the standard directories, a
//! user `QSTEST` with `/home/QSTEST` its own, a `/QSYS.LIB` with
//! `QGPL/QCLSRC` and `QSTEST/QRPGLESRC`, and a [`LocalIfs`] per caller.

use std::sync::Arc;

use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::client::LocalIfs;
use ifs::ops::audit::MemAudit;
use ifs::ops::{Config, Ctx, Ifs, Owner};
use ifs::vfs::qsys::{MemQsys, MemberData};
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::profile::spcaut::SpecialAuth;

/// The test user's home.
pub const HOME: &str = "/home/QSTEST";

/// The test system.
pub struct MemIfs {
    /// The file system.
    pub ifs: Arc<Ifs<MemBackend>>,
    /// Its profiles.
    pub env: Arc<MemEnv>,
    /// Its audit events.
    pub audit: Arc<MemAudit>,
}

/// A caller's principal: `QSECOFR` has `*ALLOBJ` and the rest, others none.
#[must_use]
pub fn principal(user: &str) -> Principal {
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

impl MemIfs {
    /// A new system.
    ///
    /// # Panics
    ///
    /// The layout cannot be made (a bug).
    #[must_use]
    pub fn new() -> Self {
        let q = MemQsys::new(Auth::USE);
        q.add_lib("QGPL");
        q.add_lib("QSTEST");
        q.add_obj("QGPL", "QCLSRC", "*FILE", "PF-SRC", "QSYS");
        q.add_member("QGPL", "QCLSRC", "HELLO", source(&["PGM", "ENDPGM"], 80));
        q.add_obj("QSTEST", "QRPGLESRC", "*FILE", "PF-SRC", "QSTEST");
        q.add_member("QSTEST", "QRPGLESRC", "PROTOS", source(&["**FREE", "dcl-s from_member int(10);"], 92));
        let env = MemEnv::default()
            .user("QSYS", 0)
            .user("QSECOFR", 1)
            .user("QDFTOWN", 3)
            .user("QSTEST", 110)
            .group("QSTESTGRP", 111, 600);
        let ifs = Ifs::new(MemBackend::new(), Box::new(q), Config::default());
        ifs.ensure_layout(&env).expect("the standard directories");
        let audit = MemAudit::default();
        let sec = principal("QSECOFR");
        let cx = Ctx {
            owner: Owner(u64::MAX),
            principal: &sec,
            env: &env,
            audit: &audit,
            job_ccsid: 37,
        };
        ifs.mkdir(&cx, HOME, 0o755).expect("the home directory");
        ifs.chgown(&cx, HOME, "QSTEST", true).expect("its owner");
        ifs.release_owner(Owner(u64::MAX));
        Self {
            ifs: Arc::new(ifs),
            env: Arc::new(env),
            audit: Arc::new(audit),
        }
    }

    /// A caller: `user` with its own descriptors (`owner`), texts in `ccsid`.
    #[must_use]
    pub fn client(&self, user: &str, owner: u64, ccsid: u16) -> LocalIfs<MemBackend> {
        LocalIfs {
            ifs: self.ifs.clone(),
            env: self.env.clone(),
            audit: self.audit.clone(),
            principal: principal(user),
            owner: Owner(owner),
            job_ccsid: ccsid,
        }
    }
}

impl Default for MemIfs {
    fn default() -> Self {
        Self::new()
    }
}
