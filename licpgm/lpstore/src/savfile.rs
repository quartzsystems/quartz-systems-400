// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Product save streams (`SaveKind::LicPgm`): what SAVLICPGM writes,
//! the `Media` RSTLICPGM reads from one, and the `Restorer` over the
//! save engine. A stream holds the product's `*PRDDFN` and `*PRDLOD`
//! objects and, per library, the objects of its loads (the whole
//! library when it is not a system library).

use std::collections::BTreeMap;
use std::io::Cursor;

use licpgm::model::load::{LoadKey, ObjEntry};
use licpgm::payload::{prddfn, prdlod};
use licpgm::plan::save::SavePlan;
use licpgm::plan::{Media, MediaError, MediaLoad, Part, RestoreError, Restorer};
use licpgm::{ProductDefinition, ProductLoad, QualName};
use saverst::{Hooks, ObjSel, RestoreOptions, SaveOptions, SaveTarget};
use savfmt::{MediaHeader, Reader, Scan, Writer};
use store::obj::{JobHandle, ObjectId, types};

use crate::objstore::Cell;

/// The payload of an opaque object's saved space: a `u32` LE length
/// (the `wm::obj` prefix), then the bytes.
fn payload(space: &[u8]) -> Option<&[u8]> {
    let n = u32::from_le_bytes(space.get(..4)?.try_into().ok()?) as usize;
    space.get(4..4 + n)
}

/// A product save stream as installation media.
pub struct SavfMedia {
    stream: Vec<u8>,
    loads: Vec<MediaLoad>,
}

impl SavfMedia {
    /// Read the product objects of a stream.
    pub fn read(stream: Vec<u8>) -> Result<Self, MediaError> {
        let mut r =
            Reader::new(Cursor::new(stream.as_slice())).map_err(|e| MediaError(e.to_string()))?;
        let mut defs: Vec<(QualName, ProductDefinition)> = Vec::new();
        let mut lods: Vec<(QualName, ProductLoad)> = Vec::new();
        while let Some(s) = r.next_object().map_err(|e| MediaError(e.to_string()))? {
            let Scan::Object(o) = s else { continue };
            let d = &o.descriptor;
            let q = QualName::new(&savfmt::text(&d.name), &savfmt::text(&d.library));
            let space = payload(o.part(savfmt::Part::SPACE)).unwrap_or_default();
            if d.type_code == types::PRDDFN {
                if let Ok(v) = prddfn::decode(space) {
                    defs.push((q, v));
                }
            } else if d.type_code == types::PRDLOD
                && let Ok(v) = prdlod::decode(space)
            {
                lods.push((q, v));
            }
        }
        let mut loads = Vec::new();
        for (lq, l) in lods {
            let Some((dq, d)) = defs
                .iter()
                .find(|(_, d)| d.product == l.key.product && d.release == l.key.release)
            else {
                continue;
            };
            loads.push(MediaLoad {
                definition_name: dq.clone(),
                definition: d.clone(),
                load_name: lq,
                load: l,
            });
        }
        Ok(Self { stream, loads })
    }
}

impl Media for SavfMedia {
    fn loads(&self) -> Vec<MediaLoad> {
        self.loads.clone()
    }

    /// The whole stream: the restorer picks the library from it.
    fn save_data(&self, _key: &LoadKey, _part: &Part) -> Result<Vec<u8>, MediaError> {
        Ok(self.stream.clone())
    }
}

/// The save engine as `licpgm`'s restorer.
pub struct SaveRestorer<'c, 's> {
    cell: &'c Cell<'s>,
    qsys: ObjectId,
    job: JobHandle,
    hooks: &'c dyn Hooks,
    now: u64,
    system: String,
}

impl<'c, 's> SaveRestorer<'c, 's> {
    /// A restorer over the store in `cell`.
    #[must_use]
    pub fn new(
        cell: &'c Cell<'s>,
        qsys: ObjectId,
        job: JobHandle,
        hooks: &'c dyn Hooks,
        now: u64,
    ) -> Self {
        Self {
            cell,
            qsys,
            job,
            hooks,
            now,
            system: "QS400".into(),
        }
    }
}

fn product_objects() -> Vec<ObjSel> {
    [types::PRDDFN, types::PRDLOD]
        .into_iter()
        .map(|t| ObjSel {
            name: "*ALL".into(),
            type_code: Some(t),
        })
        .collect()
}

impl Restorer for SaveRestorer<'_, '_> {
    /// Restore what the stream saved from `library` into it. Libraries
    /// install under their primary names (no dynamic naming yet).
    fn restore(&mut self, stream: &[u8], library: &str) -> Result<Vec<ObjEntry>, RestoreError> {
        let mut r = Reader::new(Cursor::new(stream)).map_err(|e| RestoreError(e.to_string()))?;
        let opts = RestoreOptions {
            libraries: vec![library.to_string()],
            omit: product_objects(),
            system: self.system.clone(),
            now: self.now,
            ..RestoreOptions::default()
        };
        let mut s = self.cell.borrow_mut();
        let rep = saverst::restore(&mut s, self.job, self.hooks, self.qsys, &mut r, &opts)
            .map_err(|e| RestoreError(e.to_string()))?;
        Ok(rep
            .objects
            .iter()
            .filter(|o| o.msgid.is_none())
            .filter_map(|o| {
                types::type_by_code(o.type_code).map(|t| ObjEntry::new(&o.library, &o.name, t.name))
            })
            .collect())
    }

    /// `MIGRATE(phase11)`: IFS directories in product streams wait for
    /// the save engine's stream files; the system's own directories
    /// exist already.
    fn restore_directory(
        &mut self,
        _stream: &[u8],
        _path: &str,
    ) -> Result<Vec<String>, RestoreError> {
        Ok(Vec::new())
    }
}

/// SAVLICPGM: write a plan's objects into a stream with `header` (its
/// kind `SaveKind::LicPgm`). Returns the stream.
pub fn save_product(
    cell: &Cell<'_>,
    qsys: ObjectId,
    job: JobHandle,
    hooks: &dyn Hooks,
    plan: &SavePlan,
    header: &MediaHeader,
    command: &str,
) -> Result<Vec<u8>, String> {
    // Per library: whole, or these objects.
    let mut libs: BTreeMap<String, Option<Vec<ObjSel>>> = BTreeMap::new();
    let mut add = |lib: &str, sel: ObjSel| {
        if let Some(Some(v)) = libs.get_mut(lib) {
            v.push(sel);
        } else if !libs.contains_key(lib) {
            libs.insert(lib.to_string(), Some(vec![sel]));
        }
    };
    for q in &plan.definitions {
        add(
            &q.lib,
            ObjSel {
                name: q.name.clone(),
                type_code: Some(types::PRDDFN),
            },
        );
    }
    for s in &plan.loads {
        add(
            &s.load_name.lib,
            ObjSel {
                name: s.load_name.name.clone(),
                type_code: Some(types::PRDLOD),
            },
        );
        for o in &s.objects {
            add(
                &o.lib,
                ObjSel {
                    name: o.name.clone(),
                    type_code: types::type_by_name(&o.typ).map(|t| t.code),
                },
            );
        }
    }
    for s in &plan.loads {
        for l in &s.libraries {
            libs.insert(l.clone(), None);
        }
    }
    let mut w = Writer::new(Vec::new(), header).map_err(|e| format!("CPF3793 {e}"))?;
    let _ = w.parameters(command);
    let mut s = cell.borrow_mut();
    for (lib, sel) in &libs {
        let opts = SaveOptions {
            select: sel.clone().unwrap_or_default(),
            library_object: sel.is_none(),
            update_history: false,
            now: header.created,
            target: SaveTarget {
                command: "SAVLICPGM".into(),
                device: "*SAVF".into(),
                ..SaveTarget::default()
            },
            ..SaveOptions::default()
        };
        saverst::save_library(&mut s, job, hooks, qsys, lib, &opts, &mut w)
            .map_err(|e| format!("CPF3793 {e}"))?;
    }
    let (_, bytes) = w.finish(0).map_err(|e| format!("CPF3793 {e}"))?;
    Ok(bytes)
}
