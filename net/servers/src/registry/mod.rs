// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The server registry: what `STRTCPSVR SERVER(...)` names. Each entry
//! says which jobs the server runs (IBM's job names, users and
//! subsystems, as the oracle's `WRKACTJOB` shows them), its ports, the
//! command that changes its attributes, its certificate application ID,
//! its exit points, and whether this release runs it. The table itself
//! is in [`table`].

mod table;

pub use table::SPECS;

/// What a job does for its server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    /// Listens; started by `STRTCPSVR`, ended by `ENDTCPSVR`.
    Listener,
    /// Serves connections the listener hands it (`QTVDEVICE`, an FTP
    /// session job); started by the server, not the framework.
    Worker,
    /// A prestart job of its subsystem (`QRWTSRVR`, `QZDASOINIT`).
    Prestart,
}

/// One kind of server job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JobSpec {
    /// The job name; `nnn` stands for a sequence number
    /// (`QTFTP00nnn`).
    pub name: &'static str,
    /// The user profile it runs under.
    pub user: &'static str,
    /// Its subsystem.
    pub subsystem: &'static str,
    /// What it does.
    pub kind: JobKind,
    /// The job process's role (`work/qsjob/src/start.rs`) for the jobs
    /// the framework starts; empty for the others.
    pub role: &'static str,
}

impl JobSpec {
    /// The name with `nnn` filled from `n` (`QTFTP00nnn`, 1 is
    /// `QTFTP00001`).
    pub fn numbered(&self, n: u32) -> String {
        match self.name.find("nnn") {
            Some(at) => format!("{}{:03}{}", &self.name[..at], n % 1000, &self.name[at + 3..]),
            None => self.name.to_string(),
        }
    }
}

/// A port a server listens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Port {
    /// The number.
    pub port: u16,
    /// `tcp` or `udp`.
    pub protocol: &'static str,
    /// Whether TLS runs on it from the first byte.
    pub tls: bool,
}

/// Whether this release runs a server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Readiness {
    /// Built: the framework starts and ends it.
    Built,
    /// Its registration slot is here; the server itself is being built
    /// (the note says where). It starts once its [`crate::TcpServer`]
    /// is registered.
    Slot(&'static str),
    /// It waits for something else; the note says what.
    Blocked(&'static str),
    /// Not planned for this phase (docs/PHASE10.md).
    NotPlanned,
}

/// One server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerSpec {
    /// The special value (`*TELNET`).
    pub special: &'static str,
    /// The name in messages (`TELNET`).
    pub name: &'static str,
    /// Its jobs, the listener first.
    pub jobs: &'static [JobSpec],
    /// Its ports.
    pub ports: &'static [Port],
    /// The command that changes its attributes (`CHGTELNA`).
    pub attrs_cmd: Option<&'static str>,
    /// Its certificate application ID.
    pub app_id: Option<&'static str>,
    /// Its exit points.
    pub exit_points: &'static [&'static str],
    /// Whether the framework starts the listener's subsystem first.
    pub start_subsystem: bool,
    /// Its instances besides the default (`*ADMIN` for `*HTTP`).
    pub instances: &'static [&'static str],
    /// Whether this release runs it.
    pub readiness: Readiness,
}

impl ServerSpec {
    /// The job `STRTCPSVR` starts.
    pub fn listener(&self) -> Option<&'static JobSpec> {
        self.jobs.iter().find(|j| j.kind == JobKind::Listener)
    }
}

/// The entry for a special value (case does not matter).
pub fn lookup(special: &str) -> Option<&'static ServerSpec> {
    SPECS.iter().find(|s| s.special.eq_ignore_ascii_case(special.trim()))
}

/// The entry whose job `name` is (a numbered name matches its
/// pattern).
pub fn of_job(name: &str) -> Option<(&'static ServerSpec, &'static JobSpec)> {
    let name = name.trim().to_ascii_uppercase();
    SPECS.iter().find_map(|s| {
        s.jobs
            .iter()
            .find(|j| match j.name.find("nnn") {
                Some(at) => {
                    name.len() == j.name.len()
                        && name.starts_with(&j.name[..at])
                        && name[at..at + 3].bytes().all(|b| b.is_ascii_digit())
                }
                None => name == j.name,
            })
            .map(|j| (s, j))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_and_job_names() {
        assert_eq!(lookup("*telnet").unwrap().name, "TELNET");
        assert!(lookup("*NOPE").is_none());
        let ftp = lookup("*FTP").unwrap();
        assert_eq!(ftp.listener().unwrap().numbered(1), "QTFTP00001");
        assert_eq!(of_job("QTFTP00123").unwrap().0.special, "*FTP");
        assert!(of_job("QTFTP0012X").is_none());
        assert_eq!(of_job("qtvdevice").unwrap().1.kind, JobKind::Worker);
        assert_eq!(of_job("ADMIN").unwrap().0.special, "*HTTP");
    }

    #[test]
    fn every_entry_is_sound() {
        let mut seen = Vec::new();
        for s in SPECS {
            assert!(s.special.starts_with('*') && s.special.len() <= 10, "{}", s.special);
            assert!(!seen.contains(&s.special), "{} twice", s.special);
            seen.push(s.special);
            if s.readiness == Readiness::Built {
                let l = s.listener().unwrap_or_else(|| panic!("{} has no listener", s.special));
                assert!(!l.role.is_empty(), "{} listener has no role", s.special);
            }
            for j in s.jobs {
                assert!(j.name.len() <= 10, "{}", j.name);
                assert!(!j.subsystem.is_empty() && !j.user.is_empty());
            }
        }
        // Every standard server of the configuration has an entry.
        for (special, _) in tcpcfg::model::svr::STANDARD {
            assert!(lookup(special).is_some(), "{special} is not in the registry");
        }
    }
}
