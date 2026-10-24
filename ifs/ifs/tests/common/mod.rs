// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A test system: the IFS over a `MemBackend`, profiles, a `/QSYS.LIB`
//! with a source file, and callers.

#![allow(dead_code)]

use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::ops::audit::MemAudit;
use ifs::ops::{Config, Ctx, Ifs, Owner};
use ifs::vfs::qsys::{MemQsys, MemberData};
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::profile::spcaut::SpecialAuth;

/// Everything a test needs.
pub struct Sys {
    pub ifs: Ifs<MemBackend>,
    pub env: MemEnv,
    pub audit: MemAudit,
}

/// Profiles: `QSECOFR` (`*ALLOBJ *IOSYSCFG *AUDIT`), `BOB`, `ANN` (in
/// `DEV`), `ZED`, group `DEV`.
pub fn env() -> MemEnv {
    MemEnv::default()
        .user("QSYS", 0)
        .user("QSECOFR", 1)
        .user("BOB", 100)
        .user("ANN", 101)
        .user("ZED", 102)
        .user("QDFTOWN", 3)
        .group("DEV", 200, 500)
}

pub fn sys() -> Sys {
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
    q.add_obj("QGPL", "SPACE", "*USRSPC", "", "BOB");
    q.add_obj("QSYS", "QSYSOPR", "*MSGQ", "", "QSYS");
    let env = env();
    let ifs = Ifs::new(MemBackend::new(), Box::new(q), Config::default());
    ifs.ensure_layout(&env).unwrap();
    Sys {
        ifs,
        env,
        audit: MemAudit::default(),
    }
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

impl Sys {
    pub fn cx<'a>(&'a self, p: &'a Principal, owner: u64) -> Ctx<'a> {
        Ctx {
            owner: Owner(owner),
            principal: p,
            env: &self.env,
            audit: &self.audit,
            job_ccsid: 37,
        }
    }

    /// Write a whole file.
    pub fn put(&self, cx: &Ctx<'_>, path: &str, data: &[u8]) {
        let fd = self.ifs.creat(cx, path, 0o644).unwrap();
        assert_eq!(self.ifs.write(cx, fd, data).unwrap(), data.len());
        self.ifs.close(cx, fd).unwrap();
    }

    /// Read a whole file.
    pub fn get(&self, cx: &Ctx<'_>, path: &str) -> Vec<u8> {
        let fd = self
            .ifs
            .open(cx, path, ifs::ops::OpenArgs::new(ifs::ops::flags::O_RDONLY))
            .unwrap();
        let mut out = Vec::new();
        let mut buf = [0u8; 7];
        loop {
            let n = self.ifs.read(cx, fd, &mut buf).unwrap();
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        self.ifs.close(cx, fd).unwrap();
        out
    }
}
