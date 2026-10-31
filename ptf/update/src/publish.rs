// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writing a service directory (`docs/UPDATE_PROTOCOL.md` §7): the PTF
//! streams, the signed group manifests and the signed catalog.

use std::fs;
use std::path::Path;

use ed25519_dalek::SigningKey;
use ptf::group::Group;
use ptf::header::PtfKind as HeaderKind;
use sha2::{Digest, Sha256};

use crate::catalog::{Catalog, CatalogGroup, CatalogPtf, FORMAT, GroupKind, Product, PtfKind};
use crate::Result;

/// A group to publish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupDef {
    /// Name.
    pub name: String,
    /// Kind.
    pub kind: GroupKind,
    /// Level.
    pub level: u32,
    /// Release.
    pub release: String,
    /// Text.
    pub text: String,
    /// Its PTFs.
    pub ptfs: Vec<String>,
}

/// What a publication holds.
pub struct Publication<'a> {
    /// PTF save streams.
    pub ptfs: &'a [Vec<u8>],
    /// PTFs that are high impact or pervasive.
    pub hiper: &'a [String],
    /// Groups.
    pub groups: &'a [GroupDef],
    /// The catalog's sequence.
    pub sequence: u64,
    /// Unix seconds.
    pub now: u64,
}

/// A product's text: the operating system's, a licensed program's, or
/// its ID.
fn product_text(id: &str) -> String {
    if id == release::PRODUCT_ID {
        return release::PRODUCT_TEXT.into();
    }
    release::PRODUCTS
        .iter()
        .find(|p| p.id == id)
        .map_or_else(|| id.to_string(), |p| p.text.to_string())
}

/// The catalog of a publication, without writing anything.
pub fn catalog(p: &Publication<'_>) -> Result<Catalog> {
    let mut cat = Catalog {
        format: FORMAT,
        sequence: p.sequence,
        generated: p.now,
        products: Vec::new(),
        ptfs: Vec::new(),
        groups: Vec::new(),
    };
    for bytes in p.ptfs {
        let (h, cover) = ptf::package::peek(bytes)?;
        let ids = |v: &[ptf::PtfRef]| v.iter().map(|r| r.ptf.clone()).collect::<Vec<_>>();
        cat.ptfs.push(CatalogPtf {
            id: h.ptf.clone(),
            product: h.product.clone(),
            option: h.option.clone(),
            release: h.release.clone(),
            kind: if h.lic {
                PtfKind::Lic
            } else if h.kind == HeaderKind::Delayed {
                PtfKind::Delayed
            } else {
                PtfKind::Immediate
            },
            hiper: p.hiper.contains(&h.ptf),
            prereqs: ids(&h.prereqs),
            coreqs: ids(&h.coreqs),
            supersedes: ids(&h.supersedes),
            size: bytes.len() as u64,
            sha256: hex::encode(Sha256::digest(bytes)),
            text: cover.lines().next().unwrap_or("").trim().to_string(),
        });
        if !cat
            .products
            .iter()
            .any(|x| x.id == h.product && x.option == h.option && x.release == h.release)
        {
            cat.products.push(Product {
                id: h.product.clone(),
                option: h.option.clone(),
                release: h.release.clone(),
                text: product_text(&h.product),
            });
        }
    }
    for g in p.groups {
        cat.groups.push(CatalogGroup {
            name: g.name.clone(),
            kind: g.kind,
            level: g.level,
            release: g.release.clone(),
            text: g.text.clone(),
            ptfs: g.ptfs.clone(),
            related: Vec::new(),
        });
    }
    Ok(cat)
}

/// A group's signed manifest, the form `ptf::group` reads.
#[must_use]
pub fn group_manifest(g: &CatalogGroup, key: &SigningKey) -> String {
    let m = Group {
        name: g.name.clone(),
        level: g.level,
        text: g.text.clone(),
        ptfs: g.ptfs.clone(),
        related: g.related.clone(),
    };
    ptf::group::sign_manifest(&ptf::group::manifest_text(&m), key)
}

/// Write `dir`: `ptf/*.savf`, `group/*.toml` and `catalog.toml`.
pub fn publish(dir: &Path, p: &Publication<'_>, key: &SigningKey) -> Result<Catalog> {
    let cat = catalog(p)?;
    fs::create_dir_all(dir.join("ptf"))?;
    fs::create_dir_all(dir.join("group"))?;
    for (bytes, entry) in p.ptfs.iter().zip(&cat.ptfs) {
        fs::write(dir.join("ptf").join(format!("{}.savf", entry.id)), bytes)?;
    }
    for g in &cat.groups {
        fs::write(
            dir.join("group").join(format!("{}.toml", g.name)),
            group_manifest(g, key),
        )?;
    }
    fs::write(dir.join("catalog.toml"), cat.sign(key))?;
    Ok(cat)
}
