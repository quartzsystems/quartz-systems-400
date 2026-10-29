// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped root CA set: Mozilla's bundle as curl publishes it,
//! embedded from `roots/cacert.pem` (see `roots/README.md` for origin,
//! date, license and the update path).

use std::sync::OnceLock;

use crate::error::Result;
use crate::store::{CertStore, EntryKind};
use crate::x509::CertInfo;

/// The bundle text.
pub const BUNDLE_PEM: &str = include_str!("../roots/cacert.pem");

/// One shipped root.
#[derive(Clone, Debug)]
pub struct Root {
    /// The label the store gives it: the bundle's title line, which is
    /// the certificate's common name or organization.
    pub label: String,
    /// DER.
    pub der: Vec<u8>,
    /// Facts.
    pub info: CertInfo,
}

fn parse() -> Vec<Root> {
    // Each certificate is preceded by its title and a line of `=`.
    let mut out = Vec::new();
    let mut title = String::new();
    let mut body: Option<String> = None;
    for line in BUNDLE_PEM.lines() {
        if let Some(b) = body.as_mut() {
            if line.starts_with("-----END CERTIFICATE-----") {
                if let Some(der) = crate::pem::base64_decode(b)
                    && let Ok(info) = CertInfo::from_der(&der)
                {
                    let label = if title.is_empty() {
                        info.common_name
                            .clone()
                            .unwrap_or_else(|| info.subject.clone())
                    } else {
                        title.clone()
                    };
                    out.push(Root { label, der, info });
                }
                body = None;
            } else {
                b.push_str(line.trim());
            }
        } else if line.starts_with("-----BEGIN CERTIFICATE-----") {
            body = Some(String::new());
        } else if line.starts_with('=') {
            // The line before this one was the title.
        } else if !line.starts_with('#') && !line.trim().is_empty() {
            title = line.trim().to_string();
        }
    }
    out
}

/// The shipped roots, parsed once.
pub fn roots() -> &'static [Root] {
    static ROOTS: OnceLock<Vec<Root>> = OnceLock::new();
    ROOTS.get_or_init(parse)
}

/// The bundle's "Certificate data from Mozilla as of" line, as text.
pub fn bundle_date() -> &'static str {
    BUNDLE_PEM
        .lines()
        .find_map(|l| l.strip_prefix("## Certificate data from Mozilla as of: "))
        .unwrap_or("unknown")
        .trim()
}

/// Add every shipped root to `store` as a trusted CA, skipping any
/// certificate already present (by fingerprint) and making labels
/// unique. Returns how many were added.
pub fn seed(store: &mut CertStore, now: u64) -> Result<usize> {
    let mut added = 0;
    for r in roots() {
        if store.find_by_sha256(&r.info.sha256)?.is_some() {
            continue;
        }
        let mut label = r.label.clone();
        let mut n = 1;
        while store.get(&label).is_ok() {
            n += 1;
            label = format!("{} ({n})", r.label);
        }
        store.add_ca(&label, r.der.clone(), true, now)?;
        added += 1;
    }
    Ok(added)
}

/// Whether `store` holds this root (by fingerprint) as a CA entry.
pub fn holds(store: &CertStore, root: &Root) -> Result<bool> {
    Ok(store
        .find_by_sha256(&root.info.sha256)?
        .is_some_and(|e| e.kind == EntryKind::Ca))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundle_parses_to_ca_roots() {
        let r = roots();
        assert!(r.len() > 100, "{} roots", r.len());
        assert!(r.iter().all(|x| x.info.is_ca && x.info.self_signed));
        assert!(r.iter().any(|x| x.label.contains("ISRG Root X1")));
        assert!(bundle_date().contains("2026"));
        let mut labels: Vec<&str> = r.iter().map(|x| x.label.as_str()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), r.len(), "titles are unique");
    }
}
