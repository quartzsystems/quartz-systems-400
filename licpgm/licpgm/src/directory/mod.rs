// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The product directory: what is installed, in what state, as
//! DSPSFWRSC and GO LICPGM option 10 show it. The states live in the
//! `*PRDLOD` objects; this module computes the views.

pub mod check;

use release::Release;

use crate::id::{LoadId, LoadType, OptionNo, ProductId};
use crate::model::definition::ProductDefinition;
use crate::model::load::ProductLoad;
use crate::model::state::LoadState;
use crate::store::ProductStore;
use crate::system::SystemInfo;

/// GO LICPGM's installed status of one option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstalledStatus {
    /// Installed and right for this operating system release.
    Compatible,
    /// Installed; compatibility with the operating system unknown.
    Installed,
    /// Installed with errors.
    Error,
    /// At a release older than one compatible with the system.
    BackLevel,
    /// The option is older than its `*BASE` allows.
    BklvlOpt,
    /// `*BASE` is older than the option needs.
    BklvlBase,
    /// The primary language load is missing.
    NoPrimary,
    /// An object is damaged.
    Damaged,
}

impl InstalledStatus {
    /// IBM's name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            InstalledStatus::Compatible => "*COMPATIBLE",
            InstalledStatus::Installed => "*INSTALLED",
            InstalledStatus::Error => "*ERROR",
            InstalledStatus::BackLevel => "*BACKLEVEL",
            InstalledStatus::BklvlOpt => "*BKLVLOPT",
            InstalledStatus::BklvlBase => "*BKLVLBASE",
            InstalledStatus::NoPrimary => "*NOPRIMARY",
            InstalledStatus::Damaged => "*DAMAGED",
        }
    }
}

/// QSZRTVPR's "requirements met between base and option": `0` not
/// known (language loads), `1` met, `2` the option is too old for
/// `*BASE`, `3` `*BASE` is too old for the option.
#[must_use]
pub fn requirements_met(
    def: &ProductDefinition,
    option: OptionNo,
    option_rel: Release,
    base_rel: Release,
) -> char {
    if option.is_base() {
        return '1';
    }
    if let Some(min) = def.option(option).and_then(|o| o.min_base)
        && base_rel < min
    {
        return '3';
    }
    if !def.allow_mixed_releases && option_rel != base_rel {
        return if option_rel < base_rel { '2' } else { '3' };
    }
    '1'
}

fn installed_code<'a>(
    loads: &'a [ProductLoad],
    p: &ProductId,
    opt: OptionNo,
) -> Option<&'a ProductLoad> {
    loads.iter().find(|l| {
        &l.key.product == p
            && l.key.option == opt
            && l.key.load_type == LoadType::Code
            && l.state.is_installed()
    })
}

/// The installed status of an option at a release; `None` when its
/// code load is not installed.
#[must_use]
pub fn option_status(
    loads: &[ProductLoad],
    def: &ProductDefinition,
    option: OptionNo,
    sys: &SystemInfo,
) -> Option<InstalledStatus> {
    let code = loads.iter().find(|l| {
        l.key.product == def.product
            && l.key.release == def.release
            && l.key.option == option
            && l.key.load_type == LoadType::Code
            && l.state.is_installed()
    })?;
    let lngs: Vec<&ProductLoad> = loads
        .iter()
        .filter(|l| {
            l.key.product == def.product
                && l.key.release == def.release
                && l.key.option == option
                && l.key.load_type == LoadType::Lng
                && l.state.is_installed()
        })
        .collect();
    let all = std::iter::once(code).chain(lngs.iter().copied());
    if all.clone().any(|l| l.state == LoadState::InstalledDamaged) {
        return Some(InstalledStatus::Damaged);
    }
    if all.clone().any(|l| l.state != LoadState::Installed) {
        return Some(InstalledStatus::Error);
    }
    let offered = def.languages_of(option, &sys.languages);
    if !offered.is_empty() && !lngs.iter().any(|l| l.key.load_id == sys.primary_lng) {
        return Some(InstalledStatus::NoPrimary);
    }
    if !option.is_base() {
        let Some(base) = installed_code(loads, &def.product, OptionNo::BASE) else {
            return Some(InstalledStatus::Error);
        };
        match requirements_met(def, option, def.release, base.key.release) {
            '2' => return Some(InstalledStatus::BklvlOpt),
            '3' => return Some(InstalledStatus::BklvlBase),
            _ => {}
        }
    }
    Some(if !def.product.is_quartz() {
        InstalledStatus::Installed
    } else if def.release.is_compatible_store(sys.release) {
        InstalledStatus::Compatible
    } else if def.release < sys.release {
        InstalledStatus::BackLevel
    } else {
        InstalledStatus::Installed
    })
}

/// One DSPSFWRSC entry: an installed load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoftwareResource {
    /// Resource ID: the product.
    pub product: ProductId,
    /// The option.
    pub option: OptionNo,
    /// The feature: code load ID or language.
    pub feature: LoadId,
    /// `*CODE` or `*LNG`.
    pub load_type: LoadType,
    /// The release.
    pub release: Release,
    /// Our description.
    pub text: String,
}

/// One GO LICPGM option 10 entry: an installed option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledProgram {
    /// The product.
    pub product: ProductId,
    /// The option.
    pub option: OptionNo,
    /// The release.
    pub release: Release,
    /// Installed status.
    pub status: InstalledStatus,
    /// Our description.
    pub text: String,
}

fn option_text(defs: &[ProductDefinition], l: &ProductLoad) -> String {
    defs.iter()
        .find(|d| d.product == l.key.product && d.release == l.key.release)
        .and_then(|d| d.option(l.key.option))
        .map_or_else(|| l.text.clone(), |o| o.text.clone())
}

/// DSPSFWRSC: every installed load, by product, option, feature.
#[must_use]
pub fn software_resources(store: &dyn ProductStore) -> Vec<SoftwareResource> {
    let defs: Vec<ProductDefinition> = store.definitions().into_iter().map(|(_, d)| d).collect();
    let mut rows: Vec<SoftwareResource> = store
        .loads()
        .into_iter()
        .map(|(_, l)| l)
        .filter(|l| l.state.is_installed())
        .map(|l| SoftwareResource {
            text: option_text(&defs, &l),
            product: l.key.product.clone(),
            option: l.key.option,
            feature: l.key.load_id.clone(),
            load_type: l.key.load_type,
            release: l.key.release,
        })
        .collect();
    rows.sort_by(|a, b| {
        (&a.product, a.option, a.load_type, &a.feature).cmp(&(
            &b.product,
            b.option,
            b.load_type,
            &b.feature,
        ))
    });
    rows
}

/// GO LICPGM option 10: every installed option and its status.
#[must_use]
pub fn installed_programs(store: &dyn ProductStore, sys: &SystemInfo) -> Vec<InstalledProgram> {
    let loads: Vec<ProductLoad> = store.loads().into_iter().map(|(_, l)| l).collect();
    let mut rows = Vec::new();
    for (_, d) in store.definitions() {
        for o in &d.options {
            if let Some(status) = option_status(&loads, &d, o.option, sys) {
                rows.push(InstalledProgram {
                    product: d.product.clone(),
                    option: o.option,
                    release: d.release,
                    status,
                    text: o.text.clone(),
                });
            }
        }
    }
    rows.sort_by(|a, b| (&a.product, a.option, a.release).cmp(&(&b.product, b.option, b.release)));
    rows
}
