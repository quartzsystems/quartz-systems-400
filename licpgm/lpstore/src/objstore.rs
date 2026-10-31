// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `licpgm::ProductStore` and `ObjectCheck` over the object store. Each
//! call is one transaction. The license repository is the user space
//! `QUSRSYS/QLZALIC`, the agreement answers `QUSRSYS/QSZAGREE`.

use std::cell::RefCell;

use licpgm::agree::AgreementBook;
use licpgm::license::repo::LicenseRepo;
use licpgm::model::load::ObjEntry;
use licpgm::payload::{prddfn, prdlod};
use licpgm::{
    ObjStatus, ObjectCheck, ProductDefinition, ProductLoad, ProductStore, QualName, StoreError,
};
use qsname::Name;
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types, usrspc};
use store::{Store, Txn};
use wm::obj::Stamp;

/// The library of the records.
pub const RECORD_LIB: &str = "QUSRSYS";
/// The license repository's user space.
pub const LICENSE_SPACE: &str = "QLZALIC";
/// The agreement answers' user space.
pub const AGREEMENT_SPACE: &str = "QSZAGREE";

/// The shared store both the product store and the restorer use.
pub type Cell<'s> = RefCell<&'s mut Store>;

/// A path check for [`ObjectCheck::path`]; the IFS is the caller's.
pub type PathCheck<'c> = &'c dyn Fn(&str) -> ObjStatus;

/// The products of a store.
pub struct Products<'c, 's> {
    cell: &'c Cell<'s>,
    qsys: ObjectId,
    stamp: Stamp,
    paths: Option<PathCheck<'c>>,
}

fn err(e: &store::StoreError) -> StoreError {
    StoreError::Other(e.to_string())
}

fn name(s: &str) -> Result<Name, StoreError> {
    Name::parse(s).map_err(|e| StoreError::Other(format!("{s}: {e}")))
}

/// Every object of a type in every library, with its qualified name.
fn all_of(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    type_code: u16,
) -> store::Result<Vec<(QualName, ObjectId)>> {
    let mut libs = vec![("QSYS".to_string(), qsys)];
    libs.extend(
        ops::libraries(txn, qsys)?
            .into_iter()
            .filter(|(_, id)| *id != qsys)
            .map(|(n, id)| (n.to_string(), id)),
    );
    let mut out = Vec::new();
    for (lib, id) in libs {
        for (k, oid) in ops::list(txn, id, Some(type_code))? {
            out.push((QualName::new(&k.name.to_string(), &lib), oid));
        }
    }
    Ok(out)
}

fn library_id(txn: &mut Txn<'_>, qsys: ObjectId, lib: &str) -> Result<ObjectId, StoreError> {
    if lib == "QSYS" {
        return Ok(qsys);
    }
    let n = name(lib)?;
    ops::find_library(txn, qsys, n)
        .map(|f| f.id)
        .map_err(|_| StoreError::NoLibrary(lib.to_string()))
}

fn find(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    at: &QualName,
    type_code: u16,
) -> Result<Option<ObjectId>, StoreError> {
    let lib = library_id(txn, qsys, &at.lib)?;
    let n = name(&at.name)?;
    let hit = ops::list(txn, lib, Some(type_code))
        .map_err(|e| err(&e))?
        .into_iter()
        .find(|(k, _)| k.name == n)
        .map(|(_, id)| id);
    Ok(hit)
}

impl<'c, 's> Products<'c, 's> {
    /// Products of the store in `cell`.
    #[must_use]
    pub fn new(cell: &'c Cell<'s>, qsys: ObjectId, stamp: Stamp) -> Self {
        Self {
            cell,
            qsys,
            stamp,
            paths: None,
        }
    }

    /// Answer [`ObjectCheck::path`] with `f`.
    #[must_use]
    pub fn with_paths(mut self, f: PathCheck<'c>) -> Self {
        self.paths = Some(f);
        self
    }

    /// Run `f` in one transaction, committed when it succeeds.
    fn txn<T>(
        &self,
        f: impl FnOnce(&mut Txn<'_>, ObjectId) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let mut s = self.cell.borrow_mut();
        let mut t = s.begin().map_err(|e| err(&e))?;
        match f(&mut t, self.qsys) {
            Ok(v) => {
                t.commit().map_err(|e| err(&e))?;
                Ok(v)
            }
            Err(e) => {
                t.abort();
                Err(e)
            }
        }
    }

    fn read_all<T>(&self, type_code: u16, decode: fn(&[u8]) -> Option<T>) -> Vec<(QualName, T)> {
        self.txn(|t, qsys| {
            let mut out = Vec::new();
            for (q, id) in all_of(t, qsys, type_code).map_err(|e| err(&e))? {
                if let Ok(b) = wm::obj::read(t, id)
                    && let Some(v) = decode(&b)
                {
                    out.push((q, v));
                }
            }
            Ok(out)
        })
        .unwrap_or_default()
    }

    fn create(
        &self,
        at: &QualName,
        type_code: u16,
        text: &str,
        payload: &[u8],
    ) -> Result<(), StoreError> {
        let stamp = self.stamp.clone();
        self.txn(|t, qsys| {
            if find(t, qsys, at, type_code)?.is_some() {
                return Err(StoreError::Exists(at.clone()));
            }
            let lib = library_id(t, qsys, &at.lib)?;
            wm::obj::create(t, lib, name(&at.name)?, type_code, text, &stamp, payload)
                .map_err(|e| StoreError::Other(e.to_string()))?;
            Ok(())
        })
    }

    fn update(&self, at: &QualName, type_code: u16, payload: &[u8]) -> Result<(), StoreError> {
        self.txn(|t, qsys| {
            let id =
                find(t, qsys, at, type_code)?.ok_or_else(|| StoreError::NotFound(at.clone()))?;
            wm::obj::write(t, id, payload).map_err(|e| StoreError::Other(e.to_string()))
        })
    }

    fn delete(&self, at: &QualName, type_code: u16) -> Result<(), StoreError> {
        self.txn(|t, qsys| {
            let id =
                find(t, qsys, at, type_code)?.ok_or_else(|| StoreError::NotFound(at.clone()))?;
            ops::delete(t, JobHandle(0), id).map_err(|e| err(&e))
        })
    }

    /// A record space's bytes (a `u32` length, then the record).
    fn record(&self, space: &str) -> Option<Vec<u8>> {
        self.txn(|t, qsys| {
            let Some(id) = find(t, qsys, &QualName::new(space, RECORD_LIB), types::USRSPC)? else {
                return Ok(None);
            };
            let head = usrspc::read(t, id, 0, 4).map_err(|e| err(&e))?;
            let n = u32::from_be_bytes(head[..4].try_into().expect("4")) as usize;
            Ok(Some(usrspc::read(t, id, 4, n).map_err(|e| err(&e))?))
        })
        .ok()
        .flatten()
    }

    fn save_record(&self, space: &str, text: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let stamp = self.stamp.clone();
        let mut data = u32::try_from(bytes.len())
            .map_err(|_| StoreError::Other("record over 4 GiB".into()))?
            .to_be_bytes()
            .to_vec();
        data.extend_from_slice(bytes);
        self.txn(|t, qsys| {
            let at = QualName::new(space, RECORD_LIB);
            let id = match find(t, qsys, &at, types::USRSPC)? {
                Some(id) => id,
                None => {
                    let lib = library_id(t, qsys, RECORD_LIB)?;
                    usrspc::create(
                        t,
                        &CreateParams {
                            library: lib,
                            name: name(space)?,
                            type_code: types::USRSPC,
                            attribute: String::new(),
                            text: text.into(),
                            owner: stamp.owner.clone(),
                            system: stamp.system.clone(),
                            now: stamp.now,
                            by_system: true,
                        },
                        4096,
                        true,
                        0,
                    )
                    .map_err(|e| err(&e))?
                }
            };
            usrspc::write(t, id, 0, &data).map_err(|e| err(&e))
        })
    }
}

impl ProductStore for Products<'_, '_> {
    fn definitions(&self) -> Vec<(QualName, ProductDefinition)> {
        self.read_all(types::PRDDFN, |b| prddfn::decode(b).ok())
    }

    fn create_definition(
        &mut self,
        at: &QualName,
        d: &ProductDefinition,
    ) -> Result<(), StoreError> {
        let b = prddfn::encode(d).map_err(|e| StoreError::Other(e.to_string()))?;
        self.create(at, types::PRDDFN, &d.text, &b)
    }

    fn delete_definition(&mut self, at: &QualName) -> Result<(), StoreError> {
        self.delete(at, types::PRDDFN)
    }

    fn loads(&self) -> Vec<(QualName, ProductLoad)> {
        self.read_all(types::PRDLOD, |b| prdlod::decode(b).ok())
    }

    fn create_load(&mut self, at: &QualName, l: &ProductLoad) -> Result<(), StoreError> {
        let b = prdlod::encode(l).map_err(|e| StoreError::Other(e.to_string()))?;
        self.create(at, types::PRDLOD, &l.text, &b)
    }

    fn update_load(&mut self, at: &QualName, l: &ProductLoad) -> Result<(), StoreError> {
        let b = prdlod::encode(l).map_err(|e| StoreError::Other(e.to_string()))?;
        self.update(at, types::PRDLOD, &b)
    }

    fn delete_load(&mut self, at: &QualName) -> Result<(), StoreError> {
        self.delete(at, types::PRDLOD)
    }

    fn library_exists(&self, lib: &str) -> bool {
        self.txn(|t, qsys| library_id(t, qsys, lib)).is_ok()
    }

    fn licenses(&self) -> LicenseRepo {
        self.record(LICENSE_SPACE)
            .and_then(|b| LicenseRepo::decode(&b).ok())
            .unwrap_or_default()
    }

    fn save_licenses(&mut self, r: &LicenseRepo) -> Result<(), StoreError> {
        let b = r.encode().map_err(|e| StoreError::Other(e.to_string()))?;
        self.save_record(LICENSE_SPACE, "License information", &b)
    }

    fn agreements(&self) -> AgreementBook {
        self.record(AGREEMENT_SPACE)
            .and_then(|b| AgreementBook::decode(&b).ok())
            .unwrap_or_default()
    }

    fn save_agreements(&mut self, a: &AgreementBook) -> Result<(), StoreError> {
        let b = a.encode().map_err(|e| StoreError::Other(e.to_string()))?;
        self.save_record(AGREEMENT_SPACE, "Software agreement answers", &b)
    }
}

impl ObjectCheck for Products<'_, '_> {
    fn object(&self, o: &ObjEntry) -> ObjStatus {
        let Some(t) = types::type_by_name(&o.typ) else {
            return ObjStatus::Missing;
        };
        let code = t.code;
        self.txn(|txn, qsys| {
            let Some(id) = find(txn, qsys, &QualName::new(&o.name, &o.lib), code)? else {
                return Ok(ObjStatus::Missing);
            };
            let (_, h) = ops::read_header(txn, id).map_err(|e| err(&e))?;
            Ok(if h.damaged {
                ObjStatus::Damaged
            } else {
                ObjStatus::Present(None)
            })
        })
        .unwrap_or(ObjStatus::Missing)
    }

    fn objects_in(&self, lib: &str) -> Vec<ObjEntry> {
        self.txn(|txn, qsys| {
            let id = library_id(txn, qsys, lib)?;
            Ok(ops::list(txn, id, None)
                .map_err(|e| err(&e))?
                .into_iter()
                // The product objects are the package, not its content.
                .filter(|(k, _)| !matches!(k.type_code, types::PRDDFN | types::PRDLOD))
                .filter_map(|(k, _)| {
                    types::type_by_code(k.type_code)
                        .map(|t| ObjEntry::new(lib, &k.name.to_string(), t.name))
                })
                .collect())
        })
        .unwrap_or_default()
    }

    fn path(&self, path: &str) -> ObjStatus {
        self.paths.map_or(ObjStatus::Present(None), |f| f(path))
    }
}
