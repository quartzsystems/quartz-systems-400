// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! CHKPRDOPT: are an option's objects there, undamaged, at the right
//! release; are its prerequisites installed. Load states follow.

use release::Release;
use thiserror::Error;

use crate::id::{LoadType, OptionNo, ProductId};
use crate::model::definition::Prereq;
use crate::model::load::{LoadKey, ObjEntry, ProductLoad};
use crate::model::state::LoadState;
use crate::store::{ObjStatus, ObjectCheck, ProductStore, StoreError};

/// One thing CHKPRDOPT found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finding {
    /// An object is missing.
    Missing(ObjEntry),
    /// An object is damaged.
    Damaged(ObjEntry),
    /// An object's description records another release.
    WrongRelease(ObjEntry, Release),
    /// An IFS path is missing.
    PathMissing(String),
    /// An IFS path is damaged.
    PathDamaged(String),
    /// A prerequisite option is not installed.
    Prereq(Prereq),
}

/// What was checked and what became of each load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckReport {
    /// Per load: its key, its state after the check, the findings.
    pub loads: Vec<(LoadKey, LoadState, Vec<Finding>)>,
}

impl CheckReport {
    /// Whether nothing was found.
    #[must_use]
    pub fn clean(&self) -> bool {
        self.loads.iter().all(|(_, _, f)| f.is_empty())
    }
}

/// CHKPRDOPT cannot run.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum CheckError {
    /// Nothing installed matches.
    #[error("product {0} option {1} is not installed")]
    NotInstalled(ProductId, String),
    /// Storing a new state failed.
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Check the objects of one load.
#[must_use]
pub fn check_load(l: &ProductLoad, objs: &dyn ObjectCheck) -> Vec<Finding> {
    let mut out = Vec::new();
    for o in &l.objects {
        // Objects install into the library's installed name.
        let lib = l
            .libraries
            .iter()
            .find(|e| e.primary == o.lib)
            .map_or(o.lib.as_str(), |e| e.installed.as_str());
        let entries = if o.name == "*ALL" || o.typ == "*ALL" {
            objs.objects_in(lib)
                .into_iter()
                .filter(|x| {
                    (o.name == "*ALL" || x.name == o.name) && (o.typ == "*ALL" || x.typ == o.typ)
                })
                .collect()
        } else {
            vec![ObjEntry::new(lib, &o.name, &o.typ)]
        };
        for e in entries {
            match objs.object(&e) {
                ObjStatus::Missing => out.push(Finding::Missing(e)),
                ObjStatus::Damaged => out.push(Finding::Damaged(e)),
                ObjStatus::Present(Some(r)) if r != l.key.release => {
                    out.push(Finding::WrongRelease(e, r));
                }
                ObjStatus::Present(_) => {}
            }
        }
    }
    for p in &l.paths {
        match objs.path(p) {
            ObjStatus::Missing => out.push(Finding::PathMissing(p.clone())),
            ObjStatus::Damaged => out.push(Finding::PathDamaged(p.clone())),
            ObjStatus::Present(_) => {}
        }
    }
    out
}

/// The state a load moves to after a check.
#[must_use]
pub fn state_after(state: LoadState, findings: &[Finding]) -> LoadState {
    let damaged = findings
        .iter()
        .any(|f| matches!(f, Finding::Damaged(_) | Finding::PathDamaged(_)));
    let missing = findings.iter().any(|f| {
        matches!(
            f,
            Finding::Missing(_) | Finding::PathMissing(_) | Finding::WrongRelease(..)
        )
    });
    let installed = state.is_installed();
    match (damaged, missing, installed) {
        (true, _, true) => LoadState::InstalledDamaged,
        (true, _, false) => LoadState::PackagedDamaged,
        (false, true, true) => LoadState::InstalledMissing,
        (false, true, false) => LoadState::PackagedMissing,
        (false, false, _) => match state {
            LoadState::Loaded | LoadState::InstalledDamaged | LoadState::InstalledMissing => {
                LoadState::Installed
            }
            LoadState::PackagedDamaged | LoadState::PackagedMissing => LoadState::Packaged,
            s => s,
        },
    }
}

/// CHKPRDOPT PRDID OPTION(`None` is `*ALL`) RLS: check every installed
/// load of the product and store the states that follow.
pub fn chkprdopt(
    store: &mut dyn ProductStore,
    objs: &dyn ObjectCheck,
    product: &ProductId,
    option: Option<OptionNo>,
    rel: Option<Release>,
) -> Result<CheckReport, CheckError> {
    let all = store.loads();
    let targets: Vec<_> = all
        .iter()
        .filter(|(_, l)| {
            &l.key.product == product
                && option.is_none_or(|o| l.key.option == o)
                && rel.is_none_or(|r| l.key.release == r)
                && l.state.is_installed()
        })
        .cloned()
        .collect();
    if targets.is_empty() {
        return Err(CheckError::NotInstalled(
            product.clone(),
            option.map_or_else(|| "*ALL".into(), |o| o.to_string()),
        ));
    }
    let defs = store.definitions();
    let mut report = CheckReport { loads: Vec::new() };
    for (q, mut l) in targets {
        let mut findings = check_load(&l, objs);
        if l.key.load_type == LoadType::Code
            && let Some((_, d)) = defs
                .iter()
                .find(|(_, d)| d.product == l.key.product && d.release == l.key.release)
            && let Some(o) = d.option(l.key.option)
        {
            for p in &o.requires {
                let ok = all.iter().any(|(_, x)| {
                    x.key.product == p.product
                        && x.key.option == p.option
                        && x.key.load_type == LoadType::Code
                        && x.state.is_installed()
                        && p.min_release.is_none_or(|m| x.key.release >= m)
                });
                if !ok {
                    findings.push(Finding::Prereq(p.clone()));
                }
            }
        }
        let new = state_after(l.state, &findings);
        if new != l.state {
            l.state = new;
            store.update_load(&q, &l)?;
        }
        report.loads.push((l.key.clone(), new, findings));
    }
    Ok(report)
}
