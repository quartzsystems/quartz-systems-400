// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! XFS metadata checksums: CRC-32C over the whole buffer with the
//! checksum field taken as zero, stored little-endian.

use ::crc::{CRC_32_ISCSI, Crc};

const CRC32C: Crc<u32> = Crc::<u32>::new(&CRC_32_ISCSI);

/// CRC-32C of `data`.
pub fn crc32c(data: &[u8]) -> u32 {
    CRC32C.checksum(data)
}

/// Zero the field at `off`, checksum `buf`, store the result there.
pub fn seal(buf: &mut [u8], off: usize) {
    buf[off..off + 4].fill(0);
    let sum = crc32c(buf);
    buf[off..off + 4].copy_from_slice(&sum.to_le_bytes());
}

/// Whether the checksum at `off` matches `buf`.
pub fn verify(buf: &[u8], off: usize) -> bool {
    let mut copy = buf.to_vec();
    let stored = copy[off..off + 4].to_vec();
    seal(&mut copy, off);
    copy[off..off + 4] == stored[..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn castagnoli_check_value() {
        assert_eq!(crc32c(b"123456789"), 0xe306_9283);
    }

    #[test]
    fn seal_then_verify() {
        let mut b = vec![7u8; 64];
        seal(&mut b, 8);
        assert!(verify(&b, 8));
        b[0] ^= 1;
        assert!(!verify(&b, 8));
    }
}
