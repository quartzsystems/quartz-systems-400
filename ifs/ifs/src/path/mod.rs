// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Path names: parsing, IBM's limits, components, and lexical joining
//! for display. Names are UTF-8 (the root's encoding); `.` and `..`
//! are kept for the resolver, which gives `..` its physical meaning.

pub mod fold;

use crate::errno::{Errno, Result};

/// Longest name in root and `/QOpenSys`, in bytes of UTF-8 (IBM: 255
/// characters; QS/400 stores UTF-8 on xfs, whose limit is 255 bytes).
pub const NAME_MAX: usize = 255;
/// Longest path the APIs accept (IBM: 16 megabytes).
pub const PATH_MAX: usize = 16 * 1024 * 1024;
/// Symbolic links one resolution may follow before `ELOOP`.
pub const SYMLOOP_MAX: usize = 40;
/// Most hard links to one object.
pub const LINK_MAX: u32 = 1_000_000;

/// One component of a parsed path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comp {
    /// `..`.
    Parent,
    /// A name (never empty, `.` or `..`).
    Name(String),
}

/// A parsed path: absolute or relative, components without `.` or
/// empty ones, and whether it ended in a slash (the final component
/// must then be a directory).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    /// Starts at `/`.
    pub absolute: bool,
    /// The components.
    pub comps: Vec<Comp>,
    /// Ended in `/`.
    pub trailing_slash: bool,
}

/// Parse a path: an empty one is `ENOENT`, a NUL `EINVAL`, a long name
/// or path `ENAMETOOLONG`.
pub fn parse(path: &str) -> Result<Parsed> {
    if path.is_empty() {
        return Err(Errno::ENOENT);
    }
    if path.len() > PATH_MAX {
        return Err(Errno::ENAMETOOLONG);
    }
    if path.contains('\0') {
        return Err(Errno::EINVAL);
    }
    let mut comps = Vec::new();
    for c in path.split('/') {
        match c {
            "" | "." => {}
            ".." => comps.push(Comp::Parent),
            n => {
                if n.len() > NAME_MAX {
                    return Err(Errno::ENAMETOOLONG);
                }
                comps.push(Comp::Name(n.to_string()));
            }
        }
    }
    Ok(Parsed {
        absolute: path.starts_with('/'),
        comps,
        trailing_slash: path.len() > 1 && (path.ends_with('/') || path.ends_with("/.")),
    })
}

/// A path's last name and the path of its parent, as text: `/a/b/c`
/// gives (`/a/b`, `c`). `None` for `/`, and for a path whose last
/// component is `..` or `.`.
#[must_use]
pub fn split_last(path: &str) -> Option<(String, String)> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let (dir, name) = match trimmed.rfind('/') {
        Some(0) => ("/".to_string(), &trimmed[1..]),
        Some(i) => (trimmed[..i].to_string(), &trimmed[i + 1..]),
        None => (".".to_string(), trimmed),
    };
    if name == "." || name == ".." {
        return None;
    }
    Some((dir, name.to_string()))
}

/// Components of a canonical absolute path (`/a/b` gives `a`, `b`).
#[must_use]
pub fn components(abs: &str) -> Vec<String> {
    abs.split('/')
        .filter(|c| !c.is_empty())
        .map(str::to_string)
        .collect()
}

/// The canonical text of absolute components.
#[must_use]
pub fn to_text(comps: &[String]) -> String {
    if comps.is_empty() {
        return "/".into();
    }
    let mut s = String::new();
    for c in comps {
        s.push('/');
        s.push_str(c);
    }
    s
}

/// Join `path` to the directory `cwd` lexically, folding `.` and `..`
/// (for display and patterns; resolution does `..` physically).
#[must_use]
pub fn join_lexical(cwd: &str, path: &str) -> String {
    let mut out = if path.starts_with('/') {
        Vec::new()
    } else {
        components(cwd)
    };
    for c in path.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            n => out.push(n.to_string()),
        }
    }
    to_text(&out)
}

/// Whether `name` is acceptable as one name: not empty, no `/`, no
/// NUL, not `.` or `..`, within [`NAME_MAX`].
pub fn check_name(name: &str) -> Result<()> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\0']) {
        return Err(Errno::EINVAL);
    }
    if name.len() > NAME_MAX {
        return Err(Errno::ENAMETOOLONG);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing() {
        let p = parse("/a//./b/../c/").unwrap();
        assert!(p.absolute && p.trailing_slash);
        assert_eq!(
            p.comps,
            vec![
                Comp::Name("a".into()),
                Comp::Name("b".into()),
                Comp::Parent,
                Comp::Name("c".into())
            ]
        );
        let p = parse("x").unwrap();
        assert!(!p.absolute && !p.trailing_slash);
        assert_eq!(parse("/").unwrap().comps, vec![]);
        assert_eq!(parse(""), Err(Errno::ENOENT));
        assert_eq!(parse("a\0b"), Err(Errno::EINVAL));
        assert_eq!(parse(&"n".repeat(256)), Err(Errno::ENAMETOOLONG));
        assert!(parse(&"n".repeat(255)).is_ok());
    }

    #[test]
    fn splitting_and_joining() {
        assert_eq!(split_last("/a/b/c"), Some(("/a/b".into(), "c".into())));
        assert_eq!(split_last("/c/"), Some(("/".into(), "c".into())));
        assert_eq!(split_last("c"), Some((".".into(), "c".into())));
        assert_eq!(split_last("/"), None);
        assert_eq!(split_last("/a/.."), None);
        assert_eq!(join_lexical("/home/bob", "../x/./y"), "/home/x/y");
        assert_eq!(join_lexical("/home", "/tmp/.."), "/");
        assert_eq!(to_text(&components("/a/b/")), "/a/b");
        assert!(check_name("ok.txt").is_ok());
        assert_eq!(check_name("a/b"), Err(Errno::EINVAL));
    }
}
