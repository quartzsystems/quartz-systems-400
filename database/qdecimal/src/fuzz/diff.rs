// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Comparing the machine's rows with ours.

use std::collections::BTreeMap;
use std::fmt;

use super::{Case, Row, expected};
use crate::decimal::Decimal;

/// One disagreement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mismatch {
    /// The case.
    pub id: u32,
    /// `inter`, `r1`, `rh` or `rr`.
    pub field: &'static str,
    /// What QS/400 computes.
    pub ours: String,
    /// What the machine wrote.
    pub theirs: String,
    /// The case, described.
    pub case: String,
}

/// The outcome of a diff.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Cases with a machine row.
    pub compared: usize,
    /// Disagreements, in case order.
    pub mismatches: Vec<Mismatch>,
    /// Cases the machine did not answer.
    pub missing: Vec<u32>,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for m in &self.mismatches {
            writeln!(
                f,
                "case {} {}: ours {:?}, machine {:?}  [{}]",
                m.id, m.field, m.ours, m.theirs, m.case
            )?;
        }
        if !self.missing.is_empty() {
            writeln!(f, "{} case(s) without a result row", self.missing.len())?;
        }
        write!(
            f,
            "{} compared, {} mismatch(es)",
            self.compared,
            self.mismatches.len()
        )
    }
}

/// Two arithmetic answers agree when both are the same error token or
/// the same number at the same scale; `0.5` and `.5` are the same.
fn same_number(ours: &str, theirs: &str) -> bool {
    if ours == theirs {
        return true;
    }
    if ours.starts_with('E') || theirs.starts_with('E') {
        return false;
    }
    match (Decimal::parse(ours), Decimal::parse(theirs)) {
        (Ok(a), Ok(b)) => a == b && a.scale() == b.scale(),
        _ => false,
    }
}

/// Compare every case that has a machine row.
#[must_use]
pub fn diff(cases: &[Case], results: &[Row]) -> Report {
    let theirs: BTreeMap<u32, &Row> = results.iter().map(|r| (r.id, r)).collect();
    let mut report = Report::default();
    for c in cases {
        let Some(t) = theirs.get(&c.id) else {
            report.missing.push(c.id);
            continue;
        };
        report.compared += 1;
        let ours = expected(c);
        let fields: [(&'static str, &String, &String, bool); 4] = [
            ("inter", &ours.inter, &t.inter, c.op.is_arithmetic()),
            ("r1", &ours.r1, &t.r1, true),
            ("rh", &ours.rh, &t.rh, true),
            ("rr", &ours.rr, &t.rr, true),
        ];
        for (field, o, m, numeric) in fields {
            let agree = if numeric { same_number(o, m) } else { o == m };
            if !agree {
                report.mismatches.push(Mismatch {
                    id: c.id,
                    field,
                    ours: o.clone(),
                    theirs: m.clone(),
                    case: c.describe(),
                });
            }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuzz::generate;

    #[test]
    fn our_own_rows_match_and_numbers_compare_loosely() {
        let cases = generate(1, 50);
        let rows: Vec<Row> = cases.iter().map(expected).collect();
        let r = diff(&cases, &rows);
        assert_eq!(r.compared, cases.len());
        assert!(r.mismatches.is_empty(), "{r}");
        assert!(same_number("0.50", ".50"));
        assert!(same_number("-0.5", "-.5"));
        assert!(!same_number("0.5", "0.50"));
        assert!(!same_number("E00103", "1"));
        let mut bent = rows.clone();
        bent[0].inter = "E00103".into();
        bent.pop();
        let r = diff(&cases, &bent);
        assert_eq!(r.mismatches.len(), 1);
        assert_eq!(r.missing.len(), 1);
        assert!(r.to_string().contains("1 mismatch(es)"));
    }
}
