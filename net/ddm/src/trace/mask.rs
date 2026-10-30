// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Credential masking for committed traces. In `SECCHK` the user ID
//! becomes the EBCDIC placeholder `MASKUSER` (blank padded to the
//! field's length) and the password, its substitute and the encrypted
//! tokens become `0xFF` bytes. Anywhere else, the user's name (EBCDIC,
//! ASCII or UTF-16, as in a library name) becomes the same number of
//! placeholder characters. Lengths never change, so a masked trace
//! still parses and re-encodes.

use crate::cp::{cmd, param};
use crate::wire::{Body, Dss, Object, Payload};

/// The user ID placeholder.
pub const USER_PLACEHOLDER: &str = "MASKUSER";

/// The byte secrets are filled with.
pub const SECRET_FILL: u8 = 0xFF;

fn mask_object(o: &mut Object, in_secchk: bool) -> usize {
    let secchk = in_secchk || o.cp == cmd::SECCHK;
    let mut n = 0;
    match &mut o.body {
        Body::Coll(items) => {
            for i in items {
                n += mask_object(i, secchk);
            }
        }
        Body::Data(d) if in_secchk => match o.cp {
            param::USRID => {
                d.fill(0x40);
                for (b, c) in d.iter_mut().zip(USER_PLACEHOLDER.chars()) {
                    *b = ccsid::byte_of(c).unwrap_or(0x40);
                }
                n += 1;
            }
            param::PASSWORD | param::NEWPASSWORD | param::SECTKN => {
                d.fill(SECRET_FILL);
                n += 1;
            }
            _ => {}
        },
        Body::Data(_) => {}
    }
    n
}

/// Mask the `SECCHK` fields of one whole DSS in place; returns how
/// many fields were masked.
pub fn mask_dss(b: &mut [u8]) -> usize {
    let Ok((mut d, used)) = Dss::decode(b) else {
        return 0;
    };
    if used != b.len() {
        return 0;
    }
    let mut n = 0;
    if let Payload::Objects(objs) = &mut d.payload {
        for o in objs {
            n += mask_object(o, false);
        }
    }
    if n > 0 {
        let e = d.encode();
        if e.len() == b.len() {
            b.copy_from_slice(&e);
        }
    }
    n
}

/// The forms a user's name takes on the wire: EBCDIC and ASCII upper
/// case, UTF-16.
pub fn user_forms(user: &str) -> Vec<(Vec<u8>, Vec<u8>)> {
    let u = user.trim().to_ascii_uppercase();
    if u.len() < 3 {
        return Vec::new();
    }
    let place: String = USER_PLACEHOLDER.chars().cycle().take(u.len()).collect();
    let mut out = Vec::new();
    if let (Ok(a), Ok(b)) = (ccsid::to_ebcdic(&u), ccsid::to_ebcdic(&place)) {
        out.push((a, b));
    }
    for (a, b) in [(u.clone(), place.clone()), (user.trim().to_string(), place.to_ascii_lowercase())] {
        out.push((a.as_bytes().to_vec(), b.as_bytes().to_vec()));
        let a16 = a.encode_utf16().flat_map(u16::to_be_bytes).collect();
        let b16 = b.encode_utf16().flat_map(u16::to_be_bytes).collect();
        out.push((a16, b16));
    }
    out
}

/// Replace every form of the user's name; returns how many were found.
pub fn mask_user(b: &mut [u8], forms: &[(Vec<u8>, Vec<u8>)]) -> usize {
    let mut n = 0;
    for (from, to) in forms {
        if from.is_empty() || from.len() > b.len() {
            continue;
        }
        let mut i = 0;
        while i + from.len() <= b.len() {
            if &b[i..i + from.len()] == from.as_slice() {
                b[i..i + from.len()].copy_from_slice(to);
                n += 1;
                i += from.len();
            } else {
                i += 1;
            }
        }
    }
    n
}

/// Whether `hay` holds `needle` anywhere (plain-text password checks).
pub fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg::Secchk;

    #[test]
    fn masks_secchk() {
        let s = Secchk {
            secmec: 3,
            rdbnam: Some("PUB400".into()),
            usrid: Some(crate::wire::ebcdic("SOMEONE")),
            password: Some(crate::wire::ebcdic("hunter22")),
            sectkn: vec![vec![1; 16]],
        };
        let mut b = Dss::request(2, s.to_object()).encode();
        assert_eq!(mask_dss(&mut b), 3);
        let (d, _) = Dss::decode(&b).unwrap();
        let back = Secchk::from_object(d.first().unwrap());
        assert_eq!(crate::wire::text(back.usrid.as_ref().unwrap()), "MASKUSE");
        assert_eq!(back.password.unwrap(), vec![0xFF; 8]);
        assert_eq!(back.sectkn[0], vec![0xFF; 16]);
    }

    #[test]
    fn masks_names() {
        let mut b = crate::wire::ebcdic("LIB SOMEONEB/FILE");
        let forms = user_forms("someone");
        assert_eq!(mask_user(&mut b, &forms), 1);
        assert_eq!(crate::wire::text(&b), "LIB MASKUSEB/FILE");
    }
}
