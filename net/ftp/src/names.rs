// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Names: IFS paths for `NAMEFMT 1`, `LIB/FILE.MBR` for `NAMEFMT 0`,
//! both made into an absolute IFS path the session works with (a
//! library member is `/QSYS.LIB/LIB.LIB/FILE.FILE/MBR.MBR`).

use ifs::vfs::qsys::{QsysNode, node_of, path_of};

/// The library prefix of the IFS, upper case.
pub const QSYS_LIB: &str = "/QSYS.LIB";

/// `path` without `.`, `..` and repeated slashes; absolute, no
/// trailing slash but for the root.
pub fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for c in path.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            c => parts.push(c),
        }
    }
    if parts.is_empty() {
        "/".into()
    } else {
        format!("/{}", parts.join("/"))
    }
}

/// `arg` against the directory `cwd`: absolute as it is, relative
/// joined; `~` is the home directory.
pub fn join(cwd: &str, arg: &str, home: &str) -> String {
    let arg = arg.trim();
    if arg == "~" {
        return normalize(home);
    }
    if let Some(rest) = arg.strip_prefix("~/") {
        return normalize(&format!("{home}/{rest}"));
    }
    if arg.starts_with('/') {
        normalize(arg)
    } else {
        normalize(&format!("{cwd}/{arg}"))
    }
}

/// Whether `path` is in `/QSYS.LIB` (or is it).
pub fn in_qsys(path: &str) -> bool {
    let u = path.to_ascii_uppercase();
    u == QSYS_LIB || u.starts_with("/QSYS.LIB/")
}

/// The `/QSYS.LIB` node of a path, `None` when the path is elsewhere or
/// does not name one.
pub fn qsys_node(path: &str) -> Option<QsysNode> {
    if !in_qsys(path) {
        return None;
    }
    let rest = &path[QSYS_LIB.len()..];
    let comps: Vec<String> = rest.split('/').filter(|c| !c.is_empty()).map(str::to_string).collect();
    node_of(&comps).ok()
}

/// The path of a node.
pub fn qsys_path(node: &QsysNode) -> String {
    path_of(node)
}

/// The library a directory is, for `NAMEFMT 0`: `/QSYS.LIB` is `QSYS`.
pub fn library_of(dir: &str) -> Option<String> {
    match qsys_node(dir)? {
        QsysNode::Root => Some("QSYS".into()),
        QsysNode::Lib(l) => Some(l),
        _ => None,
    }
}

/// The directory of a library.
pub fn library_dir(lib: &str) -> String {
    if lib.eq_ignore_ascii_case("QSYS") {
        QSYS_LIB.into()
    } else {
        qsys_path(&QsysNode::Lib(lib.to_string()))
    }
}

/// A `NAMEFMT 0` name: `[LIB/]FILE[.MBR]`. Parts may be generic
/// (`ABC*`, `*`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibName {
    /// The library, when given.
    pub lib: Option<String>,
    /// The file (or library, for a bare name some commands take).
    pub file: String,
    /// The member, when given.
    pub mbr: Option<String>,
}

/// One part of a name: quoted as typed, else upper case.
fn part(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(q) = s.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        return (!q.is_empty() && q.chars().count() <= 10).then(|| q.to_string());
    }
    let ok = s.chars().count() <= 10
        && s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '$' | '#' | '@' | '_' | '*'));
    ok.then(|| s.to_ascii_uppercase())
}

/// Split at the first `.` outside quotes.
fn split_dot(s: &str) -> (&str, Option<&str>) {
    let mut quoted = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '.' if !quoted => return (&s[..i], Some(&s[i + 1..])),
            _ => {}
        }
    }
    (s, None)
}

/// Parse a `NAMEFMT 0` name; `None` when it is not one.
pub fn parse_lib_name(arg: &str) -> Option<LibName> {
    let arg = arg.trim();
    if arg.is_empty() || arg.starts_with('/') {
        return None;
    }
    let (lib, rest) = match arg.split_once('/') {
        Some((l, r)) => (Some(part(l)?), r),
        None => (None, arg),
    };
    let (f, m) = split_dot(rest);
    let file = part(f)?;
    let mbr = match m {
        Some(m) => Some(part(m)?),
        None => None,
    };
    Some(LibName { lib, file, mbr })
}

/// Whether a name part is generic.
pub fn generic(s: &str) -> bool {
    s.contains('*')
}

/// Whether `name` fits a generic `pattern` (`*` anywhere, any case).
pub fn matches(pattern: &str, name: &str) -> bool {
    fn go(p: &[u8], n: &[u8]) -> bool {
        match p.split_first() {
            None => n.is_empty(),
            Some((b'*', rest)) => (0..=n.len()).any(|i| go(rest, &n[i..])),
            Some((c, rest)) => n
                .split_first()
                .is_some_and(|(d, nr)| c.eq_ignore_ascii_case(d) && go(rest, nr)),
        }
    }
    go(pattern.as_bytes(), name.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        assert_eq!(normalize("/a//b/./c/../d/"), "/a/b/d");
        assert_eq!(normalize("/.."), "/");
        assert_eq!(join("/home/x", "y", "/home/x"), "/home/x/y");
        assert_eq!(join("/home/x", "../..", "/home/x"), "/");
        assert_eq!(join("/home/x", "~/q", "/home/x"), "/home/x/q");
        assert!(in_qsys("/qsys.lib/qgpl.lib"));
        assert!(!in_qsys("/QSYS.LIBX"));
    }

    #[test]
    fn qsys_paths() {
        assert_eq!(library_of("/QSYS.LIB"), Some("QSYS".into()));
        assert_eq!(library_of("/qsys.lib/qgpl.lib"), Some("QGPL".into()));
        assert_eq!(library_of("/home"), None);
        assert_eq!(library_dir("QGPL"), "/QSYS.LIB/QGPL.LIB");
        assert_eq!(
            qsys_node("/QSYS.LIB/A.LIB/F.FILE/M.MBR"),
            Some(QsysNode::Mbr("A".into(), "F".into(), "M".into()))
        );
    }

    #[test]
    fn lib_names() {
        let n = parse_lib_name("qgpl/qclsrc.hello").unwrap();
        assert_eq!(n.lib.as_deref(), Some("QGPL"));
        assert_eq!(n.file, "QCLSRC");
        assert_eq!(n.mbr.as_deref(), Some("HELLO"));
        let n = parse_lib_name("F").unwrap();
        assert_eq!((n.lib, n.mbr), (None, None));
        assert!(parse_lib_name("/home").is_none());
        assert!(parse_lib_name("TOOLONGNAME1").is_none());
        assert_eq!(parse_lib_name("F.*").unwrap().mbr.as_deref(), Some("*"));
        assert!(matches("QSF*", "qsfsrc") && !matches("QSF*", "QXF"));
        assert!(matches("*", "") && matches("A*B", "AXXB"));
    }
}
