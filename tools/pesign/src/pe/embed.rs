// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The certificate table: one `WIN_CERTIFICATE` of type
//! `WIN_CERT_TYPE_PKCS_SIGNED_DATA` at the end of the file, 8-byte
//! aligned, named by the fifth data directory; then the PE checksum.

use super::Layout;
use crate::Error;

/// `WIN_CERT_REVISION_2_0`.
const REVISION: u16 = 0x0200;
/// `WIN_CERT_TYPE_PKCS_SIGNED_DATA`.
const PKCS_SIGNED_DATA: u16 = 0x0002;

/// `image` without its certificate table, padded to 8 bytes: what is
/// hashed and then signed.
pub fn unsigned(image: &[u8]) -> Result<Vec<u8>, Error> {
    let l = Layout::of(image)?;
    let mut out = image[..image.len() - l.cert_table.1].to_vec();
    out[l.cert_entry..l.cert_entry + 8].fill(0);
    out.resize(out.len().next_multiple_of(8), 0);
    Ok(out)
}

/// Append `signed_data` (a PKCS#7 `ContentInfo`) to `image` (from
/// [`unsigned`]) as its certificate table and fix the checksum.
pub fn attach(image: &mut Vec<u8>, signed_data: &[u8]) -> Result<(), Error> {
    let l = Layout::of(image)?;
    if l.cert_table.1 != 0 || !image.len().is_multiple_of(8) {
        return Err(Error::Pe("the image is not prepared for a signature"));
    }
    let offset = image.len();
    let length = 8 + signed_data.len();
    let padded = length.next_multiple_of(8);
    let len32 = |n: usize| u32::try_from(n).map_err(|_| Error::Pe("the signature is too large"));
    image.extend_from_slice(&len32(length)?.to_le_bytes());
    image.extend_from_slice(&REVISION.to_le_bytes());
    image.extend_from_slice(&PKCS_SIGNED_DATA.to_le_bytes());
    image.extend_from_slice(signed_data);
    image.resize(offset + padded, 0);
    image[l.cert_entry..l.cert_entry + 4].copy_from_slice(&len32(offset)?.to_le_bytes());
    image[l.cert_entry + 4..l.cert_entry + 8].copy_from_slice(&len32(padded)?.to_le_bytes());
    let sum = checksum(image, l.checksum);
    image[l.checksum..l.checksum + 4].copy_from_slice(&sum.to_le_bytes());
    Ok(())
}

/// The `PKCS#7` blob of a signed image's certificate table.
pub fn signature(image: &[u8]) -> Result<&[u8], Error> {
    let l = Layout::of(image)?;
    let (off, size) = l.cert_table;
    if size < 8 {
        return Err(Error::Signature("the image is not signed".into()));
    }
    let entry = &image[off..off + size];
    let length =
        usize::try_from(u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]])).unwrap_or(0);
    let kind = u16::from_le_bytes([entry[6], entry[7]]);
    if kind != PKCS_SIGNED_DATA || length < 8 || length > size {
        return Err(Error::Signature(
            "the certificate table holds no PKCS#7 signature".into(),
        ));
    }
    Ok(&entry[8..length])
}

/// The PE checksum: 16-bit words summed with carries folded, the
/// checksum field itself skipped, plus the file length.
fn checksum(image: &[u8], field: usize) -> u32 {
    let mut sum: u64 = 0;
    for (i, w) in image.chunks(2).enumerate() {
        let at = i * 2;
        if at == field || at == field + 2 {
            continue;
        }
        let v = u64::from(w[0]) | (u64::from(*w.get(1).unwrap_or(&0)) << 8);
        sum += v;
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    sum = (sum & 0xFFFF) + (sum >> 16);
    u32::try_from(sum)
        .unwrap_or(0)
        .wrapping_add(u32::try_from(image.len()).unwrap_or(0))
}
