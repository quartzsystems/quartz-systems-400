// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Syntax checking of DDS as SEU does it: the member compiled by its
//! type's compiler, and the first error on a record the operator just
//! added or changed reported. CL is checked by the job's analyzer; RPG
//! has a hook and no checker until its compiler exists.

use dspf::{CompileOpts, Diagnostic, NoRefs, Severity, Source};

/// The first error of a DDS member on one of the positions `touched`:
/// the position and the message.
#[must_use]
pub fn dds(srctype: &str, records: &[(u32, u32, String)], touched: &[usize]) -> Option<(usize, String)> {
    let bytes: Vec<Vec<u8>> = records
        .iter()
        .map(|(_, _, d)| d.chars().map(|c| ccsid::byte_of(c).unwrap_or(ccsid::SPACE)).collect())
        .collect();
    let src = Source::from_records(
        records
            .iter()
            .zip(&bytes)
            .map(|((s, d, _), b)| (*s, *d, b.as_slice())),
    );
    let opts = CompileOpts::default();
    let diags: Vec<Diagnostic> = match srctype.trim().to_ascii_uppercase().as_str() {
        "DSPF" | "DSPF38" => dspf::compile_dspf(&src, &NoRefs, &opts).diagnostics,
        "PRTF" | "PRTF38" => dds::compile_prtf(&src, &NoRefs, &opts).diagnostics,
        "PF" | "PF38" => dds::compile_pf(&src, &NoRefs, &opts).diagnostics,
        "LF" | "LF38" => dds::compile_lf(&src, &NoRefs, &opts).diagnostics,
        _ => return None,
    };
    diags
        .iter()
        .filter(|d| d.severity >= Severity::Error)
        .find_map(|d| {
            d.line
                .filter(|l| touched.contains(l))
                .map(|l| (l, format!("{} {}", d.id, d.text)))
        })
}

/// RPG's checker: none yet. `TODO(phase9)`: the RPG compiler's syntax
/// check of one specification.
#[must_use]
pub fn rpg(_record: &str) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(lines: &[&str]) -> Vec<(u32, u32, String)> {
        lines
            .iter()
            .enumerate()
            .map(|(i, l)| (u32::try_from(i + 1).unwrap() * 100, 0, (*l).to_string()))
            .collect()
    }

    #[test]
    fn a_bad_dds_line_is_reported_where_it_was_typed() {
        let good = rec(&[
            "     A          R FMT1",
            "     A            NAME          10A  B  5 10",
        ]);
        assert_eq!(dds("DSPF", &good, &[0, 1]), None);
        let bad = rec(&[
            "     A          R FMT1",
            "     A            NAME          10Q  B  5 10",
        ]);
        let (at, msg) = dds("DSPF", &bad, &[1]).expect("an error");
        assert_eq!(at, 1);
        assert!(msg.starts_with("DDS"));
        assert_eq!(dds("DSPF", &bad, &[0]), None);
        assert_eq!(dds("TXT", &bad, &[1]), None);
    }
}
