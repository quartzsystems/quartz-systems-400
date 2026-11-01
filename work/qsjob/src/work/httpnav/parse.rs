// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The figures panel data carries as text, back to numbers.

/// `12.3` (a percentage) as tenths: 123.
pub fn tenths(text: &str) -> u32 {
    let t = text.trim().replace(',', ".");
    t.parse::<f64>().map_or(0, |v| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped
        let r = (v * 10.0).round().clamp(0.0, f64::from(u32::MAX)) as u32;
        r
    })
}

/// `hh:mm:ss` as seconds.
pub fn hms(text: &str) -> u32 {
    text.trim()
        .split(':')
        .try_fold(0u32, |acc, p| p.trim().parse::<u32>().ok().map(|n| acc * 60 + n))
        .unwrap_or(0)
}

/// `1.5 G`, `300 M`, `12 K` as bytes; a bare number is bytes.
pub fn size(text: &str) -> u64 {
    let t = text.trim().replace(',', ".");
    let (num, unit) = t.split_once(' ').unwrap_or((t.as_str(), ""));
    let mult: f64 = match unit.trim() {
        "G" => 1024.0 * 1024.0 * 1024.0,
        "M" => 1024.0 * 1024.0,
        "K" => 1024.0,
        _ => 1.0,
    };
    num.parse::<f64>().map_or(0, |v| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped
        let r = (v * mult).round().max(0.0) as u64;
        r
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn figures() {
        assert_eq!(tenths("12.3"), 123);
        assert_eq!(tenths(" .0"), 0);
        assert_eq!(tenths("x"), 0);
        assert_eq!(hms("01:02:03"), 3723);
        assert_eq!(size("1.5 G"), 1_610_612_736);
        assert_eq!(size("300 M"), 314_572_800);
        assert_eq!(size("12"), 12);
    }
}
