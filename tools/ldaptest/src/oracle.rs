// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `oracle`: the anonymous, read-only scenarios against the primary,
//! recorded with its names masked. Every connection is read-only (see
//! [`crate::client::Conn`]): no bind but the anonymous one, nothing written.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::mask::Mask;
use crate::scenarios::{self, Ctx};
use crate::{Error, Report};

/// Where the record goes by default (from the repository's root).
pub const DEFAULT_OUT: &str = "tools/conformance/oracle/dirsrv/ldaptest-primary.txt";

/// Runs the scenarios and writes the masked record: the report and the file.
pub fn run(c: &Ctx) -> Result<(Report, PathBuf), Error> {
    let rep = scenarios::run(c);
    let mask = Mask::from_dse(&c.dse, &c.t.host);
    let mut text = format!(
        "# ldaptest oracle: the anonymous read-only scenarios against the primary,\n\
         # port {}, recorded {} (`ldaptest oracle`). Names masked as the README says.\n\
         # Lines ending in `diag: ...` are the server's own wording.\n\n",
        c.t.port,
        today()
    );
    text.push_str(&mask.apply(&rep.to_string()));
    if mask.leaks(&text) {
        return Err(Error::Check(
            "the record still holds a masked name; not written".into(),
        ));
    }
    let path =
        c.t.out
            .clone()
            .unwrap_or_else(|| PathBuf::from(DEFAULT_OUT));
    std::fs::write(&path, text)?;
    Ok((rep, path))
}

/// Today as YYYY-MM-DD (UTC).
fn today() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (y, m, d) = civil(i64::try_from(secs / 86_400).unwrap_or(0));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since 1970-01-01 to a date (Howard Hinnant's algorithm).
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    #[test]
    fn dates() {
        assert_eq!(super::civil(0), (1970, 1, 1));
        assert_eq!(super::civil(20_405), (2025, 11, 13));
        assert_eq!(super::civil(11_016), (2000, 2, 29));
    }
}
