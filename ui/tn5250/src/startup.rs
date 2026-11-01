// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The TN5250E start-up response record (RFC 4777, section 10): what the
//! server sends once negotiation names a device, before any screen.
//!
//! ```text
//! 0   length          2   0x0049
//! 2   record type     2   0x12A0
//! 4   fixed fields   12   90 00 05 60 06 00 20 C0 00 3D 00 00 (success)
//!                         90 00 05 60 06 00 82 00 00 3D 00 00 (error)
//! 16  response code   4   EBCDIC, I902 or 8902 ...
//! 20  system name     8   EBCDIC, blank padded
//! 28  device name    10   EBCDIC, blank padded
//! 38  diagnostics    35   zeros
//! ```

use crate::Error;

/// The record's length.
pub const LEN: usize = 73;

/// The fixed fields of a successful start-up.
const FIXED_OK: [u8; 12] = [0x90, 0x00, 0x05, 0x60, 0x06, 0x00, 0x20, 0xC0, 0x00, 0x3D, 0x00, 0x00];
/// The fixed fields of a refused start-up.
const FIXED_ERR: [u8; 12] = [0x90, 0x00, 0x05, 0x60, 0x06, 0x00, 0x82, 0x00, 0x00, 0x3D, 0x00, 0x00];

/// The response codes QS/400 sends.
pub mod code {
    /// Session started; the virtual device has less function than the
    /// client's.
    pub const LESS_FUNCTION: &str = "I901";
    /// Session started.
    pub const STARTED: &str = "I902";
    /// Device not available (in use by another session).
    pub const BUSY: &str = "8902";
    /// Device not valid for the session (the console's display, a
    /// device of the wrong type).
    pub const NOT_VALID: &str = "8903";
    /// Creating the device failed.
    pub const CREATE_FAILED: &str = "8925";
    /// Automatic configuration failed or is not allowed.
    pub const NO_AUTOCFG: &str = "8940";
    /// Every code QS/400 sends.
    pub const ALL: [&str; 6] = [LESS_FUNCTION, STARTED, BUSY, NOT_VALID, CREATE_FAILED, NO_AUTOCFG];
}

/// Whether `code` starts a session (`I9xx`).
pub fn is_success(code: &str) -> bool {
    code.starts_with('I')
}

/// A decoded start-up response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Startup {
    /// The response code (`I902`).
    pub code: String,
    /// The system name, trimmed.
    pub system: String,
    /// The device name, trimmed.
    pub device: String,
}

fn ebcdic(s: &str, width: usize) -> Vec<u8> {
    let mut t: String = s.chars().take(width).collect();
    while t.chars().count() < width {
        t.push(' ');
    }
    ccsid::to_ebcdic(&t).unwrap_or_else(|_| vec![0x40; width])
}

/// The record for `code`, system `system` and device `device` (without
/// the Telnet `IAC EOR`).
pub fn encode(code: &str, system: &str, device: &str) -> Vec<u8> {
    let mut r = Vec::with_capacity(LEN);
    r.extend_from_slice(&u16::try_from(LEN).unwrap_or(0).to_be_bytes());
    r.extend_from_slice(&crate::record::GDS.to_be_bytes());
    r.extend_from_slice(if is_success(code) { &FIXED_OK } else { &FIXED_ERR });
    r.extend(ebcdic(code, 4));
    r.extend(ebcdic(system, 8));
    r.extend(ebcdic(device, 10));
    r.resize(LEN, 0);
    r
}

/// Whether `b` is a start-up response rather than a 5250 record (whose
/// reserved field is zero).
pub fn is_startup(b: &[u8]) -> bool {
    b.len() >= 38 && b[2..4] == crate::record::GDS.to_be_bytes() && b[4] == 0x90 && b[6] == 0x05
}

/// Decode a start-up response.
pub fn decode(b: &[u8]) -> Result<Startup, Error> {
    if !is_startup(b) {
        return Err(Error::Negotiation("not a start-up response record".into()));
    }
    let text = |r: std::ops::Range<usize>| ccsid::from_ebcdic(&b[r]).trim_end().to_string();
    Ok(Startup {
        code: text(16..20),
        system: text(20..28),
        device: text(28..38),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4777, figure 1: I902 from system TARGET for device PCPRINTER.
    const RFC_OK: &str = "004912A090000560060020C0003D0000C9F9F0F2E3C1D9C7C5E34040D7C3D7D9\
C9D5E3C5D9400000000000000000000000000000000000000000000000000000\
000000000000000000";
    /// RFC 4777, figure 2: 8902, the device is in use.
    const RFC_ERR: &str = "004912A09000056006008200003D0000F8F9F0F2E3C1D9C7C5E34040D7C3D7D9\
C9D5E3C5D9400000000000000000000000000000000000000000000000000000\
000000000000000000";

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn matches_the_rfc_examples() {
        assert_eq!(encode("I902", "TARGET", "PCPRINTER"), hex(RFC_OK));
        assert_eq!(encode("8902", "TARGET", "PCPRINTER"), hex(RFC_ERR));
        let d = decode(&hex(RFC_ERR)).unwrap();
        assert_eq!((d.code.as_str(), d.system.as_str(), d.device.as_str()), ("8902", "TARGET", "PCPRINTER"));
    }

    #[test]
    fn every_code_round_trips() {
        for c in code::ALL {
            let r = encode(c, "QSHQ400", "QPADEV0001");
            assert_eq!(r.len(), LEN);
            assert_eq!(r[10..12], if is_success(c) { [0x20, 0xC0] } else { [0x82, 0x00] }, "{c}");
            assert_eq!(decode(&r).unwrap().code, c);
        }
        // Long names are cut to their fields.
        let r = decode(&encode("I902", "SYSTEMNAME9", "DEVICENAME12")).unwrap();
        assert_eq!((r.system.as_str(), r.device.as_str()), ("SYSTEMNA", "DEVICENAME"));
        // An ordinary 5250 record is not one.
        assert!(!is_startup(&[0, 10, 0x12, 0xA0, 0, 0, 4, 0, 0, 3]));
    }
}
