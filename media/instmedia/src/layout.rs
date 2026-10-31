// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The install media layout (docs/INSTALL.md, "Media layout"). Every
//! payload has a path on an optical volume (or a FAT copy of one) and a
//! file label on tape; the manifest lists the payload with sizes and
//! hashes, so the same reader serves distribution media and `SAVSYS`
//! output alike.

use optical::manifest::{self, Manifest};

/// Product loads on optical media live here, one `<ID>.SAVF` each.
pub const LP_DIR: &str = "QS400/LP";
/// `SAVSYS` data on an optical volume.
pub const SAVSYS_DIR: &str = "QSAVSYS";
/// The operating system's product.
pub const OS_PRODUCT: &str = "QS00SS1";

/// One payload file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    /// The slot A UKI.
    LicA,
    /// The slot B UKI.
    LicB,
    /// A product load: the SAVLICPGM stream of one product.
    Product(String),
    /// `SAVSECDTA`'s stream.
    SecData,
    /// `SAVCFG`'s stream.
    Config,
}

impl Payload {
    /// Its path on optical media.
    #[must_use]
    pub fn path(&self) -> String {
        match self {
            Self::LicA => manifest::SLOT_A.into(),
            Self::LicB => manifest::SLOT_B.into(),
            Self::Product(id) => format!("{LP_DIR}/{id}.SAVF"),
            Self::SecData => format!("{SAVSYS_DIR}/SECDTA"),
            Self::Config => format!("{SAVSYS_DIR}/CFG"),
        }
    }

    /// Its file label on tape (17 characters at most).
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::LicA => "QS400LICA".into(),
            Self::LicB => "QS400LICB".into(),
            Self::Product(id) => format!("QS400LP{id}"),
            Self::SecData => "QSAVSECDTA".into(),
            Self::Config => "QSAVCFG".into(),
        }
    }

    /// The payload a path names.
    #[must_use]
    pub fn from_path(path: &str) -> Option<Self> {
        let p = path.trim_start_matches('/');
        let up = p.to_ascii_uppercase();
        if up == manifest::SLOT_A {
            return Some(Self::LicA);
        }
        if up == manifest::SLOT_B {
            return Some(Self::LicB);
        }
        if let Some(rest) = up.strip_prefix(&format!("{LP_DIR}/")) {
            return rest
                .strip_suffix(".SAVF")
                .filter(|id| !id.is_empty() && !id.contains('/'))
                .map(|id| Self::Product(id.to_string()));
        }
        match up.as_str() {
            "QSAVSYS/SECDTA" => Some(Self::SecData),
            "QSAVSYS/CFG" => Some(Self::Config),
            _ => None,
        }
    }

    /// The payload a tape file label names.
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        match label.trim() {
            "QS400LICA" => Some(Self::LicA),
            "QS400LICB" => Some(Self::LicB),
            "QSAVSECDTA" => Some(Self::SecData),
            "QSAVCFG" => Some(Self::Config),
            l => l
                .strip_prefix("QS400LP")
                .filter(|id| !id.is_empty())
                .map(|id| Self::Product(id.to_string())),
        }
    }
}

/// The manifest's file label on tape: the first file of the volume.
pub const MANIFEST_LABEL: &str = "QS400MANIFEST";

/// What a medium holds, from its manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contents {
    /// The manifest.
    pub manifest: Manifest,
    /// The release the media installs.
    pub release: release::Release,
    /// `SAVSYS` output rather than distribution media.
    pub savsys: bool,
    /// The payloads listed, in manifest order.
    pub payloads: Vec<Payload>,
}

impl Contents {
    /// From a parsed manifest.
    pub fn of(manifest: Manifest) -> crate::Result<Self> {
        let release = manifest
            .release
            .parse()
            .map_err(|_| crate::Error::media(format!("manifest release {}", manifest.release)))?;
        let savsys = manifest.kind() == manifest::KIND_SAVSYS;
        let payloads = manifest
            .files
            .iter()
            .filter_map(|f| Payload::from_path(&f.path))
            .collect();
        Ok(Self {
            manifest,
            release,
            savsys,
            payloads,
        })
    }

    /// The products whose loads are on the media, the operating system
    /// first.
    #[must_use]
    pub fn products(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .payloads
            .iter()
            .filter_map(|p| match p {
                Payload::Product(id) => Some(id.clone()),
                _ => None,
            })
            .collect();
        v.sort_by_key(|id| (id != OS_PRODUCT, id.clone()));
        v
    }

    /// Whether the operating system's load is there.
    #[must_use]
    pub fn has_os(&self) -> bool {
        self.payloads.contains(&Payload::Product(OS_PRODUCT.into()))
    }

    /// One line for a display: kind, release, products.
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "{} {} ({})",
            if self.savsys {
                "SAVSYS media"
            } else {
                "Install media"
            },
            self.release,
            self.products().join(" ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_labels_round_trip() {
        for p in [
            Payload::LicA,
            Payload::LicB,
            Payload::Product("QS00SS1".into()),
            Payload::SecData,
            Payload::Config,
        ] {
            assert_eq!(Payload::from_path(&p.path()), Some(p.clone()));
            assert_eq!(Payload::from_label(&p.label()), Some(p.clone()));
            assert!(p.label().len() <= 17);
        }
        assert_eq!(
            Payload::from_path("/qs400/lp/qs00wds.savf"),
            Some(Payload::Product("QS00WDS".into()))
        );
        assert_eq!(Payload::from_path("EFI/BOOT/BOOTX64.EFI"), None);
    }
}
