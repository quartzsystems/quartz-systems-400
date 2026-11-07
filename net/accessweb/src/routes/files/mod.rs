// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Files (handoff steps 13-14), the desktop's Integrated File System
//! window: `/files?path=&pat=&sort=&page=&rows=` browses a directory,
//! `/files/props`, `/files/edit`, `/files/shares`; the operations
//! (`ops`) and the transfers (`transfer`). Everything runs as the
//! signed-on user through [`crate::env::files::Files`]; the policies
//! `files.*` come first.

pub mod browse;
pub mod edit;
pub mod ops;
pub mod props;
pub mod shares;
pub mod transfer;

use qsweb::html::query;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};

use super::Rq;
use crate::policy;

/// The tree group.
pub const GROUP: &str = "Files";
/// Browse files.
pub const BROWSE: &str = "/files";
/// My files.
pub const SHARES: &str = "/files/shares";
/// The session value holding the cut or copied paths: `cut` or `copy`,
/// then one path a line.
pub const CLIP: &str = "files.clip";

/// Route the area's first path segments.
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    if let Err(r) = rq.policy(policy::FILES_BROWSE, &[GROUP]) {
        return r;
    }
    match segs {
        ["files"] => browse::page(rq),
        ["files", "props"] => props::page(rq),
        ["files", "shares"] => shares::page(rq),
        ["files", "edit"] => edit::page(rq),
        ["files", "download"] => transfer::download(rq, false),
        ["files", "open"] => transfer::download(rq, true),
        ["files", "queue"] => transfer::queue_op(rq),
        ["files", "rename"] => ops::rename(rq),
        ["files", "mkdir"] => ops::mkdir(rq),
        ["files", op @ ("cut" | "copy")] => ops::clip(rq, op),
        ["files", "confirm"] => ops::delete(rq),
        _ => rq.not_found(&segs.join("/")),
    }
}

/// `path` with `.` and `..` resolved, one slash between parts.
pub fn clean(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    format!("/{}", parts.join("/"))
}

/// The directory holding `path`.
pub fn parent(path: &str) -> String {
    match path.rfind('/') {
        Some(0) | None => "/".into(),
        Some(i) => path[..i].to_string(),
    }
}

/// The last part of `path`.
pub fn name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `dir/name`.
pub fn join(dir: &str, name: &str) -> String {
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// Whether `name` may name an entry of a directory.
pub fn good_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\0')
        && name.len() <= 255
}

/// Whether `path` is `dir` or under it.
pub fn within(path: &str, dir: &str) -> bool {
    path == dir || path.starts_with(&format!("{}/", dir.trim_end_matches('/')))
}

/// The path a request names (`?path=`), the user's home directory when
/// it names none.
pub fn path_arg(rq: &Rq<'_>) -> String {
    let q = rq.query();
    match q.get("path").map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => clean(p),
        None => rq.env().files.home(rq.user()),
    }
}

/// Policy `files.outside` for a path outside the home directory.
pub fn may_reach(rq: &Rq<'_>, path: &str, crumb: &[&str]) -> Result<(), Response> {
    let home = rq.env().files.home(rq.user());
    if within(path, &home) {
        return Ok(());
    }
    rq.policy(policy::FILES_OUTSIDE, crumb)
}

/// The route of directory `dir` in Browse files.
pub fn browse_route(dir: &str) -> String {
    format!("{BROWSE}{}", query(&[("path", dir)]))
}

/// `route` with `?t=` the session's form token, for the links that
/// change something (they need the token as a form does).
pub fn tokened(rq: &Rq<'_>, route: &str, pairs: &[(&str, &str)]) -> String {
    let mut q: Vec<(&str, &str)> = pairs.to_vec();
    q.push(("t", &rq.session.csrf));
    format!("{route}{}", query(&q))
}

/// Whether a link carried the session's token.
pub fn token_ok(rq: &Rq<'_>) -> bool {
    qsweb::session::csrf_ok(&rq.session, rq.query().str("t"))
}

/// Back to directory `dir` with `m` waiting there.
pub fn back(rq: &Rq<'_>, dir: &str, m: Message) -> Response {
    rq.done(&browse_route(dir), m)
}

/// A refused link: the token was missing.
pub fn stale(rq: &Rq<'_>) -> Response {
    let _ = rq;
    qsweb::http::plain(
        403,
        "The link is out of date. Go back, reload the page and try again.",
    )
}

/// A message row of the system's error.
pub fn error(rq: &Rq<'_>, m: &Message) -> Message {
    let data: Vec<&str> = m.data.iter().map(String::as_str).collect();
    let mut r = rq.message(Kind::Error, &m.id, &data);
    if r.text.is_empty() {
        r.text.clone_from(&m.text);
    }
    r
}

/// Whether `name` matches `pattern` (`*` any run, `?` one character),
/// case-blind as the root file system is.
pub fn matches(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    let (mut pi, mut ni, mut star, mut mark) = (0, 0, None, 0);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ni;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

/// `rwxr-x---` of permission bits.
pub fn rwx(mode: u32) -> String {
    let mut s = String::with_capacity(9);
    for shift in [6, 3, 0] {
        let b = (mode >> shift) & 7;
        s.push(if b & 4 != 0 { 'r' } else { '-' });
        s.push(if b & 2 != 0 { 'w' } else { '-' });
        s.push(if b & 1 != 0 { 'x' } else { '-' });
    }
    s
}

/// Permission bits of `rwxr-x---`, `None` when it is not nine of them.
pub fn mode_of(s: &str) -> Option<u32> {
    let b = s.trim().as_bytes();
    if b.len() != 9 {
        return None;
    }
    let mut m = 0;
    for (i, c) in b.iter().enumerate() {
        let want = b"rwx"[i % 3];
        m <<= 1;
        match *c {
            c if c == want => m |= 1,
            b'-' => {}
            _ => return None,
        }
    }
    Some(m)
}

/// Whether bytes are text: no NUL in the first 8 KiB.
pub fn looks_text(head: &[u8]) -> bool {
    !head.iter().take(8192).any(|b| *b == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_patterns() {
        assert_eq!(clean("/home//CWELLMAN/./x/../y/"), "/home/CWELLMAN/y");
        assert_eq!(clean("/.."), "/");
        assert_eq!(parent("/home"), "/");
        assert_eq!(parent("/home/CWELLMAN"), "/home");
        assert_eq!(join("/", "home"), "/home");
        assert!(
            within("/home/CWELLMAN/x", "/home/CWELLMAN")
                && !within("/home/CWELLMANX", "/home/CWELLMAN")
        );
        assert!(matches("*.CSV", "assets_br1.csv") && matches("a?sets*", "assets_br1.csv"));
        assert!(!matches("*.txt", "notes.txt.bak") && matches("*", ".profile"));
        assert_eq!(rwx(0o750), "rwxr-x---");
        assert_eq!(mode_of("rw-r-----"), Some(0o640));
        assert_eq!(mode_of("rw-r--"), None);
        assert_eq!(mode_of("rwq------"), None);
        assert!(!good_name("a/b") && !good_name("..") && good_name("new folder"));
    }
}
