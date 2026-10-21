// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Printer files in the service: resolving one (a `*FILE` whose
//! attribute is `PRTF`) with the overrides that apply, and the output
//! queue its spooled files go to.

use store::obj::{ops, types};
use wm::cmd::Params;
use wm::obj::prtf::Prtf;

use crate::ipl::system::System;

/// The queue spooled files go to when nothing else names one.
pub const DEFAULT_OUTQ: &str = "QGPL/QPRINT";

/// A printer file, `lib/name`, with `overrides` applied in order.
pub fn resolve(system: &mut System, qual: &str, overrides: &[Params]) -> Result<Prtf, String> {
    let (lib, name) = wm::obj::params::qualified(qual);
    let mut f = super::spool::with_store(system, |txn, qsys| {
        let names = crate::svc::ile::name_context(txn, qsys);
        let id = wm::obj::find(txn, qsys, &names, &lib, &name, types::FILE)
            .map_err(|_| format!("CPF4101 file {name} in library {lib} not found"))?;
        let (_, h) = ops::read_header(txn, id).map_err(|e| e.to_string())?;
        if ccsid::decode_trimmed(&h.attribute) != "PRTF" {
            return Err(format!("CPF4123 file {name} in library {lib} is not a printer file"));
        }
        let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
        Prtf::decode(&b).map_err(|e| e.to_string())
    })?;
    for o in overrides {
        f.apply(o).map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPD0084")))?;
    }
    Ok(f)
}

/// Whether an output queue exists.
fn exists(system: &mut System, q: &str) -> bool {
    super::spool::with_store(system, |txn, qsys| super::spool::outq_object(txn, qsys, q)).is_ok()
}

/// The output queue a file of job `job` opened through `f` goes to:
/// the file's `OUTQ`, else the job's, else the printer device's
/// (`MIGRATE(phase10)`: printer devices have queues), else `QGPL/QPRINT`.
pub fn outq_of(system: &mut System, f: &Prtf, job: u32) -> String {
    let job_q = system.attrs.get(&job).map(|a| a.outq.clone()).unwrap_or_default();
    let mut tries = Vec::new();
    if f.outq != "*JOB" && !f.outq.is_empty() {
        tries.push(f.outq.clone());
    }
    if !job_q.starts_with('*') && !job_q.is_empty() {
        tries.push(job_q);
    }
    tries.push(DEFAULT_OUTQ.into());
    tries
        .into_iter()
        .find(|q| exists(system, q))
        .unwrap_or_else(|| DEFAULT_OUTQ.into())
}
