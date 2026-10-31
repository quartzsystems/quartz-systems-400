// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QS400/MEDIA.MANIFEST`: what marks a file system as QS/400
//! installation media, whatever carries it (an ISO 9660 CD or raw
//! disk, or a FAT32 copy of the tree), and what the install payload is.
//! Text, one record per line:
//!
//! ```text
//! QS400-MEDIA 1
//! release V1R0M0
//! volume QS400_V1R0M0
//! kind install
//! file QS400A.EFI 31457280 <sha256>
//! signature <key id hex> <ed25519 hex>
//! ```
//!
//! `kind` is `install` (distribution media) or `savsys` (a `SAVSYS`
//! copy of a system); absent means `install`. The signature, when
//! there, covers every line before it ([`Manifest::signed_text`]).
//!
//! Every path is a level 2 name, so it reads the same through Joliet,
//! Rock Ridge, the primary tree and FAT.

use std::fmt::Write as _;

use crate::error::{Error, Result};

/// The manifest's path on the media.
pub const PATH: &str = "QS400/MEDIA.MANIFEST";
/// The first line.
pub const MAGIC: &str = "QS400-MEDIA 1";
/// Slot A's UKI: the installer copies it onto the disk.
pub const SLOT_A: &str = "QS400A.EFI";
/// Slot B's UKI.
pub const SLOT_B: &str = "QS400B.EFI";
/// The media UKI where firmware looks for it.
pub const BOOT_UKI: &str = "EFI/BOOT/BOOTX64.EFI";
/// The response file of an automatic install (docs/INSTALL.md).
pub const RESPONSE: &str = "QS400/INSTALL.RSP";
/// Distribution media.
pub const KIND_INSTALL: &str = "install";
/// A `SAVSYS` copy of a system.
pub const KIND_SAVSYS: &str = "savsys";

/// One payload file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestFile {
    /// Path on the media.
    pub path: String,
    /// Bytes.
    pub size: u64,
    /// SHA-256, lower-case hex.
    pub sha256: String,
}

/// The parsed manifest.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Manifest {
    /// Release, `V1R0M0`.
    pub release: String,
    /// The ISO volume identifier.
    pub volume: String,
    /// `install` or `savsys`; empty reads as `install`.
    pub kind: String,
    /// The payload.
    pub files: Vec<ManifestFile>,
    /// Key ID and signature, hex, over [`Manifest::signed_text`].
    pub signature: Option<(String, String)>,
}

impl Manifest {
    /// The text form.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut s = self.signed_text();
        if let Some((key, sig)) = &self.signature {
            let _ = writeln!(s, "signature {key} {sig}");
        }
        s
    }

    /// The lines the signature covers: everything but the signature.
    #[must_use]
    pub fn signed_text(&self) -> String {
        let mut s = format!(
            "{MAGIC}\nrelease {}\nvolume {}\n",
            self.release, self.volume
        );
        if !self.kind.is_empty() {
            let _ = writeln!(s, "kind {}", self.kind);
        }
        for f in &self.files {
            let _ = writeln!(s, "file {} {} {}", f.path, f.size, f.sha256);
        }
        s
    }

    /// `install` or `savsys`.
    #[must_use]
    pub fn kind(&self) -> &str {
        if self.kind.is_empty() {
            KIND_INSTALL
        } else {
            &self.kind
        }
    }

    /// Parse the text form.
    #[allow(clippy::many_single_char_names)]
    pub fn parse(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(MAGIC) {
            return Err(Error::bad("not a QS/400 media manifest"));
        }
        let mut m = Self::default();
        for line in lines {
            let mut w = line.split_whitespace();
            match (w.next(), w.next(), w.next(), w.next()) {
                (Some("release"), Some(r), None, None) => m.release = r.to_string(),
                (Some("volume"), Some(v), None, None) => m.volume = v.to_string(),
                (Some("kind"), Some(k), None, None) => m.kind = k.to_string(),
                (Some("signature"), Some(k), Some(v), None) => {
                    m.signature = Some((k.to_string(), v.to_string()));
                }
                (Some("file"), Some(p), Some(n), Some(h)) => m.files.push(ManifestFile {
                    path: p.to_string(),
                    size: n.parse().map_err(|_| Error::bad("manifest file size"))?,
                    sha256: h.to_string(),
                }),
                (None, ..) => {}
                _ => return Err(Error::bad(format!("manifest line: {line}"))),
            }
        }
        Ok(m)
    }

    /// The entry for `path`.
    #[must_use]
    pub fn file(&self, path: &str) -> Option<&ManifestFile> {
        self.files
            .iter()
            .find(|f| f.path.eq_ignore_ascii_case(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let m = Manifest {
            release: "V1R0M0".into(),
            volume: "QS400_V1R0M0".into(),
            kind: String::new(),
            files: vec![ManifestFile {
                path: SLOT_A.into(),
                size: 12,
                sha256: "ab".into(),
            }],
            signature: None,
        };
        assert_eq!(Manifest::parse(&m.to_text()).unwrap(), m);
        assert_eq!(m.kind(), KIND_INSTALL);
        let s = Manifest {
            kind: KIND_SAVSYS.into(),
            signature: Some(("01".into(), "ff".into())),
            ..m.clone()
        };
        let back = Manifest::parse(&s.to_text()).unwrap();
        assert_eq!(back, s);
        assert!(!back.signed_text().contains("signature"));
        assert_eq!(m.file("qs400a.efi").unwrap().size, 12);
        assert!(Manifest::parse("nope").is_err());
        assert!(Manifest::parse("QS400-MEDIA 1\nfile x y z\n").is_err());
    }
}
