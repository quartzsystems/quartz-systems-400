// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The programs of distribution media (docs/INSTALL.md, "The media
//! UKI"). The media UKI's initramfs holds only a loader, so the boot
//! image stays small whatever the system grows to; `slic` and `qsjob`
//! are files of the media, and the loader runs them once they check out:
//! the manifest signed by a key the UKI trusts, of the UKI's release, and
//! each file's size and SHA-256 as the manifest gives them.

use optical::manifest::Manifest;
use sha2::{Digest, Sha256};

use crate::{Error, Result, sign};

/// `slic` on the media: the loader makes it `/init`.
pub const SLIC: &str = "QS400/LIC/SLIC";
/// The job program on the media: `/qsjob`.
pub const QSJOB: &str = "QS400/LIC/QSJOB";

/// Check `manifest` itself: signed by one of `keys` (the trusted
/// signers' public keys, 32 bytes each) and of `release`.
pub fn check_manifest(manifest: &Manifest, keys: &[u8], release: &str) -> Result<()> {
    let mut signed = Err(Error::Signature("no trusted key".into()));
    for k in keys.as_chunks::<32>().0 {
        signed = sign::verify(manifest, k);
        if signed.is_ok() {
            break;
        }
    }
    signed?;
    if manifest.release != release {
        return Err(Error::media(format!(
            "the media is {}, its boot program {release}",
            manifest.release
        )));
    }
    Ok(())
}

/// Check `data`, read from `path`, against a manifest that passed
/// [`check_manifest`].
pub fn check_file(manifest: &Manifest, path: &str, data: &[u8]) -> Result<()> {
    let f = manifest
        .file(path)
        .ok_or_else(|| Error::media(format!("{path} is not in the media manifest")))?;
    if f.size != data.len() as u64 {
        return Err(Error::media(format!(
            "{path} is {} bytes, the manifest says {}",
            data.len(),
            f.size
        )));
    }
    if f.sha256 != hex::encode(Sha256::digest(data)) {
        return Err(Error::media(format!(
            "{path} does not match the manifest's SHA-256"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use optical::manifest::ManifestFile;

    fn manifest(data: &[u8], key: &SigningKey) -> Manifest {
        let mut m = Manifest {
            release: "V1R2M0".into(),
            volume: "QS400_V1R2M0".into(),
            files: vec![ManifestFile {
                path: QSJOB.into(),
                size: data.len() as u64,
                sha256: hex::encode(Sha256::digest(data)),
            }],
            ..Manifest::default()
        };
        sign::sign(&mut m, key);
        m
    }

    #[test]
    fn only_a_trusted_manifest_of_this_release_passes() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let other = SigningKey::from_bytes(&[8; 32]);
        let m = manifest(b"program", &key);
        // The second trusted key signed it.
        let mut keys = other.verifying_key().to_bytes().to_vec();
        keys.extend_from_slice(&key.verifying_key().to_bytes());
        check_manifest(&m, &keys, "V1R2M0").unwrap();
        assert!(check_manifest(&m, &keys, "V1R3M0").is_err());
        assert!(check_manifest(&m, &keys[..32], "V1R2M0").is_err());
        assert!(check_manifest(&m, &[], "V1R2M0").is_err());
        let mut unsigned = m.clone();
        unsigned.signature = None;
        assert!(check_manifest(&unsigned, &keys, "V1R2M0").is_err());
        let mut changed = m.clone();
        changed.files[0].size += 1;
        assert!(check_manifest(&changed, &keys, "V1R2M0").is_err());
    }

    #[test]
    fn only_the_listed_bytes_pass() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let m = manifest(b"program", &key);
        check_file(&m, QSJOB, b"program").unwrap();
        assert!(check_file(&m, QSJOB, b"progrbm").is_err());
        assert!(check_file(&m, QSJOB, b"program2").is_err());
        assert!(check_file(&m, SLIC, b"program").is_err());
    }
}
