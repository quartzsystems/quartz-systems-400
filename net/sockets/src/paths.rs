// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `AF_UNIX` addresses live in the IFS namespace: a program names
//! `/tmp/app.sock`, the kernel binds a host path inside the IFS's
//! backing directory. [`IfsPathMap`] makes that translation both ways;
//! the IFS layer implements it over its resolver (authority checks and
//! the metadata record of the new socket file are its business).
//! [`RootedPaths`] maps under one host directory with no checks, for
//! tests and jobs without an IFS.

use crate::Errno;

/// Why a path is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnixUse {
    /// `bind`: the socket file will be created.
    Bind,
    /// `connect` or `sendto`: the socket file must exist.
    Connect,
}

/// The IFS path map.
pub trait IfsPathMap {
    /// The host path for an IFS path (absolute or relative to the job's
    /// current directory). `ENOENT`, `EACCES`, `ENAMETOOLONG` as the
    /// IFS answers.
    fn to_host(&self, ifs: &str, why: UnixUse) -> Result<String, Errno>;

    /// The IFS path of a host path the kernel reported (`accept`,
    /// `getsockname`, `recvfrom`); the host path itself when it lies
    /// outside the IFS.
    fn to_ifs(&self, host: &str) -> String;

    /// Told after a `bind` created the socket file.
    fn bound(&self, _ifs: &str) {}
}

/// IFS paths under one host directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootedPaths {
    /// The host directory `/` maps to; empty maps `/` to `/`.
    pub root: String,
    /// The job's current directory, for relative paths.
    pub cwd: String,
}

impl RootedPaths {
    /// Under `root`, current directory `/`.
    #[must_use]
    pub fn new(root: &str) -> Self {
        Self {
            root: root.trim_end_matches('/').to_string(),
            cwd: "/".to_string(),
        }
    }

    fn absolute(&self, p: &str) -> String {
        let joined = if p.starts_with('/') {
            p.to_string()
        } else {
            format!("{}/{p}", self.cwd.trim_end_matches('/'))
        };
        let mut parts: Vec<&str> = Vec::new();
        for c in joined.split('/') {
            match c {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                c => parts.push(c),
            }
        }
        format!("/{}", parts.join("/"))
    }
}

impl Default for RootedPaths {
    fn default() -> Self {
        Self::new("")
    }
}

impl IfsPathMap for RootedPaths {
    fn to_host(&self, ifs: &str, _why: UnixUse) -> Result<String, Errno> {
        if ifs.is_empty() {
            return Err(Errno::ENOENT);
        }
        Ok(format!("{}{}", self.root, self.absolute(ifs)))
    }

    fn to_ifs(&self, host: &str) -> String {
        if self.root.is_empty() {
            return host.to_string();
        }
        match host.strip_prefix(&self.root) {
            Some("") => "/".to_string(),
            Some(rest) if rest.starts_with('/') => rest.to_string(),
            _ => host.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rooted_paths_both_ways() {
        let mut m = RootedPaths::new("/qs400/ifs/");
        assert_eq!(m.to_host("/tmp/a.sock", UnixUse::Bind).unwrap(), "/qs400/ifs/tmp/a.sock");
        m.cwd = "/home/QSTEST".into();
        assert_eq!(m.to_host("s/../b.sock", UnixUse::Connect).unwrap(), "/qs400/ifs/home/QSTEST/b.sock");
        assert_eq!(m.to_host("/../../etc", UnixUse::Bind).unwrap(), "/qs400/ifs/etc", "never above the root");
        assert_eq!(m.to_ifs("/qs400/ifs/tmp/a.sock"), "/tmp/a.sock");
        assert_eq!(m.to_ifs("/elsewhere"), "/elsewhere");
        assert_eq!(m.to_host("", UnixUse::Bind), Err(Errno::ENOENT));
        let id = RootedPaths::default();
        assert_eq!(id.to_host("/tmp/x", UnixUse::Bind).unwrap(), "/tmp/x");
        assert_eq!(id.to_ifs("/tmp/x"), "/tmp/x");
    }
}
