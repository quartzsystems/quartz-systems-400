// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `EFI_SIGNATURE_LIST` of one X.509 certificate (UEFI 2.10
//! §32.4.1): what a firmware's "enrol from file" and `efi-updatevar`
//! take for the db.

/// `EFI_CERT_X509_GUID`, a5c059a1-94e4-4aa7-87b5-ab155c2bf072.
pub const CERT_X509: [u8; 16] = guid(
    0xa5c0_59a1,
    0x94e4,
    0x4aa7,
    [0x87, 0xb5, 0xab, 0x15, 0x5c, 0x2b, 0xf0, 0x72],
);
/// The signature owner QS/400 writes, 51533430-3030-4442-9b51-533430304442.
pub const OWNER: [u8; 16] = guid(
    0x5153_3430,
    0x3030,
    0x4442,
    [0x9b, 0x51, 0x53, 0x34, 0x30, 0x30, 0x44, 0x42],
);

/// A GUID in its EFI byte order: the first three fields little-endian.
#[must_use]
pub const fn guid(a: u32, b: u16, c: u16, d: [u8; 8]) -> [u8; 16] {
    let a = a.to_le_bytes();
    let b = b.to_le_bytes();
    let c = c.to_le_bytes();
    [
        a[0], a[1], a[2], a[3], b[0], b[1], c[0], c[1], d[0], d[1], d[2], d[3], d[4], d[5], d[6],
        d[7],
    ]
}

/// The signature list holding `cert` (DER).
#[must_use]
pub fn list(cert: &[u8]) -> Vec<u8> {
    let sig_size = 16 + cert.len();
    let total = 16 + 12 + sig_size;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&CERT_X509);
    for v in [total, 0, sig_size] {
        out.extend_from_slice(&u32::try_from(v).unwrap_or(0).to_le_bytes());
    }
    out.extend_from_slice(&OWNER);
    out.extend_from_slice(cert);
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn layout() {
        let l = super::list(&[1, 2, 3]);
        assert_eq!(l.len(), 16 + 12 + 16 + 3);
        assert_eq!(&l[..4], &[0xa1, 0x59, 0xc0, 0xa5]);
        assert_eq!(&l[16..20], &(47u32).to_le_bytes());
        assert_eq!(&l[24..28], &(19u32).to_le_bytes());
        assert_eq!(&l[44..], &[1, 2, 3]);
    }
}
