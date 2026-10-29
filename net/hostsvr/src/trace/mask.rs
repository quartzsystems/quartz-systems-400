// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Credential masking for committed traces. In every frame of the
//! sign-on family (request IDs `0x70xx`/`0x71xx`, replies `0xF0xx`/
//! `0xF1xx`) the user ID becomes the EBCDIC placeholder `MASKUSER`
//! (blank padded to the field's length; in replies after the field's
//! CCSID) and every password substitute,
//! protected password and profile token becomes `0xFF` bytes. Lengths
//! never change, so a masked trace still parses and re-encodes.

use crate::cp::signon as cp;
use crate::frame::header::{HEADER_LEN, be16, be32};
use crate::frame::param::PARAM_HEAD;

/// The user ID placeholder.
pub const USER_PLACEHOLDER: &str = "MASKUSER";

/// The byte secrets are filled with.
pub const SECRET_FILL: u8 = 0xFF;

/// Code points whose data is a secret.
pub const SECRET_CPS: [u16; 4] = [
    cp::PASSWORD,
    cp::OLD_PASSWORD,
    cp::NEW_PASSWORD,
    cp::PROFILE_TOKEN,
];

/// Whether a frame belongs to the sign-on family.
fn signon_family(id: u16) -> bool {
    matches!(id >> 8, 0x70 | 0x71 | 0xF0 | 0xF1)
}

/// Mask one whole frame in place; returns the number of fields masked.
pub fn mask_frame(b: &mut [u8]) -> usize {
    if b.len() < HEADER_LEN || !signon_family(be16(b, 18)) {
        return 0;
    }
    let reply = be16(b, 18) & 0x8000 != 0;
    let mut at = HEADER_LEN + usize::from(be16(b, 16));
    let mut n = 0;
    while at + PARAM_HEAD <= b.len() {
        let ll = be32(b, at) as usize;
        if ll < PARAM_HEAD || at + ll > b.len() {
            break;
        }
        let code = be16(b, at + 4);
        // In replies the user ID follows a CCSID (u32).
        let skip = if reply && code == cp::USER_ID {
            4.min(ll - PARAM_HEAD)
        } else {
            0
        };
        let data = &mut b[at + PARAM_HEAD + skip..at + ll];
        if code == cp::USER_ID {
            data.fill(0x40);
            for (d, c) in data.iter_mut().zip(USER_PLACEHOLDER.chars()) {
                *d = ccsid::byte_of(c).unwrap_or(0x40);
            }
            n += 1;
        } else if SECRET_CPS.contains(&code) {
            data.fill(SECRET_FILL);
            n += 1;
        }
        at += ll;
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
    use crate::frame::{Frame, Param};

    #[test]
    fn masks_user_and_password() {
        let f = Frame::new(
            0xE009,
            0x7004,
            vec![3],
            vec![
                Param::new(cp::PASSWORD, vec![1u8; 20]),
                Param::new(cp::USER_ID, ccsid::to_ebcdic("SOMEONE   ").unwrap()),
            ],
        );
        let mut b = f.encode();
        assert_eq!(mask_frame(&mut b), 2);
        let m = Frame::decode(&b).unwrap();
        assert_eq!(m.params[0].data, vec![0xFF; 20]);
        assert_eq!(ccsid::from_ebcdic(&m.params[1].data), "MASKUSER  ");
    }
}
