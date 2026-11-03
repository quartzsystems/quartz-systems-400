// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! UTF-8 and UTF-16 big-endian decoding that stops before a partial
//! character.

pub(super) fn decode_utf8(bytes: &[u8], out: &mut Vec<char>) -> usize {
    let mut at = 0;
    while at < bytes.len() {
        match std::str::from_utf8(&bytes[at..]) {
            Ok(s) => {
                out.extend(s.chars());
                return bytes.len();
            }
            Err(e) => {
                let good = e.valid_up_to();
                out.extend(
                    std::str::from_utf8(&bytes[at..at + good])
                        .expect("valid prefix")
                        .chars(),
                );
                at += good;
                match e.error_len() {
                    Some(n) => {
                        out.push('\u{FFFD}');
                        at += n;
                    }
                    None => return at,
                }
            }
        }
    }
    at
}

pub(super) fn decode_utf16(bytes: &[u8], out: &mut Vec<char>) -> usize {
    let mut at = 0;
    while at + 2 <= bytes.len() {
        let u = u16::from_be_bytes([bytes[at], bytes[at + 1]]);
        if (0xD800..0xDC00).contains(&u) {
            if at + 4 > bytes.len() {
                return at;
            }
            let l = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]);
            if (0xDC00..0xE000).contains(&l) {
                let cp = 0x10000 + ((u32::from(u) - 0xD800) << 10) + (u32::from(l) - 0xDC00);
                out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                at += 4;
                continue;
            }
            out.push('\u{FFFD}');
            at += 2;
            continue;
        }
        out.push(char::from_u32(u32::from(u)).unwrap_or('\u{FFFD}'));
        at += 2;
    }
    at
}
