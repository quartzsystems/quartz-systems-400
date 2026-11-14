// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! LDAP URLs (RFC 4516): `ldap://host:port/dn?attrs?scope?filter?exts`,
//! with `ldaps` and `ldapi` taken too. Components are kept
//! percent-decoded; printing encodes them again.

use std::fmt;

use crate::filter::{Filter, FilterError};
use crate::msg::Scope;

/// One extension of an LDAP URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlExtension {
    /// Marked `!`.
    pub critical: bool,
    /// The extension's type.
    pub name: String,
    /// Its value, if any.
    pub value: Option<String>,
}

/// A parsed LDAP URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LdapUrl {
    /// The scheme, lower case: `ldap`, `ldaps` or `ldapi`.
    pub scheme: String,
    /// The host (no brackets around an IPv6 address); none for the
    /// client's default.
    pub host: Option<String>,
    /// The port, if given.
    pub port: Option<u16>,
    /// The base DN.
    pub dn: String,
    /// The attributes asked for.
    pub attrs: Vec<String>,
    /// The scope, if given (base by default).
    pub scope: Option<Scope>,
    /// The filter as written, if given (`(objectClass=*)` by default).
    pub filter: Option<String>,
    /// The extensions.
    pub extensions: Vec<UrlExtension>,
}

/// Why a string is not an LDAP URL.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid LDAP URL: {0}")]
pub struct UrlError(pub &'static str);

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Percent-decodes `s` to UTF-8.
///
/// # Errors
///
/// A [`UrlError`] for a bad escape or bytes that are not UTF-8.
pub fn decode(s: &str) -> Result<String, UrlError> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = b.get(i + 1).copied().and_then(hex);
            let l = b.get(i + 2).copied().and_then(hex);
            match (h, l) {
                (Some(h), Some(l)) => out.push((h << 4) | l),
                _ => return Err(UrlError("bad percent escape")),
            }
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| UrlError("escaped bytes are not UTF-8"))
}

/// Percent-encodes `s`: everything but RFC 3986's unreserved characters,
/// sub-delimiters, `:`, `@` and `/`, and those in `also` too.
#[must_use]
pub fn encode(s: &str, also: &[u8]) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        let safe = b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/".contains(&b);
        if safe && !also.contains(&b) {
            out.push(b as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[usize::from(b >> 4)] as char);
            out.push(HEX[usize::from(b & 15)] as char);
        }
    }
    out
}

impl LdapUrl {
    /// Parses an LDAP URL.
    ///
    /// # Errors
    ///
    /// A [`UrlError`].
    pub fn parse(s: &str) -> Result<Self, UrlError> {
        let (scheme, rest) = s.split_once("://").ok_or(UrlError("expected scheme://"))?;
        let scheme = scheme.to_ascii_lowercase();
        if !matches!(scheme.as_str(), "ldap" | "ldaps" | "ldapi") {
            return Err(UrlError("the scheme is not ldap, ldaps or ldapi"));
        }
        let (hostport, path) = if let Some(p) = rest.find('/') {
            (&rest[..p], Some(&rest[p + 1..]))
        } else {
            if rest.contains('?') {
                return Err(UrlError("'?' before the DN's '/'"));
            }
            (rest, None)
        };
        let (host, port) = parse_hostport(hostport)?;
        let mut url = Self {
            scheme,
            host,
            port,
            dn: String::new(),
            attrs: Vec::new(),
            scope: None,
            filter: None,
            extensions: Vec::new(),
        };
        let Some(path) = path else {
            return Ok(url);
        };
        let mut parts = path.split('?');
        url.dn = decode(parts.next().unwrap_or_default())?;
        if let Some(a) = parts.next() {
            url.attrs = a
                .split(',')
                .filter(|x| !x.is_empty())
                .map(decode)
                .collect::<Result<_, _>>()?;
        }
        if let Some(sc) = parts.next() {
            url.scope = match sc.to_ascii_lowercase().as_str() {
                "" => None,
                "base" => Some(Scope::Base),
                "one" => Some(Scope::One),
                "sub" => Some(Scope::Sub),
                _ => return Err(UrlError("unknown scope")),
            };
        }
        if let Some(f) = parts.next() {
            url.filter = (!f.is_empty()).then(|| decode(f)).transpose()?;
        }
        if let Some(e) = parts.next() {
            for ext in e.split(',').filter(|x| !x.is_empty()) {
                let (critical, ext) = match ext.strip_prefix('!') {
                    Some(r) => (true, r),
                    None => (false, ext),
                };
                let (name, value) = match ext.split_once('=') {
                    Some((n, v)) => (n, Some(decode(v)?)),
                    None => (ext, None),
                };
                if name.is_empty() {
                    return Err(UrlError("empty extension type"));
                }
                url.extensions.push(UrlExtension {
                    critical,
                    name: decode(name)?,
                    value,
                });
            }
        }
        if parts.next().is_some() {
            return Err(UrlError("too many '?' parts"));
        }
        Ok(url)
    }

    /// The scheme's port: 636 for `ldaps`, else 389.
    #[must_use]
    pub fn default_port(&self) -> u16 {
        if self.scheme == "ldaps" { 636 } else { 389 }
    }

    /// The port, given or by default.
    #[must_use]
    pub fn port_or_default(&self) -> u16 {
        self.port.unwrap_or_else(|| self.default_port())
    }

    /// The filter, parsed; `(objectClass=*)` if none was given.
    ///
    /// # Errors
    ///
    /// A [`FilterError`].
    pub fn filter(&self) -> Result<Filter, FilterError> {
        self.filter
            .as_deref()
            .map_or_else(|| Ok(Filter::any()), Filter::parse)
    }
}

fn parse_hostport(s: &str) -> Result<(Option<String>, Option<u16>), UrlError> {
    if s.is_empty() {
        return Ok((None, None));
    }
    let (host, port) = if let Some(rest) = s.strip_prefix('[') {
        let (h, after) = rest.split_once(']').ok_or(UrlError("unclosed '['"))?;
        let port = match after {
            "" => None,
            p => Some(p.strip_prefix(':').ok_or(UrlError("text after ']'"))?),
        };
        (h.to_string(), port)
    } else {
        match s.rsplit_once(':') {
            Some((h, p)) => (decode(h)?, Some(p)),
            None => (decode(s)?, None),
        }
    };
    let port = match port {
        None | Some("") => None,
        Some(p) => Some(p.parse::<u16>().map_err(|_| UrlError("bad port"))?),
    };
    Ok(((!host.is_empty()).then_some(host), port))
}

impl fmt::Display for LdapUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://", self.scheme)?;
        if let Some(h) = &self.host {
            if h.contains(':') {
                write!(f, "[{h}]")?;
            } else {
                f.write_str(&encode(h, b"/:"))?;
            }
        }
        if let Some(p) = self.port {
            write!(f, ":{p}")?;
        }
        f.write_str("/")?;
        f.write_str(&encode(&self.dn, b""))?;
        // Print only as far as the last component given.
        let ext = !self.extensions.is_empty();
        let filt = ext || self.filter.is_some();
        let scope = filt || self.scope.is_some();
        let attrs = scope || !self.attrs.is_empty();
        if attrs {
            f.write_str("?")?;
            let a: Vec<String> = self.attrs.iter().map(|a| encode(a, b",")).collect();
            f.write_str(&a.join(","))?;
        }
        if scope {
            f.write_str("?")?;
            f.write_str(match self.scope {
                None => "",
                Some(Scope::Base) => "base",
                Some(Scope::One) => "one",
                Some(Scope::Sub) => "sub",
            })?;
        }
        if filt {
            f.write_str("?")?;
            if let Some(x) = &self.filter {
                f.write_str(&encode(x, b""))?;
            }
        }
        if ext {
            f.write_str("?")?;
            for (i, e) in self.extensions.iter().enumerate() {
                if i > 0 {
                    f.write_str(",")?;
                }
                if e.critical {
                    f.write_str("!")?;
                }
                f.write_str(&encode(&e.name, b",="))?;
                if let Some(v) = &e.value {
                    write!(f, "={}", encode(v, b","))?;
                }
            }
        }
        Ok(())
    }
}

impl std::str::FromStr for LdapUrl {
    type Err = UrlError;

    fn from_str(s: &str) -> Result<Self, UrlError> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::LdapUrl;
    use crate::msg::Scope;

    fn p(s: &str) -> LdapUrl {
        LdapUrl::parse(s).unwrap_or_else(|e| panic!("{s}: {e}"))
    }

    #[test]
    fn rfc4516_examples() {
        let u = p("ldap:///o=University%20of%20Michigan,c=US");
        assert_eq!(u.host, None);
        assert_eq!(u.dn, "o=University of Michigan,c=US");
        let u = p("ldap://ldap1.example.net/o=University%20of%20Michigan,c=US");
        assert_eq!(u.host.as_deref(), Some("ldap1.example.net"));
        let u = p("ldap://ldap1.example.net/o=University%20of%20Michigan,c=US?postalAddress");
        assert_eq!(u.attrs, ["postalAddress"]);
        let u = p(
            "ldap://ldap1.example.net:6666/o=University%20of%20Michigan,c=US??sub?(cn=Babs%20Jensen)",
        );
        assert_eq!(u.port, Some(6666));
        assert_eq!(u.scope, Some(Scope::Sub));
        assert_eq!(u.filter.as_deref(), Some("(cn=Babs Jensen)"));
        assert!(u.filter().is_ok());
        let u = p("LDAP://ldap1.example.com/c=GB?objectClass?ONE");
        assert_eq!(u.scheme, "ldap");
        assert_eq!(u.scope, Some(Scope::One));
        let u = p("ldap://ldap2.example.com/o=Question%3f,c=US?mail");
        assert_eq!(u.dn, "o=Question?,c=US");
        let u = p("ldap://ldap3.example.com/o=Babsco,c=US???(four-octet=%5c00%5c00%5c00%5c04)");
        assert_eq!(u.filter.as_deref(), Some("(four-octet=\\00\\00\\00\\04)"));
        assert_eq!(
            u.filter().unwrap().to_string(),
            "(four-octet=\\00\\00\\00\\04)"
        );
        let u = p("ldap://ldap.example.com/o=An%20Example%5C2C%20Inc.,c=US");
        assert_eq!(u.dn, "o=An Example\\2C Inc.,c=US");
        let u = p("ldap:///??sub??e-bindname=cn=Manager%2cdc=example%2cdc=com");
        assert_eq!(u.extensions[0].name, "e-bindname");
        assert_eq!(
            u.extensions[0].value.as_deref(),
            Some("cn=Manager,dc=example,dc=com")
        );
        let u = p("ldap:///??sub??!e-bindname=cn=Manager%2cdc=example%2cdc=com");
        assert!(u.extensions[0].critical);
        let u = p("ldap://[2001:db8::7]:389/c=GB");
        assert_eq!(u.host.as_deref(), Some("2001:db8::7"));
        assert_eq!(p("ldaps://h").port_or_default(), 636);
        assert_eq!(p("ldap://h:").port_or_default(), 389);
    }

    #[test]
    fn prints_back() {
        for s in [
            "ldap:///",
            "ldap://h:6666/o=University%20of%20Michigan,c=US??sub?(cn=Babs%20Jensen)",
            "ldap://h/o=Question%3F,c=US?mail,cn",
            "ldap://[2001:db8::7]:389/c=GB?objectClass?one",
            "ldaps://h/??sub??!e-bindname=cn=Manager%2Cdc=example%2Cdc=com,x",
            "ldap://h/dc=x???(cn=%5C2a)",
        ] {
            let u = p(s);
            assert_eq!(u.to_string(), s);
            assert_eq!(p(&u.to_string()), u);
        }
    }

    #[test]
    fn errors() {
        for bad in [
            "http://h/",
            "ldap:/h",
            "ldap://h:99999/",
            "ldap://h:x/",
            "ldap://h/?a?bogus",
            "ldap://h/%zz",
            "ldap://h/?a?base?(x)?e?more",
            "ldap://[::1/",
            "ldap://h/????=v",
            "ldap://h?x",
            "ldap://h/%ff",
        ] {
            assert!(LdapUrl::parse(bad).is_err(), "{bad}");
        }
    }
}
