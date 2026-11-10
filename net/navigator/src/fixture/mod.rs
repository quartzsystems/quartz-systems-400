// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The prototype's sample system, so served pages can be compared with
//! the golden pages and the demo server has something to show: system
//! `QSHQ400`, a clock stopped at 10/29/26 14:03:40, and each area's
//! sample data in its own module.

pub mod cl;
pub mod config;
pub mod db;
pub mod msgs;
pub mod net;
pub mod netsvr;
pub mod perf;
pub mod sec;
pub mod sw;
pub mod text;
pub mod users;
pub mod work;

use std::sync::{Arc, Mutex};

use qsweb::msg::{Kind, Message, SecondLevel};

use crate::env::{Audit, Env, Status, System};
use crate::util::{mdy, secs};

/// The fixture's password for every user.
pub const PASSWORD: &str = "QS400";

/// The fixture clock.
pub fn now() -> u64 {
    secs(2026, 10, 29, 14, 3, 40)
}

/// A profile the sign-on knows: name, special authorities, class.
pub struct Profile {
    /// The name.
    pub name: &'static str,
    /// Special authorities.
    pub special: &'static [&'static str],
    /// The user class.
    pub class: &'static str,
}

/// The profiles the sign-on knows.
pub const PROFILES: &[Profile] = &[
    Profile {
        name: "QSECOFR",
        special: &[
            "*ALLOBJ",
            "*AUDIT",
            "*IOSYSCFG",
            "*JOBCTL",
            "*SAVSYS",
            "*SECADM",
            "*SERVICE",
            "*SPLCTL",
        ],
        class: "*SECOFR",
    },
    Profile {
        name: "QSYSOPR",
        special: &["*JOBCTL", "*SAVSYS"],
        class: "*SYSOPR",
    },
    Profile {
        name: "QPGMR",
        special: &["*JOBCTL", "*SAVSYS"],
        class: "*PGMR",
    },
    Profile {
        name: "WEBUSER",
        special: &[],
        class: "*USER",
    },
];

/// Whether sign-on profile `user` has special authority `what`
/// (`*ALLOBJ` has every one).
pub fn has_special(user: &str, what: &str) -> bool {
    PROFILES
        .iter()
        .find(|p| p.name == user)
        .is_some_and(|p| p.special.iter().any(|s| *s == what || *s == "*ALLOBJ"))
}

/// The sample system.
#[derive(Default)]
pub struct FixtureSystem {
    /// Audit entries written.
    pub audit: Mutex<Vec<Audit>>,
}

impl System for FixtureSystem {
    fn name(&self) -> String {
        "QSHQ400".into()
    }

    fn now(&self) -> u64 {
        now()
    }

    fn stamp(&self, secs: u64) -> (String, String) {
        mdy(secs, 0)
    }

    fn sign_on(&self, user: &str, password: &str, _from: &str) -> Result<String, Message> {
        let u = user.trim().to_ascii_uppercase();
        let known = PROFILES.iter().any(|p| p.name == u);
        if known && password == PASSWORD {
            return Ok(u);
        }
        self.audit.lock().expect("audit").push(Audit::Password {
            user: u.clone(),
            code: if known { 'P' } else { 'U' },
        });
        let (id, data) = if known {
            ("CPF22E2", u.clone())
        } else {
            ("CPF2204", u.clone())
        };
        let text = text::second_level(id, &[&data])
            .map(|s| s.text)
            .unwrap_or_default();
        Err(Message::new(Kind::Error, id, &text))
    }

    fn special(&self, user: &str) -> Vec<String> {
        PROFILES
            .iter()
            .find(|p| p.name == user)
            .map(|p| p.special.iter().map(ToString::to_string).collect())
            .unwrap_or_default()
    }

    fn user_class(&self, user: &str) -> String {
        PROFILES
            .iter()
            .find(|p| p.name == user)
            .map_or("*USER", |p| p.class)
            .to_string()
    }

    fn audit(&self, entry: Audit) {
        self.audit.lock().expect("audit").push(entry);
    }

    fn message(&self, id: &str, data: &[&str]) -> Option<SecondLevel> {
        text::second_level(id, data)
    }

    fn status(&self, _user: &str, _reset: bool) -> Status {
        const GB: u64 = 1 << 30;
        const MB: u64 = 1 << 20;
        Status {
            serial: "10-7A2B9C".into(),
            model: "Q400-M2".into(),
            machine_type: "9Q04".into(),
            hardware: "Dell Inc. PowerEdge R650".into(),
            release: release::RELEASE.to_string(),
            ptf_level: "QS400-CUM-26290".into(),
            processors: "8 (shared, uncapped)".into(),
            main_storage: 65_536 * MB,
            memory_used: 21_904 * MB,
            memory_service: 6_210 * MB,
            memory_jobs: 9_377 * MB,
            memory_top: [
                ("123411/QUSER/QZDASOINIT", 1_844),
                ("123301/QSYS/QDBSRV01", 912),
                ("123456/QTMHHTTP/ADMIN", 604),
                ("123105/QSYS/QSPLMAINT", 233),
                ("123488/TESTUSER/QPADEV0003", 118),
            ]
            .into_iter()
            .map(|(job, mb)| crate::env::MemJob {
                job: job.into(),
                bytes: mb * MB,
            })
            .collect(),
            ipl: secs(2026, 10, 27, 5, 58, 12),
            ipl_kind: "Normal, B side".into(),
            zone: "EDT".into(),
            asp_capacity: 18_624 * GB / 10,
            asp_used: 673,
            temp_used: 4_120 * MB,
            temp_peak: 6_003 * MB,
            pools: 6,
            machine_pool: 8_192 * MB,
            units: 12,
            units_inactive: 0,
            units_failed: 0,
            cpu: 234,
            elapsed: 60,
            jobs: 1_204,
            active: 318,
            interactive: 14,
            queued: 37,
            subsystems: 9,
            msgw: 2,
            spooled: 212,
            qsysopr: 41,
            inquiries: 3,
            failed_signons: 6,
            ptfs_not_applied: 2,
            servers_down: vec!["*SNMP".into()],
        }
    }
}

/// The fixture's parts, for tests to look into.
pub struct Fixture {
    /// The environment.
    pub env: Env,
    /// The system.
    pub system: Arc<FixtureSystem>,
    /// Work management.
    pub work: Arc<work::FixtureWork>,
    /// Messages.
    pub msgs: Arc<msgs::FixtureMsgs>,
    /// Users and groups.
    pub users: Arc<users::FixtureUsers>,
    /// System values, disks, hardware, time.
    pub config: Arc<config::FixtureConfig>,
    /// TCP/IP.
    pub net: Arc<net::FixtureNet>,
    /// `NetServer`.
    pub netsvr: Arc<netsvr::FixtureNetServer>,
    /// The database.
    pub db: Arc<db::FixtureDb>,
    /// The audit journal.
    pub sec: Arc<sec::FixtureSecurity>,
    /// Software.
    pub sw: Arc<sw::FixtureSoftware>,
    /// Performance.
    pub perf: Arc<perf::FixturePerf>,
}

/// The sample system.
pub fn fixture() -> Fixture {
    let system = Arc::new(FixtureSystem::default());
    let work = Arc::new(work::FixtureWork::default());
    let msgs = Arc::new(msgs::FixtureMsgs::default());
    let users = Arc::new(users::FixtureUsers::default());
    let config = Arc::new(config::FixtureConfig::default());
    let net = Arc::new(net::FixtureNet::default());
    let netsvr = Arc::new(netsvr::FixtureNetServer::default());
    let db = Arc::new(db::FixtureDb::default());
    let sec = Arc::new(sec::FixtureSecurity::default());
    let sw = Arc::new(sw::FixtureSoftware::default());
    let perf = Arc::new(perf::FixturePerf::default());
    let env = Env {
        system: system.clone(),
        work: work.clone(),
        msgs: msgs.clone(),
        users: users.clone(),
        config: config.clone(),
        net: net.clone(),
        netsvr: netsvr.clone(),
        db: db.clone(),
        sec: sec.clone(),
        sw: sw.clone(),
        perf: perf.clone(),
    };
    Fixture {
        env,
        system,
        work,
        msgs,
        users,
        config,
        net,
        netsvr,
        db,
        sec,
        sw,
        perf,
    }
}
