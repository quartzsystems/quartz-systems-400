// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QAP's `Manage` service (docs/QAP.md, version 7): the parts the
//! connection job cannot read through the requests the 5250 commands
//! use. `ListPtfs` reads the PTF database as `DSPPTF` does,
//! `ListDiskUnits` the disk configuration as DST shows it (without a
//! service tools sign-on, as `WRKDSKSTS` needs none), `IplStatus` the
//! LIC record and `QABNORMSW`, and `PowerDown` checks `*JOBCTL` as
//! `PWRDWNSYS` does (a refusal audited `AF K`) before it hands the
//! power down to `slic`. The rest runs in the job (`work/qsjob/src/work/qap/manage.rs`).

use qap_proto::ops::manage::{
    Asp, DiskUnit, DiskUnits, EndHow, IplAction, IplMode, IplSource, IplStatus, PowerDown, PreviousEnd, Protection,
    Ptf, PtfStatus, UnitStatus,
};
use stg::QapRefused;
use svctools::DstEnv;

use super::super::served::JobCtx;
use super::{ERROR, NOT_AUTHORIZED, connection_job, refused};
use crate::ipl::system::System;

/// A message (`CPFnnnn text`) as the client is told it.
fn failed(status: u16, e: &str) -> QapRefused {
    let id = e
        .split_whitespace()
        .next()
        .filter(|w| w.len() == 7 && w.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("CPF9898");
    let text = e.strip_prefix(id).unwrap_or(e).trim();
    let status = if id == "CPF222E" { NOT_AUTHORIZED } else { status };
    refused(status, id, text, &[])
}

fn ptf_status(s: ptf::Status) -> PtfStatus {
    use ptf::Status as S;
    match s {
        S::Loaded => PtfStatus::NotApplied,
        S::TempApplied => PtfStatus::TemporarilyApplied,
        S::PermApplied => PtfStatus::PermanentlyApplied,
        S::TempRemoved => PtfStatus::TemporarilyRemoved,
        S::PermRemoved => PtfStatus::PermanentlyRemoved,
        S::Superseded => PtfStatus::Superseded,
        S::Damaged => PtfStatus::Damaged,
        S::NotLoaded | S::OnOrder | S::SaveFileOnly | S::CoverLetterOnly => PtfStatus::SaveFileOnly,
    }
}

/// `ListPtfs`: the PTFs of `product` (`*ALL` every product), by
/// product then identifier.
pub fn ptfs(system: &mut System, ctx: &JobCtx, product: &str) -> Result<Vec<Ptf>, QapRefused> {
    connection_job(ctx)?;
    let db = crate::cpp::ptfcmd::load_db(system).map_err(|e| failed(ERROR, &e))?;
    let want = product.trim().to_ascii_uppercase();
    let all = want.is_empty() || want == "*ALL";
    let mut out: Vec<Ptf> = db
        .ptfs
        .values()
        .filter(|r| all || r.product == want)
        .map(|r| Ptf {
            product: r.product.clone(),
            id: r.ptf.clone(),
            release: r.release.clone(),
            status: ptf_status(r.status),
            ipl_action: match r.action {
                ptf::Action::None => IplAction::None,
                ptf::Action::ApplyAtIpl => IplAction::Apply,
                ptf::Action::RemoveAtIpl => IplAction::Remove,
            },
            delayed: r.delayed,
            text: r.status.display().to_string(),
        })
        .collect();
    out.sort_by(|a, b| (&a.product, &a.id).cmp(&(&b.product, &b.id)));
    Ok(out)
}

fn unit_status(s: &str) -> UnitStatus {
    let s = s.to_ascii_lowercase();
    if s.contains("suspend") {
        UnitStatus::Suspended
    } else if s.contains("resum") {
        UnitStatus::Resuming
    } else if s.contains("active") || s.contains("unprotected") {
        UnitStatus::Active
    } else {
        UnitStatus::Failed
    }
}

const MIB: u64 = 1024 * 1024;

/// `ListDiskUnits`: the configured units by number, then the disks in
/// no ASP; each ASP's capacity, use and threshold.
pub fn disks(system: &mut System, ctx: &JobCtx) -> Result<DiskUnits, QapRefused> {
    connection_job(ctx)?;
    if let Some((part, record)) = system.lic.as_mut()
        && let Ok(fresh) = part.read()
    {
        *record = fresh;
    }
    let mut e = crate::ipl::dst::Env::new(system);
    let parity: Vec<u16> = e.parity_units().iter().map(|p| p.unit).collect();
    let threshold = e.asp_threshold();
    let mut configured = e.disk_units();
    configured.sort_by_key(|u| u.unit);
    let mut units: Vec<DiskUnit> = configured
        .iter()
        .map(|u| {
            let capacity = u.size_mib * MIB;
            DiskUnit {
                // Numbered from 1, as DST shows them; 0 is not configured.
                unit: u.unit + 1,
                asp: u.asp,
                resource: u.resource.clone(),
                type_model: u.type_model.clone(),
                serial: u.serial.clone(),
                capacity,
                used: capacity / 100 * u64::from(u.percent_used),
                status: unit_status(&u.status),
                protection: if parity.contains(&u.unit) {
                    Protection::Parity
                } else if u.mirrored {
                    Protection::Mirrored
                } else {
                    Protection::Unprotected
                },
            }
        })
        .collect();
    let mut asps: Vec<Asp> = Vec::new();
    for u in &units {
        // A mirrored pair holds its data once.
        if u.protection == Protection::Mirrored
            && configured
                .iter()
                .any(|c| c.unit + 1 == u.unit && c.partner.is_some_and(|p| p < c.unit))
        {
            continue;
        }
        match asps.iter_mut().find(|a| a.asp == u.asp) {
            Some(a) => {
                a.capacity += u.capacity;
                a.used += u.used;
            }
            None => asps.push(Asp {
                asp: u.asp,
                capacity: u.capacity,
                used: u.used,
                threshold,
            }),
        }
    }
    units.extend(e.nonconfigured().into_iter().map(|d| DiskUnit {
        resource: d.resource,
        type_model: d.type_model,
        serial: d.serial,
        capacity: d.size_mib * MIB,
        status: UnitStatus::NonConfigured,
        ..DiskUnit::default()
    }));
    Ok(DiskUnits { units, asps })
}

/// `IplStatus`: the LIC record's last IPL, the slot booted, the one the
/// next IPL boots once, how the previous IPL ended (`QABNORMSW`) and
/// whether every subsystem has ended (restricted state). `ipl_time` is
/// the IPL's start as the record has it; the job puts `SCPF`'s start.
pub fn ipl(system: &mut System, ctx: &JobCtx) -> Result<IplStatus, QapRefused> {
    connection_job(ctx)?;
    let next = ptf::slot::next_slot(&crate::cpp::ptflic::load(system), None).map(|s| format!("{s:?}"));
    if let Some((part, record)) = system.lic.as_mut()
        && let Ok(fresh) = part.read()
    {
        *record = fresh;
    }
    let mut st = IplStatus {
        slot: format!("{:?}", system.boot.slot),
        next_slot: next,
        restricted: system.sbs.is_empty(),
        ..IplStatus::default()
    };
    if let Some((_, record)) = system.lic.as_ref() {
        let h = &record.header;
        st.ipl_type = format!("{:?}", h.last_ipl_type);
        st.mode = match h.last_ipl_mode {
            licpart::IplMode::Normal => IplMode::Normal,
            licpart::IplMode::Manual => IplMode::Manual,
        };
        st.slot = format!("{:?}", h.last_slot);
        if let Some(last) = record.history.last() {
            st.ipl_time = i64::try_from(last.start_time).unwrap_or(0).saturating_mul(1000);
            st.last_src = format!("{:<8}", last.final_src_str());
            st.src_text = crate::src::lookup(last.final_src_str()).map_or_else(String::new, |s| s.text.to_string());
        }
    }
    let abnormal = system
        .sysvals
        .as_ref()
        .and_then(|v| v.get("QABNORMSW").ok())
        .is_some_and(|v| v.text() == "1");
    st.previous_end = if abnormal { PreviousEnd::Abnormal } else { PreviousEnd::Normal };
    Ok(st)
}

/// `PowerDown`: `*JOBCTL` as `PWRDWNSYS` needs it, then the request to
/// `slic`, as `PWRDWNSYS` from a job not at the console makes it.
pub fn power(system: &mut System, ctx: &JobCtx, req: &PowerDown) -> Result<(), QapRefused> {
    connection_job(ctx)?;
    // The job's own profile, whatever the request runs as.
    let me = ctx.session.borrow().principal.clone();
    me.require(sec::profile::SpecialAuth::JOBCTL, "power down the system")
        .map_err(|e| failed(NOT_AUTHORIZED, &e.to_string()))?;
    if std::path::Path::new(crate::ipl::power::REQUEST_FILE).exists() {
        return Err(refused(ERROR, "CPF9898", "A power down of the system has already started.", &[]));
    }
    system.current_job = ctx.job.number;
    let iplsrc = match req.ipl_source {
        IplSource::Panel => 0,
        IplSource::A => 1,
        IplSource::B => 2,
    };
    let immediate = req.how == EndHow::Immediate;
    let delay = req.delay.unwrap_or(3600);
    std::fs::write(
        crate::ipl::power::REQUEST_FILE,
        format!("{} {delay} {} {iplsrc}\n", u8::from(immediate), u8::from(req.restart)),
    )
    .map_err(|e| failed(ERROR, &e.to_string()))?;
    rustix::process::kill_process(rustix::process::Pid::INIT, rustix::process::Signal::USR1)
        .map_err(|e| failed(ERROR, &e.to_string()))
}
