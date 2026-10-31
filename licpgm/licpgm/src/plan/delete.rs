// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DLTLICPGM: which loads, libraries, objects and paths go, and
//! carrying it out.

use release::Release;

use crate::id::{LoadId, LoadType, OptionNo, ProductId};
use crate::model::QualName;
use crate::model::load::{LoadKey, ObjEntry, ProductLoad};
use crate::model::state::LoadState;
use crate::plan::{ExitOp, ExitRunner, PlanError, SYSTEM_DIRS, SYSTEM_LIBS};
use crate::store::ProductStore;

/// One DLTLICPGM.
#[derive(Clone, Debug)]
pub struct DltLicPgm {
    /// `LICPGM`.
    pub product: ProductId,
    /// `OPTION`; `None` is `*ALL`.
    pub option: Option<OptionNo>,
    /// `RLS`; `None` is any.
    pub release: Option<Release>,
    /// `LNG`; `None` deletes code and every language, a language
    /// deletes only that language load.
    pub lng: Option<LoadId>,
}

/// What DLTLICPGM will remove.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeletePlan {
    /// Loads, language loads first, options before `*BASE`.
    pub loads: Vec<(QualName, ProductLoad)>,
    /// Libraries deleted whole.
    pub libraries: Vec<String>,
    /// Objects deleted one by one (in libraries that stay).
    pub objects: Vec<ObjEntry>,
    /// IFS paths deleted.
    pub paths: Vec<String>,
    /// Directories deleted.
    pub directories: Vec<String>,
    /// Definitions deleted because no load of them is left.
    pub definitions: Vec<QualName>,
}

/// Plan a DLTLICPGM.
pub fn plan_delete(store: &dyn ProductStore, req: &DltLicPgm) -> Result<DeletePlan, PlanError> {
    let all = store.loads();
    let base_goes = req.lng.is_none() && req.option.is_none_or(OptionNo::is_base);
    let mut loads: Vec<(QualName, ProductLoad)> = all
        .iter()
        .filter(|(_, l)| {
            let k = &l.key;
            k.product == req.product
                && req.release.is_none_or(|r| k.release == r)
                && (base_goes || req.option.is_none_or(|o| k.option == o))
                && match &req.lng {
                    Some(lng) => k.load_type == LoadType::Lng && &k.load_id == lng,
                    None => true,
                }
                && l.state.is_installed()
        })
        .cloned()
        .collect();
    if loads.is_empty() {
        return Err(PlanError::NotInstalled(req.product.clone()));
    }
    loads.sort_by(|(_, a), (_, b)| {
        (b.key.load_type, b.key.option, &b.key.load_id).cmp(&(
            a.key.load_type,
            a.key.option,
            &a.key.load_id,
        ))
    });
    let going: Vec<&LoadKey> = loads.iter().map(|(_, l)| &l.key).collect();
    let stays = |l: &ProductLoad| !going.contains(&&l.key);

    // Installed options elsewhere that need a code load going away.
    let defs = store.definitions();
    let mut dependents = Vec::new();
    for (_, l) in all
        .iter()
        .filter(|(_, l)| stays(l) && l.state.is_installed() && l.key.load_type == LoadType::Code)
    {
        let Some((_, d)) = defs
            .iter()
            .find(|(_, d)| d.product == l.key.product && d.release == l.key.release)
        else {
            continue;
        };
        let Some(o) = d.option(l.key.option) else {
            continue;
        };
        let needs_gone = o.requires.iter().any(|p| {
            going.iter().any(|k| {
                k.product == p.product && k.option == p.option && k.load_type == LoadType::Code
            })
        });
        if needs_gone {
            dependents.push((l.key.product.clone(), l.key.option));
        }
    }
    if !dependents.is_empty() {
        dependents.sort();
        dependents.dedup();
        return Err(PlanError::Dependents(dependents));
    }

    let remaining: Vec<&ProductLoad> = all.iter().map(|(_, l)| l).filter(|l| stays(l)).collect();
    let mut plan = DeletePlan::default();
    for (_, l) in &loads {
        for lib in &l.libraries {
            let shared = SYSTEM_LIBS.contains(&lib.installed.as_str())
                || remaining
                    .iter()
                    .any(|r| r.libraries.iter().any(|x| x.installed == lib.installed));
            if !shared && !plan.libraries.contains(&lib.installed) {
                plan.libraries.push(lib.installed.clone());
            }
        }
        for o in &l.objects {
            let lib = l
                .libraries
                .iter()
                .find(|e| e.primary == o.lib)
                .map_or(o.lib.clone(), |e| e.installed.clone());
            if !plan.libraries.contains(&lib) {
                plan.objects.push(ObjEntry::new(&lib, &o.name, &o.typ));
            }
        }
        plan.paths.extend(l.paths.iter().cloned());
        for d in &l.directories {
            let shared = SYSTEM_DIRS.contains(&d.installed.as_str())
                || remaining
                    .iter()
                    .any(|r| r.directories.iter().any(|x| x.installed == d.installed));
            if !shared && !plan.directories.contains(&d.installed) {
                plan.directories.push(d.installed.clone());
            }
        }
    }
    plan.objects.sort();
    plan.objects.dedup();
    for (q, d) in &defs {
        let has_left = remaining
            .iter()
            .any(|l| l.key.product == d.product && l.key.release == d.release);
        let touched = going
            .iter()
            .any(|k| k.product == d.product && k.release == d.release);
        if touched && !has_left {
            plan.definitions.push(q.clone());
        }
    }
    plan.loads = loads;
    Ok(plan)
}

/// Removes objects for DLTLICPGM.
pub trait Remover {
    /// Delete a library and everything in it.
    fn delete_library(&mut self, lib: &str) -> Result<(), String>;
    /// Delete one object.
    fn delete_object(&mut self, o: &ObjEntry) -> Result<(), String>;
    /// Delete an IFS path (a directory with what is below it).
    fn delete_path(&mut self, path: &str) -> Result<(), String>;
}

/// Carry out a delete plan: preoperation exits, then the objects, then
/// the product records. A failure leaves the loads in state `64`.
pub fn execute_delete(
    plan: &DeletePlan,
    store: &mut dyn ProductStore,
    remover: &mut dyn Remover,
    exits: &mut dyn ExitRunner,
) -> Result<(), PlanError> {
    for (q, l) in &plan.loads {
        let mut l = l.clone();
        for e in &l.pre_exits {
            exits
                .call(e, ExitOp::Delete, &l.key)
                .map_err(|why| PlanError::Exit(e.clone(), l.key.clone(), why))?;
        }
        l.state = LoadState::DeletingInstalled;
        store.update_load(q, &l)?;
    }
    let fail = |store: &mut dyn ProductStore, why: String| -> PlanError {
        for (q, l) in &plan.loads {
            let mut l = l.clone();
            l.state = LoadState::InstalledMissing;
            let _ = store.update_load(q, &l);
        }
        PlanError::Restore(plan.loads[0].1.key.clone(), why)
    };
    let mut r: Result<(), String> = Ok(());
    for o in &plan.objects {
        r = r.and_then(|()| remover.delete_object(o));
    }
    if let Err(why) = r {
        return Err(fail(store, why));
    }
    // The product's own records go before its libraries: the *PRDDFN
    // and *PRDLODs usually live in the principal library.
    for (q, _) in &plan.loads {
        store.delete_load(q)?;
    }
    let mut book = store.agreements();
    let mut lic = store.licenses();
    for (q, d) in store.definitions() {
        if plan.definitions.contains(&q) {
            book.forget(&d.product, d.release);
            lic.forget_product(&d.product);
            store.delete_definition(&q)?;
        }
    }
    store.save_agreements(&book)?;
    store.save_licenses(&lic)?;
    for lib in &plan.libraries {
        r = r.and_then(|()| remover.delete_library(lib));
    }
    for p in plan.paths.iter().chain(&plan.directories) {
        r = r.and_then(|()| remover.delete_path(p));
    }
    r.map_err(|why| PlanError::Restore(plan.loads[0].1.key.clone(), why))
}
