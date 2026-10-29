// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! POSIX option parsing: clustered single-letter options, option
//! arguments attached or following, `--` and the first operand ending
//! the options.

use std::fmt;

/// The options found and the operands after them.
#[derive(Debug, Default, Clone)]
pub struct Parsed {
    /// Each option in order, with its argument when it takes one.
    pub opts: Vec<(char, Option<String>)>,
    /// The operands.
    pub operands: Vec<String>,
}

impl Parsed {
    /// Was the option given?
    pub fn has(&self, c: char) -> bool {
        self.opts.iter().any(|(o, _)| *o == c)
    }

    /// The last argument given for an option.
    pub fn value(&self, c: char) -> Option<&str> {
        self.opts
            .iter()
            .rev()
            .find(|(o, _)| *o == c)
            .and_then(|(_, v)| v.as_deref())
    }

    /// Every argument given for an option, in order.
    pub fn values(&self, c: char) -> Vec<&str> {
        self.opts
            .iter()
            .filter(|(o, _)| *o == c)
            .filter_map(|(_, v)| v.as_deref())
            .collect()
    }

    /// Which of two exclusive options came last.
    pub fn last_of(&self, a: char, b: char) -> Option<char> {
        self.opts
            .iter()
            .rev()
            .find(|(o, _)| *o == a || *o == b)
            .map(|(o, _)| *o)
    }
}

/// Why the options could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptError {
    /// Not an option of this utility.
    Unknown(char),
    /// The option needs an argument.
    Missing(char),
}

impl fmt::Display for OptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(c) => write!(f, "Option -{c} is not valid."),
            Self::Missing(c) => write!(f, "Option -{c} needs an argument."),
        }
    }
}

/// Parse `args` (without `argv[0]`) against `spec`, where a letter
/// followed by `:` takes an argument.
pub fn getopt(args: &[String], spec: &str) -> Result<Parsed, OptError> {
    getopt_from(args, spec, &|_| false)
}

/// [`getopt`] where `numeric(arg)` says an argument such as `-5` is an
/// operand-like option kept whole (`head -5`, `uniq +2`).
pub fn getopt_from(
    args: &[String],
    spec: &str,
    special: &dyn Fn(&str) -> bool,
) -> Result<Parsed, OptError> {
    let takes = |c: char| {
        spec.char_indices()
            .any(|(i, s)| s == c && spec[i + 1..].starts_with(':'))
    };
    let known = |c: char| c != ':' && spec.contains(c);
    let mut p = Parsed::default();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        i += 1;
        if a == "--" {
            break;
        }
        if special(a) {
            p.opts.push(('\u{0}', Some(a.clone())));
            continue;
        }
        if a.len() < 2 || !a.starts_with('-') {
            i -= 1;
            break;
        }
        let chars: Vec<char> = a[1..].chars().collect();
        let mut j = 0;
        while j < chars.len() {
            let c = chars[j];
            j += 1;
            if !known(c) {
                return Err(OptError::Unknown(c));
            }
            if takes(c) {
                let rest: String = chars[j..].iter().collect();
                let v = if rest.is_empty() {
                    if i >= args.len() {
                        return Err(OptError::Missing(c));
                    }
                    i += 1;
                    args[i - 1].clone()
                } else {
                    rest
                };
                p.opts.push((c, Some(v)));
                break;
            }
            p.opts.push((c, None));
        }
    }
    p.operands = args[i..].to_vec();
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn clusters_and_arguments() {
        let p = getopt(&v(&["-ln", "-tx", "-f", "a", "--", "-b"]), "lnt:f:").unwrap();
        assert!(p.has('l') && p.has('n'));
        assert_eq!(p.value('t'), Some("x"));
        assert_eq!(p.value('f'), Some("a"));
        assert_eq!(p.operands, v(&["-b"]));
        assert_eq!(getopt(&v(&["-z"]), "a").unwrap_err(), OptError::Unknown('z'));
        assert_eq!(getopt(&v(&["-t"]), "t:").unwrap_err(), OptError::Missing('t'));
        let p = getopt(&v(&["a", "-l"]), "l").unwrap();
        assert_eq!(p.operands, v(&["a", "-l"]));
        let p = getopt(&v(&["-"]), "l").unwrap();
        assert_eq!(p.operands, v(&["-"]));
    }
}
