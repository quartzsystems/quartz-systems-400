// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Results as `ok NAME`, `fail NAME: ...` and `skip NAME: ...` lines, with
//! indented detail lines, as the `JTOpen` suite prints them.

use std::fmt;

/// One scenario's result.
#[derive(Debug)]
pub enum Outcome {
    /// It passed; the lines are what it observed.
    Ok(Vec<String>),
    /// It failed.
    Fail(String),
    /// The client crate cannot express it; the reason.
    Skip(&'static str),
}

/// Every scenario run, in order.
#[derive(Debug, Default)]
pub struct Report {
    /// Name and outcome.
    pub rows: Vec<(String, Outcome)>,
}

impl Report {
    /// Adds one.
    pub fn push(&mut self, name: impl Into<String>, outcome: Outcome) {
        self.rows.push((name.into(), outcome));
    }

    /// The outcome of `name`.
    pub fn get(&self, name: &str) -> Option<&Outcome> {
        self.rows.iter().find(|(n, _)| n == name).map(|(_, o)| o)
    }

    /// The names that failed.
    pub fn failures(&self) -> Vec<&str> {
        self.rows
            .iter()
            .filter(|(_, o)| matches!(o, Outcome::Fail(_)))
            .map(|(n, _)| n.as_str())
            .collect()
    }

    /// A detail line of `name` starting with `prefix`, without the prefix.
    pub fn detail(&self, name: &str, prefix: &str) -> Option<&str> {
        match self.get(name)? {
            Outcome::Ok(lines) => lines.iter().find_map(|l| l.strip_prefix(prefix)),
            _ => None,
        }
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, outcome) in &self.rows {
            match outcome {
                Outcome::Ok(lines) => {
                    writeln!(f, "ok {name}")?;
                    for l in lines {
                        writeln!(f, "  {l}")?;
                    }
                }
                Outcome::Fail(why) => writeln!(f, "fail {name}: {why}")?,
                Outcome::Skip(why) => writeln!(f, "skip {name}: {why}")?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_each_form() {
        let mut r = Report::default();
        r.push("a", Outcome::Ok(vec!["dialect: 3.1.1".into()]));
        r.push("b", Outcome::Fail("no".into()));
        r.push("c", Outcome::Skip("cannot"));
        assert_eq!(r.to_string(), "ok a\n  dialect: 3.1.1\nfail b: no\nskip c: cannot\n");
        assert_eq!(r.failures(), ["b"]);
        assert_eq!(r.detail("a", "dialect: "), Some("3.1.1"));
    }
}
