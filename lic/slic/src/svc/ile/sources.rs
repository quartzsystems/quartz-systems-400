// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job's sources over the lent store: programs, user spaces,
//! message descriptions, data areas and data queues, and the store
//! handle the file I/O runs on.

use std::cell::RefMut;
use std::sync::Arc;

use ile::{PgmKind, ProgramObject};
use mi::io::{DataAreaKind, DataAreaValue, DataSource, IoFail};
use mi::{ObjectSource, UserSpaceSource};
use msgf::MsgDesc;
use qsname::{LibSpec, Name};
use store::Store;
use store::obj::dtaara::{self, DataAreaType, DataAreaValue as StoreValue};
use store::obj::dtaq::{self, Select};
use store::obj::library::Library;
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types, usrspc};
use store::txn::Txn;

use super::{Shared, with_main, with_temp};

/// Find `lib/name` of a type: `QTEMP` in the temporary store, else the
/// main store's library or list. The library name it was found in
/// comes back with the object.
pub fn find_object(
    shared: &Shared,
    name: &str,
    lib: &str,
    type_code: u16,
) -> Option<(ObjectId, String, bool)> {
    let n = Name::parse(name).ok()?;
    let lib_u = lib.to_ascii_uppercase();
    if lib_u == "QTEMP" || lib_u == "*LIBL" || lib_u == "*CURLIB" {
        let hit = with_temp(shared, |txn, qtemp| {
            let l = Library::load(txn, qtemp).map_err(|e| e.to_string())?;
            l.lookup(txn, type_code, n).map_err(|e| e.to_string())
        });
        if let Ok(Some(id)) = hit {
            return Some((id, "QTEMP".into(), true));
        }
        if lib_u == "QTEMP" {
            return None;
        }
    }
    with_main(shared, |txn, qsys| {
        let spec = match lib_u.as_str() {
            "*LIBL" => LibSpec::Libl,
            "*CURLIB" => LibSpec::Curlib,
            l => LibSpec::Name(Name::parse(l).map_err(|e| e.to_string())?),
        };
        let names = super::name_context(txn, qsys);
        let found =
            ops::resolve(txn, qsys, &names, spec, n, type_code).map_err(|e| e.to_string())?;
        let lib_name = ops::describe(txn, found.library)
            .map(|h| h.name.to_string())
            .map_err(|e| e.to_string())?;
        Ok((found.id, lib_name, false))
    })
    .ok()
}

/// Programs and service programs from the store.
pub struct StoreObjects {
    shared: Shared,
    /// System pointer identifiers handed out: `(name, library)`.
    ids: Vec<(String, String)>,
}

impl StoreObjects {
    /// Over the lent store.
    pub fn new(shared: Shared) -> Self {
        Self {
            shared,
            ids: Vec::new(),
        }
    }
}

impl ObjectSource for StoreObjects {
    fn program(
        &mut self,
        name: &str,
        lib: &str,
        kind: PgmKind,
    ) -> Option<(Arc<ProgramObject>, String)> {
        let type_code = match kind {
            PgmKind::Pgm => types::PGM,
            PgmKind::SrvPgm => types::SRVPGM,
        };
        let (id, lib_name, temp) = find_object(&self.shared, name, lib, type_code)?;
        let load = |txn: &mut Txn<'_>| ile::store::load_program(txn, id).map_err(|e| e.to_string());
        let object = if temp {
            with_temp(&self.shared, |txn, _| load(txn))
        } else {
            with_main(&self.shared, |txn, _| load(txn))
        }
        .ok()?;
        Some((Arc::new(object), lib_name))
    }

    fn resolve(&mut self, name: &str, lib: &str, obj_type: &str) -> Option<u64> {
        let type_code = match obj_type {
            "*PGM" => types::PGM,
            "*SRVPGM" => types::SRVPGM,
            _ => return None,
        };
        let (_, lib_name, _) = find_object(&self.shared, name, lib, type_code)?;
        let key = (name.to_string(), lib_name);
        let i = if let Some(i) = self.ids.iter().position(|k| *k == key) {
            i
        } else {
            self.ids.push(key);
            self.ids.len() - 1
        };
        Some(u64::try_from(i + 1).expect("few pointers"))
    }

    fn by_id(&mut self, id: u64) -> Option<(String, String)> {
        let i = usize::try_from(id.checked_sub(1)?).ok()?;
        self.ids.get(i).cloned()
    }
}

/// User spaces in the store.
pub struct StoreUserSpaces {
    shared: Shared,
}

impl StoreUserSpaces {
    /// Over the lent store.
    pub fn new(shared: Shared) -> Self {
        Self { shared }
    }

    fn locate(&self, name: &str, lib: &str) -> Option<(ObjectId, String, bool)> {
        find_object(&self.shared, name, lib, types::USRSPC)
    }
}

impl UserSpaceSource for StoreUserSpaces {
    fn load(&mut self, name: &str, lib: &str) -> Option<Vec<u8>> {
        let (id, _, temp) = self.locate(name, lib)?;
        let read = |txn: &mut Txn<'_>| {
            let attr = usrspc::describe(txn, id).map_err(|e| e.to_string())?;
            let len = usize::try_from(attr.size).map_err(|e| e.to_string())?;
            usrspc::read(txn, id, 0, len).map_err(|e| e.to_string())
        };
        if temp {
            with_temp(&self.shared, |txn, _| read(txn)).ok()
        } else {
            with_main(&self.shared, |txn, _| read(txn)).ok()
        }
    }

    fn store(&mut self, name: &str, lib: &str, data: &[u8]) -> bool {
        let Some((id, _, temp)) = self.locate(name, lib) else {
            return false;
        };
        let write = |txn: &mut Txn<'_>| {
            let attr = usrspc::describe(txn, id).map_err(|e| e.to_string())?;
            if attr.size < data.len() as u64 {
                usrspc::change_size(txn, id, data.len() as u64).map_err(|e| e.to_string())?;
            }
            usrspc::write(txn, id, 0, data).map_err(|e| e.to_string())
        };
        if temp {
            with_temp(&self.shared, |txn, _| write(txn)).is_ok()
        } else {
            with_main(&self.shared, |txn, _| write(txn)).is_ok()
        }
    }

    fn create(
        &mut self,
        name: &str,
        lib: &str,
        size: usize,
        init: u8,
        text: &str,
    ) -> Result<(), &'static str> {
        let n = Name::parse(name).map_err(|_| "CPF9810")?;
        let lib_u = lib.to_ascii_uppercase();
        let params = |library: ObjectId| CreateParams {
            library,
            name: n,
            type_code: types::USRSPC,
            attribute: String::new(),
            text: text.into(),
            owner: "QSYS".into(),
            system: "QS400".into(),
            now: crate::ipl::system::now_unix(),
            by_system: false,
        };
        let r = if lib_u == "QTEMP" {
            with_temp(&self.shared, |txn, qtemp| {
                usrspc::create(txn, &params(qtemp), size as u64, true, init)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })
        } else {
            with_main(&self.shared, |txn, qsys| {
                let target = if lib_u == "*CURLIB" || lib_u == "*LIBL" {
                    "QGPL".to_string()
                } else {
                    lib_u.clone()
                };
                let l =
                    ops::find_library(txn, qsys, Name::parse(&target).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                usrspc::create(txn, &params(l.id), size as u64, true, init)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })
        };
        r.map_err(|e| {
            if e.contains("exists") {
                "CPF9870"
            } else {
                "CPF9810"
            }
        })
    }

    fn delete(&mut self, name: &str, lib: &str) -> bool {
        let Some((id, _, temp)) = self.locate(name, lib) else {
            return false;
        };
        let del = |txn: &mut Txn<'_>| ops::delete(txn, JobHandle(1), id).map_err(|e| e.to_string());
        if temp {
            with_temp(&self.shared, |txn, _| del(txn)).is_ok()
        } else {
            with_main(&self.shared, |txn, _| del(txn)).is_ok()
        }
    }

    fn library_of(&mut self, name: &str, lib: &str) -> Option<String> {
        self.locate(name, lib).map(|(_, l, _)| l)
    }
}

/// Message descriptions from the store's message files, the built-in
/// sources behind them.
pub struct StoreMsgs {
    shared: Shared,
    builtin: msgf::BuiltinMsgs,
}

impl StoreMsgs {
    /// Over the lent store.
    pub fn new(shared: Shared) -> Self {
        Self {
            shared,
            builtin: msgf::BuiltinMsgs::new(),
        }
    }
}

impl msgf::MsgLookup for StoreMsgs {
    fn describe(&mut self, msgid: &str, msgf: &str, lib: &str) -> Option<MsgDesc> {
        let found = with_main(&self.shared, |txn, qsys| {
            let names = super::name_context(txn, qsys);
            let f = Name::parse(msgf).map_err(|e| e.to_string())?;
            let spec = match lib {
                "*LIBL" | "" => LibSpec::Libl,
                l => LibSpec::Name(Name::parse(l).map_err(|e| e.to_string())?),
            };
            msgf::store::rtvmsg(txn, qsys, &names, msgid, f, spec).map_err(|e| e.to_string())
        });
        match found {
            Ok(Some(d)) => Some(d),
            _ => self.builtin.describe(msgid, msgf, lib),
        }
    }
}

/// Data areas and data queues in the store.
pub struct StoreData {
    shared: Shared,
}

impl StoreData {
    /// Over the lent store.
    pub fn new(shared: Shared) -> Self {
        Self { shared }
    }

    fn run<T>(
        &self,
        name: &str,
        lib: &str,
        type_code: u16,
        f: impl Fn(&mut Txn<'_>, ObjectId) -> Result<T, String>,
    ) -> Result<T, IoFail> {
        let msg = if type_code == types::DTAARA {
            "CPF1015"
        } else {
            "CPF9801"
        };
        let (id, _, temp) = find_object(&self.shared, name, lib, type_code)
            .ok_or_else(|| IoFail::new(1299, msg, &[name, lib]))?;
        let r = if temp {
            with_temp(&self.shared, |txn, _| f(txn, id))
        } else {
            with_main(&self.shared, |txn, _| f(txn, id))
        };
        r.map_err(|e| IoFail {
            status: 1299,
            msgid: "CPF9899".into(),
            data: ccsid::to_ebcdic(&e).unwrap_or_default(),
        })
    }
}

/// A store data area as the runtime sees it.
fn value_of(kind: DataAreaType, v: &StoreValue) -> DataAreaValue {
    match (kind, v) {
        (DataAreaType::Char { len }, StoreValue::Char(s)) => {
            let mut b = ccsid::to_ebcdic(s).unwrap_or_default();
            b.resize(usize::from(len), 0x40);
            DataAreaValue {
                kind: DataAreaKind::Char,
                len: u32::from(len),
                decimals: 0,
                value: b,
            }
        }
        (DataAreaType::Dec { digits, decimals }, StoreValue::Dec(n)) => {
            let p = qdecimal::Packed::new(digits, decimals).expect("valid");
            let d = qdecimal::Decimal::new(*n, decimals).expect("fits");
            DataAreaValue {
                kind: DataAreaKind::Dec,
                len: u32::from(digits),
                decimals: u32::from(decimals),
                value: p
                    .encode(&d, qdecimal::Rounding::Truncate)
                    .unwrap_or_default(),
            }
        }
        (_, StoreValue::Lgl(b)) => DataAreaValue {
            kind: DataAreaKind::Lgl,
            len: 1,
            decimals: 0,
            value: vec![if *b { 0xF1 } else { 0xF0 }],
        },
        (kind, _) => DataAreaValue::chars(u32::from(match kind {
            DataAreaType::Char { len } => len,
            _ => 1,
        })),
    }
}

impl DataSource for StoreData {
    fn dtaara_lock(&mut self, name: &str, lib: &str) -> Result<(), IoFail> {
        self.run(name, lib, types::DTAARA, |txn, id| {
            dtaara::set_locked(txn, id, true).map_err(|e| e.to_string())
        })
    }

    fn dtaara_unlock(&mut self, name: &str, lib: &str) -> Result<(), IoFail> {
        self.run(name, lib, types::DTAARA, |txn, id| {
            dtaara::set_locked(txn, id, false).map_err(|e| e.to_string())
        })
    }

    fn dtaara_read(&mut self, name: &str, lib: &str) -> Result<DataAreaValue, IoFail> {
        self.run(name, lib, types::DTAARA, |txn, id| {
            let (kind, _) = dtaara::describe(txn, id).map_err(|e| e.to_string())?;
            let v = dtaara::read(txn, id).map_err(|e| e.to_string())?;
            Ok(value_of(kind, &v))
        })
    }

    fn dtaara_write(&mut self, name: &str, lib: &str, value: &DataAreaValue) -> Result<(), IoFail> {
        self.run(name, lib, types::DTAARA, |txn, id| {
            let (kind, _) = dtaara::describe(txn, id).map_err(|e| e.to_string())?;
            let v = match kind {
                DataAreaType::Char { .. } => StoreValue::Char(ccsid::from_ebcdic(&value.value)),
                DataAreaType::Dec { digits, decimals } => {
                    let p = qdecimal::Packed::new(digits, decimals).map_err(|e| e.to_string())?;
                    let d = p.decode(&value.value).map_err(|e| e.to_string())?;
                    StoreValue::Dec(
                        d.rescale(decimals, qdecimal::Rounding::Truncate)
                            .ok()
                            .and_then(|d| d.mantissa_i128())
                            .unwrap_or(0),
                    )
                }
                DataAreaType::Lgl => StoreValue::Lgl(value.value.first() == Some(&0xF1)),
            };
            dtaara::write(txn, id, &v).map_err(|e| e.to_string())
        })
    }

    fn dtaq_send(&mut self, name: &str, lib: &str, data: &[u8]) -> Result<(), IoFail> {
        self.run(name, lib, types::DTAQ, |txn, id| {
            dtaq::send(txn, id, &[], data, None)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
    }

    fn dtaq_receive(
        &mut self,
        name: &str,
        lib: &str,
        _wait: i64,
    ) -> Result<Option<Vec<u8>>, IoFail> {
        self.run(name, lib, types::DTAQ, |txn, id| {
            dtaq::receive(txn, id, &Select::Next)
                .map(|e| e.map(|e| e.data))
                .map_err(|e| e.to_string())
        })
    }
}

/// The main store, for the file I/O session.
pub struct MainStore {
    shared: Shared,
}

impl MainStore {
    /// Over the lent store.
    pub fn new(shared: Shared) -> Self {
        Self { shared }
    }
}

impl mi::io::store::StoreHandle for MainStore {
    fn store(&self) -> RefMut<'_, Store> {
        RefMut::map(self.shared.borrow_mut(), |o| {
            &mut o
                .as_mut()
                .expect("the store is lent while a program runs")
                .storage
                .store
        })
    }
}
