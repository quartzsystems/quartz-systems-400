// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writing audit entries to `QSYS/QAUDJRN`. A receiver past its
//! threshold under `MNGRCV(*USER)` is full: nothing more is written and
//! the caller acts as `QAUDENDACN` says.

use qsname::Name;
use store::obj::{ObjectId, types};
use store::txn::Txn;

use crate::Result;
use crate::audit::Event;

/// The audit journal's name.
pub const JOURNAL: &str = "QAUDJRN";

/// How a write went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Every entry written.
    Written(usize),
    /// There is no `QSYS/QAUDJRN`.
    NoJournal,
    /// The attached receiver is full.
    Full,
}

/// The audit journal, if it exists.
pub fn journal(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Option<ObjectId>> {
    let lib = store::obj::library::Library::load(txn, qsys)?;
    let name = Name::parse(JOURNAL).expect("valid");
    let jrn_type = types::type_by_name("*JRN").expect("registered").code;
    Ok(lib.lookup(txn, jrn_type, name)?)
}

/// Write `events`, each from the job given with it.
pub fn write(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    events: &[(Event, jrn::JobInfo)],
) -> Result<Outcome> {
    if events.is_empty() {
        return Ok(Outcome::Written(0));
    }
    let Some(id) = journal(txn, qsys)? else {
        return Ok(Outcome::NoJournal);
    };
    let mut j = jrn::Journal::load(txn, id).map_err(|e| crate::SecError::Corrupt(e.to_string()))?;
    let rcv = j
        .attached(txn)
        .map_err(|e| crate::SecError::Corrupt(e.to_string()))?;
    if rcv.over_threshold() && !j.attr.mngrcv_system {
        return Ok(Outcome::Full);
    }
    for (e, job) in events {
        let code = e.kind.code().as_bytes();
        let draft = jrn::Draft {
            code: jrn::code::AUDIT,
            kind: [code[0], code[1]],
            flags: 0,
            job: job.clone(),
            program: String::new(),
            object: None,
            count: 0,
            commit_cycle: 0,
            data: e.encode(),
            time: None,
        };
        j.append(txn, draft)
            .map_err(|e| crate::SecError::Corrupt(e.to_string()))?;
    }
    Ok(Outcome::Written(events.len()))
}

/// The audit entries in `QAUDJRN` that pass `filter`, oldest first.
pub fn entries(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    filter: &jrn::Filter,
) -> Result<Vec<(jrn::Entry, Event)>> {
    let Some(id) = journal(txn, qsys)? else {
        return Ok(Vec::new());
    };
    let all = jrn::dspjrn(txn, id, filter).map_err(|e| crate::SecError::Corrupt(e.to_string()))?;
    Ok(all
        .into_iter()
        .filter(|e| e.code == jrn::code::AUDIT)
        .filter_map(|e| {
            let k = crate::audit::Kind::by_code(std::str::from_utf8(&e.kind).ok()?)?;
            let ev = Event::decode(k, &e.data)?;
            Some((e, ev))
        })
        .collect())
}
