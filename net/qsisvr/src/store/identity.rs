// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSIIDENT`: the system's QSI identity (its Ed25519 key and
//! certificate), sealed as the certificate stores' stashes are, under
//! the key the LIC partition's record holds (`docs/CERTS.md` "The stash
//! and the LIC key slot"). A copied disk without its LIC partition does
//! not open it.
//!
//! ```text
//! 0   magic "QS400QSI"   8
//! 8   version u16        2   = 1
//! 10  generation u32     4   the key slot's generation
//! 14  sealed                 nonce || ciphertext || tag, AAD = [0, 14)
//! ```
//!
//! The sealed text is the certificate and the PKCS #8 key, both DER, in
//! the `ipc` wire form.

use qsitrust::Identity;

use super::{StoreError, body};

/// The file's name under [`super::DIR`].
pub const NAME: &str = "QSIIDENT";
/// Its magic.
pub const MAGIC: &[u8; 8] = b"QS400QSI";
/// Its format version.
pub const FORMAT_VERSION: u16 = 1;

/// The file's bytes: the header, then what `seal` makes of the identity
/// with the header as its additional data.
///
/// # Errors
/// `seal` failed.
pub fn encode(
    id: &Identity,
    generation: u32,
    seal: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<Vec<u8>, StoreError> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_be_bytes());
    out.extend_from_slice(&generation.to_be_bytes());
    let plain = ipc::encode(&(id.cert.clone(), id.key.clone()));
    let sealed = seal(&out, &plain).ok_or(StoreError::Sealed)?;
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// The key slot generation a file was sealed under.
///
/// # Errors
/// Not the file, or another version.
pub fn generation(bytes: &[u8]) -> Result<u32, StoreError> {
    let b = body(bytes, *MAGIC, FORMAT_VERSION)?;
    let g = b.get(..4).ok_or(StoreError::Invalid("no generation"))?;
    Ok(u32::from_be_bytes([g[0], g[1], g[2], g[3]]))
}

/// The identity a file holds, opened by `unseal`.
///
/// # Errors
/// Not the file, another version, a sealed part that does not open, or
/// an identity that is not one.
pub fn decode(
    bytes: &[u8],
    unseal: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<Identity, StoreError> {
    generation(bytes)?;
    let (aad, sealed) = bytes.split_at(14);
    let plain = unseal(aad, sealed).ok_or(StoreError::Sealed)?;
    let (cert, key): (Vec<u8>, Vec<u8>) =
        ipc::decode(&plain).map_err(|e| StoreError::Body(e.to_string()))?;
    Identity::new(cert, key).map_err(|_| StoreError::Invalid("not an Ed25519 identity"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in seal: the additional data's length, then the text
    /// reversed.
    fn seal(aad: &[u8], plain: &[u8]) -> Option<Vec<u8>> {
        let mut v = vec![u8::try_from(aad.len()).ok()?];
        v.extend(plain.iter().rev());
        Some(v)
    }

    fn unseal(aad: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
        (sealed.first() == Some(&u8::try_from(aad.len()).ok()?))
            .then(|| sealed[1..].iter().rev().copied().collect())
    }

    #[test]
    fn an_identity_round_trips_sealed() {
        let id = Identity::from_seed(&[3; 32], "QS400", "S1", 0, [1; 16]);
        let bytes = encode(&id, 4, seal).unwrap();
        assert_eq!(generation(&bytes).unwrap(), 4);
        let back = decode(&bytes, unseal).unwrap();
        assert_eq!(back.fingerprint(), id.fingerprint());
        assert_eq!(decode(&bytes, |_, _| None).unwrap_err(), StoreError::Sealed);
        assert_eq!(
            decode(b"QS400QSC\0\x01", unseal).unwrap_err(),
            StoreError::Magic
        );
    }
}
