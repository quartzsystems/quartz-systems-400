// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SAVLICPGM: what goes on the media for a product.

use release::Release;

use crate::id::{LoadId, LoadType, OptionNo, ProductId};
use crate::model::QualName;
use crate::model::load::{ObjEntry, ProductLoad};
use crate::model::state::LoadState;
use crate::plan::{Objects, PlanError, SYSTEM_LIBS};
use crate::store::ProductStore;

/// One SAVLICPGM.
#[derive(Clone, Debug)]
pub struct SavLicPgm {
    /// `LICPGM`.
    pub product: ProductId,
    /// `OPTION`; `None` is `*ALL`.
    pub option: Option<OptionNo>,
    /// `RLS`; `None` is any.
    pub release: Option<Release>,
    /// `LNG`; `None` is every language.
    pub lng: Option<LoadId>,
    /// `OBJTYPE`.
    pub objects: Objects,
}

/// What one load contributes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveLoad {
    /// The `*PRDLOD` object.
    pub load_name: QualName,
    /// The load.
    pub load: ProductLoad,
    /// Libraries saved whole (installed names).
    pub libraries: Vec<String>,
    /// Objects saved one by one, from the system libraries.
    pub objects: Vec<ObjEntry>,
    /// IFS directories saved.
    pub directories: Vec<String>,
}

/// What SAVLICPGM writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavePlan {
    /// The `*PRDDFN` objects.
    pub definitions: Vec<QualName>,
    /// The loads, base and code first.
    pub loads: Vec<SaveLoad>,
}

/// Plan a SAVLICPGM.
pub fn plan_save(store: &dyn ProductStore, req: &SavLicPgm) -> Result<SavePlan, PlanError> {
    let mut sel: Vec<(QualName, ProductLoad)> = store
        .loads()
        .into_iter()
        .filter(|(_, l)| {
            let k = &l.key;
            k.product == req.product
                && req.option.is_none_or(|o| k.option == o)
                && req.release.is_none_or(|r| k.release == r)
                && match req.objects {
                    Objects::All => true,
                    Objects::Pgm => k.load_type == LoadType::Code,
                    Objects::Lng => k.load_type == LoadType::Lng,
                }
                && (k.load_type == LoadType::Code
                    || req.lng.as_ref().is_none_or(|g| &k.load_id == g))
        })
        .collect();
    if sel.is_empty() {
        return Err(PlanError::NotInstalled(req.product.clone()));
    }
    sel.sort_by(|(_, a), (_, b)| a.key.cmp(&b.key));
    let mut loads = Vec::new();
    for (q, l) in sel {
        if !matches!(l.state, LoadState::Installed | LoadState::Packaged) {
            return Err(PlanError::NotSavable(l.key.clone(), l.state));
        }
        let mut libraries = Vec::new();
        let mut objects = Vec::new();
        for lib in &l.libraries {
            if SYSTEM_LIBS.contains(&lib.installed.as_str()) {
                objects.extend(
                    l.objects
                        .iter()
                        .filter(|o| o.lib == lib.primary)
                        .map(|o| ObjEntry::new(&lib.installed, &o.name, &o.typ)),
                );
            } else {
                libraries.push(lib.installed.clone());
            }
        }
        let directories = l.directories.iter().map(|d| d.installed.clone()).collect();
        loads.push(SaveLoad {
            load_name: q,
            load: l,
            libraries,
            objects,
            directories,
        });
    }
    let definitions = store
        .definitions()
        .into_iter()
        .filter(|(_, d)| {
            loads
                .iter()
                .any(|s| s.load.key.product == d.product && s.load.key.release == d.release)
        })
        .map(|(q, _)| q)
        .collect();
    Ok(SavePlan { definitions, loads })
}
