// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the product needs from the system, as traits by area: the
//! running system answers them in the `NAVIGATOR` job (`work/qsjob`)
//! through the commands and APIs a 5250 user has, the fixture
//! ([`crate::fixture`]) in tests and the demo server.

pub mod config;
pub mod db;
pub mod dirsrv;
pub mod msgs;
pub mod net;
pub mod netsvr;
pub mod nfs;
pub mod perf;
pub mod sec;
pub mod sw;
pub mod users;
pub mod work;

use std::sync::Arc;

use qsweb::msg::{Message, SecondLevel};

/// A special authority an action needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// `*ALLOBJ`.
    AllObj,
    /// `*AUDIT`.
    Audit,
    /// `*IOSYSCFG`.
    IoSysCfg,
    /// `*JOBCTL`.
    JobCtl,
    /// `*SAVSYS`.
    SavSys,
    /// `*SECADM`.
    SecAdm,
    /// `*SERVICE`.
    Service,
    /// `*SPLCTL`.
    SplCtl,
}

impl Need {
    /// The special authority's name.
    pub fn name(self) -> &'static str {
        match self {
            Need::AllObj => "*ALLOBJ",
            Need::Audit => "*AUDIT",
            Need::IoSysCfg => "*IOSYSCFG",
            Need::JobCtl => "*JOBCTL",
            Need::SavSys => "*SAVSYS",
            Need::SecAdm => "*SECADM",
            Need::Service => "*SERVICE",
            Need::SplCtl => "*SPLCTL",
        }
    }
}

/// An audit journal entry the product writes itself. The commands it
/// runs write their own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Audit {
    /// `PW`: a refused sign-on (user, the reason code).
    Password {
        /// The user named.
        user: String,
        /// `P` wrong password, `U` no such user, `D` disabled, ...
        code: char,
    },
    /// `AF`: a refused action.
    Refused {
        /// The user.
        user: String,
        /// The object or function.
        object: String,
        /// What was missing.
        need: String,
    },
}

/// The object a refused action named, for the Not authorized page and
/// its `CPF2207` text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Target {
    /// As the page shows it (`123456/QTMHHTTP/ADMIN`).
    pub shown: String,
    /// The object (`ADMIN`).
    pub name: String,
    /// Its library (`QSYS`).
    pub lib: String,
    /// Its type (`*JOB`).
    pub typ: String,
}

impl Target {
    /// Object `lib/name` of type `typ`.
    pub fn obj(lib: &str, name: &str, typ: &str) -> Self {
        Self {
            shown: format!("{lib}/{name}"),
            name: name.into(),
            lib: lib.into(),
            typ: typ.into(),
        }
    }

    /// Job `number/user/name`.
    pub fn job(qualified: &str) -> Self {
        Self {
            shown: qualified.into(),
            name: qualified.rsplit('/').next().unwrap_or(qualified).into(),
            lib: "QSYS".into(),
            typ: "*JOB".into(),
        }
    }
}

/// A job and the main storage it holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemJob {
    /// `number/user/name`.
    pub job: String,
    /// Bytes resident.
    pub bytes: u64,
}

/// What System status shows (`WRKSYSSTS`, `DSPSYSVAL`, the counts of
/// the other pages). Sizes in bytes, percentages in tenths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// `10-7A2B9C`.
    pub serial: String,
    /// `Q400-M2`.
    pub model: String,
    /// `9Q04`.
    pub machine_type: String,
    /// The firmware's vendor and product (`Dell Inc. PowerEdge R650`),
    /// empty when it names neither.
    pub hardware: String,
    /// `V1R0M0`.
    pub release: String,
    /// The cumulative PTF level (`QS400-CUM-26290`), empty for none.
    pub ptf_level: String,
    /// `8 (shared, uncapped)`.
    pub processors: String,
    /// Main storage (shown as Memory).
    pub main_storage: u64,
    /// Main storage in use; 0 when not known.
    pub memory_used: u64,
    /// The storage service's own main storage.
    pub memory_service: u64,
    /// All jobs' main storage.
    pub memory_jobs: u64,
    /// The jobs holding the most main storage, most first.
    pub memory_top: Vec<MemJob>,
    /// When the last IPL started, seconds since 1970.
    pub ipl: u64,
    /// `Normal, B side`.
    pub ipl_kind: String,
    /// The time zone's abbreviation now (`EDT`).
    pub zone: String,
    /// The system ASP's capacity.
    pub asp_capacity: u64,
    /// The system ASP used, tenths of a percent.
    pub asp_used: u32,
    /// Temporary storage used now.
    pub temp_used: u64,
    /// The most temporary storage used since IPL.
    pub temp_peak: u64,
    /// Main storage pools.
    pub pools: u32,
    /// The machine pool's size.
    pub machine_pool: u64,
    /// Disk units.
    pub units: u32,
    /// Disk units not active.
    pub units_inactive: u32,
    /// Disk units with errors.
    pub units_failed: u32,
    /// CPU used over the elapsed interval, tenths of a percent.
    pub cpu: u32,
    /// The elapsed interval, seconds.
    pub elapsed: u32,
    /// Jobs in the system.
    pub jobs: u32,
    /// Active jobs.
    pub active: u32,
    /// Active interactive jobs.
    pub interactive: u32,
    /// Jobs waiting in job queues.
    pub queued: u32,
    /// Active subsystems.
    pub subsystems: u32,
    /// Active jobs in `MSGW`.
    pub msgw: u32,
    /// Spooled files waiting to print.
    pub spooled: u32,
    /// Messages on `QSYSOPR`.
    pub qsysopr: u32,
    /// Inquiry messages on `QSYSOPR` not answered.
    pub inquiries: u32,
    /// Failed sign-on attempts in the last 24 hours (`PW` entries).
    pub failed_signons: u32,
    /// PTFs loaded but not applied.
    pub ptfs_not_applied: u32,
    /// TCP servers set to autostart that are not active (`*SNMP`).
    pub servers_down: Vec<String>,
}

/// The system: its name and clock, users, authority, audit, messages,
/// status.
pub trait System: Send + Sync {
    /// The system name (`QSHQ400`).
    fn name(&self) -> String;
    /// Seconds since 1970.
    fn now(&self) -> u64;
    /// `secs` as the date and time the user's job shows
    /// (`10/29/26`, `14:02:17`).
    fn stamp(&self, secs: u64) -> (String, String);
    /// Check a user and password with the 5250 sign-on rules
    /// (`QMAXSIGN`, disabled profiles, expired passwords; `PW` entries).
    /// `from` is the client's address. Gives the profile name.
    fn sign_on(&self, user: &str, password: &str, from: &str) -> Result<String, Message>;
    /// The user's special authorities.
    fn special(&self, user: &str) -> Vec<String>;
    /// The user's class (`*SECOFR`, `*USER`, ...).
    fn user_class(&self, user: &str) -> String;
    /// Write an audit journal entry.
    fn audit(&self, entry: Audit);
    /// A message's text from its message file, `data` substituted.
    fn message(&self, id: &str, data: &[&str]) -> Option<SecondLevel>;
    /// System status. `reset` starts a new elapsed interval for `user`
    /// (Refresh).
    fn status(&self, user: &str, reset: bool) -> Status;
}

/// The system Navigator runs over.
#[derive(Clone)]
pub struct Env {
    /// Name, clock, sign-on, authority, audit, messages, status.
    pub system: Arc<dyn System>,
    /// Work management.
    pub work: Arc<dyn work::Work>,
    /// Message queues and the history log.
    pub msgs: Arc<dyn msgs::Msgs>,
    /// User and group profiles, authorization lists.
    pub users: Arc<dyn users::Users>,
    /// System values, disks, hardware, time.
    pub config: Arc<dyn config::Config>,
    /// TCP/IP and the TCP servers.
    pub net: Arc<dyn net::Net>,
    /// `NetServer`.
    pub netsvr: Arc<dyn netsvr::NetServer>,
    /// NFS.
    pub nfs: Arc<dyn nfs::Nfs>,
    /// The directory server.
    pub dirsrv: Arc<dyn dirsrv::DirAdmin + Send + Sync>,
    /// The database.
    pub db: Arc<dyn db::Db>,
    /// The audit journal.
    pub sec: Arc<dyn sec::Security>,
    /// Licensed programs and PTFs.
    pub sw: Arc<dyn sw::Software>,
    /// Performance.
    pub perf: Arc<dyn perf::Perf>,
}
