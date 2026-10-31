// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The problem log in the store: user space `QSYS/QSPRBLOG` holding the
//! `prb` crate's text, the support contact information in
//! `QSYS/QSCNTINF`, and the problems the system logs on its own. Text
//! spaces hold an eight-byte length, then the text.

use prb::analyze::Detected;
use prb::{Contact, PrbType, ProblemLog, Reported, Sender};
use store::obj::{ObjectId, ops, types, usrspc};
use store::txn::Txn;

use crate::ipl::system::{System, now_unix};

const SPACE: &str = "QSPRBLOG";
const CONTACT: &str = "QSCNTINF";

/// The library `lib` (`QSYS` for the system's own).
fn library(txn: &mut Txn<'_>, qsys: ObjectId, lib: &str) -> Result<ObjectId, String> {
    if lib.eq_ignore_ascii_case("QSYS") {
        return Ok(qsys);
    }
    let name = qsname::Name::parse(lib).map_err(|e| e.to_string())?;
    ops::find_library(txn, qsys, name)
        .map(|f| f.id)
        .map_err(|e| format!("CPF2110 Library {lib} not found: {e}"))
}

fn space(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    lib: &str,
    name: &str,
    create: Option<&str>,
) -> Result<Option<ObjectId>, String> {
    let lib_id = library(txn, qsys, lib)?;
    let n = qsname::Name::parse(name).map_err(|e| e.to_string())?;
    let l = store::obj::library::Library::load(txn, lib_id).map_err(|e| e.to_string())?;
    if let Some(id) = l.lookup(txn, types::USRSPC, n).map_err(|e| e.to_string())? {
        return Ok(Some(id));
    }
    let Some(text) = create else {
        return Ok(None);
    };
    let params = ops::CreateParams {
        library: lib_id,
        name: n,
        type_code: types::USRSPC,
        attribute: String::new(),
        text: text.into(),
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

/// The text a text space holds; `None` when there is no such space.
pub fn load_text(system: &mut System, lib: &str, name: &str) -> Result<Option<String>, String> {
    with_store(system, |txn, qsys| {
        let Some(id) = space(txn, qsys, lib, name, None)? else {
            return Ok(None);
        };
        let len = usrspc::read(txn, id, 0, 8).map_err(|e| e.to_string())?;
        let len = u64::from_be_bytes(len.try_into().expect("8"));
        let text = usrspc::read(txn, id, 8, usize::try_from(len).unwrap_or(0))
            .map_err(|e| e.to_string())?;
        Ok(Some(String::from_utf8_lossy(&text).into_owned()))
    })
}

/// Keep `text` in a text space, made with description `desc` when new.
pub fn store_text(system: &mut System, lib: &str, name: &str, text: &str, desc: &str) -> Result<(), String> {
    with_store(system, |txn, qsys| {
        let id = space(txn, qsys, lib, name, Some(desc))?.expect("created");
        usrspc::write(txn, id, 0, &(text.len() as u64).to_be_bytes()).map_err(|e| e.to_string())?;
        usrspc::write(txn, id, 8, text.as_bytes()).map_err(|e| e.to_string())
    })
}

/// The problem log.
pub fn load(system: &mut System) -> Result<ProblemLog, String> {
    Ok(load_text(system, "QSYS", SPACE)?
        .map(|t| ProblemLog::decode(&t))
        .unwrap_or_default())
}

/// Keep the problem log.
pub fn store(system: &mut System, log: &ProblemLog) -> Result<(), String> {
    store_text(system, "QSYS", SPACE, &log.encode(), "Problem log")
}

/// The support contact information.
pub fn contact(system: &mut System) -> Result<Contact, String> {
    Ok(load_text(system, "QSYS", CONTACT)?
        .map(|t| Contact::decode(&t))
        .unwrap_or_default())
}

/// Keep the support contact information.
pub fn set_contact(system: &mut System, c: &Contact) -> Result<(), String> {
    store_text(system, "QSYS", CONTACT, &c.encode(), "Support contact information")
}

/// Log a problem the system detected; its identifier.
pub fn log(system: &mut System, kind: PrbType, d: &Detected) -> Result<String, String> {
    let name = system.system_name();
    let mut l = load(system)?;
    let id = l.create(kind, d, &name, now_unix());
    store(system, &l)?;
    super::liclog::write(0x0400, 0x0001, system.current_job, &format!("Problem {id}: {}", d.symptoms));
    super::trace::internal("PRB", system.current_job, &format!("{id} {}", d.symptoms));
    Ok(id)
}

/// A message just sent to `QSYSOPR`: severe ones are problems.
pub fn message_sent(system: &mut System, msgid: &str, severity: u8, text: &str) {
    if let Some(d) = prb::analyze::from_message(msgid, severity, text) {
        let _ = log(system, PrbType::Machine, &d);
    }
}

/// The hook the update service's client fills in: today no service
/// provider is reachable, so a report is prepared and kept.
/// MIGRATE(ptf): the client under `ptf/` sends it.
fn sender() -> Box<dyn Sender> {
    Box::new(prb::Unconnected)
}

/// Report problem `id` to the service provider.
pub fn report(system: &mut System, id: &str) -> Result<Reported, String> {
    let mut l = load(system)?;
    let r = l
        .report(id, sender().as_mut(), now_unix())
        .ok_or_else(|| format!("CPF3925 Problem {id} not found in the problem log."))?;
    store(system, &l)?;
    super::liclog::write(0x0400, 0x0002, system.current_job, &format!("Problem {id}: {r:?}"));
    Ok(r)
}
