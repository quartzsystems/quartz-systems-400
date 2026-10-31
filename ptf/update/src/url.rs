// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Service URLs: `https://host[:port][/path]` or `http://...`.

use std::fmt;

use crate::{Error, Result};

/// A parsed service URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    /// `https`.
    pub tls: bool,
    /// Host name or address.
    pub host: String,
    /// Port.
    pub port: u16,
    /// Path prefix, no trailing slash (`/qs400`, or empty).
    pub path: String,
}

impl Url {
    /// Parse a URL.
    pub fn parse(text: &str) -> Result<Self> {
        let t = text.trim();
        let (tls, rest) = if let Some(r) = t.strip_prefix("https://") {
            (true, r)
        } else if let Some(r) = t.strip_prefix("http://") {
            (false, r)
        } else {
            return Err(Error::Url(t.into()));
        };
        let (auth, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
        let (host, port) = host_port(auth, if tls { 443 } else { 80 }).ok_or_else(|| Error::Url(t.into()))?;
        Ok(Self {
            tls,
            host,
            port,
            path: path.trim_end_matches('/').to_string(),
        })
    }

    /// `host:port`.
    #[must_use]
    pub fn authority(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// The full path of `rel` (`/v1/catalog`) under this URL.
    #[must_use]
    pub fn path_of(&self, rel: &str) -> String {
        format!("{}{rel}", self.path)
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}://{}{}",
            if self.tls { "https" } else { "http" },
            self.authority(),
            self.path
        )
    }
}

/// `host[:port]` (`[v6]:port` too), with `default` for a missing port.
#[must_use]
pub fn host_port(text: &str, default: u16) -> Option<(String, u16)> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(r) = t.strip_prefix('[') {
        let (h, rest) = r.split_once(']')?;
        let port = match rest.strip_prefix(':') {
            Some(p) => p.parse().ok()?,
            None if rest.is_empty() => default,
            None => return None,
        };
        return Some((h.to_string(), port));
    }
    match t.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') => Some((h.to_string(), p.parse().ok()?)),
        _ => Some((t.to_string(), default)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_parse() {
        let u = Url::parse("https://updates.quartz.systems/qs400/").unwrap();
        assert!(u.tls);
        assert_eq!(u.port, 443);
        assert_eq!(u.path_of("/v1/catalog"), "/qs400/v1/catalog");
        let u = Url::parse("http://127.0.0.1:8080").unwrap();
        assert_eq!((u.host.as_str(), u.port, u.path.as_str()), ("127.0.0.1", 8080, ""));
        assert_eq!(u.to_string(), "http://127.0.0.1:8080");
        assert!(Url::parse("ftp://x").is_err());
        assert_eq!(host_port("[::1]:99", 1), Some(("::1".into(), 99)));
    }
}
