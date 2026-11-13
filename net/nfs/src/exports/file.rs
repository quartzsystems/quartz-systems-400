// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/etc/exports` (docs/NFS.md section 6.1): one export a logical line,
//!
//! ```text
//! path [-option[,option]...] [HOSTOPT host[:data[:path[:write]]]]...
//! ```
//!
//! The path is absolute, in double quotes when it holds blanks. The
//! options are those of `EXPORTFS -O` and may have blanks after their
//! commas. Each `HOSTOPT` gives one host's options: the data CCSID
//! (`*BINARY`, `*ASCII`, `*JOBCCSID` or a number), the path name CCSID
//! (`*ASCII`, `*JOBCCSID` or a number) and the write mode (`*SYNC`,
//! `*ASYNC`); the ones left out take their defaults.

use std::fmt::Write as _;

use super::error::EntryError;
use super::lines;
use super::model::{DataCcsid, Export, HostOpt, HostSpec, PathCcsid};
use super::options;

/// The most host options an entry has, as `EXPORTFS HOSTOPT`.
pub const MAX_HOSTOPT: usize = 10;

/// The entries of the file, each parsed or refused with its line.
#[must_use]
pub fn parse(text: &str) -> Vec<Result<Export, EntryError>> {
    lines::logical(text)
        .into_iter()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(line, l)| entry(line, &l))
        .collect()
}

/// One logical line.
pub fn entry(line: usize, text: &str) -> Result<Export, EntryError> {
    let fail = |path: &str, reason: u32, detail: String| EntryError {
        line,
        path: path.to_string(),
        reason,
        detail,
    };
    let text = text.trim();
    let (path, rest) = if let Some(q) = text.strip_prefix('"') {
        let end = q
            .find('"')
            .ok_or_else(|| fail("", 3, "the path's quote is not closed".into()))?;
        (q[..end].to_string(), &q[end + 1..])
    } else {
        let end = text.find(char::is_whitespace).unwrap_or(text.len());
        (text[..end].to_string(), &text[end..])
    };
    if !path.starts_with('/') || path.contains('\0') {
        return Err(fail(&path, 3, "the path is not absolute".into()));
    }
    let path = normal(&path);
    let mut e = Export::new(&path);
    // The option list runs to the first HOSTOPT.
    let words: Vec<&str> = rest.split_whitespace().collect();
    let first_host = words
        .iter()
        .position(|w| w.eq_ignore_ascii_case("HOSTOPT"))
        .unwrap_or(words.len());
    let opts = words[..first_host].join(" ");
    if !opts.is_empty() {
        let list = opts
            .strip_prefix('-')
            .ok_or_else(|| fail(&path, 3, format!("{opts} does not start with -")))?;
        e.options = options::parse(list).map_err(|o| fail(&path, o.reason(), o.to_string()))?;
    }
    let mut i = first_host;
    while i < words.len() {
        if !words[i].eq_ignore_ascii_case("HOSTOPT") {
            return Err(fail(&path, 3, format!("{} where HOSTOPT goes", words[i])));
        }
        let spec = words
            .get(i + 1)
            .ok_or_else(|| fail(&path, 3, "HOSTOPT names no host".into()))?;
        e.hosts
            .push(host_opt(spec).ok_or_else(|| fail(&path, 9, format!("HOSTOPT {spec}")))?);
        i += 2;
    }
    if e.hosts.len() > MAX_HOSTOPT {
        return Err(fail(&path, 3, "more than 10 HOSTOPT".into()));
    }
    Ok(e)
}

/// A path without its trailing slashes (`/` stays).
#[must_use]
pub fn normal(path: &str) -> String {
    let t = path.trim_end_matches('/');
    if t.is_empty() { "/".into() } else { t.into() }
}

/// `host[:data[:path[:write]]]`.
fn host_opt(spec: &str) -> Option<HostOpt> {
    let (host, rest) = if spec.starts_with('[') || spec.starts_with("@[") {
        let close = spec.find(']')?;
        let end = spec[close..].find(':').map_or(spec.len(), |i| close + i);
        (&spec[..end], spec.get(end + 1..).unwrap_or(""))
    } else {
        spec.split_once(':').unwrap_or((spec, ""))
    };
    let mut h = HostOpt::new(options::host(host)?);
    let parts: Vec<&str> = if rest.is_empty() {
        Vec::new()
    } else {
        rest.split(':').collect()
    };
    if parts.len() > 3 {
        return None;
    }
    if let Some(d) = parts.first() {
        h.data = match d.to_ascii_uppercase().as_str() {
            "*BINARY" => DataCcsid::Binary,
            "*ASCII" => DataCcsid::Ascii,
            "*JOBCCSID" => DataCcsid::Job,
            n => DataCcsid::Ccsid(n.parse().ok()?),
        };
    }
    if let Some(p) = parts.get(1) {
        h.path = match p.to_ascii_uppercase().as_str() {
            "*ASCII" => PathCcsid::Ascii,
            "*JOBCCSID" => PathCcsid::Job,
            n => PathCcsid::Ccsid(n.parse().ok()?),
        };
    }
    if let Some(w) = parts.get(2) {
        h.sync = match w.to_ascii_uppercase().as_str() {
            "*SYNC" => true,
            "*ASYNC" => false,
            _ => return None,
        };
    }
    Some(h)
}

/// One entry as a line.
#[must_use]
pub fn print_entry(e: &Export) -> String {
    let mut s = if e.path.contains(char::is_whitespace) || e.path.contains('#') {
        format!("\"{}\"", e.path)
    } else {
        e.path.clone()
    };
    let o = options::print(&e.options);
    if !o.is_empty() {
        let _ = write!(s, " -{o}");
    }
    for h in &e.hosts {
        let host = options::print_hosts(std::slice::from_ref(&h.host));
        let data = match h.data {
            DataCcsid::Binary => "*BINARY".to_string(),
            DataCcsid::Ascii => "*ASCII".to_string(),
            DataCcsid::Job => "*JOBCCSID".to_string(),
            DataCcsid::Ccsid(n) => n.to_string(),
        };
        let path = match h.path {
            PathCcsid::Ascii => "*ASCII".to_string(),
            PathCcsid::Job => "*JOBCCSID".to_string(),
            PathCcsid::Ccsid(n) => n.to_string(),
        };
        let write = if h.sync { "*SYNC" } else { "*ASYNC" };
        let _ = write!(s, " HOSTOPT {host}:{data}:{path}:{write}");
    }
    s
}

/// Whether two export paths are the same directory (the root file
/// system compares names without case).
#[must_use]
pub fn same_path(a: &str, b: &str) -> bool {
    normal(a).eq_ignore_ascii_case(&normal(b))
}

/// The file with `e`'s entry put in: the line of the same path
/// replaced, or a line added at the end. Comments and other lines are
/// kept.
#[must_use]
pub fn upsert(text: &str, e: &Export) -> String {
    edit(text, &e.path, Some(&print_entry(e)))
}

/// The file without the entry of `path`.
#[must_use]
pub fn remove(text: &str, path: &str) -> String {
    edit(text, path, None)
}

fn edit(text: &str, path: &str, with: Option<&str>) -> String {
    let mut out = String::new();
    let mut done = false;
    let raw: Vec<&str> = text.lines().collect();
    let logical = lines::logical(text);
    for (i, (line, l)) in logical.iter().enumerate() {
        // The physical lines of a logical line run to the next one's.
        let end = logical.get(i + 1).map_or(raw.len(), |(n, _)| n - 1);
        let is_it = first_word_is(l, path);
        if is_it && !done {
            if let Some(w) = with {
                out.push_str(w);
                out.push('\n');
            }
            done = true;
        } else if !is_it {
            for r in &raw[line - 1..end] {
                out.push_str(r);
                out.push('\n');
            }
        }
    }
    if !done && let Some(w) = with {
        out.push_str(w);
        out.push('\n');
    }
    out
}

fn first_word_is(logical: &str, path: &str) -> bool {
    let t = logical.trim();
    let p = t
        .strip_prefix('"')
        .and_then(|q| q.split('"').next())
        .unwrap_or_else(|| t.split_whitespace().next().unwrap_or(""));
    !p.is_empty() && same_path(p, path)
}

/// The entry for `path` among parsed entries.
#[must_use]
pub fn find<'a>(
    entries: &'a [Result<Export, EntryError>],
    path: &str,
) -> Option<&'a Result<Export, EntryError>> {
    entries.iter().find(|e| match e {
        Ok(x) => same_path(&x.path, path),
        Err(x) => !x.path.is_empty() && same_path(&x.path, path),
    })
}

/// Hosts of an entry's lists, for a check that every name is known.
#[must_use]
pub fn names_of(e: &Export) -> Vec<&str> {
    let o = &e.options;
    o.rw.iter()
        .flatten()
        .chain(o.access.iter().flatten())
        .chain(&o.root_hosts)
        .chain(e.hosts.iter().map(|h| &h.host))
        .filter_map(|h| match h {
            HostSpec::Name(n) => Some(n.as_str()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "# QS/400 exports\n/home/joe -ro, access=alpha:beta HOSTOPT alpha:819:*ASCII:*ASYNC\n\n\"/home/my data\" -rw=alpha \\\n   ,anon=-1\n/QSYS.LIB/MYLIB.LIB\nrelative -ro\n/bad -bogus\n";

    #[test]
    fn reads_entries() {
        let v = parse(FILE);
        assert_eq!(v.len(), 5);
        let joe = v[0].as_ref().unwrap();
        assert_eq!(joe.path, "/home/joe");
        assert!(joe.options.ro);
        assert_eq!(joe.hosts.len(), 1);
        assert_eq!(joe.hosts[0].data, DataCcsid::Ccsid(819));
        assert!(!joe.hosts[0].sync);
        let my = v[1].as_ref().unwrap();
        assert_eq!(my.path, "/home/my data");
        assert_eq!(my.options.anon, Some(-1));
        assert_eq!(v[2].as_ref().unwrap(), &Export::new("/QSYS.LIB/MYLIB.LIB"));
        assert_eq!(v[3].as_ref().unwrap_err().reason, 3);
        let bad = v[4].as_ref().unwrap_err();
        assert_eq!((bad.line, bad.reason, bad.path.as_str()), (8, 11, "/bad"));
        assert!(find(&v, "/HOME/JOE/").is_some());
        assert!(find(&v, "/bad").is_some_and(Result::is_err));
    }

    #[test]
    fn prints_what_it_reads() {
        for e in parse(FILE).into_iter().flatten() {
            assert_eq!(entry(1, &print_entry(&e)).unwrap(), e);
        }
        assert_eq!(
            print_entry(&parse(FILE)[0].clone().unwrap()),
            "/home/joe -ro,access=alpha:beta HOSTOPT alpha:819:*ASCII:*ASYNC"
        );
    }

    #[test]
    fn edits_keep_the_rest() {
        let mut e = Export::new("/home/my data");
        e.options.ro = true;
        let t = upsert(FILE, &e);
        assert!(t.starts_with("# QS/400 exports\n/home/joe"));
        assert!(t.contains("\"/home/my data\" -ro\n"));
        assert!(!t.contains("anon=-1"));
        assert!(t.contains("/QSYS.LIB/MYLIB.LIB\n"));
        let t = remove(&t, "/home/joe");
        assert!(!t.contains("/home/joe"));
        let t = upsert(&t, &Export::new("/new"));
        assert!(t.ends_with("/new\n"));
        // A refused entry can be removed too.
        assert!(!remove(FILE, "/bad").contains("/bad"));
    }

    #[test]
    fn host_options() {
        let e = entry(1, "/a HOSTOPT @10.0.0.0/8 HOSTOPT [fd00::1]:*BINARY:850").unwrap();
        assert_eq!(e.hosts.len(), 2);
        assert_eq!(
            e.hosts[0],
            HostOpt::new(options::host("@10.0.0.0/8").unwrap())
        );
        assert_eq!(e.hosts[1].path, PathCcsid::Ccsid(850));
        assert_eq!(entry(1, "/a HOSTOPT").unwrap_err().reason, 3);
        assert_eq!(entry(1, "/a HOSTOPT h:*X").unwrap_err().reason, 9);
        assert_eq!(entry(1, "/a -ro extra").unwrap_err().reason, 11);
    }
}
