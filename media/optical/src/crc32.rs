// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! CRC-32 (IEEE 802.3, reflected), as GPT uses it.

#[allow(clippy::cast_possible_truncation)]
const fn table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

const TABLE: [u32; 256] = table();

/// The CRC of `data`.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn crc32(data: &[u8]) -> u32 {
    !data
        .iter()
        .fold(!0u32, |c, &b| TABLE[usize::from((c as u8) ^ b)] ^ (c >> 8))
}

#[cfg(test)]
mod tests {
    #[test]
    fn known_value() {
        assert_eq!(super::crc32(b"123456789"), 0xCBF4_3926);
    }
}
