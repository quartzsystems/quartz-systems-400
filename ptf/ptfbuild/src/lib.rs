// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! PTFs made at build time (`build ptf`, `docs/PTF.md` §8): the same
//! package `QSCRTPTF` makes on a system, from a manifest in the tree.
//! Replacement objects come from a store in memory holding what the
//! product ships (`osobjects`, as `build lp` makes it); a LIC PTF
//! carries the build's UKIs. Everything is signed with the key given.

#![deny(missing_docs)]

use ed25519_dalek::SigningKey;
use ptf::manifest::Manifest;
use ptf::package::{self, Build};
use thiserror::Error;

/// A PTF that could not be built.
#[derive(Debug, Error)]
pub enum BuildError {
    /// The manifest or the packaging refused it.
    #[error(transparent)]
    Ptf(#[from] ptf::Error),
    /// The store in memory could not be made.
    #[error(transparent)]
    System(#[from] lpbuild::BuildError),
    /// The product's objects could not be made.
    #[error(transparent)]
    Objects(#[from] osobjects::Error),
}

/// What a PTF is built from.
pub struct Input<'a> {
    /// The manifest (its `lic`, `lic_b` and `cover_file` already read
    /// by the caller into the fields below).
    pub manifest: &'a Manifest,
    /// The cover letter.
    pub cover: String,
    /// Slot A's UKI, for a LIC PTF.
    pub lic: Option<&'a [u8]>,
    /// Slot B's UKI.
    pub lic_b: Option<&'a [u8]>,
    /// The signing key.
    pub key: &'a SigningKey,
    /// Unix seconds, the PTF's creation time.
    pub now: u64,
}

/// A built PTF.
#[derive(Debug)]
pub struct Built {
    /// `QSnnnnn`.
    pub ptf: String,
    /// The save stream, the bytes `QGPL/QSnnnnn` holds.
    pub stream: Vec<u8>,
}

/// Build the PTF of `input` for this release.
///
/// # Errors
/// The manifest names an object the product does not ship, a field is
/// wrong, or the store refused something.
pub fn build(input: &Input<'_>) -> Result<Built, BuildError> {
    let m = input.manifest;
    if !m.lic.is_empty() && input.lic.is_none() {
        return Err(ptf::Error::refused(
            "CPF3598",
            "the manifest names a LIC image; none was given",
        )
        .into());
    }
    let header = m.header(&release::RELEASE.to_string(), input.now)?;
    let objects = m.objects()?;
    let (mut store, qsys) = lpbuild::empty(input.now)?;
    if !objects.is_empty() {
        let stamp = lpstore::stamp(input.now);
        let mut t = store.begin().map_err(ptf::Error::from)?;
        if release::product_id(&header.product) == release::PRODUCT_ID {
            osobjects::create(&mut t, qsys, &stamp)?;
        } else {
            osobjects::products::create(&mut t, qsys, &stamp, &header.product)?;
        }
        t.commit().map_err(ptf::Error::from)?;
    }
    let ptf = header.ptf.clone();
    let b = Build {
        header,
        cover: input.cover.clone(),
        objects,
        lic: input.lic,
        lic_b: input.lic_b,
        key: input.key,
        system: "QS400".into(),
    };
    let stream = package::build(&mut store, qsys, &b, input.now)?;
    Ok(Built { ptf, stream })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7; 32])
    }

    #[test]
    fn a_lic_ptf_opens_with_its_images() {
        let m = Manifest::parse(
            "ptf = \"QS00001\"\nproduct = \"QS00SS1\"\nkind = \"delayed\"\nlic = \"*BUILD\"\ncover = \"Fixes.\"\n",
        )
        .unwrap();
        let k = key();
        let b = build(&Input {
            manifest: &m,
            cover: m.cover.clone(),
            lic: Some(b"slot a image"),
            lic_b: Some(b"slot b image"),
            key: &k,
            now: 1_800_000_000,
        })
        .unwrap();
        assert_eq!(b.ptf, "QS00001");
        let p = package::open(&b.stream, &[k.verifying_key().to_bytes()]).unwrap();
        assert!(p.header.lic);
        assert_eq!(p.lic.as_deref(), Some(&b"slot a image"[..]));
        assert_eq!(p.lic_b.as_deref(), Some(&b"slot b image"[..]));
        assert_eq!(p.cover, "Fixes.");
        // Another key does not open it.
        assert!(package::open(&b.stream, &[[9; 32]]).is_err());
    }

    #[test]
    fn a_shipped_object_is_packaged() {
        let m = Manifest::parse(
            "ptf = \"QS00002\"\nproduct = \"5770SS1\"\ncover = \"Replaces the startup program.\"\n\n[[object]]\nlibrary = \"QSYS\"\nname = \"QSTRUP\"\ntype = \"*PGM\"\n",
        )
        .unwrap();
        let k = key();
        let b = build(&Input {
            manifest: &m,
            cover: m.cover.clone(),
            lic: None,
            lic_b: None,
            key: &k,
            now: 1_800_000_000,
        })
        .unwrap();
        let p = package::open(&b.stream, &[k.verifying_key().to_bytes()]).unwrap();
        assert_eq!(p.header.object_count, 1);
        assert!(!p.header.lic);
    }

    #[test]
    fn an_object_not_shipped_is_refused() {
        let m = Manifest::parse(
            "ptf = \"QS00003\"\nproduct = \"QS00SS1\"\ncover = \"x\"\n\n[[object]]\nlibrary = \"QSYS\"\nname = \"NOSUCH\"\ntype = \"*PGM\"\n",
        )
        .unwrap();
        let k = key();
        assert!(
            build(&Input {
                manifest: &m,
                cover: m.cover.clone(),
                lic: None,
                lic_b: None,
                key: &k,
                now: 1_800_000_000,
            })
            .is_err()
        );
    }

    #[test]
    fn a_lic_manifest_without_an_image_is_refused() {
        let m = Manifest::parse(
            "ptf = \"QS00004\"\nproduct = \"QS00SS1\"\nlic = \"*BUILD\"\ncover = \"x\"\n",
        )
        .unwrap();
        let k = key();
        assert!(
            build(&Input {
                manifest: &m,
                cover: m.cover.clone(),
                lic: None,
                lic_b: None,
                key: &k,
                now: 1_800_000_000,
            })
            .is_err()
        );
    }
}
