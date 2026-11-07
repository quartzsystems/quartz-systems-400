// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Command strings as the web F4 reads and writes them: the command
//! name, its `KEYWORD(value)` and positional values, a list value's
//! elements. The analyzer still checks every string before it runs.

/// The words of `s` at the top level: blanks inside quotes or
/// parentheses do not split.
pub fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0u32;
    let mut quoted = false;
    for c in s.chars() {
        match c {
            '\'' => {
                quoted = !quoted;
                cur.push(c);
            }
            '(' if !quoted => {
                depth += 1;
                cur.push(c);
            }
            ')' if !quoted => {
                depth = depth.saturating_sub(1);
                cur.push(c);
            }
            c if c.is_whitespace() && !quoted && depth == 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// A parsed command: its name and its values, keyword (`None` for a
/// positional value) and the value as typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    /// `WRKACTJOB`, or `QSYS/WRKACTJOB`.
    pub name: String,
    /// Keyword and value, in the order typed.
    pub values: Vec<(Option<String>, String)>,
}

/// `s` split into its name and values.
pub fn parse(s: &str) -> Parsed {
    let mut w = words(s.trim()).into_iter();
    let name = w.next().unwrap_or_default().to_ascii_uppercase();
    let values = w
        .map(|word| {
            let open = word.find('(');
            match open {
                Some(i)
                    if i > 0
                        && word.ends_with(')')
                        && word[..i].chars().all(|c| c.is_ascii_alphanumeric()) =>
                {
                    (
                        Some(word[..i].to_ascii_uppercase()),
                        word[i + 1..word.len() - 1].to_string(),
                    )
                }
                _ => (None, word),
            }
        })
        .collect();
    Parsed { name, values }
}

/// The elements of a list value (`QHTTPSVR QBATCH`).
pub fn elements(v: &str) -> Vec<String> {
    words(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values() {
        let p = parse("wrkactjob sbs(QHTTPSVR QBATCH) JOB(*ALL)");
        assert_eq!(p.name, "WRKACTJOB");
        assert_eq!(p.values[0], (Some("SBS".into()), "QHTTPSVR QBATCH".into()));
        assert_eq!(elements(&p.values[0].1), ["QHTTPSVR", "QBATCH"]);
        let p = parse("SBMJOB CMD(CALL PGM(A/B) PARM('x y')) JOB(J)");
        assert_eq!(
            p.values[0],
            (Some("CMD".into()), "CALL PGM(A/B) PARM('x y')".into())
        );
        let p = parse("DSPMSG QSYSOPR");
        assert_eq!(p.values, [(None, "QSYSOPR".to_string())]);
        assert_eq!(
            words("SNDMSG MSG('a ( b') TOUSR(X)"),
            ["SNDMSG", "MSG('a ( b')", "TOUSR(X)"]
        );
    }
}
