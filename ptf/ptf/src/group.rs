// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! PTF groups and cumulative packages (`docs/PTF.md` §4): a signed TOML
//! manifest and the status `WRKPTFGRP` shows.

use ed25519_dalek::SigningKey;
use serde::Deserialize;

use crate::db::PtfDb;
use crate::status::{Action, Status};
use crate::{Error, Result};

/// A group's manifest.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Group {
    /// `QS99nnn`, or `Cyyddd110` for a cumulative package.
    pub name: String,
    /// The level.
    pub level: u32,
    /// Text.
    pub text: String,
    /// Its PTFs, `QSnnnnn`.
    pub ptfs: Vec<String>,
    /// Related groups.
    #[serde(default)]
    pub related: Vec<String>,
}

/// What `WRKPTFGRP` shows for a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupStatus {
    /// Every PTF applied or superseded.
    Installed,
    /// Some PTF missing.
    NotInstalled,
    /// Some PTF waits for the next IPL.
    ApplyAtIpl,
    /// Some PTF is damaged.
    Error,
    /// The manifest does not verify.
    Unknown,
    /// Some PTF is on order.
    OnOrder,
}

impl GroupStatus {
    /// The text `WRKPTFGRP` shows.
    #[must_use]
    pub fn display(self) -> &'static str {
        match self {
            Self::Installed => "Installed",
            Self::NotInstalled => "Not installed",
            Self::ApplyAtIpl => "Apply at next IPL",
            Self::Error => "Error",
            Self::Unknown => "Unknown",
            Self::OnOrder => "On order",
        }
    }
}

/// A manifest and its signature, as shipped: the TOML text, a line
/// `# signature <hex>`, signing the text before it.
#[must_use]
pub fn sign_manifest(toml_text: &str, key: &SigningKey) -> String {
    use ed25519_dalek::Signer;
    let sig = key.sign(toml_text.as_bytes()).to_bytes();
    let hex = sig.iter().fold(String::new(), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    });
    format!("{toml_text}\n# signature {hex}\n")
}

/// Parse a signed manifest; `verified` is whether one of `trusted`
/// signed it.
pub fn parse(text: &str, trusted: &[[u8; 32]]) -> Result<(Group, bool)> {
    let (body, sig) = match text.rfind("\n# signature ") {
        Some(at) => (&text[..at], Some(text[at + 13..].trim())),
        None => (text, None),
    };
    let g: Group = toml::from_str(body).map_err(|e| Error::NotPtf(format!("group manifest: {e}")))?;
    let verified = sig
        .and_then(|h| {
            let bytes: Vec<u8> = (0..h.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(h.get(i..i + 2)?, 16).ok())
                .collect::<Option<_>>()?;
            <[u8; 64]>::try_from(bytes).ok()
        })
        .is_some_and(|s| {
            trusted.iter().any(|k| {
                use ed25519_dalek::Verifier;
                ed25519_dalek::VerifyingKey::from_bytes(k).is_ok_and(|vk| {
                    vk.verify(body.as_bytes(), &ed25519_dalek::Signature::from_bytes(&s))
                        .is_ok()
                })
            })
        });
    Ok((g, verified))
}

/// A group's status from the PTF database.
#[must_use]
pub fn status(g: &Group, verified: bool, db: &PtfDb) -> GroupStatus {
    if !verified {
        return GroupStatus::Unknown;
    }
    let recs: Vec<_> = g.ptfs.iter().map(|p| db.get(p)).collect();
    if recs.iter().flatten().any(|r| r.status == Status::Damaged) {
        return GroupStatus::Error;
    }
    if recs.iter().flatten().any(|r| r.action == Action::ApplyAtIpl) {
        return GroupStatus::ApplyAtIpl;
    }
    if recs
        .iter()
        .all(|r| r.is_some_and(|r| r.status.applied() || r.status == Status::Superseded))
    {
        return GroupStatus::Installed;
    }
    if recs.iter().flatten().any(|r| r.status == Status::OnOrder) {
        return GroupStatus::OnOrder;
    }
    GroupStatus::NotInstalled
}
