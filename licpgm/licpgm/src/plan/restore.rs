// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! RSTLICPGM: what installs, in what order, and carrying it out.

use std::collections::BTreeSet;

use release::Release;

use crate::id::{LoadId, LoadType, OptionNo, ProductId};
use crate::model::QualName;
use crate::model::definition::ProductDefinition;
use crate::model::load::{LoadKey, ObjEntry, ProductLoad};
use crate::model::state::LoadState;
use crate::plan::{ExitOp, ExitRunner, Media, MediaLoad, Objects, Part, PlanError, Restorer};
use crate::store::{ProductStore, find_definition};
use crate::system::SystemInfo;

/// One RSTLICPGM.
#[derive(Clone, Debug)]
pub struct RstLicPgm {
    /// `LICPGM`.
    pub product: ProductId,
    /// `OPTION`.
    pub option: OptionNo,
    /// `RLS`; `None` is the only one on the media.
    pub release: Option<Release>,
    /// `LNG`; `None` is the system's primary language.
    pub lng: Option<LoadId>,
    /// `RSTOBJ`.
    pub objects: Objects,
}

impl RstLicPgm {
    /// `RSTLICPGM LICPGM(p) OPTION(o)` with the defaults.
    #[must_use]
    pub fn new(product: ProductId, option: OptionNo) -> Self {
        Self {
            product,
            option,
            release: None,
            lng: None,
            objects: Objects::All,
        }
    }
}

/// One load to install.
#[derive(Clone, Debug, PartialEq)]
pub struct InstallStep {
    /// The load as on the media.
    pub media: MediaLoad,
    /// The installed load it replaces, if any.
    pub replaces: Option<(QualName, ProductLoad)>,
}

impl InstallStep {
    /// Which load.
    #[must_use]
    pub fn key(&self) -> &LoadKey {
        &self.media.load.key
    }
}

/// What RSTLICPGM (or GO LICPGM's install list) will do.
#[derive(Clone, Debug, PartialEq)]
pub struct InstallPlan {
    /// Loads in install order: prerequisites first, `*BASE` before its
    /// options, `*CODE` before `*LNG`.
    pub steps: Vec<InstallStep>,
    /// Products whose software agreement must be accepted first.
    pub agreements: Vec<(ProductId, Release)>,
}

fn installed<'a>(
    store_loads: &'a [ProductLoad],
    p: &ProductId,
    o: OptionNo,
    t: LoadType,
) -> Option<&'a ProductLoad> {
    store_loads.iter().find(|l| {
        &l.key.product == p && l.key.option == o && l.key.load_type == t && l.state.is_installed()
    })
}

/// Plan the restore of one or more requests.
pub fn plan_restore(
    store: &dyn ProductStore,
    media: &dyn Media,
    sys: &SystemInfo,
    requests: &[RstLicPgm],
) -> Result<InstallPlan, PlanError> {
    let on_media = media.loads();
    let have: Vec<ProductLoad> = store.loads().into_iter().map(|(_, l)| l).collect();
    let mut picked: Vec<MediaLoad> = Vec::new();
    for req in requests {
        let mine: Vec<&MediaLoad> = on_media
            .iter()
            .filter(|m| m.load.key.product == req.product && m.load.key.option == req.option)
            .collect();
        let rels: BTreeSet<Release> = mine.iter().map(|m| m.load.key.release).collect();
        let rel = match req.release {
            Some(r) => r,
            None if rels.len() == 1 => *rels.iter().next().expect("one"),
            None if rels.is_empty() => {
                return Err(PlanError::NotOnMedia {
                    product: req.product.clone(),
                    option: req.option,
                    what: "*CODE".into(),
                });
            }
            None => return Err(PlanError::SeveralReleases(req.product.clone())),
        };
        let at_rel: Vec<&MediaLoad> = mine
            .into_iter()
            .filter(|m| m.load.key.release == rel)
            .collect();
        let code = at_rel
            .iter()
            .find(|m| m.load.key.load_type == LoadType::Code);
        if req.objects != Objects::Lng {
            let code = code.ok_or_else(|| PlanError::NotOnMedia {
                product: req.product.clone(),
                option: req.option,
                what: "*CODE".into(),
            })?;
            picked.push((*code).clone());
        }
        if req.objects != Objects::Pgm {
            let lng = req.lng.clone().unwrap_or_else(|| sys.primary_lng.clone());
            let def = code
                .map(|m| &m.definition)
                .or_else(|| at_rel.first().map(|m| &m.definition));
            let offered = def
                .map(|d| d.languages_of(req.option, &sys.languages))
                .unwrap_or_default();
            if !offered.is_empty() || req.objects == Objects::Lng {
                let l = at_rel
                    .iter()
                    .find(|m| m.load.key.load_type == LoadType::Lng && m.load.key.load_id == lng)
                    .ok_or_else(|| PlanError::NotOnMedia {
                        product: req.product.clone(),
                        option: req.option,
                        what: lng.to_string(),
                    })?;
                picked.push((*l).clone());
            }
            if req.objects == Objects::Lng
                && installed(&have, &req.product, req.option, LoadType::Code).is_none()
            {
                return Err(PlanError::CodeMissing(req.product.clone(), req.option));
            }
        }
    }
    picked.sort_by(|a, b| a.load.key.cmp(&b.load.key));
    picked.dedup_by(|a, b| a.load.key == b.load.key);

    let in_plan = |p: &ProductId, o: OptionNo| {
        picked.iter().any(|m| {
            &m.load.key.product == p
                && m.load.key.option == o
                && m.load.key.load_type == LoadType::Code
        })
    };
    // Base and prerequisites.
    for m in &picked {
        let k = &m.load.key;
        if k.load_type != LoadType::Code {
            continue;
        }
        if !k.option.is_base()
            && !in_plan(&k.product, OptionNo::BASE)
            && installed(&have, &k.product, OptionNo::BASE, LoadType::Code).is_none()
        {
            return Err(PlanError::BaseMissing(k.product.clone()));
        }
        if let Some(o) = m.definition.option(k.option) {
            for p in &o.requires {
                let there = in_plan(&p.product, p.option)
                    || installed(&have, &p.product, p.option, LoadType::Code)
                        .is_some_and(|l| p.min_release.is_none_or(|r| l.key.release >= r));
                if !there {
                    return Err(PlanError::Prereq {
                        for_product: k.product.clone(),
                        for_option: k.option,
                        needs: p.clone(),
                    });
                }
            }
        }
    }
    // Order: a load after everything it depends on in the plan.
    let deps = |m: &MediaLoad| -> Vec<LoadKey> {
        let k = &m.load.key;
        let mut d = Vec::new();
        let code_of = |p: &ProductId, o: OptionNo| {
            picked
                .iter()
                .find(|x| {
                    &x.load.key.product == p
                        && x.load.key.option == o
                        && x.load.key.load_type == LoadType::Code
                })
                .map(|x| x.load.key.clone())
        };
        if k.load_type == LoadType::Lng {
            d.extend(code_of(&k.product, k.option));
        } else {
            if !k.option.is_base() {
                d.extend(code_of(&k.product, OptionNo::BASE));
            }
            if let Some(o) = m.definition.option(k.option) {
                for p in &o.requires {
                    d.extend(code_of(&p.product, p.option));
                }
            }
        }
        d
    };
    let mut order: Vec<MediaLoad> = Vec::new();
    let mut left = picked.clone();
    while !left.is_empty() {
        let placed: BTreeSet<LoadKey> = order.iter().map(|m| m.load.key.clone()).collect();
        let at = left
            .iter()
            .position(|m| deps(m).iter().all(|d| placed.contains(d)))
            .ok_or(PlanError::Cycle)?;
        order.push(left.remove(at));
    }

    let existing = store.loads();
    let agreements_book = store.agreements();
    let mut agreements = Vec::new();
    let mut steps = Vec::new();
    for m in order {
        let k = &m.load.key;
        let replaces = existing
            .iter()
            .find(|(_, l)| {
                l.key.product == k.product
                    && l.key.option == k.option
                    && l.key.load_type == k.load_type
                    && l.key.load_id == k.load_id
                    && (l.key.release == k.release || !m.definition.allow_multiple_releases)
            })
            .cloned();
        if !m.load.agreements.is_empty()
            && agreements_book.require(&k.product, k.release).is_err()
            && !agreements.contains(&(k.product.clone(), k.release))
        {
            agreements.push((k.product.clone(), k.release));
        }
        steps.push(InstallStep { media: m, replaces });
    }
    Ok(InstallPlan { steps, agreements })
}

/// What one installed load ended with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    /// The load.
    pub key: LoadKey,
    /// Objects restored.
    pub objects: Vec<ObjEntry>,
    /// Paths restored.
    pub paths: Vec<String>,
}

fn set_state(
    store: &mut dyn ProductStore,
    q: &QualName,
    l: &mut ProductLoad,
    s: LoadState,
) -> Result<(), PlanError> {
    l.state = s;
    store.update_load(q, l)?;
    Ok(())
}

/// Carry out a plan: definitions and loads into the store, each part
/// restored, exit programs called, states set as IBM's RSTLICPGM sets
/// them. `now` is seconds since the epoch.
pub fn execute_restore(
    plan: &InstallPlan,
    store: &mut dyn ProductStore,
    media: &dyn Media,
    restorer: &mut dyn Restorer,
    exits: &mut dyn ExitRunner,
    now: u64,
) -> Result<Vec<Installed>, PlanError> {
    let book = store.agreements();
    for (p, r) in &plan.agreements {
        book.require(p, *r)?;
    }
    let mut done = Vec::new();
    let mut synced: Vec<ProductDefinition> = Vec::new();
    for step in &plan.steps {
        let m = &step.media;
        let key = m.load.key.clone();
        if find_definition(&*store, &key.product, key.release).is_none() {
            store.create_definition(&m.definition_name, &m.definition)?;
        }
        let was_installed = step
            .replaces
            .as_ref()
            .is_some_and(|(_, l)| l.state.is_installed());
        let q = step
            .replaces
            .as_ref()
            .map_or_else(|| m.load_name.clone(), |(q, _)| q.clone());
        let mut l = m.load.clone();
        l.state = if was_installed {
            LoadState::RestoringInstalled
        } else {
            LoadState::RestoringPackaged
        };
        if step.replaces.is_some() {
            store.update_load(&q, &l)?;
        } else {
            store.create_load(&q, &l)?;
        }
        let failed = |pre: bool| match (pre, was_installed) {
            (true, true) => LoadState::PreExitFailedInstalled,
            (true, false) => LoadState::PreExitFailedPackaged,
            (false, true) => LoadState::RestoreFailedInstalled,
            (false, false) => LoadState::RestoreFailedPackaged,
        };
        for e in &m.load.pre_exits {
            if let Err(why) = exits.call(e, ExitOp::Restore, &key) {
                set_state(store, &q, &mut l, failed(true))?;
                return Err(PlanError::Exit(e.clone(), key, why));
            }
        }
        let mut objects = Vec::new();
        let mut paths = Vec::new();
        let mut parts: Vec<(Part, String, bool)> = l
            .libraries
            .iter()
            .map(|e| (Part::Library(e.dev.clone()), e.installed.clone(), true))
            .collect();
        parts.extend(l.directories.iter().map(|d| {
            (
                Part::Directory(d.primary.clone()),
                d.installed.clone(),
                false,
            )
        }));
        for (part, target, is_lib) in parts {
            let r = media.save_data(&key, &part).map_err(|e| e.0).and_then(|s| {
                if is_lib {
                    restorer
                        .restore(&s, &target)
                        .map(|o| objects.extend(o))
                        .map_err(|e| e.0)
                } else {
                    restorer
                        .restore_directory(&s, &target)
                        .map(|p| paths.extend(p))
                        .map_err(|e| e.0)
                }
            });
            if let Err(why) = r {
                set_state(store, &q, &mut l, failed(false))?;
                return Err(PlanError::Restore(key, why));
            }
        }
        if l.objects.is_empty() {
            l.objects.clone_from(&objects);
        }
        if l.paths.is_empty() {
            l.paths.clone_from(&paths);
        }
        set_state(store, &q, &mut l, LoadState::Loaded)?;
        for lib in l.libraries.clone() {
            if let Some(pe) = &lib.post_exit {
                let pgm = QualName::new(pe, &lib.installed);
                if let Err(why) = exits.call(&pgm, ExitOp::Restore, &key) {
                    set_state(store, &q, &mut l, LoadState::PostExitFailed)?;
                    return Err(PlanError::Exit(pgm, key, why));
                }
            }
        }
        l.installed_at = now;
        set_state(store, &q, &mut l, LoadState::Installed)?;
        if !synced
            .iter()
            .any(|d| d.product == m.definition.product && d.release == m.definition.release)
        {
            let mut repo = store.licenses();
            repo.sync_definition(&m.definition);
            store.save_licenses(&repo)?;
            synced.push(m.definition.clone());
        }
        done.push(Installed {
            key,
            objects,
            paths,
        });
    }
    Ok(done)
}
