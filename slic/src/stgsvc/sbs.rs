// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Subsystems in the service: which are active, which are ending, the
//! storage pools and their activity levels. Monitors register here and
//! ask what to do; `ENDSBS` marks a subsystem ending and its monitor
//! carries it out.

use std::fmt::Write as _;

use wm::obj::sbsd::{PoolKind, Sbsd};
use wm::pools::PoolTable;
use wm::sbs::{SbsInfo, SbsState};

use crate::ipl::system::System;

/// A subsystem the service knows is active.
#[derive(Clone, Debug)]
pub struct Active {
    /// Library of its description.
    pub lib: String,
    /// Its monitor job.
    pub monitor: u32,
    /// Ending: immediate, and the delay in seconds.
    pub ending: Option<(bool, u32)>,
    /// `MAXJOBS`.
    pub maxjobs: u32,
    /// Its pools: subsystem pool number and storage pool name.
    pub pools: Vec<(u8, String)>,
}

/// The pool table from the system values.
pub fn pool_table(system: &System) -> PoolTable {
    let num = |n: &str, d: u64| {
        system
            .sysvals
            .as_ref()
            .and_then(|v| v.get(n).ok())
            .and_then(|v| v.text().trim().parse().ok())
            .unwrap_or(d)
    };
    PoolTable::new(
        num("QMCHPOOL", 131_072),
        num("QBASPOOL", 262_144),
        u32::try_from(num("QBASACTLVL", 20)).unwrap_or(20),
    )
}

/// The storage pool a subsystem pool is.
pub fn pool_name(sbs: &str, id: u8, kind: PoolKind) -> String {
    match kind {
        PoolKind::Private => format!("{sbs}/{id}"),
        k => k.name(),
    }
}

/// Read a work management object's payload: `lib/name` of a type.
pub fn wm_object(system: &mut System, qual: &str, ty: &str) -> Result<Vec<u8>, String> {
    let code = store::obj::types::type_by_name(ty)
        .map(|t| t.code)
        .ok_or_else(|| format!("CPD0084 type {ty}"))?;
    let (lib, name) = wm::obj::params::qualified(qual);
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    let mut txn = s.store.begin().map_err(|e| e.to_string())?;
    let names = crate::svc::ile::name_context(&mut txn, qsys);
    let r = wm::obj::find(&mut txn, qsys, &names, &lib, &name, code)
        .and_then(|id| wm::obj::read(&mut txn, id))
        .map_err(|e| format!("{} {ty} {qual}: {e}", e.msgid().unwrap_or("CPF9801")));
    txn.abort();
    r
}

/// A subsystem that may start: its library and description.
pub fn check(system: &mut System, name: &str) -> Result<(String, Vec<u8>), String> {
    let name = name.to_ascii_uppercase();
    let bare = wm::obj::params::qualified(&name).1;
    if let Some(a) = system.sbs.get(&bare) {
        return Err(format!(
            "CPF1010 subsystem {bare} is active, monitor job {:06}",
            a.monitor
        ));
    }
    let (lib, _) = wm::obj::params::qualified(&name);
    let lib = if lib == "*LIBL" {
        "QSYS".to_string()
    } else {
        lib
    };
    let qual = format!("{lib}/{}", wm::obj::params::qualified(&name).1);
    let payload = wm_object(system, &qual, "*SBSD")
        .map_err(|_| format!("CPF1003 subsystem description {qual} not found"))?;
    Sbsd::decode(&payload).map_err(|e| e.to_string())?;
    Ok((lib, payload))
}

/// A monitor registers its subsystem: the description.
pub fn register(
    system: &mut System,
    monitor: u32,
    name: &str,
    lib: &str,
) -> Result<Vec<u8>, String> {
    if system.sbs.contains_key(name) {
        return Err(format!("CPF1010 subsystem {name} is active"));
    }
    let payload = wm_object(system, &format!("{lib}/{name}"), "*SBSD")?;
    let d = Sbsd::decode(&payload).map_err(|e| e.to_string())?;
    let pools = d
        .pools
        .iter()
        .map(|p| (p.id, pool_name(name, p.id, p.kind)))
        .collect();
    let queues: Vec<String> = d
        .jobqes
        .iter()
        .map(|q| format!("{}/{}", q.jobq.1, q.jobq.0))
        .collect();
    system.sbs.insert(
        name.to_string(),
        Active {
            lib: lib.to_string(),
            monitor,
            ending: None,
            maxjobs: d.maxjobs,
            pools,
        },
    );
    super::jobqs::attach(system, name, &queues);
    let who = crate::stgsvc::msgs::sender(system, monitor);
    let data = wm::joblog::data(&[(name, 10), (lib, 10)]);
    super::hist::system_message(system, "CPF0902", &data, "04", who);
    Ok(payload)
}

/// The monitor job `monitor` ended: its subsystem is no longer active.
pub fn monitor_ended(system: &mut System, monitor: u32) {
    let ended: Vec<String> = system
        .sbs
        .iter()
        .filter(|(_, a)| a.monitor == monitor)
        .map(|(n, _)| n.clone())
        .collect();
    for name in ended {
        if let Some(a) = system.sbs.remove(&name) {
            super::jobqs::detach(system, &name);
            for (_, p) in &a.pools {
                system.pools.drop_private(p);
            }
            let data = wm::joblog::data(&[(&name, 10), (&a.lib, 10)]);
            super::hist::system_message(
                system,
                "CPF0903",
                &data,
                "04",
                ("QSYS".into(), "QSYS".into(), monitor),
            );
        }
    }
}

/// What the subsystem of monitor `monitor` is to do.
pub fn state(system: &System, monitor: u32) -> SbsState {
    SbsState {
        end_jobs: Vec::new(),
        ending: system
            .sbs
            .values()
            .find(|a| a.monitor == monitor)
            .and_then(|a| a.ending),
    }
}

/// `ENDSBS`: `*ALL` ends every subsystem but the controlling one,
/// which needs `*ALL` to end the last.
pub fn end(system: &mut System, name: &str, immediate: bool, delay: u32) -> Result<String, String> {
    let name = name.to_ascii_uppercase();
    let ctl = controlling(system);
    if name == "*ALL" {
        let n = system.sbs.len();
        for a in system.sbs.values_mut() {
            a.ending.get_or_insert((immediate, delay));
        }
        return Ok(format!(
            "{n} subsystem(s) ending; the system goes to the restricted state.\n"
        ));
    }
    if name == ctl && system.sbs.len() > 1 {
        return Err(format!(
            "CPF1034 {name} is the controlling subsystem; end the others first, or use ENDSBS *ALL"
        ));
    }
    let a = system
        .sbs
        .get_mut(&name)
        .ok_or_else(|| format!("CPF1054 subsystem {name} is not active"))?;
    if a.ending.is_some() {
        return Err(format!("CPF1053 subsystem {name} is already ending"));
    }
    a.ending = Some((immediate, delay));
    Ok(format!(
        "Subsystem {name} is ending {}.\n",
        if immediate {
            "immediately"
        } else {
            "in a controlled manner"
        }
    ))
}

/// `QCTLSBSD`'s subsystem name.
pub fn controlling(system: &System) -> String {
    system
        .sysvals
        .as_ref()
        .and_then(|v| v.get("QCTLSBSD").ok())
        .map(|v| v.text())
        .and_then(|t| t.split_whitespace().next().map(str::to_string))
        .unwrap_or_else(|| "QBASE".into())
}

/// Every active subsystem.
pub fn list(system: &System) -> Vec<SbsInfo> {
    system
        .sbs
        .iter()
        .map(|(name, a)| SbsInfo {
            name: name.clone(),
            lib: a.lib.clone(),
            monitor: a.monitor,
            status: if a.ending.is_some() {
                "ENDING"
            } else {
                "ACTIVE"
            }
            .into(),
            jobs: u32::try_from(
                system
                    .jobs
                    .all()
                    .iter()
                    .filter(|j| j.subsystem == *name && j.number != a.monitor)
                    .count(),
            )
            .unwrap_or(u32::MAX),
            maxjobs: a.maxjobs,
            pools: a.pools.clone(),
        })
        .collect()
}

/// `WRKSBS`.
pub fn wrksbs(system: &System) -> String {
    let mut out = String::from(
        "Subsystem     Total     -----------Subsystem Pools------------\nDescription  Storage (M)  1   2   3   4   5   6   7   8   9  10   Status\n",
    );
    let pools = system.pools.info();
    for s in list(system) {
        let mut cols = [
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ];
        let mut total = 0;
        for (id, p) in &s.pools {
            if let Some(info) = pools.iter().find(|i| i.name == *p) {
                if let Some(c) = cols.get_mut(usize::from(*id).saturating_sub(1)) {
                    *c = info.number.to_string();
                }
                total += info.size_kb / 1024;
            }
        }
        let _ = writeln!(
            out,
            "{:<12}  {:>9}   {}   {}",
            s.name,
            total,
            cols.iter()
                .map(|c| format!("{c:<3}"))
                .collect::<Vec<_>>()
                .join(" "),
            s.status
        );
    }
    out
}

/// `WRKSBSJOB SBS(name|*ALL)`.
pub fn wrksbsjob(system: &System, sbs: &str) -> String {
    let mut out = String::from("Opt  Job         User        Type   -----Status-----  Function\n");
    let mut jobs = system.jobs.all();
    jobs.retain(|j| !j.subsystem.is_empty() && (sbs == "*ALL" || j.subsystem == sbs));
    jobs.sort_by(|a, b| (&a.subsystem, &a.name).cmp(&(&b.subsystem, &b.name)));
    for j in jobs {
        let ty = j
            .job_type()
            .map_or("   ", |t| wm::job::wrkactjob_type(t, j.sub_type()));
        let _ = writeln!(
            out,
            "     {:<10}  {:<10}  {:<5}  ACTIVE  {:<6}  {}",
            j.name,
            j.user,
            ty,
            j.job_status().code(),
            j.function
        );
    }
    out
}

/// Whether a subsystem has room for another job (`MAXJOBS`).
pub fn has_room(system: &System, name: &str) -> bool {
    let Some(a) = system.sbs.get(name) else {
        return false;
    };
    a.maxjobs == 0
        || system
            .jobs
            .all()
            .iter()
            .filter(|j| j.subsystem == name && j.number != a.monitor)
            .count()
            < a.maxjobs as usize
}
