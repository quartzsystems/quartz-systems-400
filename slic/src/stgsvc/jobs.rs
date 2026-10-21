// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the service keeps per job beyond its files: the job number
//! counter in `QSYS/QJOBNBR`, each job's `QTEMP` (a library of its own
//! in ASP 0, `QT` and the job number), and the active job list.

use std::fmt::Write as _;

use qsname::Name;
use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types};
use wm::job::{Status, next_number, wrkactjob_type};

use crate::ipl::system::{System, now_unix};

/// The counter's data area in `QSYS`.
const COUNTER: &str = "QJOBNBR";

fn params(library: ObjectId, name: Name, type_code: u16, text: &str) -> CreateParams {
    CreateParams {
        library,
        name,
        type_code,
        attribute: String::new(),
        text: text.into(),
        owner: "QSYS".into(),
        system: "QS400".into(),
        now: now_unix(),
        by_system: true,
    }
}

/// The next job number, skipping numbers of active jobs; the last one
/// handed out is kept in `QSYS/QJOBNBR`.
pub fn next(system: &mut System) -> Result<u32, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    let jobs = &system.jobs;
    let mut txn = s.store.begin().map_err(|e| e.to_string())?;
    let name = Name::parse(COUNTER).expect("valid");
    let lib = store::obj::library::Library::load(&mut txn, qsys).map_err(|e| e.to_string())?;
    let id = match lib
        .lookup(&mut txn, types::DTAARA, name)
        .map_err(|e| e.to_string())?
    {
        Some(id) => id,
        None => dtaara::create(
            &mut txn,
            &params(qsys, name, types::DTAARA, "Last job number"),
            DataAreaType::Dec {
                digits: 6,
                decimals: 0,
            },
            &DataAreaValue::Dec(0),
        )
        .map_err(|e| e.to_string())?,
    };
    let last = match dtaara::read(&mut txn, id).map_err(|e| e.to_string())? {
        DataAreaValue::Dec(n) => u32::try_from(n).unwrap_or(0),
        _ => 0,
    };
    let n = next_number(last, |n| jobs.in_use(n)).ok_or("every job number is in use")?;
    dtaara::write(&mut txn, id, &DataAreaValue::Dec(i128::from(n))).map_err(|e| e.to_string())?;
    txn.commit().map_err(|e| e.to_string())?;
    Ok(n)
}

/// A job's `QTEMP`: a new library in ASP 0.
pub fn make_qtemp(system: &mut System, number: u32) -> Result<ObjectId, String> {
    let t = system.temp.as_mut().ok_or("the temporary ASP is not open")?;
    let name = Name::parse(&format!("QT{number:06}")).map_err(|e| e.to_string())?;
    let mut txn = t.store.begin().map_err(|e| e.to_string())?;
    let id = ops::create(
        &mut txn,
        &params(t.qtemp, name, types::LIB, "Job temporary library"),
    )
    .map_err(|e| e.to_string())?;
    txn.commit().map_err(|e| e.to_string())?;
    Ok(id)
}

/// Delete a job's `QTEMP` and everything in it.
pub fn drop_qtemp(system: &mut System, number: u32, lib: ObjectId) {
    let Some(t) = system.temp.as_mut() else {
        return;
    };
    let job = JobHandle(u64::from(number));
    let r = t.store.begin().map_err(|e| e.to_string()).and_then(|mut txn| {
        let objs = ops::list(&mut txn, lib, None).map_err(|e| e.to_string())?;
        for (_, id) in objs {
            ops::delete(&mut txn, job, id).map_err(|e| e.to_string())?;
        }
        ops::delete(&mut txn, job, lib).map_err(|e| e.to_string())?;
        txn.commit().map_err(|e| e.to_string())
    });
    if let Err(e) = r {
        eprintln!("qsstg: QTEMP of job {number:06}: {e}");
    }
}

/// `wrkactjob`: the active jobs, by subsystem then name.
pub fn wrkactjob(system: &System) -> String {
    let mut jobs = system.jobs.all();
    jobs.sort_by(|a, b| {
        (a.subsystem.as_str(), a.name.as_str()).cmp(&(b.subsystem.as_str(), b.name.as_str()))
    });
    let mut out = String::from(
        "Subsystem/Job  User        Number  Type  Status  Function\n",
    );
    for j in &jobs {
        let ty = j
            .job_type()
            .map_or("   ", |t| wrkactjob_type(t, j.sub_type()));
        let _ = writeln!(
            out,
            "{:<14} {:<10}  {:06}  {:<4}  {:<6}  {}",
            j.name,
            j.user,
            j.number,
            ty,
            j.job_status().code(),
            j.function
        );
    }
    let _ = writeln!(out, "{} active job(s)", jobs.len());
    out
}

/// The status of a job that has just said hello.
pub fn initial_status() -> u8 {
    Status::Run.byte()
}
