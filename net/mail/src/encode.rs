// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The encodings a message needs: base64 in 76-character lines (RFC
//! 2045 section 6.8), quoted-printable (section 6.7), encoded words for
//! headers (RFC 2047), and the date (RFC 5322 section 3.3).

use base64ct::{Base64, Encoding};

/// Base64 with CRLF after every 76 characters and at the end.
#[must_use]
pub fn base64_lines(data: &[u8]) -> String {
    let all = Base64::encode_string(data);
    let mut out = String::with_capacity(all.len() + all.len() / 38 + 2);
    for chunk in all.as_bytes().chunks(76) {
        out.push_str(std::str::from_utf8(chunk).unwrap_or(""));
        out.push_str("\r\n");
    }
    out
}

/// Base64 on one line.
#[must_use]
pub fn base64(data: &[u8]) -> String {
    Base64::encode_string(data)
}

/// Quoted-printable of UTF-8 text: line breaks become CRLF, lines are
/// kept under 76 characters with soft breaks, blanks before a break
/// and every byte outside printable ASCII are encoded.
#[must_use]
pub fn quoted_printable(text: &str) -> String {
    let mut out = String::new();
    let norm = text.replace("\r\n", "\n");
    let lines: Vec<&str> = norm.split('\n').collect();
    for (i, line) in lines.iter().enumerate() {
        let bytes = line.as_bytes();
        let mut col = 0;
        for (j, &b) in bytes.iter().enumerate() {
            let last = j + 1 == bytes.len();
            let piece = if (b == b' ' || b == b'\t') && last {
                format!("={b:02X}")
            } else if (33..=126).contains(&b) && b != b'=' || b == b' ' || b == b'\t' {
                char::from(b).to_string()
            } else {
                format!("={b:02X}")
            };
            if col + piece.len() > 75 {
                out.push_str("=\r\n");
                col = 0;
            }
            col += piece.len();
            out.push_str(&piece);
        }
        if i + 1 < lines.len() {
            out.push_str("\r\n");
        }
    }
    out
}

/// A header value: as it is when it is printable ASCII, else one or
/// more `=?utf-8?B?...?=` words of whole characters.
#[must_use]
pub fn word(text: &str) -> String {
    if text.chars().all(|c| (' '..='~').contains(&c)) {
        return text.to_string();
    }
    // 45 bytes encode to 60 characters, keeping each word under 75.
    let mut words = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if cur.len() + c.len_utf8() > 45 {
            words.push(format!("=?utf-8?B?{}?=", base64(cur.as_bytes())));
            cur.clear();
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        words.push(format!("=?utf-8?B?{}?=", base64(cur.as_bytes())));
    }
    words.join("\r\n ")
}

/// Unix seconds as the civil date, the time of day and the weekday.
#[allow(clippy::many_single_char_names)]
fn civil(unix: i64) -> (i64, u32, u32, u32, u32, u32, usize) {
    let days = unix.div_euclid(86_400);
    let rem = unix.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    // 1970-01-01 was a Thursday.
    let wd = usize::try_from((days + 4).rem_euclid(7)).unwrap_or(0);
    let n = |v: i64| u32::try_from(v).unwrap_or(0);
    (y, n(m), n(d), n(rem / 3600), n(rem % 3600 / 60), n(rem % 60), wd)
}

/// The `Date:` value of a Unix time, in UTC.
#[must_use]
#[allow(clippy::many_single_char_names)]
pub fn date(unix: i64) -> String {
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let (y, m, d, h, mi, s, wd) = civil(unix);
    let month = MONTHS[usize::try_from(m.clamp(1, 12) - 1).unwrap_or(0)];
    format!("{}, {d:02} {month} {y} {h:02}:{mi:02}:{s:02} +0000", DAYS[wd])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_wraps() {
        assert_eq!(base64_lines(b"Hello"), "SGVsbG8=\r\n");
        let long = base64_lines(&[0u8; 100]);
        let lines: Vec<&str> = long.split("\r\n").collect();
        assert_eq!(lines[0].len(), 76);
        assert_eq!(lines[1].len(), 136 - 76);
    }

    #[test]
    fn quoted_printable_lines() {
        assert_eq!(quoted_printable("a=b\nnext "), "a=3Db\r\nnext=20");
        assert_eq!(quoted_printable("caf\u{e9}"), "caf=C3=A9");
        let long = quoted_printable(&"x".repeat(100));
        assert!(long.split("\r\n").all(|l| l.len() <= 76), "{long}");
        assert_eq!(long.replace("=\r\n", ""), "x".repeat(100));
    }

    #[test]
    fn encoded_words() {
        assert_eq!(word("Monthly report"), "Monthly report");
        assert_eq!(word("Grüße"), "=?utf-8?B?R3LDvMOfZQ==?=");
        let long = word(&"é".repeat(40));
        assert_eq!(long.matches("=?utf-8?B?").count(), 2);
    }

    #[test]
    fn dates() {
        assert_eq!(date(0), "Thu, 01 Jan 1970 00:00:00 +0000");
        // 2026-10-30T12:34:56Z.
        assert_eq!(date(1_793_363_696), "Fri, 30 Oct 2026 12:34:56 +0000");
        assert_eq!(date(951_782_400), "Tue, 29 Feb 2000 00:00:00 +0000");
    }
}
