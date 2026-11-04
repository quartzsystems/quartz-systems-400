// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Dates and times in the service's answers, in the formats of the job
//! that asked (`DATFMT`, `DATSEP`, `TIMSEP`, `DECFMT`), through the one
//! formatting path (`ccsid::nls::Formats`). A request outside a job
//! (the service console) uses the system values' defaults.

use std::cell::Cell;

use ccsid::nls::Formats;

thread_local! {
    /// The formats of the job whose request is being served.
    static JOB: Cell<Option<Formats>> = const { Cell::new(None) };
}

/// The formats in force until the guard drops.
pub struct Scope(Option<Formats>);

impl Drop for Scope {
    fn drop(&mut self) {
        JOB.with(|j| j.set(self.0));
    }
}

/// Serve a request in `f`'s formats.
pub fn enter(f: Option<Formats>) -> Scope {
    Scope(JOB.with(|j| j.replace(f)))
}

/// The formats of the job being served.
#[must_use]
pub fn current() -> Formats {
    JOB.with(Cell::get).unwrap_or_default()
}

/// The date of Unix seconds `t` (UTC), blank for 0.
#[must_use]
pub fn date(t: u64) -> String {
    if t == 0 {
        return String::new();
    }
    current().stamp(i64::try_from(t).unwrap_or(0), 0).0
}

/// The date and time of Unix seconds `t` (UTC), two blanks between;
/// blank for 0.
#[must_use]
pub fn date_time(t: u64) -> String {
    if t == 0 {
        return String::new();
    }
    let (d, tm) = current().stamp(i64::try_from(t).unwrap_or(0), 0);
    format!("{d}  {tm}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_jobs_formats_while_it_is_served() {
        let t = 1_793_282_537;
        assert_eq!(date_time(t), "10/29/26  14:02:17");
        {
            let _s = enter(Some(Formats::parse("*YMD", "-", ".", "")));
            assert_eq!(date_time(t), "26-10-29  14.02.17");
        }
        assert_eq!(date(t), "10/29/26");
        assert_eq!(date(0), "");
    }
}
