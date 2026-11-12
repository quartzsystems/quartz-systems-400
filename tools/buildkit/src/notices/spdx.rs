// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SPDX license expressions as crates write them (`MIT OR Apache-2.0`,
//! the old `MIT/Apache-2.0`, `(A OR B) AND C`, `X WITH Y`), and the
//! choice of the licenses the product takes a crate under: allowed ones only
//! (`deny.toml`), the first by [`PREFERENCE`] where there is a choice.

/// Which license to take when a crate offers several.
pub const PREFERENCE: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "BSD-3-Clause",
    "BSD-2-Clause",
    "ISC",
    "Zlib",
    "0BSD",
    "Unlicense",
    "CC0-1.0",
    "Unicode-3.0",
    "Apache-2.0 WITH LLVM-exception",
    "CDLA-Permissive-2.0",
    "MPL-2.0",
];

#[derive(Debug, PartialEq, Eq)]
enum Node {
    Id(String),
    And(Vec<Node>),
    Or(Vec<Node>),
}

fn tokens(s: &str) -> Vec<String> {
    s.replace('(', " ( ")
        .replace(')', " ) ")
        .replace('/', " OR ")
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

struct Parser {
    t: Vec<String>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<&str> {
        self.t.get(self.i).map(String::as_str)
    }

    fn next(&mut self) -> Option<String> {
        let v = self.t.get(self.i).cloned();
        self.i += 1;
        v
    }

    fn or(&mut self) -> Result<Node, String> {
        let mut v = vec![self.and()?];
        while self.peek() == Some("OR") {
            self.i += 1;
            v.push(self.and()?);
        }
        Ok(if v.len() == 1 {
            v.remove(0)
        } else {
            Node::Or(v)
        })
    }

    fn and(&mut self) -> Result<Node, String> {
        let mut v = vec![self.with()?];
        while self.peek() == Some("AND") {
            self.i += 1;
            v.push(self.with()?);
        }
        Ok(if v.len() == 1 {
            v.remove(0)
        } else {
            Node::And(v)
        })
    }

    fn with(&mut self) -> Result<Node, String> {
        let a = self.atom()?;
        if self.peek() == Some("WITH") {
            self.i += 1;
            let ex = self.next().ok_or("WITH without an exception")?;
            return match a {
                Node::Id(id) => Ok(Node::Id(format!("{id} WITH {ex}"))),
                _ => Err("WITH after a parenthesis".into()),
            };
        }
        Ok(a)
    }

    fn atom(&mut self) -> Result<Node, String> {
        match self.next().as_deref() {
            Some("(") => {
                let e = self.or()?;
                if self.next().as_deref() != Some(")") {
                    return Err("unbalanced parenthesis".into());
                }
                Ok(e)
            }
            Some(id) if !matches!(id, ")" | "AND" | "OR" | "WITH") => Ok(Node::Id(id.to_string())),
            other => Err(format!("unexpected {other:?}")),
        }
    }
}

fn rank(id: &str) -> usize {
    PREFERENCE
        .iter()
        .position(|p| *p == id)
        .unwrap_or(PREFERENCE.len())
}

fn choose(n: &Node, allowed: &[String]) -> Option<Vec<String>> {
    match n {
        Node::Id(id) => allowed.iter().any(|a| a == id).then(|| vec![id.clone()]),
        Node::And(v) => {
            let mut out = Vec::new();
            for c in v {
                out.extend(choose(c, allowed)?);
            }
            Some(out)
        }
        Node::Or(v) => v
            .iter()
            .filter_map(|c| choose(c, allowed))
            .min_by_key(|c| c.iter().map(|id| rank(id)).max().unwrap_or(0)),
    }
}

/// The licenses `expr` is taken under, every one of them in `allowed`;
/// an error when the expression does not parse or no allowed choice
/// satisfies it.
pub fn chosen(expr: &str, allowed: &[String]) -> Result<Vec<String>, String> {
    let mut p = Parser {
        t: tokens(expr),
        i: 0,
    };
    let n = p.or()?;
    if p.i != p.t.len() {
        return Err(format!("trailing tokens in {expr:?}"));
    }
    choose(&n, allowed).ok_or_else(|| format!("{expr:?} needs a license deny.toml does not allow"))
}

#[cfg(test)]
mod tests {
    use super::chosen;

    fn allow() -> Vec<String> {
        [
            "MIT",
            "Apache-2.0",
            "Unicode-3.0",
            "Apache-2.0 WITH LLVM-exception",
            "ISC",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect()
    }

    #[test]
    fn choices() {
        let a = allow();
        assert_eq!(chosen("MIT OR Apache-2.0", &a).unwrap(), ["MIT"]);
        assert_eq!(chosen("Apache-2.0 OR MIT", &a).unwrap(), ["MIT"]);
        assert_eq!(chosen("MIT/Apache-2.0", &a).unwrap(), ["MIT"]);
        assert_eq!(
            chosen("(MIT OR Apache-2.0) AND Unicode-3.0", &a).unwrap(),
            ["MIT", "Unicode-3.0"]
        );
        assert_eq!(
            chosen("Apache-2.0 WITH LLVM-exception", &a).unwrap(),
            ["Apache-2.0 WITH LLVM-exception"]
        );
        assert_eq!(
            chosen("MIT OR Apache-2.0 OR LGPL-2.1-or-later", &a).unwrap(),
            ["MIT"]
        );
        assert_eq!(
            chosen("Apache-2.0 AND ISC", &a).unwrap(),
            ["Apache-2.0", "ISC"]
        );
        assert!(chosen("GPL-3.0-only", &a).is_err());
        assert!(chosen("MIT AND GPL-2.0", &a).is_err());
        assert!(chosen("(MIT", &a).is_err());
    }
}
