// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QUPSDLYTIM`: how long the system runs on its uninterruptible power
//! supply before it powers down. Two elements, as IBM keeps them: the
//! value as set (`*CALC`, `*BASIC`, `*NOMAX` or 0 to 99999 seconds) and
//! the delay in seconds it gives, which the system fills in.

use crate::error::SysValError;

/// The delay `*CALC` and `*BASIC` give, seconds (IBM's fixed delay).
pub const CALC_SECONDS: u32 = 200;

/// The delay the value gives: `None` for `*NOMAX` (no power down of the
/// system's own).
pub fn delay(value: &str) -> Result<Option<u32>, SysValError> {
    let bad = || SysValError::NotAllowed {
        name: "QUPSDLYTIM".into(),
        got: value.into(),
        allowed: "*CALC, *BASIC, *NOMAX, 0-99999".into(),
    };
    match value.trim().to_ascii_uppercase().as_str() {
        "*CALC" | "*BASIC" => Ok(Some(CALC_SECONDS)),
        "*NOMAX" => Ok(None),
        n => n
            .parse::<u32>()
            .ok()
            .filter(|n| *n <= 99_999)
            .map(Some)
            .ok_or_else(bad),
    }
}

/// The two elements for a value as typed: the value, then its seconds
/// (`*NOMAX` keeps `*NOMAX`).
pub fn elements(typed: &[String]) -> Result<Vec<String>, SysValError> {
    let first = typed
        .first()
        .map_or("*CALC", String::as_str)
        .trim()
        .to_ascii_uppercase();
    let first = first.parse::<u32>().map_or(first, |n| n.to_string());
    let secs = delay(&first)?.map_or_else(|| "*NOMAX".to_string(), |s| s.to_string());
    Ok(vec![first, secs])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_and_their_delays() {
        let e = |v: &str| elements(&[v.to_string()]).unwrap();
        assert_eq!(e("*calc"), ["*CALC", "200"]);
        assert_eq!(e("*BASIC"), ["*BASIC", "200"]);
        assert_eq!(e("*NOMAX"), ["*NOMAX", "*NOMAX"]);
        assert_eq!(e("0045"), ["45", "45"]);
        assert_eq!(e("0"), ["0", "0"]);
        assert!(elements(&["100000".into()]).is_err());
        assert!(elements(&["*NEVER".into()]).is_err());
        assert_eq!(delay("*NOMAX").unwrap(), None);
    }
}
