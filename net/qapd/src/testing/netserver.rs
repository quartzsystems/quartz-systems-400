// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The test system's NetServer (`NetServer`, version 10, draft): the
//! `QZLS*` APIs themselves (`netsvrcfg::api`) over a host in memory,
//! called as the signed-on profile (`SECOFR` has `*IOSYSCFG` and
//! `*SECADM`, `BOB` and `OPER` neither), so every check and message is
//! the APIs'.
//!
//! The fixtures ([`shipped`]): `QSHQ400` active, shares `QIBM`,
//! `BOBDOCS` (`/home/BOB`, which `BOB` owns) and print share `PRT01` on
//! `QUSRSYS/PRT01`; two sessions (`BOB` from `PC1` with `BOBDOCS`, the
//! guest from `10.0.0.9` with `QIBM` and `PRT01`); `ANN` disabled.

#![allow(clippy::doc_markdown)] // NetServer

use netsvrcfg::api::{ApiError, MemHost, ServerControl};
use netsvrcfg::{Qualified, Session, Share};

use super::EPOCH;

/// `QZLSSERVER`: start and end flip it.
#[derive(Debug)]
pub struct Control {
    /// Active.
    pub active: bool,
}

impl ServerControl for Control {
    fn active(&mut self) -> bool {
        self.active
    }

    fn start(&mut self, _reset: bool) -> Result<(), ApiError> {
        self.active = true;
        Ok(())
    }

    fn end(&mut self) -> Result<(), ApiError> {
        self.active = false;
        Ok(())
    }

    fn end_session(&mut self, _job: &str, _id: u64) -> Result<(), ApiError> {
        Ok(())
    }
}

/// The test system's NetServer.
pub type NetServer = MemHost<Control>;

fn session(ws: &str, user: &str, guest: bool, since: u64) -> Session {
    Session {
        id: 0,
        workstation: ws.into(),
        expanded: ws.into(),
        user: user.into(),
        job: "000210/QUSER/QZLSFILE".into(),
        guest,
        since,
        last_active: since,
        connections: Vec::new(),
        ending: false,
    }
}

/// The shipped state.
#[must_use]
#[allow(clippy::many_single_char_names)]
pub fn shipped() -> NetServer {
    let mut h = MemHost::with_server("QSHQ400", Control { active: true });
    h.clock = EPOCH;
    let mut c = netsvrcfg::Config::shipped("QSHQ400");
    for s in [
        Share::file("BOBDOCS", "/home/BOB", "Bob's documents"),
        Share::print("PRT01", Qualified::new("PRT01", "QUSRSYS"), "Laser"),
    ] {
        c.shares.insert(s.name.clone(), s);
    }
    h.config = Some(c);
    h.paths.extend([
        ("/home".to_string(), "QSYS".to_string()),
        ("/home/BOB".to_string(), "BOB".to_string()),
    ]);
    h.outqs
        .push((Qualified::new("PRT01", "QUSRSYS"), "QSYS".to_string()));
    for p in ["BOB", "ANN", "OPER", "SECOFR", "QSHGUEST"] {
        h.profiles.push((p.to_string(), Vec::new()));
    }
    h.disabled.push("ANN".into());
    let t = &mut h.table;
    t.started(EPOCH - 3_600);
    let a = t.register(session("PC1", "BOB", false, EPOCH - 600));
    let c = t.connect(a, "BOBDOCS", 0, EPOCH - 590).expect("session");
    t.set_opens(a, c, 2, EPOCH - 10).expect("connection");
    let b = t.register(session("10.0.0.9", "QSHGUEST", true, EPOCH - 300));
    t.connect(b, "QIBM", 0, EPOCH - 290).expect("session");
    t.connect(b, "PRT01", 1, EPOCH - 280).expect("session");
    h
}
