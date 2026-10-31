// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The signed catalog (`docs/UPDATE_PROTOCOL.md` §3): products, PTFs and
//! groups. The publisher writes the canonical text; a client verifies
//! the bytes it received and parses only those.

use std::fmt::Write as _;

use ed25519_dalek::SigningKey;
use ptf::group::quote;
use serde::Deserialize;

use crate::{Error, Result};

/// The catalog format this code reads and writes.
pub const FORMAT: u32 = 1;

/// A product the service has fixes for.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Product {
    /// Product ID.
    pub id: String,
    /// Option, `0000` for `*BASE`.
    pub option: String,
    /// `VxRyMz`.
    pub release: String,
    /// Text.
    #[serde(default)]
    pub text: String,
}

/// How a PTF is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PtfKind {
    /// While the system runs.
    Immediate,
    /// At the next IPL.
    Delayed,
    /// A Licensed Internal Code image, through the other slot.
    Lic,
}

impl PtfKind {
    /// The catalog's word.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::Delayed => "delayed",
            Self::Lic => "lic",
        }
    }
}

/// One PTF.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CatalogPtf {
    /// `QSnnnnn`.
    pub id: String,
    /// Product ID.
    pub product: String,
    /// Option.
    pub option: String,
    /// Release.
    pub release: String,
    /// Kind.
    pub kind: PtfKind,
    /// High impact or pervasive.
    #[serde(default)]
    pub hiper: bool,
    /// Prerequisites.
    #[serde(default)]
    pub prereqs: Vec<String>,
    /// Co-requisites.
    #[serde(default)]
    pub coreqs: Vec<String>,
    /// PTFs it supersedes.
    #[serde(default)]
    pub supersedes: Vec<String>,
    /// Bytes of the save stream.
    pub size: u64,
    /// SHA-256 of the save stream, hex.
    pub sha256: String,
    /// Text.
    #[serde(default)]
    pub text: String,
}

/// What a group is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GroupKind {
    /// A PTF group, `QS99nnn`.
    Group,
    /// A cumulative package, `Cyyddd110`.
    Cumulative,
    /// The HIPER group of a release.
    Hiper,
}

impl GroupKind {
    /// The catalog's word.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Group => "group",
            Self::Cumulative => "cumulative",
            Self::Hiper => "hiper",
        }
    }
}

/// One group at its current level.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CatalogGroup {
    /// Name.
    pub name: String,
    /// Kind.
    pub kind: GroupKind,
    /// Level.
    pub level: u32,
    /// Release.
    pub release: String,
    /// Text.
    #[serde(default)]
    pub text: String,
    /// Its PTFs.
    #[serde(default)]
    pub ptfs: Vec<String>,
    /// Related groups.
    #[serde(default)]
    pub related: Vec<String>,
}

/// The catalog.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Catalog {
    /// [`FORMAT`].
    pub format: u32,
    /// Grows with each catalog.
    pub sequence: u64,
    /// Unix seconds.
    pub generated: u64,
    /// Products.
    #[serde(default, rename = "product")]
    pub products: Vec<Product>,
    /// PTFs.
    #[serde(default, rename = "ptf")]
    pub ptfs: Vec<CatalogPtf>,
    /// Groups.
    #[serde(default, rename = "group")]
    pub groups: Vec<CatalogGroup>,
}

fn list(v: &[String]) -> String {
    v.iter().map(|s| quote(s)).collect::<Vec<_>>().join(", ")
}

impl Catalog {
    /// The canonical text: keys in the documented order, entries sorted.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "format = {}", self.format);
        let _ = writeln!(out, "sequence = {}", self.sequence);
        let _ = writeln!(out, "generated = {}", self.generated);
        let mut products = self.products.clone();
        products.sort_by(|a, b| (&a.id, &a.option, &a.release).cmp(&(&b.id, &b.option, &b.release)));
        for p in &products {
            let _ = write!(
                out,
                "\n[[product]]\nid = {}\noption = {}\nrelease = {}\ntext = {}\n",
                quote(&p.id),
                quote(&p.option),
                quote(&p.release),
                quote(&p.text)
            );
        }
        let mut ptfs = self.ptfs.clone();
        ptfs.sort_by(|a, b| a.id.cmp(&b.id));
        for p in &ptfs {
            let _ = write!(
                out,
                "\n[[ptf]]\nid = {}\nproduct = {}\noption = {}\nrelease = {}\nkind = {}\nhiper = {}\n\
                 prereqs = [{}]\ncoreqs = [{}]\nsupersedes = [{}]\nsize = {}\nsha256 = {}\ntext = {}\n",
                quote(&p.id),
                quote(&p.product),
                quote(&p.option),
                quote(&p.release),
                quote(p.kind.word()),
                p.hiper,
                list(&p.prereqs),
                list(&p.coreqs),
                list(&p.supersedes),
                p.size,
                quote(&p.sha256),
                quote(&p.text)
            );
        }
        let mut groups = self.groups.clone();
        groups.sort_by(|a, b| a.name.cmp(&b.name));
        for g in &groups {
            let _ = write!(
                out,
                "\n[[group]]\nname = {}\nkind = {}\nlevel = {}\nrelease = {}\ntext = {}\nptfs = [{}]\nrelated = [{}]\n",
                quote(&g.name),
                quote(g.kind.word()),
                g.level,
                quote(&g.release),
                quote(&g.text),
                list(&g.ptfs),
                list(&g.related)
            );
        }
        out
    }

    /// The canonical text, signed.
    #[must_use]
    pub fn sign(&self, key: &SigningKey) -> String {
        ptf::group::sign_manifest(&self.encode(), key)
    }

    /// A signed catalog, refused unless one of `trusted` signed it.
    pub fn parse(text: &str, trusted: &[[u8; 32]]) -> Result<Self> {
        let (body, ok) = ptf::group::verify_signed(text, trusted);
        if !ok {
            return Err(Error::Verify("the catalog is not signed by a trusted signer".into()));
        }
        Self::parse_body(body)
    }

    /// The catalog's body, unverified: for the server, which reads its
    /// own directory.
    pub fn parse_unverified(text: &str) -> Result<Self> {
        Self::parse_body(ptf::group::verify_signed(text, &[]).0)
    }

    fn parse_body(body: &str) -> Result<Self> {
        let c: Self = toml::from_str(body).map_err(|e| Error::Protocol(format!("catalog: {e}")))?;
        if c.format != FORMAT {
            return Err(Error::Protocol(format!("catalog format {}", c.format)));
        }
        Ok(c)
    }

    /// A PTF by ID.
    #[must_use]
    pub fn ptf(&self, id: &str) -> Option<&CatalogPtf> {
        self.ptfs.iter().find(|p| p.id == id)
    }

    /// A group by name.
    #[must_use]
    pub fn group(&self, name: &str) -> Option<&CatalogGroup> {
        self.groups.iter().find(|g| g.name == name)
    }

    /// The newest cumulative package of `release`.
    #[must_use]
    pub fn cumulative(&self, release: &str) -> Option<&CatalogGroup> {
        self.groups
            .iter()
            .filter(|g| g.kind == GroupKind::Cumulative && g.release == release)
            .max_by_key(|g| {
                g.name
                    .parse::<release::Cumulative>()
                    .map_or((0, 0), |c| (c.year, c.day))
            })
    }

    /// The HIPER group of `release`.
    #[must_use]
    pub fn hiper(&self, release: &str) -> Option<&CatalogGroup> {
        self.groups
            .iter()
            .find(|g| g.kind == GroupKind::Hiper && g.release == release)
    }
}
