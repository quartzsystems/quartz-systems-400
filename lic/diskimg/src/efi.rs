// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! UEFI boot variables as efivarfs holds them: a `Boot####` load option
//! for each slot (a hard drive device path to the ESP and the slot's
//! file), `BootNext` for the one-shot boot and `BootOrder` for the
//! default (UEFI 2.10 §3.1.3, §10.3).

use uuid::Uuid;

/// The EFI global variable GUID, the suffix of the efivarfs names.
pub const GLOBAL: &str = "8be4df61-93ca-11d2-aa0d-00e098032b8c";
/// Non-volatile, boot service and runtime access.
pub const ATTRS: u32 = 0x7;
/// The load option is active.
const LOAD_OPTION_ACTIVE: u32 = 1;

/// Where the ESP is on its disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EspLocation {
    /// GPT entry number, from 1.
    pub index: u32,
    /// First LBA (512-byte sectors).
    pub first_lba: u64,
    /// Sectors.
    pub sectors: u64,
    /// The partition's unique GUID.
    pub guid: Uuid,
}

fn ucs2(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 2 + 2);
    for u in s.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out.extend_from_slice(&[0, 0]);
    out
}

/// A `Boot####` load option: `description`, then the device path to
/// `path` (`EFI/QS400/B.EFI`) on the ESP.
#[must_use]
pub fn load_option(description: &str, esp: &EspLocation, path: &str) -> Vec<u8> {
    let mut dp = Vec::new();
    // Hard drive media device path.
    dp.extend_from_slice(&[0x04, 0x01, 42, 0]);
    dp.extend_from_slice(&esp.index.to_le_bytes());
    dp.extend_from_slice(&esp.first_lba.to_le_bytes());
    dp.extend_from_slice(&esp.sectors.to_le_bytes());
    dp.extend_from_slice(&esp.guid.to_bytes_le());
    dp.push(0x02); // GPT
    dp.push(0x02); // GUID signature
    // File path media device path, backslashes and a leading one.
    let file = ucs2(&format!("\\{}", path.replace('/', "\\")));
    let len = u16::try_from(4 + file.len()).expect("path fits");
    dp.extend_from_slice(&[0x04, 0x04]);
    dp.extend_from_slice(&len.to_le_bytes());
    dp.extend_from_slice(&file);
    // End of the whole device path.
    dp.extend_from_slice(&[0x7F, 0xFF, 0x04, 0x00]);
    let mut out = Vec::new();
    out.extend_from_slice(&LOAD_OPTION_ACTIVE.to_le_bytes());
    out.extend_from_slice(&u16::try_from(dp.len()).expect("device path fits").to_le_bytes());
    out.extend_from_slice(&ucs2(description));
    out.extend_from_slice(&dp);
    out
}

/// The path a load option boots, back from its bytes; `None` when it is
/// not one with a file path node.
#[must_use]
pub fn load_option_path(bytes: &[u8]) -> Option<String> {
    let dp_len = usize::from(u16::from_le_bytes(bytes.get(4..6)?.try_into().ok()?));
    let mut at = 6;
    while u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?) != 0 {
        at += 2;
    }
    let mut dp = bytes.get(at + 2..at + 2 + dp_len)?;
    while dp.len() >= 4 {
        let (kind, sub) = (dp[0], dp[1]);
        let n = usize::from(u16::from_le_bytes([dp[2], dp[3]]));
        if n < 4 || n > dp.len() {
            return None;
        }
        if (kind, sub) == (0x04, 0x04) {
            let units: Vec<u16> = dp[4..n]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .take_while(|u| *u != 0)
                .collect();
            return String::from_utf16(&units).ok();
        }
        if kind == 0x7F {
            return None;
        }
        dp = &dp[n..];
    }
    None
}

/// The efivarfs file name of a variable.
#[must_use]
pub fn var_name(name: &str) -> String {
    format!("{name}-{GLOBAL}")
}

/// `Boot####`.
#[must_use]
pub fn boot_name(number: u16) -> String {
    format!("Boot{number:04X}")
}

/// An efivarfs file's bytes: the attributes, then the data.
#[must_use]
pub fn file_bytes(data: &[u8]) -> Vec<u8> {
    let mut out = ATTRS.to_le_bytes().to_vec();
    out.extend_from_slice(data);
    out
}

/// A list of boot numbers (`BootOrder`, `BootNext`) as data.
#[must_use]
pub fn numbers(list: &[u16]) -> Vec<u8> {
    list.iter().flat_map(|n| n.to_le_bytes()).collect()
}

/// A list of boot numbers from an efivarfs file (attributes first).
#[must_use]
pub fn parse_numbers(file: &[u8]) -> Vec<u16> {
    file.get(4..)
        .unwrap_or(&[])
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect()
}

/// `BootOrder` with `first` at the front, once.
#[must_use]
pub fn order_with_first(order: &[u16], first: u16) -> Vec<u16> {
    let mut out = vec![first];
    out.extend(order.iter().copied().filter(|n| *n != first));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_options_round_trip() {
        let esp = EspLocation {
            index: 1,
            first_lba: 2048,
            sectors: 524_288,
            guid: Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef),
        };
        let b = load_option("QS/400 slot B", &esp, "EFI/QS400/B.EFI");
        assert_eq!(&b[..4], &[1, 0, 0, 0]);
        assert_eq!(load_option_path(&b).as_deref(), Some("\\EFI\\QS400\\B.EFI"));
        // The device path length covers the HD node, the file node and the end.
        let dp = usize::from(u16::from_le_bytes([b[4], b[5]]));
        assert_eq!(dp, 42 + 4 + 2 * 17 + 4);
        assert_eq!(&b[b.len() - 4..], &[0x7F, 0xFF, 4, 0]);
        assert_eq!(boot_name(0x5142), "Boot5142");
        assert_eq!(parse_numbers(&file_bytes(&numbers(&[1, 0x5142]))), [1, 0x5142]);
        assert_eq!(order_with_first(&[3, 0x5142, 1], 0x5142), [0x5142, 3, 1]);
    }
}
