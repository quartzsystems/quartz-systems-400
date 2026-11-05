// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `EXPLAIN`'s columns beside the plan text: for each step the database
//! monitor record that step writes (`QQRID`, as `STRDBMON` writes them
//! in the `QQQDBMN` layout Visual Explain reads), its reason code
//! (`QQRCOD`) and the join method (`QVC13`). Db2 for i has no `EXPLAIN`
//! statement; ACS's Visual Explain draws its diagrams from monitor
//! records through the internal procedure `QSYS.QQQDBVE`, whose
//! interface IBM does not document. The monitor records are the
//! structure the two share.

use crate::value::Value;

/// The monitor columns of one plan line: record ID, reason code, join
/// method.
#[must_use]
pub fn columns(line: &str) -> [Value; 3] {
    let s = |x: &str| Value::Str(x.to_string());
    let none = || [Value::Null, Value::Null, Value::Null];
    if line.starts_with("SCAN") {
        if line.contains("QSYS2 catalog") {
            return none();
        }
        if line.contains(" keyed via ") {
            let positioned = !line.contains("eq=0 range=no");
            let code = match (positioned, line.contains("ordered")) {
                (true, true) => "I3",
                (false, true) => "I2",
                _ => "I1",
            };
            return [Value::int(3001), s(code), Value::Null];
        }
        if line.contains(" bitmap over ") {
            // A bitmap of record numbers made from the paths.
            return [Value::int(3021), s("I1"), Value::Null];
        }
        return [Value::int(3000), s("T1"), Value::Null];
    }
    if let Some(rest) = line.strip_prefix("JOIN ") {
        let m = if rest.contains("hash join") {
            "H"
        } else if rest.contains("sort-merge") {
            "M"
        } else {
            "N"
        };
        return [Value::int(3014), Value::Null, s(m)];
    }
    if line == "SORT" {
        return [Value::int(3003), s("F7"), Value::Null];
    }
    if line == "DISTINCT" {
        return [Value::int(3025), s("F4"), Value::Null];
    }
    if line.starts_with("GROUP BY") {
        return [Value::int(3028), s("F1"), Value::Null];
    }
    if line.starts_with("UNION") {
        return [Value::int(3026), s("F5"), Value::Null];
    }
    none()
}
