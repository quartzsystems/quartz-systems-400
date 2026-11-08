// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `REPLACE` of the create commands: an object of the new one's name and
//! type in the library is refused with `REPLACE(*NO)`, and so is one of another kind (a
//! physical file where a display file is made); with `*YES` it is
//! deleted, or, while a job holds a lock on it, moved to `QRPLOBJ` under
//! a generated name so the job goes on with the old one.

use qsname::Name;
use store::obj::library::Library;
use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;

/// What happened to the object that was there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Room {
    /// Nothing of the name and type was there.
    Free,
    /// It was deleted.
    Deleted,
    /// It was in use and is now `QRPLOBJ/<name>`.
    Moved(String),
}

/// Make room for object `name` of type `ty` in library `lib`. `exists`
/// is the refusal with `REPLACE(*NO)` or when `same_kind` says the
/// object there is not one the command replaces: an identifier and text
/// (`CPF7302 File X in library L not created...`).
pub fn make_room(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    (lib, name, ty): (ObjectId, Name, u16),
    replace: bool,
    same_kind: &dyn Fn(&mut Txn<'_>, ObjectId) -> bool,
    exists: impl FnOnce() -> String,
    now: u64,
) -> Result<Room, String> {
    let l = Library::load(txn, lib).map_err(|e| e.to_string())?;
    let Some(old) = l.lookup(txn, ty, name).map_err(|e| e.to_string())? else {
        return Ok(Room::Free);
    };
    if !replace || !same_kind(txn, old) {
        return Err(exists());
    }
    if txn.store().locks().holders(old).is_empty() {
        ops::delete(txn, JobHandle(0), old).map_err(|e| e.to_string())?;
        return Ok(Room::Deleted);
    }
    let rpl = qrplobj(txn, qsys, now)?;
    let n = format!("Q{:09}", txn.peek_serial() % 1_000_000_000);
    let new = Name::parse(&n).map_err(|e| e.to_string())?;
    ops::move_to(txn, old, rpl, now).map_err(|e| e.to_string())?;
    ops::rename(txn, old, new, now).map_err(|e| e.to_string())?;
    Ok(Room::Moved(n))
}

/// `QRPLOBJ`, made when missing.
fn qrplobj(txn: &mut Txn<'_>, qsys: ObjectId, now: u64) -> Result<ObjectId, String> {
    let name = Name::parse(ptf::apply::QRPLOBJ).expect("name");
    if let Ok(f) = ops::find_library(txn, qsys, name) {
        return Ok(f.id);
    }
    ops::create(
        txn,
        &ops::CreateParams {
            library: qsys,
            name,
            type_code: types::LIB,
            attribute: String::new(),
            text: "Objects replaced while in use".into(),
            owner: "QSYS".into(),
            system: String::new(),
            now,
            by_system: true,
        },
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use blockdev::{BlockDev, MemBlockDev};
    use store::obj::CreateParams;
    use store::obj::lock::LockState;
    use store::{FormatParams, Store, StoreConfig};

    use super::*;

    const NOW: u64 = 1_792_713_600;

    fn create(t: &mut Txn<'_>, library: ObjectId, name: &str, ty: u16, attr: &str) -> ObjectId {
        ops::create(
            t,
            &CreateParams {
                library,
                name: Name::parse(name).unwrap(),
                type_code: ty,
                attribute: attr.into(),
                text: String::new(),
                owner: "QSECOFR".into(),
                system: "QS400".into(),
                now: NOW,
                by_system: ty == types::LIB,
            },
        )
        .unwrap()
    }

    fn open() -> (Store, ObjectId, ObjectId) {
        let d = Arc::new(MemBlockDev::with_seed(16384, 3));
        let params = FormatParams {
            asp_number: 1,
            asp_uuid: [4; 16],
            unit_uuids: vec![[5; 16]],
            unit_serials: Vec::new(),
            wal_pages: 128,
            now: NOW,
        };
        let mut store = Store::format(
            vec![Box::new(d) as Box<dyn BlockDev>],
            &params,
            StoreConfig::default(),
        )
        .unwrap();
        let mut t = store.begin().unwrap();
        let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
        let lib = create(&mut t, qsys, "INVMGR", types::LIB, "");
        t.commit().unwrap();
        (store, qsys, lib)
    }

    fn dspf(t: &mut Txn<'_>, id: ObjectId) -> bool {
        ops::read_header(t, id).is_ok_and(|(_, h)| ccsid::decode_trimmed(&h.attribute) == "DSPF")
    }

    fn room(t: &mut Txn<'_>, qsys: ObjectId, lib: ObjectId, replace: bool) -> Result<Room, String> {
        let n = Name::parse("INVMGR01D").unwrap();
        make_room(
            t,
            qsys,
            (lib, n, types::FILE),
            replace,
            &dspf,
            || "CPF7302 exists".into(),
            NOW,
        )
    }

    #[test]
    fn replace_no_refuses_and_yes_deletes() {
        let (mut store, qsys, lib) = open();
        let mut t = store.begin().unwrap();
        assert_eq!(room(&mut t, qsys, lib, false), Ok(Room::Free));
        create(&mut t, lib, "INVMGR01D", types::FILE, "DSPF");
        assert_eq!(room(&mut t, qsys, lib, false), Err("CPF7302 exists".into()));
        assert_eq!(room(&mut t, qsys, lib, true), Ok(Room::Deleted));
        assert_eq!(room(&mut t, qsys, lib, true), Ok(Room::Free));
        // A physical file of the name is not replaced by a display file.
        create(&mut t, lib, "INVMGR01D", types::FILE, "PF");
        assert_eq!(room(&mut t, qsys, lib, true), Err("CPF7302 exists".into()));
    }

    #[test]
    fn a_file_in_use_moves_to_qrplobj() {
        let (mut store, qsys, lib) = open();
        let mut t = store.begin().unwrap();
        let old = create(&mut t, lib, "INVMGR01D", types::FILE, "DSPF");
        t.store()
            .locks_mut()
            .lock(JobHandle(7), old, LockState::SharedRead)
            .unwrap();
        let Ok(Room::Moved(to)) = room(&mut t, qsys, lib, true) else {
            panic!("not moved");
        };
        let rpl = ops::find_library(&mut t, qsys, Name::parse("QRPLOBJ").unwrap())
            .unwrap()
            .id;
        let there = Library::load(&mut t, rpl)
            .unwrap()
            .lookup(&mut t, types::FILE, Name::parse(&to).unwrap())
            .unwrap();
        assert_eq!(there, Some(old), "the same object, renamed");
        assert_eq!(room(&mut t, qsys, lib, true), Ok(Room::Free));
    }
}
