// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The service's side of directory publishing (docs/DIRSRV.md §10.1):
//! `publish.conf` with the passwords sealed by the LIC partition's key,
//! the profiles and printer devices to publish with the log of their
//! changes (`dirsrv::publish::Tracker`), and the agents' last statuses.
//! Requests 1570-1572 come only from jobs running as `QDIRSRV`.

use std::sync::{LazyLock, Mutex, MutexGuard};

use dirsrv::publish::config::{self, Kind, PublishConfig};
use dirsrv::publish::{Change, Cursor, Object, PrinterInfo, Tracker, UserInfo};
use stg::JobInfo;
use stg::dirpub::{PubChange, PubChanges, PubConfig, PubCursor, PubListing, PubObject, PubStatus};
use store::obj::types;

use crate::ipl::system::{System, now_unix};

/// The profile the publishing jobs run as.
pub const USER: &str = "QDIRSRV";

struct State {
    tracker: Tracker,
    /// The security stamp profiles were last listed at.
    stamp: Option<(u64, u64)>,
    /// Each agent's last status and when it came.
    status: [Option<(u64, PubStatus)>; 2],
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| {
    // Nanoseconds at start: a cursor of an earlier service never matches.
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX));
    Mutex::new(State {
        tracker: Tracker::new(epoch),
        stamp: None,
        status: [None, None],
    })
});

fn state() -> MutexGuard<'static, State> {
    STATE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn only_qdirsrv(job: &JobInfo) -> Result<(), String> {
    if job.user.trim().eq_ignore_ascii_case(USER) {
        Ok(())
    } else {
        Err(format!("CPF2189 {USER}"))
    }
}

fn kind_of(agent: &str) -> Result<Kind, String> {
    Kind::from_name(agent).ok_or_else(|| format!("CPF9898 {agent}"))
}

/// The configuration as the file has it; none when there is no file.
///
/// # Errors
///
/// The IFS's, or a file that is not in the format.
pub fn read_config(system: &mut System) -> Result<Option<PublishConfig>, String> {
    let b = crate::stgsvc::certs::as_system(system, |c| {
        crate::stgsvc::certs::read_file(c, config::PATH)
    })?;
    match b {
        None => Ok(None),
        Some(b) => config::parse(&b)
            .map(Some)
            .map_err(|e| format!("CPF9898 {}: {e}", config::PATH)),
    }
}

/// Keep `cfg`; each `(kind, password)` in `passwords` seals a new
/// password for that agent (none removes it). For `dir_admin`,
/// Navigator and QAP.
///
/// # Errors
///
/// No key in the LIC partition, or the IFS's.
pub fn write_config(
    system: &mut System,
    mut cfg: PublishConfig,
    passwords: &[(Kind, Option<String>)],
) -> Result<(), String> {
    for (kind, pw) in passwords {
        cfg.agent_mut(*kind).bind_pw = match pw {
            None => None,
            Some(p) => {
                let key = crate::stgsvc::certs::sealer(system).ok_or_else(|| {
                    eprintln!("slic: {}: the LIC partition holds no key", config::PATH);
                    format!("GLD0127 {}", config::PATH)
                })?;
                Some(config::seal_password(&key, *kind, p).map_err(|e| format!("CPF9898 {e}"))?)
            }
        };
    }
    let text = config::write(&cfg);
    crate::stgsvc::certs::as_system(system, |c| {
        crate::stgsvc::certs::write_file(c, config::PATH, text.as_bytes())
    })
}

/// 1570: the file's text and the passwords opened.
///
/// # Errors
///
/// Not `QDIRSRV`, a bad file, no key, or a password that does not open.
pub fn config(system: &mut System, job: &JobInfo) -> Result<PubConfig, String> {
    only_qdirsrv(job)?;
    let cfg = if let Some(c) = read_config(system)? {
        c
    } else {
        // The shipped file, so there is one to change.
        let c = PublishConfig::default();
        if let Err(e) = write_config(system, c.clone(), &[]) {
            eprintln!("slic: {}: {e}", config::PATH);
        }
        c
    };
    let mut passwords = Vec::new();
    for a in &cfg.agents {
        let Some(sealed) = &a.bind_pw else { continue };
        let key = crate::stgsvc::certs::sealer(system).ok_or_else(|| {
            eprintln!("slic: {}: the LIC partition holds no key", config::PATH);
            format!("GLD0127 {}", config::PATH)
        })?;
        let pw = config::unseal_password(&key, a.kind, sealed).map_err(|e| {
            eprintln!(
                "slic: {}: the {} agent's password does not open: {e}",
                config::PATH,
                a.kind.name()
            );
            format!("GLD0127 {}", config::PATH)
        })?;
        passwords.push((a.kind.name().to_string(), pw));
    }
    // The job is given the passwords opened, not the sealed ones.
    let mut open = cfg;
    for a in &mut open.agents {
        a.bind_pw = None;
    }
    Ok(PubConfig {
        text: config::write(&open),
        passwords,
        system: system.system_name(),
    })
}

/// 1571: the changes after `since`, the status kept.
///
/// # Errors
///
/// Not `QDIRSRV`, an unknown agent, or the store's.
pub fn events(
    system: &mut System,
    job: &JobInfo,
    agent: &str,
    since: PubCursor,
    status: PubStatus,
) -> Result<PubChanges, String> {
    only_qdirsrv(job)?;
    let kind = kind_of(agent)?;
    // A new error is logged once.
    let before = self::status(kind).and_then(|(_, s)| s.last_error);
    if let Some((_, e)) = &status.last_error
        && before.as_ref() != status.last_error.as_ref()
    {
        eprintln!("slic: {}: {e}", kind.job());
    }
    state().status[kind as usize] = Some((now_unix(), status));
    take(system, kind)?;
    let c = state().tracker.since(
        kind,
        Cursor {
            epoch: since.epoch,
            seq: since.seq,
        },
    );
    Ok(PubChanges {
        next: cursor(c.next),
        resync: c.resync,
        changes: c.changes.into_iter().map(change).collect(),
    })
}

/// 1572: every object of `agent`.
///
/// # Errors
///
/// Not `QDIRSRV`, an unknown agent, or the store's.
pub fn list(system: &mut System, job: &JobInfo, agent: &str) -> Result<PubListing, String> {
    only_qdirsrv(job)?;
    let kind = kind_of(agent)?;
    take(system, kind)?;
    let l = state().tracker.listing(kind);
    Ok(PubListing {
        cursor: cursor(l.cursor),
        objects: l.objects.into_iter().map(object).collect(),
    })
}

/// An agent's last status and when it was reported (Navigator, QAP).
#[must_use]
pub fn status(kind: Kind) -> Option<(u64, PubStatus)> {
    state().status[kind as usize].clone()
}

/// Take a fresh list of `kind` into the tracker: profiles only when one
/// was written since the last list, devices each time.
fn take(system: &mut System, kind: Kind) -> Result<(), String> {
    match kind {
        Kind::Users => {
            let stamp = qsyslib::env::stamp();
            {
                let s = state();
                if s.stamp == Some(stamp) && s.tracker.listed(kind) {
                    return Ok(());
                }
            }
            let v = users(system)?;
            let mut s = state();
            s.tracker.update(kind, v);
            s.stamp = Some(stamp);
        }
        Kind::Printers => {
            let v = printers(system)?;
            state().tracker.update(kind, v);
        }
    }
    Ok(())
}

/// Every profile but the shipped ones and the groups.
fn users(system: &mut System) -> Result<Vec<Object>, String> {
    use sec::profile::{groups, objects, shipped};
    sec::guard::as_system(|| {
        crate::stgsvc::spool::with_store(system, |txn, qsys| {
            let mut v = Vec::new();
            for n in objects::list(txn, qsys).map_err(|e| e.to_string())? {
                if shipped::shipped(&n).is_some()
                    || groups::is_group(txn, qsys, &n).unwrap_or(false)
                {
                    continue;
                }
                if let Ok((_, p)) = objects::read(txn, qsys, &n) {
                    v.push(Object::User(UserInfo {
                        name: p.name.clone(),
                        text: p.text.trim().to_string(),
                        // QS/400 keeps no SMTP name for a profile.
                        mail: None,
                    }));
                }
            }
            Ok(v)
        })
    })
}

/// `lpd://HOST` when the LPD server starts with TCP/IP and the system
/// has a host name.
fn lpd_base(system: &mut System) -> Option<String> {
    let cfg = crate::stgsvc::tcp::config(system).ok()?;
    if !cfg.servers.get("*LPD").is_some_and(|s| s.autostart) {
        return None;
    }
    let host = cfg.domain.host_name.trim();
    if host.is_empty() {
        return None;
    }
    let domain = cfg.domain.domain_name.trim();
    Some(if domain.is_empty() {
        format!("lpd://{host}")
    } else {
        format!("lpd://{host}.{domain}")
    })
    .map(|s| s.to_ascii_lowercase())
}

/// Every printer device with an output queue but the virtual printers
/// of display sessions and the capture device.
fn printers(system: &mut System) -> Result<Vec<Object>, String> {
    use store::obj::library::Library;
    let lpd = lpd_base(system);
    sec::guard::as_system(|| {
        crate::stgsvc::spool::with_store(system, |txn, qsys| {
            let lib = Library::load(txn, qsys).map_err(|e| e.to_string())?;
            let mut v = Vec::new();
            for (key, id) in lib
                .list(txn, Some(types::DEVD))
                .map_err(|e| e.to_string())?
            {
                let name = key.name.to_string();
                let Ok(b) = wm::obj::read(txn, id) else {
                    continue;
                };
                let Ok(d) = wm::obj::devd::Devd::decode(&b) else {
                    continue;
                };
                if !d.is_printer()
                    || d.is_display()
                    || d.devcls == "*VRT"
                    || name == wm::obj::devd::CAPTURE
                {
                    continue;
                }
                let outq = d.outq.trim();
                if outq.is_empty() || outq.eq_ignore_ascii_case("*NONE") {
                    continue;
                }
                let queue = outq.rsplit('/').next().unwrap_or(outq).to_string();
                let make_model = d
                    .printer
                    .as_ref()
                    .map(|p| p.mfrtypmdl.trim().to_string())
                    .filter(|m| !m.is_empty());
                v.push(Object::Printer(PrinterInfo {
                    device: name,
                    text: d.text.trim().to_string(),
                    outq: outq.to_string(),
                    make_model,
                    uri: lpd.as_ref().map(|b| format!("{b}/{queue}")),
                }));
            }
            Ok(v)
        })
    })
}

const fn cursor(c: Cursor) -> PubCursor {
    PubCursor {
        epoch: c.epoch,
        seq: c.seq,
    }
}

fn object(o: Object) -> PubObject {
    match o {
        Object::User(u) => PubObject::User {
            name: u.name,
            text: u.text,
            mail: u.mail,
        },
        Object::Printer(p) => PubObject::Printer {
            device: p.device,
            text: p.text,
            outq: p.outq,
            make_model: p.make_model,
            uri: p.uri,
        },
    }
}

fn change(c: Change) -> PubChange {
    match c {
        Change::Created(o) => PubChange::Created { object: object(o) },
        Change::Changed(o) => PubChange::Changed { object: object(o) },
        Change::Deleted(_, name) => PubChange::Deleted { name },
    }
}
