// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The window `DSPSPLF` and `DSPPFM` show their lines through: the
//! Control line's commands, the ruler, and the hexadecimal forms of a
//! record (side by side and vertical).

/// The columns a window shows.
pub const WIDTH: usize = 78;

/// Bytes a side-by-side row shows.
pub const SIDE_BYTES: usize = 20;

/// A Control line command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Control {
    /// Nothing typed.
    None,
    /// `+n` (`+` alone: a page).
    Down(Option<usize>),
    /// `-n` (`-` alone: a page).
    Up(Option<usize>),
    /// `n`: line or record `n`.
    Line(usize),
    /// `Pn`: page `n`.
    Page(usize),
    /// `Wn`: the window starts at column `n`.
    Column(usize),
    /// `T`, `*TOP`.
    Top,
    /// `B`, `*BOT`.
    Bottom,
    /// `*HEX` (`true`) or `*CHAR`.
    Hex(bool),
    /// Not a command: why.
    Bad(String),
}

fn number(s: &str) -> Option<usize> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// What a Control line value asks for.
#[must_use]
pub fn parse(text: &str) -> Control {
    let t = text.trim().to_ascii_uppercase();
    if t.is_empty() {
        return Control::None;
    }
    let bad = |why: &str| Control::Bad(why.to_string());
    match t.as_str() {
        "T" | "*TOP" => return Control::Top,
        "B" | "*BOT" => return Control::Bottom,
        "*HEX" => return Control::Hex(true),
        "*CHAR" => return Control::Hex(false),
        "+" => return Control::Down(None),
        "-" => return Control::Up(None),
        _ => {}
    }
    let (head, rest) = t.split_at(1);
    match head {
        "+" => number(rest).map_or_else(
            || bad("A number must follow the plus sign."),
            |n| Control::Down(Some(n)),
        ),
        "-" => number(rest).map_or_else(
            || bad("A number must follow the minus sign."),
            |n| Control::Up(Some(n)),
        ),
        "P" => number(rest).map_or_else(|| bad("A page number must follow P."), Control::Page),
        "W" => number(rest)
            .filter(|n| *n > 0)
            .map_or_else(|| bad("A column number must follow W."), Control::Column),
        "T" | "B" => bad("T and B stand alone on the Control line."),
        _ => number(&t).map_or_else(
            || bad("The Control line takes +n, -n, n, Pn, Wn, T, B, *HEX or *CHAR."),
            Control::Line,
        ),
    }
}

/// The ruler character of column `c` (from 1): a digit every tenth
/// column, `+` every fifth, `.` between.
fn mark(c: usize) -> char {
    if c.is_multiple_of(10) {
        char::from_digit(u32::try_from(c / 10 % 10).unwrap_or(0), 10).unwrap_or('0')
    } else if c.is_multiple_of(5) {
        '+'
    } else {
        '.'
    }
}

/// The ruler over columns `from + 1 ..= from + width`, `*` in the first
/// column of the data.
#[must_use]
pub fn ruler(from: usize, width: usize) -> String {
    (from + 1..=from + width)
        .map(|c| if c == 1 { '*' } else { mark(c) })
        .collect()
}

/// The part of `line` the window from column `from` shows.
#[must_use]
pub fn window(line: &str, from: usize, width: usize) -> String {
    line.chars().skip(from).take(width).collect()
}

/// A side-by-side row: twenty bytes from `from` in groups of four, in
/// hexadecimal, then their characters between asterisks.
#[must_use]
pub fn side(hex: &str, text: &str, from: usize) -> String {
    let bytes: Vec<&str> = hex
        .as_bytes()
        .chunks(2)
        .map(|c| std::str::from_utf8(c).unwrap_or("  "))
        .collect();
    let shown: Vec<&str> = bytes.iter().skip(from).take(SIDE_BYTES).copied().collect();
    let groups: Vec<String> = shown.chunks(4).map(<[&str]>::concat).collect();
    let chars: String = text.chars().skip(from).take(shown.len()).collect();
    format!("{:<44}   *{chars}*", groups.join(" "))
}

/// The ruler over a side-by-side row from byte `from`.
#[must_use]
pub fn side_ruler(from: usize) -> String {
    let groups: Vec<String> = (0..SIDE_BYTES / 4)
        .map(|g| {
            (0..4)
                .map(|i| {
                    let c = from + g * 4 + i + 1;
                    (if c == 1 { '*' } else { mark(c) }).to_string()
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    format!("{:<45}   {}", groups.join("  "), ruler(from, SIDE_BYTES))
}

/// The vertical form of a record from column `from`: the characters,
/// the zone digits, the numeric digits.
#[must_use]
pub fn vertical(hex: &str, text: &str, from: usize, width: usize) -> [String; 3] {
    let digits: Vec<char> = hex.chars().collect();
    let take = |i: usize| -> String {
        digits
            .chunks(2)
            .skip(from)
            .take(width)
            .map(|c| c.get(i).copied().unwrap_or(' '))
            .collect()
    };
    [window(text, from, width), take(0), take(1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_values() {
        assert_eq!(parse(""), Control::None);
        assert_eq!(parse("+5"), Control::Down(Some(5)));
        assert_eq!(parse("-12"), Control::Up(Some(12)));
        assert_eq!(parse("+"), Control::Down(None));
        assert_eq!(parse("w20"), Control::Column(20));
        assert_eq!(parse("P3"), Control::Page(3));
        assert_eq!(parse("t"), Control::Top);
        assert_eq!(parse("*bot"), Control::Bottom);
        assert_eq!(parse("*hex"), Control::Hex(true));
        assert_eq!(parse("42"), Control::Line(42));
        assert!(matches!(parse("B5"), Control::Bad(_)));
        assert!(matches!(parse("W0"), Control::Bad(_)));
        assert!(matches!(parse("x"), Control::Bad(_)));
    }

    #[test]
    fn rulers() {
        assert!(ruler(0, 20).starts_with("*...+....1....+....2"));
        assert!(ruler(78, 4).starts_with(".8.."));
        assert_eq!(window("abcdef", 2, 3), "cde");
    }

    /// The golden's row (1-based) without its first column.
    fn golden_row(name: &str, row: usize) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/conformance/golden/verified/pub400")
            .join(format!("{name}.txt"));
        let text = std::fs::read_to_string(path).expect("golden");
        let line = text
            .lines()
            .nth(row - 1)
            .unwrap_or("")
            .trim_end_matches('\r');
        line.chars()
            .skip(1)
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]
    fn hex_forms_match_the_capture() {
        // The first record of the captured member, as its vertical form
        // gives it back: text, zones, digits.
        let text = golden_row("dsppfm-f11", 7);
        let zones = golden_row("dsppfm-f11", 8);
        let digits = golden_row("dsppfm-f11", 9);
        let hex: String = zones
            .chars()
            .zip(digits.chars())
            .flat_map(|(z, d)| [z, d])
            .collect();
        let v = vertical(&hex, &text, 0, WIDTH);
        assert_eq!(v[0], text);
        assert_eq!(v[1], zones);
        assert_eq!(v[2], digits);
        assert_eq!(side(&hex, &text, 0).trim_end(), golden_row("dsppfm-f10", 7));
        assert_eq!(side_ruler(0).trim_end(), golden_row("dsppfm-f10", 6));
        assert_eq!(ruler(0, 60), golden_row("dsppfm", 6));
    }
}
