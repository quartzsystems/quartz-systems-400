// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSZPKGPO (Package Product Option) and PKGPRDOPT: every object of
//! the loads must be there and undamaged; their loads become `30`.

use release::Release;

use crate::api::{ApiError, blank_at, char_at};
use crate::id::{self, LoadId, LoadType, OptionNo, ProductId};
use crate::model::QualName;
use crate::model::load::{ObjEntry, ProductLoad};
use crate::model::state::LoadState;
use crate::store::{ObjStatus, ObjectCheck, ProductStore};

/// Which loads of the option to package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadSel {
    /// One load ID (feature or language).
    One(LoadId),
    /// `*ALL`.
    All,
}

/// Package loads; returns those packaged. A load with no object list
/// takes every object of its development libraries.
pub fn package(
    store: &mut dyn ProductStore,
    objs: &dyn ObjectCheck,
    product: &ProductId,
    release: Option<Release>,
    option: OptionNo,
    sel: &LoadSel,
    repackage: bool,
) -> Result<Vec<QualName>, ApiError> {
    let loads = store.loads();
    let rel = match release {
        Some(r) => r,
        None => {
            let mut rels: Vec<Release> = loads
                .iter()
                .filter(|(_, l)| &l.key.product == product)
                .map(|(_, l)| l.key.release)
                .collect();
            rels.sort();
            rels.dedup();
            match rels.as_slice() {
                [r] => *r,
                _ => return Err(ApiError::new("CPF0C1C").text(product.as_str(), 7)),
            }
        }
    };
    let targets: Vec<(QualName, ProductLoad)> = loads
        .into_iter()
        .filter(|(_, l)| {
            &l.key.product == product
                && l.key.release == rel
                && l.key.option == option
                && match sel {
                    LoadSel::All => true,
                    LoadSel::One(id) => &l.key.load_id == id,
                }
        })
        .collect();
    if targets.is_empty() {
        return Err(ApiError::new("CPF0C1F")
            .text(product.as_str(), 7)
            .text(&id::release_text(rel), 6)
            .text(&option.api(), 4));
    }
    let mut done = Vec::new();
    for (q, mut l) in targets {
        if l.state.is_installed() || (l.state >= LoadState::Packaged && !repackage) {
            continue;
        }
        if l.objects.is_empty() && l.key.load_type == LoadType::Code {
            for lib in &l.libraries {
                l.objects
                    .extend(objs.objects_in(&lib.dev).into_iter().map(|o| ObjEntry {
                        lib: lib.primary.clone(),
                        ..o
                    }));
            }
        }
        for o in &l.objects {
            let dev = l
                .libraries
                .iter()
                .find(|e| e.primary == o.lib)
                .map_or(o.lib.as_str(), |e| e.dev.as_str());
            let at = ObjEntry::new(dev, &o.name, &o.typ);
            if o.name == "*ALL" || o.typ == "*ALL" {
                continue;
            }
            match objs.object(&at) {
                ObjStatus::Present(_) => {}
                ObjStatus::Missing => {
                    return Err(ApiError::new("CPF9801").text(&o.name, 10).text(dev, 10));
                }
                ObjStatus::Damaged => {
                    return Err(ApiError::new("CPF0C19").text(&o.name, 10).text(dev, 10));
                }
            }
        }
        l.state = LoadState::Packaged;
        store
            .update_load(&q, &l)
            .map_err(|e| crate::api::crtpd::store_error(&e))?;
        done.push(q);
    }
    Ok(done)
}

/// QSZPKGPO over its parameter bytes: product option information
/// (35), repackage (4), allow object change (5).
pub fn qszpkgpo(
    store: &mut dyn ProductStore,
    objs: &dyn ObjectCheck,
    info: &[u8],
    repackage: &[u8],
    allow_change: &[u8],
) -> Result<Vec<QualName>, ApiError> {
    if info.len() < 35 {
        return Err(ApiError::new("CPF24B4"));
    }
    let os = char_at(info, 0, 4);
    let option: OptionNo = os
        .parse()
        .map_err(|_| ApiError::new("CPF0C8A").text(&os, 4))?;
    let pid = char_at(info, 4, 7);
    let product: ProductId = pid
        .parse()
        .map_err(|_| ApiError::new("CPF0CB2").text(&pid, 7))?;
    let rs = char_at(info, 11, 6);
    let release = if rs == "*ONLY" {
        None
    } else {
        Some(id::parse_release(&rs).map_err(|_| ApiError::new("CPF358A").text(&rs, 6))?)
    };
    let ls = char_at(info, 17, 8);
    let sel = match ls.as_str() {
        "*ALL" => LoadSel::All,
        "*CODEDFT" => LoadSel::One(LoadId::feature(LoadId::CODE_DEFAULT).expect("5001")),
        s => LoadSel::One(
            LoadId::feature(s)
                .or_else(|_| LoadId::nlv(s))
                .map_err(|_| ApiError::new("CPF0C84").text(s, 8))?,
        ),
    };
    if !blank_at(info, 25, 10) {
        return Err(ApiError::new("CPF0CB3"));
    }
    let repack = match char_at(repackage, 0, 4).as_str() {
        "*YES" => true,
        "*NO" => false,
        _ => return Err(ApiError::new("CPF24B4")),
    };
    // *SAME and *NO only differ in the objects' descriptions, which the
    // integrator keeps.
    if !matches!(char_at(allow_change, 0, 5).as_str(), "*SAME" | "*NO") {
        return Err(ApiError::new("CPF24B4"));
    }
    package(store, objs, &product, release, option, &sel, repack)
}
