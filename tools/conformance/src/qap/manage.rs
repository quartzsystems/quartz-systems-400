// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 123: the `Manage` service (version 7) through the real
//! connection jobs. As `QAPTEST` (no special authority): the subsystems
//! with the connection's own `QUSRWRK` active, system values by name
//! and `CPF1842` for one not there, the hardware resources (all, and
//! the communications ones), the disk units, the IPL status;
//! `PowerDown`, `StartSubsystem`, `SetSystemValue` and `ListPtfs`
//! (`DSPPTF` is `*PUBLIC *EXCLUDE`) refused `NotAuthorized`, and nothing
//! powered down. As `QSECOFR`: the PTFs, and `QPRTTXT` set and read
//! back, then set as it was. Then `*PUBLIC *EXCLUDE` on a command, a
//! program and a library refuses `QAPTEST`'s `RunCommand` and not the
//! security officer's.

use qap_proto::ops::manage::{
    DiskUnits, EndHow, GetSystemValues, HardwareResources, IplSource, IplStatus,
    ListHardwareResources, ListPtfs, ListSubsystems, PowerDown, Ptfs, ResourceType, SetSystemValue,
    SubsystemRef, SubsystemStatus, Subsystems, SystemValue, SystemValues,
};
use qap_proto::types::ObjectName;
use qap_proto::{Op, Status};

use super::client::{Conn, brief, msgid};

fn refused(c: &mut Conn, what: &str, op: Op, body: &impl ipc::Wire) -> Result<String, String> {
    let r = c.call(op, body)?;
    if r.status == Status::NotAuthorized {
        Ok(format!("{what} refused ({})", msgid(&r)))
    } else {
        Err(format!(
            "{what} as a user without the authority: {}",
            brief(&r)
        ))
    }
}

/// Run the step on `a` (`QAPTEST`) and `sec` (`QSECOFR`), both signed on.
pub fn run(a: &mut Conn, sec: &mut Conn) -> Result<String, String> {
    let (lists, ipl) = lists(a)?;
    let refusals = refusals(a, &ipl)?;
    let ptfs = secofr(sec)?;
    Ok(format!(
        "{lists}; {refusals}; QSECOFR: {ptfs} PTFs, QPRTTXT set and restored"
    ))
}

fn all_ptfs() -> ListPtfs {
    ListPtfs {
        product: "*ALL".into(),
        start: 0,
        max: 0,
    }
}

/// The lists and the IPL status; the summary and the status.
fn lists(a: &mut Conn) -> Result<(String, IplStatus), String> {
    let sbs: Subsystems = a.ok(
        Op::ListSubsystems,
        &ListSubsystems {
            library: "*ALL".into(),
            start: 0,
            max: 0,
        },
    )?;
    let usrwrk = sbs
        .subsystems
        .iter()
        .find(|s| s.sbsd.name == "QUSRWRK")
        .ok_or_else(|| format!("no QUSRWRK among {:?}", sbs.subsystems))?;
    if usrwrk.status != SubsystemStatus::Active || usrwrk.monitor.is_none() {
        return Err(format!("QUSRWRK: {usrwrk:?}"));
    }
    let active = sbs
        .subsystems
        .iter()
        .filter(|s| s.status == SubsystemStatus::Active)
        .count();
    if sbs
        .subsystems
        .iter()
        .take(active)
        .any(|s| s.status != SubsystemStatus::Active)
    {
        return Err("active subsystems do not come first".into());
    }
    let vals: SystemValues = a.ok(
        Op::GetSystemValues,
        &GetSystemValues {
            names: vec!["QSYSNAME".into(), "QCCSID".into(), "QAUDLVL".into()],
        },
    )?;
    let names: Vec<&str> = vals.values.iter().map(|v| v.name.as_str()).collect();
    if names != ["QSYSNAME", "QCCSID", "QAUDLVL"]
        || vals.values[0].value.is_empty()
        || vals.values[0].category.is_empty()
    {
        return Err(format!("GetSystemValues: {:?}", vals.values));
    }
    let unknown = a.call(
        Op::GetSystemValues,
        &GetSystemValues {
            names: vec!["QSYSNAME".into(), "QNOSUCHVAL".into()],
        },
    )?;
    if unknown.status != Status::NotFound || msgid(&unknown) != "CPF1842" {
        return Err(format!("an unknown system value: {}", brief(&unknown)));
    }
    let all: HardwareResources = a.ok(
        Op::ListHardwareResources,
        &ListHardwareResources {
            kind: ResourceType::All,
        },
    )?;
    let cmn: HardwareResources = a.ok(
        Op::ListHardwareResources,
        &ListHardwareResources {
            kind: ResourceType::Communication,
        },
    )?;
    if all.resources.is_empty()
        || cmn.resources.is_empty()
        || all
            .resources
            .iter()
            .any(|r| r.depth > 0 && r.parent.is_empty())
    {
        return Err(format!(
            "hardware: {} in all, {} *CMN",
            all.resources.len(),
            cmn.resources.len()
        ));
    }
    let disks: DiskUnits = a.ok(Op::ListDiskUnits, &())?;
    if !disks.units.iter().any(|u| u.unit > 0 && u.capacity > 0) || disks.asps.is_empty() {
        return Err(format!("disk units: {disks:?}"));
    }
    let ipl: IplStatus = a.ok(Op::IplStatus, &())?;
    if ipl.restricted || ipl.ipl_time <= 0 || ipl.slot.is_empty() {
        return Err(format!("IplStatus: {ipl:?}"));
    }
    let summary = format!(
        "{} subsystems ({active} active), {} values, {} resources ({} *CMN), {} units, IPL slot {} SRC {}",
        sbs.total,
        vals.values.len(),
        all.resources.len(),
        cmn.resources.len(),
        disks.units.len(),
        ipl.slot,
        ipl.last_src.trim(),
    );
    Ok((summary, ipl))
}

/// What a user without special authority may not do; the system still
/// up after.
fn refusals(a: &mut Conn, ipl: &IplStatus) -> Result<String, String> {
    let power = refused(
        a,
        "PowerDown",
        Op::PowerDown,
        &PowerDown {
            how: EndHow::Immediate,
            delay: None,
            restart: true,
            ipl_source: IplSource::Panel,
        },
    )?;
    let strsbs = refused(
        a,
        "StartSubsystem",
        Op::StartSubsystem,
        &SubsystemRef {
            sbsd: ObjectName {
                library: "QSYS".into(),
                name: "QSERVER".into(),
            },
        },
    )?;
    let chg = refused(
        a,
        "SetSystemValue",
        Op::SetSystemValue,
        &SetSystemValue {
            name: "QPRTTXT".into(),
            value: "Not allowed".into(),
        },
    )?;
    // Still up: the refused power down did nothing.
    let after: IplStatus = a.ok(Op::IplStatus, &())?;
    if after.ipl_time != ipl.ipl_time {
        return Err(format!("the IPL changed: {after:?}"));
    }
    let ptfs = refused(a, "ListPtfs", Op::ListPtfs, &all_ptfs())?;
    Ok(format!("{power}, {strsbs}, {chg}, {ptfs}"))
}

/// The security officer lists the PTFs (`DSPPTF` is `*PUBLIC
/// *EXCLUDE`), changes a value and puts it back; the PTFs counted.
fn secofr(sec: &mut Conn) -> Result<u32, String> {
    let ptfs: Ptfs = sec.ok(Op::ListPtfs, &all_ptfs())?;
    let before: SystemValues = sec.ok(
        Op::GetSystemValues,
        &GetSystemValues {
            names: vec!["QPRTTXT".into()],
        },
    )?;
    let was = before
        .values
        .first()
        .map(|v| v.value.clone())
        .unwrap_or_default();
    let set: SystemValue = sec.ok(
        Op::SetSystemValue,
        &SetSystemValue {
            name: "QPRTTXT".into(),
            value: "Step 123".into(),
        },
    )?;
    let back: SystemValue = sec.ok(
        Op::SetSystemValue,
        &SetSystemValue {
            name: "QPRTTXT".into(),
            value: if was.is_empty() {
                " ".into()
            } else {
                was.clone()
            },
        },
    )?;
    if set.value != "Step 123" || back.value != was {
        return Err(format!("QPRTTXT: set {set:?}, back {back:?}"));
    }
    Ok(ptfs.total)
}

/// `*PUBLIC *EXCLUDE` on a command (`DSPPTF`, shipped so), a program
/// (called directly, through QCMDEXC and from a CL program), `*USE`
/// without `*EXECUTE` on a program, and `*EXCLUDE` on a library holds
/// for commands run through QAP as `QAPTEST`; the security officer
/// runs each.
pub fn exclusions(a: &mut Conn, sec: &mut Conn) -> Result<String, String> {
    let lines = [
        ("the command", "DSPPTF"),
        ("the program", "CALL PGM(QGPL/QAPXPGM)"),
        ("the program without *EXECUTE", "CALL PGM(QGPL/QAPXNOX)"),
        (
            "the program through QCMDEXC",
            "CALL PGM(QCMDEXC) PARM('CALL QGPL/QAPXPGM' 17)",
        ),
        (
            "the library",
            "CHKOBJ OBJ(QAPXLIB/QAPXDTA) OBJTYPE(*DTAARA)",
        ),
    ];
    let mut out = Vec::new();
    for (what, line) in lines {
        let (status, id) = super::command_fails(a, line)?;
        if matches!(id.as_str(), "CPF9801" | "CPF9810" | "CPD0030" | "CPF2105") {
            return Err(format!(
                "{line} as QAPTEST: {id}, as if {what} were not there"
            ));
        }
        // CPD0032 for the command, CPD0171 then CPF0001 for the program
        // (pub400, tools/conformance/oracle/sec/call-authority.md).
        if what != "the library" && status != Status::NotAuthorized {
            return Err(format!(
                "{line} as QAPTEST: {status:?} {id}, not NotAuthorized"
            ));
        }
        super::command(sec, line).map_err(|e| format!("as QSECOFR: {e}"))?;
        out.push(format!("{what} refused ({status:?} {id})"));
    }
    // A CL program's CALL of it: CPF0001, which the program monitors.
    super::command(a, "CALL PGM(QGPL/QAPXCL)")
        .map_err(|e| format!("the CL program as QAPTEST: {e}"))?;
    out.push("a CL program's CALL of it ends in CPF0001".into());
    Ok(format!(
        "*PUBLIC *EXCLUDE through RunCommand: {}",
        out.join(", ")
    ))
}
