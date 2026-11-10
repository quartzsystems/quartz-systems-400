// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the APIs need from the system: the configuration files, the
//! caller's authority, objects and profiles, the session table and the
//! server jobs. The storage service implements [`Host`]; tests use
//! [`MemHost`].

use super::error::{ApiError, msg};
use crate::model::config::Config;
use crate::model::session::SessionTable;
use crate::model::share::Qualified;

/// Who called the API.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Caller {
    /// The profile.
    pub user: String,
    /// `*IOSYSCFG` special authority.
    pub iosyscfg: bool,
    /// `*SECADM` special authority.
    pub secadm: bool,
}

/// Whether a profile may be the guest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuestCheck {
    /// It may.
    Usable,
    /// It does not exist or the caller has no `*USE` to it (`CPFB687`).
    NotFound,
    /// It (or its group) has this special authority (`CPFB68C`).
    Special(String),
}

/// What enabling a user for NetServer did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Enabled {
    /// The user can sign on again.
    Done,
    /// The caller lacks `*OBJMGT` and `*USE` to the profile (`CPDB680`).
    NotAuthorized,
    /// It failed otherwise (no such profile, ...).
    Failed(String),
}

/// The `QZLSSERVER` job: started, ended, asked to end a session. The
/// `QZLSSERVER`/`QZLSFILE` jobs implement it; [`NoServer`] stands in
/// until then.
pub trait ServerControl {
    /// Whether `QZLSSERVER` is active.
    fn active(&mut self) -> bool;
    /// Start it; `reset` when the pending attributes were just made
    /// current.
    fn start(&mut self, reset: bool) -> Result<(), ApiError>;
    /// End it and every session.
    fn end(&mut self) -> Result<(), ApiError>;
    /// Ask job `job` to end session `id`.
    fn end_session(&mut self, job: &str, id: u64) -> Result<(), ApiError>;
}

/// The server control when no `QZLSSERVER` job can run: never active,
/// and start, end and end-session fail with `CPFB680` (the required
/// job has ended).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoServer;

impl ServerControl for NoServer {
    fn active(&mut self) -> bool {
        false
    }

    fn start(&mut self, _reset: bool) -> Result<(), ApiError> {
        Err(ApiError::new(msg::JOB_ENDED))
    }

    fn end(&mut self) -> Result<(), ApiError> {
        Err(ApiError::new(msg::JOB_ENDED))
    }

    fn end_session(&mut self, _job: &str, _id: u64) -> Result<(), ApiError> {
        Err(ApiError::new(msg::JOB_ENDED))
    }
}

/// The system, as the APIs see it.
pub trait Host {
    /// The configuration (the shipped one when there are no files).
    fn load(&mut self) -> Result<Config, ApiError>;
    /// Write the configuration.
    fn save(&mut self, c: &Config) -> Result<(), ApiError>;
    /// The caller.
    fn caller(&mut self) -> Caller;
    /// The owner of an IFS path; `None` when it does not exist.
    fn path_owner(&mut self, path: &str) -> Option<String>;
    /// The owner of an output queue; `None` when it does not exist.
    fn outq_owner(&mut self, q: &Qualified) -> Option<String>;
    /// Whether authorization list `name` exists.
    fn autl_exists(&mut self, name: &str) -> bool;
    /// Whether `name` may be the guest profile.
    fn guest_check(&mut self, name: &str) -> GuestCheck;
    /// Enable `name` for NetServer again.
    fn enable_user(&mut self, name: &str) -> Enabled;
    /// The users disabled for NetServer.
    fn disabled_users(&mut self) -> Vec<String>;
    /// The session table.
    fn sessions(&mut self) -> &mut SessionTable;
    /// The server jobs.
    fn server(&mut self) -> &mut dyn ServerControl;
    /// Now, seconds since the epoch.
    fn now(&mut self) -> u64;
    /// A time as `CYYMMDDHHMMSS` in the system's time zone.
    fn stamp(&mut self, unix: u64) -> String;
    /// The job's CCSID (paths in the list formats use it).
    fn job_ccsid(&mut self) -> u16 {
        37
    }
}

/// A [`Host`] in memory.
#[derive(Debug)]
pub struct MemHost<S: ServerControl = NoServer> {
    /// The configuration; `None` until saved.
    pub config: Option<Config>,
    /// The server name a first configuration takes.
    pub system_name: String,
    /// The caller.
    pub caller: Caller,
    /// IFS paths and owners.
    pub paths: Vec<(String, String)>,
    /// Output queues and owners.
    pub outqs: Vec<(Qualified, String)>,
    /// Authorization lists.
    pub autls: Vec<String>,
    /// Profiles and their special authorities (`*ALLOBJ` ...).
    pub profiles: Vec<(String, Vec<String>)>,
    /// Users disabled for NetServer.
    pub disabled: Vec<String>,
    /// Profiles the caller may not enable.
    pub not_manageable: Vec<String>,
    /// The session table.
    pub table: SessionTable,
    /// The server.
    pub server: S,
    /// Now.
    pub clock: u64,
    /// Saves so far.
    pub saves: usize,
}

impl MemHost<NoServer> {
    /// A host with no files, the caller `QSECOFR` with every authority.
    #[must_use]
    pub fn new(system_name: &str) -> Self {
        Self::with_server(system_name, NoServer)
    }
}

impl<S: ServerControl> MemHost<S> {
    /// A host with server control `server`.
    #[must_use]
    pub fn with_server(system_name: &str, server: S) -> Self {
        Self {
            config: None,
            system_name: system_name.into(),
            caller: Caller {
                user: "QSECOFR".into(),
                iosyscfg: true,
                secadm: true,
            },
            paths: vec![("/QIBM".into(), "QSYS".into())],
            outqs: Vec::new(),
            autls: Vec::new(),
            profiles: Vec::new(),
            disabled: Vec::new(),
            not_manageable: Vec::new(),
            table: SessionTable::new(),
            server,
            clock: 1_762_675_200,
            saves: 0,
        }
    }
}

impl<S: ServerControl> Host for MemHost<S> {
    fn load(&mut self) -> Result<Config, ApiError> {
        Ok(self
            .config
            .clone()
            .unwrap_or_else(|| Config::shipped(&self.system_name)))
    }

    fn save(&mut self, c: &Config) -> Result<(), ApiError> {
        self.config = Some(c.clone());
        self.saves += 1;
        Ok(())
    }

    fn caller(&mut self) -> Caller {
        self.caller.clone()
    }

    fn path_owner(&mut self, path: &str) -> Option<String> {
        self.paths
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, o)| o.clone())
    }

    fn outq_owner(&mut self, q: &Qualified) -> Option<String> {
        self.outqs
            .iter()
            .find(|(x, _)| x == q)
            .map(|(_, o)| o.clone())
    }

    fn autl_exists(&mut self, name: &str) -> bool {
        self.autls.iter().any(|a| a == name)
    }

    fn guest_check(&mut self, name: &str) -> GuestCheck {
        match self.profiles.iter().find(|(p, _)| p == name) {
            None => GuestCheck::NotFound,
            Some((_, sp)) if !sp.is_empty() => GuestCheck::Special(sp[0].clone()),
            Some(_) => GuestCheck::Usable,
        }
    }

    fn enable_user(&mut self, name: &str) -> Enabled {
        if self.not_manageable.iter().any(|n| n == name) {
            return Enabled::NotAuthorized;
        }
        if !self.profiles.iter().any(|(p, _)| p == name) {
            return Enabled::Failed(format!("no profile {name}"));
        }
        self.disabled.retain(|d| d != name);
        Enabled::Done
    }

    fn disabled_users(&mut self) -> Vec<String> {
        self.disabled.clone()
    }

    fn sessions(&mut self) -> &mut SessionTable {
        &mut self.table
    }

    fn server(&mut self) -> &mut dyn ServerControl {
        &mut self.server
    }

    fn now(&mut self) -> u64 {
        self.clock
    }

    fn stamp(&mut self, unix: u64) -> String {
        stamp_utc(unix)
    }
}

/// A time as `CYYMMDDHHMMSS` in UTC (century digit 1 for 2000 to 2099).
#[must_use]
pub fn stamp_utc(unix: u64) -> String {
    let days = i64::try_from(unix / 86_400).unwrap_or(0);
    let secs = unix % 86_400;
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let c = i32::from(y >= 2000);
    format!(
        "{c}{:02}{m:02}{d:02}{:02}{:02}{:02}",
        y % 100,
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_are_cyymmddhhmmss() {
        assert_eq!(stamp_utc(0), "0700101000000");
        assert_eq!(stamp_utc(1_794_213_478), "1261109083758");
    }
}
