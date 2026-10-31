// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The problem log in the store: user space `QSYS/QSPRBLOG` holding the
//! `prb` crate's text, and the problems the system logs on its own.

use prb::analyze::Detected;
use prb::{PrbType, ProblemLog};
use store::obj::{ObjectId, ops, types, usrspc};
use store::txn::Txn;

use crate::ipl::system::{System, now_unix};

const SPACE: &str = "QSPRBLOG";

fn space(txn: &mut Txn<'_>, qsys: ObjectId, create: bool) -> Result<Option<ObjectId>, String> {
    let name = qsname::Name::parse(SPACE).expect("name");
    let lib = store::obj::library::Library::load(txn, qsys).map_err(|e| e.to_string())?;
    if let Some(id) = lib.lookup(txn, types::USRSPC, name).map_err(|e| e.to_string())? {
        return Ok(Some(id));
    }
    if !create {
        return Ok(None);
    }
    let params = ops::CreateParams {
        library: qsys,
        name,
        type_code: types::USRSPC,
        attribute: String::new(),
        text: "Problem log".into(),
        owner: "QSYS".into(),
        system: String::new(),
        now: now_unix(),
        by_system: true,
    };
    usrspc::create(txn, &params, 16, true, 0)
        .map(Some)
        .map_err(|e| e.to_string())
}

fn with_store<T>(
    system: &mut System,
    f: impl FnOnce(&mut Txn<'_>, ObjectId) -> Result<T, String>,
) -> Result<T, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    crate::svc::db::with_txn(&mut s.store, |txn| f(txn, qsys))
}

/// The problem log.
pub fn load(system: &mut System) -> Result<ProblemLog, String> {
    with_store(system, |txn, qsys| {
        let Some(id) = space(txn, qsys, false)? else {
            return Ok(ProblemLog::default());
        };
        let len = usrspc::read(txn, id, 0, 8).map_err(|e| e.to_string())?;
        let len = u64::from_be_bytes(len.try_into().expect("8"));
        let text = usrspc::read(txn, id, 8, usize::try_from(len).unwrap_or(0))
            .map_err(|e| e.to_string())?;
        Ok(ProblemLog::decode(&String::from_utf8_lossy(&text)))
    })
}

/// Keep the problem log.
pub fn store(system: &mut System, log: &ProblemLog) -> Result<(), String> {
    let text = log.encode();
    with_store(system, |txn, qsys| {
        let id = space(txn, qsys, true)?.expect("created");
        usrspc::write(txn, id, 0, &(text.len() as u64).to_be_bytes()).map_err(|e| e.to_string())?;
        usrspc::write(txn, id, 8, text.as_bytes()).map_err(|e| e.to_string())
    })
}

/// Log a problem the system detected; its identifier.
pub fn log(system: &mut System, kind: PrbType, d: &Detected) -> Result<String, String> {
    let name = system.system_name();
    let mut l = load(system)?;
    let id = l.create(kind, d, &name, now_unix());
    store(system, &l)?;
    Ok(id)
}

/// A message just sent to `QSYSOPR`: severe ones are problems.
pub fn message_sent(system: &mut System, msgid: &str, severity: u8, text: &str) {
    if let Some(d) = prb::analyze::from_message(msgid, severity, text) {
        let _ = log(system, PrbType::Machine, &d);
    }
}
