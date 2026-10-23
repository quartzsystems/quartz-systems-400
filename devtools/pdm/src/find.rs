// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Option 25 and `FNDSTRPDM`: a string searched in the source data of
//! members, between two columns, matching case or not, and the printed
//! report of what was found.

use std::fmt::Write;

/// What to look for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    /// The string.
    pub string: String,
    /// The first column searched, from 1.
    pub from: usize,
    /// The last column searched; `None` for the record's end.
    pub to: Option<usize>,
    /// Upper and lower case differ.
    pub match_case: bool,
}

impl Query {
    /// A query over whole records, case ignored.
    #[must_use]
    pub fn new(string: &str) -> Self {
        Self {
            string: string.to_string(),
            from: 1,
            to: None,
            match_case: false,
        }
    }

    /// Whether a line holds the string within the columns.
    #[must_use]
    pub fn matches(&self, line: &str) -> bool {
        if self.string.is_empty() {
            return false;
        }
        let chars: Vec<char> = line.chars().collect();
        let start = self.from.max(1) - 1;
        let end = self.to.map_or(chars.len(), |t| t.min(chars.len()));
        if start >= end {
            return false;
        }
        let window: String = chars[start..end].iter().collect();
        if self.match_case {
            window.contains(&self.string)
        } else {
            window.to_uppercase().contains(&self.string.to_uppercase())
        }
    }

    /// The lines that match, numbered from 1.
    #[must_use]
    pub fn search<'a>(&self, lines: impl IntoIterator<Item = &'a str>) -> Vec<(usize, String)> {
        lines
            .into_iter()
            .enumerate()
            .filter(|(_, l)| self.matches(l))
            .map(|(i, l)| (i + 1, l.trim_end().to_string()))
            .collect()
    }
}

/// One member's hits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hits {
    /// The member.
    pub member: String,
    /// Its type.
    pub ty: String,
    /// The matching lines.
    pub lines: Vec<(usize, String)>,
}

/// The printed report: the members searched with their counts and,
/// when `records` asks, the matching lines (`None` for all of them).
#[must_use]
pub fn report(
    lib: &str,
    file: &str,
    q: &Query,
    hits: &[Hits],
    searched: usize,
    records: Option<Option<usize>>,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Find string {:?} in {lib}/{file}", q.string);
    let _ = writeln!(
        out,
        "Columns {} to {}, case {}",
        q.from,
        q.to.map_or_else(|| "end".to_string(), |t| t.to_string()),
        if q.match_case { "matched" } else { "ignored" }
    );
    let _ = writeln!(out);
    let _ = writeln!(out, " Member      Type        Found");
    for h in hits {
        let _ = writeln!(out, " {:<10}  {:<10}  {:>5}", h.member, h.ty, h.lines.len());
        if let Some(max) = records {
            for (n, l) in h.lines.iter().take(max.unwrap_or(usize::MAX)) {
                let _ = writeln!(out, "   {n:>6}  {l}");
            }
        }
    }
    let found = hits.iter().filter(|h| !h.lines.is_empty()).count();
    let _ = writeln!(out);
    let _ = writeln!(out, "{searched} members searched, {found} with the string.");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_and_case() {
        let mut q = Query::new("dcl");
        assert!(q.matches("             DCL        VAR(&A)"));
        q.match_case = true;
        assert!(!q.matches("             DCL        VAR(&A)"));
        q.match_case = false;
        q.from = 20;
        assert!(!q.matches("             DCL        VAR(&A)"));
        q.from = 14;
        q.to = Some(16);
        assert!(q.matches("             DCL        VAR(&A)"));
        q.to = Some(15);
        assert!(!q.matches("             DCL        VAR(&A)"));
        assert!(!Query::new("").matches("x"));
    }

    #[test]
    fn search_and_report() {
        let q = Query::new("pgm");
        let hits = q.search(["PGM", "  DCL", "ENDPGM"]);
        assert_eq!(hits, [(1, "PGM".into()), (3, "ENDPGM".into())]);
        let r = report(
            "L",
            "F",
            &q,
            &[Hits {
                member: "A".into(),
                ty: "CLLE".into(),
                lines: hits,
            }],
            3,
            Some(None),
        );
        assert!(r.contains("ENDPGM"));
        assert!(r.contains("3 members searched, 1 with the string."));
    }
}
