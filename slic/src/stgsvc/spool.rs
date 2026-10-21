// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Spooled files in the service: every `*SPLF` in `QSPL` indexed by its
//! spool identifier, creating and writing them for jobs, their place on
//! output queues, and what a job's end does to them. A file a lost job
//! left open is closed at the next start.

use std::collections::BTreeMap;

use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;
use wm::obj::outq::{OutQ, Seq};
use wm::obj::params::qualified;
use wm::obj::splf::{self, Schedule, SplfAttrs, Status};

use crate::ipl::system::{System, now_unix};

/// One spooled file.
#[derive(Clone, Debug)]
pub struct Entry {
    /// Its object in `QSPL`.
    pub obj: ObjectId,
    /// Its attributes, as stored.
    pub attrs: SplfAttrs,
}

/// Every spooled file.
#[derive(Debug, Default)]
pub struct Spool {
    /// By spool identifier.
    pub files: BTreeMap<u32, Entry>,
    next: u32,
}

fn stamp() -> wm::obj::Stamp {
    wm::obj::Stamp {
        owner: "QSPL".into(),
        system: "QS400".into(),
        now: now_unix(),
    }
}

/// Run `f` in a transaction with `QSYS`.
pub fn with_store<T>(
    system: &mut System,
    f: impl FnOnce(&mut Txn<'_>, ObjectId) -> Result<T, String>,
) -> Result<T, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    crate::svc::db::with_txn(&mut s.store, |txn| f(txn, qsys))
}

/// At start: index `QSPL`, and close what lost jobs left open.
pub fn load(system: &mut System) {
    let found = with_store(system, |txn, qsys| {
        let qspl = wm::obj::library(txn, qsys, "QSPL").map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for (_, id) in ops::list(txn, qspl, Some(types::SPLF)).map_err(|e| e.to_string())? {
            let Ok(mut a) = splf::read_attrs(txn, id) else {
                continue;
            };
            if matches!(
                a.status,
                Status::Opn | Status::Clo | Status::Wtr | Status::Pnd
            ) {
                a.status = Status::Rdy;
                a.writer.clear();
                splf::write_attrs(txn, id, &a).map_err(|e| e.to_string())?;
            }
            out.push(Entry { obj: id, attrs: a });
        }
        Ok(out)
    });
    let Ok(found) = found else {
        return;
    };
    let sp = &mut system.spool;
    for e in found {
        sp.next = sp.next.max(e.attrs.id + 1);
        sp.files.insert(e.attrs.id, e);
    }
    sp.next = sp.next.max(1);
}

/// An output queue's object and real `lib/name`.
pub fn outq_object(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    q: &str,
) -> Result<(ObjectId, String), String> {
    let (lib, name) = qualified(q);
    let names = crate::svc::ile::name_context(txn, qsys);
    let id = wm::obj::find(txn, qsys, &names, &lib, &name, types::OUTQ)
        .map_err(|_| format!("CPF3357 output queue {name} in library {lib} not found"))?;
    let real = ile::store::library_name(txn, id).map_err(|e| e.to_string())?;
    Ok((id, format!("{real}/{name}")))
}

/// Run `f` on an output queue; written back when `f` says it changed it.
pub fn with_outq<T>(
    system: &mut System,
    q: &str,
    f: impl FnOnce(&mut OutQ, &str) -> Result<(T, bool), String>,
) -> Result<T, String> {
    with_store(system, |txn, qsys| {
        let (id, real) = outq_object(txn, qsys, q)?;
        let mut oq = OutQ::decode(&wm::obj::read(txn, id).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let (r, changed) = f(&mut oq, &real)?;
        if changed {
            wm::obj::write(txn, id, &oq.encode()).map_err(|e| e.to_string())?;
        }
        Ok(r)
    })
}

/// Put `id` on its queue in the queue's order.
fn enqueue(system: &mut System, id: u32, q: &str) -> Result<String, String> {
    let jobs: BTreeMap<u32, u32> = system
        .spool
        .files
        .iter()
        .map(|(k, e)| (*k, e.attrs.job.2))
        .collect();
    let mine = jobs.get(&id).copied().unwrap_or(0);
    with_outq(system, q, |oq, real| {
        oq.files.retain(|f| *f != id);
        let at = match oq.seq {
            Seq::Fifo => oq.files.len(),
            Seq::JobNbr => oq
                .files
                .iter()
                .position(|f| jobs.get(f).copied().unwrap_or(0) > mine)
                .unwrap_or(oq.files.len()),
        };
        oq.files.insert(at, id);
        Ok((real.to_string(), true))
    })
}

/// Take `id` off queue `q`.
fn dequeue(system: &mut System, id: u32, q: &str) {
    let _ = with_outq(system, q, |oq, _| {
        let before = oq.files.len();
        oq.files.retain(|f| *f != id);
        Ok(((), oq.files.len() != before))
    });
}

/// A new spooled file, open (`OPN`) on `attrs.outq`, numbered in its
/// job: its spool identifier.
pub fn create(system: &mut System, mut attrs: SplfAttrs) -> Result<u32, String> {
    let id = system.spool.next.max(1);
    system.spool.next = id + 1;
    attrs.id = id;
    attrs.number = system
        .spool
        .files
        .values()
        .filter(|e| e.attrs.job == attrs.job)
        .map(|e| e.attrs.number)
        .max()
        .unwrap_or(0)
        + 1;
    attrs.status = Status::Opn;
    attrs.created = crate::stgsvc::msgs::now_micros();
    attrs.copies_left = attrs.copies;
    if attrs.devtype.is_empty() {
        attrs.devtype = "*SCS".into();
    }
    let real = with_store(system, |txn, qsys| {
        outq_object(txn, qsys, &attrs.outq).map(|o| o.1)
    })?;
    attrs.outq.clone_from(&real);
    let name = SplfAttrs::object_name(id);
    let obj = with_store(system, |txn, qsys| {
        let qspl = wm::obj::library(txn, qsys, "QSPL").map_err(|e| e.to_string())?;
        let n = qsname::Name::parse(&name).map_err(|e| e.to_string())?;
        let obj = wm::obj::create(txn, qspl, n, types::SPLF, &attrs.name, &stamp(), &[])
            .map_err(|e| e.to_string())?;
        splf::init(txn, obj, &attrs).map_err(|e| e.to_string())?;
        Ok(obj)
    })?;
    system.spool.files.insert(id, Entry { obj, attrs });
    enqueue(system, id, &real)?;
    Ok(id)
}

/// Append SCS data to an open file.
pub fn append(
    system: &mut System,
    id: u32,
    data: &[u8],
    pages: u32,
    records: u32,
) -> Result<(), String> {
    let mut e = system
        .spool
        .files
        .get(&id)
        .cloned()
        .ok_or_else(|| format!("CPF3309 no spooled file {id}"))?;
    with_store(system, |txn, _| {
        splf::append(txn, e.obj, &mut e.attrs, data, pages, records).map_err(|x| x.to_string())
    })?;
    system.spool.files.insert(id, e);
    Ok(())
}

/// Change a file's attributes and store them.
pub fn update(
    system: &mut System,
    id: u32,
    f: impl FnOnce(&mut SplfAttrs) -> Result<(), String>,
) -> Result<SplfAttrs, String> {
    let mut e = system
        .spool
        .files
        .get(&id)
        .cloned()
        .ok_or_else(|| format!("CPF3309 no spooled file {id}"))?;
    f(&mut e.attrs)?;
    with_store(system, |txn, _| {
        splf::write_attrs(txn, e.obj, &e.attrs).map_err(|x| x.to_string())
    })?;
    let a = e.attrs.clone();
    system.spool.files.insert(id, e);
    Ok(a)
}

/// Close an open file: ready, held, or closed until its job ends.
pub fn close(system: &mut System, id: u32, hold: bool) -> Result<(), String> {
    update(system, id, |a| {
        a.status = match (hold, a.schedule) {
            (true, _) => {
                a.held_from = Status::Rdy;
                Status::Hld
            }
            (false, Schedule::JobEnd) => Status::Clo,
            (false, _) => Status::Rdy,
        };
        Ok(())
    })
    .map(|_| ())
}

/// A job ended: its files held for the end become ready, and any it
/// left open are closed.
pub fn job_ended(system: &mut System, job: &(String, String, u32)) {
    let ids: Vec<u32> = system
        .spool
        .files
        .values()
        .filter(|e| e.attrs.job == *job && matches!(e.attrs.status, Status::Clo | Status::Opn))
        .map(|e| e.attrs.id)
        .collect();
    for id in ids {
        let _ = update(system, id, |a| {
            a.status = Status::Rdy;
            Ok(())
        });
    }
}

/// Delete a file: off its queue, out of `QSPL`.
pub fn delete(system: &mut System, id: u32) -> Result<(), String> {
    let e = system
        .spool
        .files
        .remove(&id)
        .ok_or_else(|| format!("CPF3309 no spooled file {id}"))?;
    dequeue(system, id, &e.attrs.outq);
    with_store(system, |txn, _| {
        ops::delete(txn, JobHandle(0), e.obj).map_err(|x| x.to_string())
    })
}

/// Move a file to another queue.
pub fn move_to(system: &mut System, id: u32, q: &str) -> Result<String, String> {
    let old = system
        .spool
        .files
        .get(&id)
        .map(|e| e.attrs.outq.clone())
        .ok_or_else(|| format!("CPF3309 no spooled file {id}"))?;
    let real = with_store(system, |txn, qsys| outq_object(txn, qsys, q).map(|o| o.1))?;
    if real != old {
        dequeue(system, id, &old);
        update(system, id, |a| {
            a.outq.clone_from(&real);
            Ok(())
        })?;
        enqueue(system, id, &real)?;
    }
    Ok(real)
}

/// A file's attributes and SCS data.
pub fn data(system: &mut System, id: u32) -> Result<(SplfAttrs, Vec<u8>), String> {
    let e = system
        .spool
        .files
        .get(&id)
        .cloned()
        .ok_or_else(|| format!("CPF3309 no spooled file {id}"))?;
    let d = with_store(system, |txn, _| {
        splf::read_data(txn, e.obj, &e.attrs).map_err(|x| x.to_string())
    })?;
    Ok((e.attrs, d))
}

/// A job as `JOB(*)`, `name`, `user/name` or `number/user/name` says,
/// matched against a file's job.
pub fn job_matches(spec: &str, job: &(String, String, u32)) -> bool {
    let parts: Vec<&str> = spec.split('/').collect();
    match parts.as_slice() {
        [name] => *name == job.0,
        [user, name] => *user == job.1 && *name == job.0,
        [n, user, name] => n.parse::<u32>().ok() == Some(job.2) && *user == job.1 && *name == job.0,
        _ => false,
    }
}

/// The file `FILE(name) JOB(...) SPLNBR(...)` names. `job` is the
/// job's spec, the caller's own job for `*`; `splnbr` is `*ONLY`,
/// `*LAST` or a number.
pub fn find(system: &System, file: &str, job: &str, splnbr: &str) -> Result<u32, String> {
    let mut hits: Vec<&SplfAttrs> = system
        .spool
        .files
        .values()
        .map(|e| &e.attrs)
        .filter(|a| a.name == file && job_matches(job, &a.job))
        .collect();
    hits.sort_by_key(|a| (a.job.2, a.number));
    match splnbr {
        "*LAST" => hits.last().map(|a| a.id),
        "*ONLY" | "" => {
            if hits.len() > 1 {
                return Err(format!(
                    "CPF3340 more than one file {file} in job {job}; give SPLNBR"
                ));
            }
            hits.first().map(|a| a.id)
        }
        n => {
            let n: u32 = n
                .parse()
                .map_err(|_| format!("CPD0084 SPLNBR({n}) is not valid"))?;
            hits.iter().find(|a| a.number == n).map(|a| a.id)
        }
    }
    .ok_or_else(|| format!("CPF3309 no spooled file {file} in job {job}"))
}

/// The files on a queue, in queue order.
pub fn on_queue(system: &mut System, q: &str) -> Result<(String, OutQ, Vec<SplfAttrs>), String> {
    let (real, oq) = with_outq(system, q, |oq, real| {
        Ok(((real.to_string(), oq.clone()), false))
    })?;
    let files = oq
        .files
        .iter()
        .filter_map(|id| system.spool.files.get(id).map(|e| e.attrs.clone()))
        .collect();
    Ok((real, oq, files))
}
