// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `http://` and `https://` URLs.

use crate::{Error, Result};

/// A URL split for a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Url {
    /// `https`.
    pub tls: bool,
    /// The host, without brackets for IPv6.
    pub host: String,
    /// The port.
    pub port: u16,
    /// The path and query, `/` at least.
    pub target: String,
    /// `user:password` before the host, if any.
    pub userinfo: Option<String>,
}

impl Url {
    /// Parse `text`.
    pub fn parse(text: &str) -> Result<Self> {
        let bad = || Error::Url(text.to_string());
        let t = text.trim();
        let (tls, rest) = if let Some(r) = strip_ci(t, "https://") {
            (true, r)
        } else if let Some(r) = strip_ci(t, "http://") {
            (false, r)
        } else {
            return Err(bad());
        };
        let (auth, target) = match rest.find(['/', '?']) {
            Some(i) => (&rest[..i], rest[i..].to_string()),
            None => (rest, "/".to_string()),
        };
        let target = if target.starts_with('?') { format!("/{target}") } else { target };
        let target = target.split('#').next().unwrap_or("/").to_string();
        let (userinfo, hostport) = match auth.rsplit_once('@') {
            Some((u, h)) => (Some(u.to_string()), h),
            None => (None, auth),
        };
        let default = if tls { 443 } else { 80 };
        let (host, port) = if let Some(r) = hostport.strip_prefix('[') {
            let (h, after) = r.split_once(']').ok_or_else(bad)?;
            let port = match after.strip_prefix(':') {
                Some(p) => p.parse().map_err(|_| bad())?,
                None if after.is_empty() => default,
                None => return Err(bad()),
            };
            (h.to_string(), port)
        } else {
            match hostport.rsplit_once(':') {
                Some((h, p)) => (h.to_string(), p.parse().map_err(|_| bad())?),
                None => (hostport.to_string(), default),
            }
        };
        if host.is_empty() {
            return Err(bad());
        }
        Ok(Self { tls, host, port, target, userinfo })
    }

    /// `host[:port]` as the `Host` header has it.
    #[must_use]
    pub fn authority(&self) -> String {
        let h = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        if self.port == if self.tls { 443 } else { 80 } { h } else { format!("{h}:{}", self.port) }
    }

    /// `location` read against this URL.
    pub fn join(&self, location: &str) -> Result<Self> {
        if strip_ci(location, "http://").is_some() || strip_ci(location, "https://").is_some() {
            return Self::parse(location);
        }
        let mut u = self.clone();
        u.target = if location.starts_with('/') {
            location.to_string()
        } else {
            let base = self.target.split('?').next().unwrap_or("/");
            let dir = &base[..=base.rfind('/').unwrap_or(0)];
            format!("{dir}{location}")
        };
        Ok(u)
    }
}

fn strip_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    (s.len() >= prefix.len() && s.is_char_boundary(prefix.len()) && s[..prefix.len()].eq_ignore_ascii_case(prefix))
        .then(|| &s[prefix.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        let u = Url::parse("https://example.com/a/b?x=1#f").unwrap();
        assert_eq!((u.tls, u.host.as_str(), u.port, u.target.as_str()), (true, "example.com", 443, "/a/b?x=1"));
        let u = Url::parse("http://u:p@[::1]:8080").unwrap();
        assert_eq!((u.host.as_str(), u.port, u.target.as_str(), u.userinfo.as_deref()), ("::1", 8080, "/", Some("u:p")));
        assert_eq!(u.authority(), "[::1]:8080");
        assert!(Url::parse("ftp://x").is_err());
        let j = Url::parse("http://h/a/b").unwrap().join("c?d").unwrap();
        assert_eq!(j.target, "/a/c?d");
    }
}
