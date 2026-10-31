// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `QSCRTPTF` manifest (`docs/PTF.md` §7): a TOML file naming the
//! header fields, the objects by library, name and type, the cover
//! letter and an optional LIC image. Paths in it are stream files the
//! integrator reads.

use serde::Deserialize;

use crate::header::{ExitProgram, ExitWhen, PtfHeader, PtfKind, PtfRef};
use crate::{Error, Result};

/// One object to package.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ObjectEntry {
    /// Library.
    pub library: String,
    /// Name.
    pub name: String,
    /// IBM type, `*PGM`.
    #[serde(rename = "type")]
    pub kind: String,
}

/// One exit program.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExitEntry {
    /// Library.
    pub library: String,
    /// Program.
    pub program: String,
    /// `before-apply`, `after-apply`, `before-remove`, `after-remove`,
    /// `migration`.
    pub when: String,
}

/// The manifest.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// `QSnnnnn`.
    pub ptf: String,
    /// Product ID.
    pub product: String,
    /// Option; `0000` when absent.
    #[serde(default = "base")]
    pub option: String,
    /// Release; the system's when absent.
    #[serde(default)]
    pub release: String,
    /// Oldest release it applies to; `release` when absent.
    #[serde(default)]
    pub target_release: String,
    /// Load ID; `5050` when absent.
    #[serde(default = "code_load")]
    pub load: String,
    /// `immediate` (the default) or `delayed`.
    #[serde(default)]
    pub kind: String,
    /// Prerequisite PTFs of the same product.
    #[serde(default)]
    pub prereqs: Vec<String>,
    /// Co-requisites.
    #[serde(default)]
    pub coreqs: Vec<String>,
    /// PTFs it supersedes.
    #[serde(default)]
    pub supersedes: Vec<String>,
    /// The cover letter's text.
    #[serde(default)]
    pub cover: String,
    /// A stream file holding the cover letter instead.
    #[serde(default)]
    pub cover_file: String,
    /// A stream file holding the new UKI (slot A's build).
    #[serde(default)]
    pub lic: String,
    /// Slot B's build of it.
    #[serde(default)]
    pub lic_b: String,
    /// The store format version a migration program brings, 0 for none.
    #[serde(default)]
    pub store_format: u32,
    /// The objects.
    #[serde(default, rename = "object")]
    pub objects: Vec<ObjectEntry>,
    /// Exit and data migration programs.
    #[serde(default, rename = "exit")]
    pub exits: Vec<ExitEntry>,
}

fn base() -> String {
    "0000".into()
}

fn code_load() -> String {
    "5050".into()
}

fn bad(what: impl Into<String>) -> Error {
    Error::refused("CPF3598", what)
}

impl Manifest {
    /// Parse the manifest's text.
    pub fn parse(text: &str) -> Result<Self> {
        let m: Self = toml::from_str(text).map_err(|e| bad(format!("manifest: {e}")))?;
        if m.ptf.parse::<release::Ptf>().is_err() {
            return Err(bad(format!("{} is not a PTF identifier", m.ptf)));
        }
        if m.objects.is_empty() && m.lic.is_empty() && m.cover.is_empty() && m.cover_file.is_empty() {
            return Err(bad("the manifest names no objects, image or cover letter"));
        }
        Ok(m)
    }

    /// The objects as library, name and type code.
    pub fn objects(&self) -> Result<Vec<(String, String, u16)>> {
        self.objects
            .iter()
            .map(|o| {
                let t = store::obj::types::type_by_name(&o.kind.to_ascii_uppercase())
                    .ok_or_else(|| bad(format!("object type {}", o.kind)))?;
                Ok((o.library.to_ascii_uppercase(), o.name.to_ascii_uppercase(), t.code))
            })
            .collect()
    }

    /// The header, for `release` when the manifest names none, created
    /// at `now`.
    pub fn header(&self, release: &str, now: u64) -> Result<PtfHeader> {
        let r = |p: &String| PtfRef {
            ptf: p.to_ascii_uppercase(),
            product: self.product.clone(),
        };
        let rel = if self.release.is_empty() { release } else { &self.release };
        let kind = match self.kind.as_str() {
            "" | "immediate" => PtfKind::Immediate,
            "delayed" => PtfKind::Delayed,
            k => return Err(bad(format!("kind {k}"))),
        };
        let exits = self
            .exits
            .iter()
            .map(|e| {
                let when = match e.when.as_str() {
                    "before-apply" => ExitWhen::BeforeApply,
                    "after-apply" => ExitWhen::AfterApply,
                    "before-remove" => ExitWhen::BeforeRemove,
                    "after-remove" => ExitWhen::AfterRemove,
                    "migration" => ExitWhen::Migration,
                    w => return Err(bad(format!("exit when {w}"))),
                };
                Ok(ExitProgram {
                    library: e.library.to_ascii_uppercase(),
                    program: e.program.to_ascii_uppercase(),
                    when,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(PtfHeader {
            ptf: self.ptf.to_ascii_uppercase(),
            product: self.product.to_ascii_uppercase(),
            option: self.option.clone(),
            release: rel.into(),
            load: self.load.clone(),
            target_release: if self.target_release.is_empty() {
                rel.into()
            } else {
                self.target_release.clone()
            },
            kind,
            lic: !self.lic.is_empty(),
            migration: exits.iter().any(|e| e.when == ExitWhen::Migration),
            cover_only: self.objects.is_empty() && self.lic.is_empty(),
            created: now,
            prereqs: self.prereqs.iter().map(r).collect(),
            coreqs: self.coreqs.iter().map(r).collect(),
            supersedes: self.supersedes.iter().map(r).collect(),
            exits,
            object_count: 0,
            store_format: self.store_format,
        })
    }
}

/// A signing key file: the 32-byte ed25519 seed as 64 hex digits (the
/// build's key file form), or the 32 raw bytes.
pub fn signing_key(bytes: &[u8]) -> Result<ed25519_dalek::SigningKey> {
    let text = String::from_utf8_lossy(bytes);
    let seed: Option<[u8; 32]> = if bytes.len() == 32 {
        bytes.try_into().ok()
    } else {
        let t = text.trim();
        (t.len() == 64)
            .then(|| {
                (0..32)
                    .map(|i| u8::from_str_radix(&t[i * 2..i * 2 + 2], 16).ok())
                    .collect::<Option<Vec<u8>>>()
            })
            .flatten()
            .and_then(|v| v.try_into().ok())
    };
    seed.map(|s| ed25519_dalek::SigningKey::from_bytes(&s))
        .ok_or_else(|| bad("the key file is not an ed25519 seed (64 hex digits)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = r#"
ptf = "QS00042"
product = "5770SS1"
kind = "delayed"
prereqs = ["QS00041"]
cover = "Corrects the level."

[[object]]
library = "QSVCLIB"
name = "level"
type = "*DTAARA"

[[exit]]
library = "QSYS"
program = "QSMIG1"
when = "migration"
"#;

    #[test]
    fn a_manifest_becomes_a_header() {
        let m = Manifest::parse(TEXT).unwrap();
        let h = m.header("V1R0M0", 9).unwrap();
        assert_eq!(h.kind, PtfKind::Delayed);
        assert_eq!(h.release, "V1R0M0");
        assert_eq!(h.target_release, "V1R0M0");
        assert_eq!(h.prereqs[0].ptf, "QS00041");
        assert!(h.migration);
        let o = m.objects().unwrap();
        assert_eq!(o[0].1, "LEVEL");
        assert!(Manifest::parse("ptf = \"X\"\nproduct = \"P\"\n").is_err());
        assert!(Manifest::parse(&TEXT.replace("kind", "flavour")).is_err());
    }

    #[test]
    fn key_files() {
        let hex = "07".repeat(32);
        let k = signing_key(hex.as_bytes()).unwrap();
        assert_eq!(k.to_bytes(), [7; 32]);
        assert_eq!(signing_key(&[7; 32]).unwrap().to_bytes(), [7; 32]);
        assert!(signing_key(b"nope").is_err());
    }
}
