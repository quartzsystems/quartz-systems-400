// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SST's storage requests (420-439): DST's environment (`ipl::dst::Env`)
//! over the running system, so `STRSST` in a job reads and changes the
//! disks, the logs and the service tools user IDs through the functions
//! DST uses. Each job signs a service tools user on first; every
//! request then needs the job's user to have `*SERVICE` and the signed-on
//! user the privilege for it. The LIC record is read afresh before each
//! use, since `slic` writes it too.

use std::collections::BTreeMap;
use std::sync::Mutex;

use sec::profile::SpecialAuth;
use svctools::env::{DiskUnit, LogEntry, NonConfigured, ResourceRow, UserChange};
use svctools::users::{SignOnError, normalize_name};
use svctools::{DstEnv, Privilege};

use crate::ipl::dst::Env;
use crate::ipl::system::System;

/// The service tools user each job signed on, by job number.
static SESSIONS: Mutex<BTreeMap<u32, String>> = Mutex::new(BTreeMap::new());

fn sessions() -> std::sync::MutexGuard<'static, BTreeMap<u32, String>> {
    SESSIONS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The job's user may use the service tools at all.
fn service() -> Result<(), String> {
    super::security::require(SpecialAuth::SERVICE, "use the system service tools")
}

/// DST's environment over the newest LIC record.
fn env(system: &mut System) -> Env<'_> {
    if let Some((part, record)) = system.lic.as_mut()
        && let Ok(fresh) = part.read()
    {
        *record = fresh;
    }
    Env::new(system)
}

/// The signed-on user, holding `p` as the user IDs say now (a user
/// disabled or changed meanwhile has what the change left).
pub(crate) fn need(system: &mut System, job: u32, p: Privilege) -> Result<String, String> {
    service()?;
    let Some(user) = sessions().get(&job).cloned() else {
        return Err("No service tools user is signed on in this job".into());
    };
    let mut e = env(system);
    let have = e
        .users()
        .get(&user)
        .filter(|u| !u.disabled)
        .map(|u| u.privileges);
    match have {
        None => {
            sessions().remove(&job);
            Err(format!("Service tools user ID {user} is no longer signed on"))
        }
        Some(h) if !h.has(p) => Err(svctools::dst::NOT_AUTHORIZED.to_string()),
        Some(_) => Ok(user),
    }
}

/// Hand the service tools events to the audit journal; an SST sign-on
/// is recorded as `SS`, not DST's `DS`.
fn audit(system: &mut System) {
    let events: Vec<(String, String, String)> = std::mem::take(&mut system.audit_lic)
        .into_iter()
        .map(|(k, c, u)| {
            let c = if k == "ST" && c == "DS" { "SS".into() } else { c };
            (k, c, u)
        })
        .collect();
    super::security::audit_lic(&events);
}

/// Sign a service tools user on for `job`.
pub fn sign_on(
    system: &mut System,
    job: u32,
    user: &str,
    password: &str,
) -> Result<Privilege, SignOnError> {
    if let Err(e) = service() {
        // The job refuses first with the message; here it only fails.
        eprintln!("qsstg: SST sign-on refused for job {job}: {e}");
        return Err(SignOnError::NotCorrect);
    }
    let user = normalize_name(user);
    let r = env(system).sign_on(&user, password);
    audit(system);
    if r.is_ok() {
        sessions().insert(job, user);
    }
    r
}

/// Change an expired password and sign the user on.
pub fn change_password(
    system: &mut System,
    job: u32,
    user: &str,
    old: &str,
    new: &str,
) -> Result<Privilege, String> {
    service()?;
    let user = normalize_name(user);
    let r = env(system).change_password(&user, old, new);
    audit(system);
    if r.is_ok() {
        sessions().insert(job, user);
    }
    r
}

/// The job's service tools user signs off.
pub fn sign_off(job: u32) {
    sessions().remove(&job);
}

/// The units and the disks in no ASP.
pub fn disks(
    system: &mut System,
    job: u32,
) -> Result<(Vec<DiskUnit>, Vec<NonConfigured>), String> {
    need(system, job, Privilege::DISK_UNITS_OPERATIONS)?;
    let mut e = env(system);
    Ok((e.disk_units(), e.nonconfigured()))
}

/// A disk change, with the privilege DST's menus ask for it.
pub fn disk_change(
    system: &mut System,
    job: u32,
    op: &str,
    unit: u16,
    device: &str,
) -> Result<u16, String> {
    let admin = matches!(op, "add" | "remove" | "mirror" | "unmirror" | "threshold");
    let p = if admin {
        Privilege::DISK_UNITS_ADMIN
    } else {
        Privilege::DISK_UNITS_OPERATIONS
    };
    let by = need(system, job, p)?;
    let mut e = env(system);
    let r = match op {
        "add" => e.add_unit(device),
        "remove" => e.remove_unit(unit).map(|()| unit),
        "mirror" => e.start_mirror(unit, device),
        "unmirror" => e.stop_mirror(unit).map(|()| unit),
        "suspend" => e.suspend(unit).map(|()| unit),
        "resume" => e.resume(unit).map(|()| unit),
        "replace" => e.replace(unit, device).map(|()| unit),
        "threshold" => u8::try_from(unit)
            .map_err(|_| "Threshold must be a number from 1 to 100".to_string())
            .and_then(|p| e.set_asp_threshold(p))
            .map(|()| unit),
        other => Err(format!("{other} is not a disk change")),
    };
    if r.is_ok() {
        eprintln!("qsstg: SST disk change {op} unit {unit} {device} by {by}");
    }
    r
}

/// The ASP threshold and the parity-capable controllers.
pub fn disk_facts(system: &mut System, job: u32) -> Result<(u8, Vec<String>), String> {
    need(system, job, Privilege::DISK_UNITS_OPERATIONS)?;
    let mut e = env(system);
    Ok((e.asp_threshold(), e.parity_capable()))
}

/// The product activity log and the LIC log: the IPLs the record keeps
/// (this process did not run them, so their codes are not here).
pub fn logs(system: &mut System, job: u32) -> Result<(Vec<LogEntry>, Vec<String>), String> {
    need(system, job, Privilege::LOGS)?;
    let pal = env(system).activity_log();
    let mut lines =
        vec!["Started              Type Mode    Slot Final SRC  Outcome".to_string()];
    if let Some((_, record)) = system.lic.as_ref() {
        for h in &record.history {
            let (y, mo, d, hh, mm, ss) = crate::platform::rtc::unix_to_civil(h.start_time);
            lines.push(format!(
                "{y:04}-{mo:02}-{d:02} {hh:02}:{mm:02}:{ss:02}  {:<4} {:<7} {:<4} {:<10} {:?}",
                format!("{:?}", h.ipl_type),
                format!("{:?}", h.mode),
                format!("{:?}", h.slot),
                h.final_src_str(),
                h.outcome
            ));
        }
    }
    Ok((pal, lines))
}

/// The hardware resource table and the processor count.
pub fn resources(system: &mut System, job: u32) -> Result<(Vec<ResourceRow>, u32), String> {
    need(system, job, Privilege::NONE)?;
    if system.hw.resources.is_empty() {
        system.hw = crate::hwrsrc::Table::coldplug();
    }
    let rows = env(system).resources();
    let cpus = std::thread::available_parallelism()
        .map_or(1, |n| u32::try_from(n.get()).unwrap_or(u32::MAX));
    Ok((rows, cpus))
}

/// A page of a unit, or an object header.
pub fn storage(
    system: &mut System,
    job: u32,
    unit: u16,
    page: u64,
    id: &str,
) -> Result<Vec<u8>, String> {
    need(system, job, Privilege::DISPLAY_ALTER_DUMP)?;
    let mut e = env(system);
    if id.trim().is_empty() {
        e.read_page(unit, page)
    } else {
        e.object_header(id.trim())
    }
}

/// The security flags.
pub fn security(system: &mut System, job: u32) -> Result<u8, String> {
    need(system, job, Privilege::SECURITY)?;
    Ok(svctools::dst::security::encode(
        env(system).security_flags(),
    ))
}

/// The security flags changed.
pub fn set_security(system: &mut System, job: u32, flags: u8) -> Result<(), String> {
    let by = need(system, job, Privilege::SECURITY)?;
    env(system).set_security_flags(svctools::dst::security::decode(flags))?;
    eprintln!("qsstg: SST security options {flags:#04x} by {by}");
    Ok(())
}

/// The user IDs without their hashes.
pub fn users(system: &mut System, job: u32) -> Result<Vec<u8>, String> {
    need(system, job, Privilege::USER_IDS)?;
    Ok(env(system).users().without_hashes().encode())
}

/// A change to the user IDs.
pub fn user_change(
    system: &mut System,
    job: u32,
    op: &str,
    name: &str,
    password: &str,
    text: &str,
    privileges: Privilege,
) -> Result<(), String> {
    let by = need(system, job, Privilege::USER_IDS)?;
    let name = name.to_string();
    let change = match op {
        "create" => UserChange::Create {
            name: name.clone(),
            password: password.into(),
            text: text.into(),
        },
        "delete" => UserChange::Delete { name: name.clone() },
        "enable" | "disable" => UserChange::Enable {
            name: name.clone(),
            enable: op == "enable",
        },
        "privileges" => UserChange::Privileges {
            name: name.clone(),
            privileges,
        },
        "reset" => UserChange::Reset {
            name: name.clone(),
            password: password.into(),
        },
        other => return Err(format!("{other} is not a user ID change")),
    };
    let r = env(system).change_user(&change);
    audit(system);
    if r.is_ok() {
        eprintln!("qsstg: SST user ID {name}: {op} by {by}");
    }
    r
}
