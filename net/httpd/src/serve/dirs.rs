// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What applies to one request's path: the server's and virtual host's
//! own directives, then every `<Directory>` that holds the file
//! (shortest first), `<Files>` matching its name and `<Location>`
//! matching the URL, `<Limit>` and `<LimitExcept>` by method. The
//! access group (`Order`, `Allow`, `Deny`) of a section that names any
//! of them replaces the one before; everything else is directive by
//! directive.

use std::net::IpAddr;

use crate::conf::{Directive, Node, Section};

/// `Order`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Order {
    /// `Deny,Allow`: allowed unless denied and not allowed.
    #[default]
    DenyAllow,
    /// `Allow,Deny`: denied unless allowed and not denied.
    AllowDeny,
    /// `Mutual-failure`: allowed only when allowed and not denied.
    MutualFailure,
}

/// Client address rules.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Access {
    /// `Order`.
    pub order: Order,
    /// `Allow From` hosts.
    pub allow: Vec<String>,
    /// `Deny From` hosts.
    pub deny: Vec<String>,
}

impl Access {
    /// Whether `client` may in.
    pub fn allows(&self, client: IpAddr) -> bool {
        let a = self.allow.iter().any(|h| host_matches(h, client));
        let d = self.deny.iter().any(|h| host_matches(h, client));
        match self.order {
            Order::DenyAllow => !d || a,
            Order::AllowDeny | Order::MutualFailure => a && !d,
        }
    }
}

fn v4(ip: IpAddr) -> Option<[u8; 4]> {
    match ip {
        IpAddr::V4(v) => Some(v.octets()),
        IpAddr::V6(v) => v.to_ipv4_mapped().map(|m| m.octets()),
    }
}

/// Whether host rule `h` (`all`, `10.1`, `10.1.2.3`, `10.0.0.0/8`,
/// `10.0.0.0/255.0.0.0`, an IPv6 address or prefix) covers `client`.
/// Host names never match: the server does no reverse lookups.
pub fn host_matches(h: &str, client: IpAddr) -> bool {
    let h = h.trim();
    if h.eq_ignore_ascii_case("all") {
        return true;
    }
    if let Some((net, mask)) = h.split_once('/') {
        let Ok(net) = net.parse::<IpAddr>() else {
            return false;
        };
        let bits: Option<u32> = mask.parse().ok().or_else(|| {
            mask.parse::<std::net::Ipv4Addr>()
                .ok()
                .map(|m| u32::from(m).count_ones())
        });
        let Some(bits) = bits else {
            return false;
        };
        return match (net, client) {
            (IpAddr::V6(n), IpAddr::V6(c)) => {
                let keep = 128u32.saturating_sub(bits.min(128));
                let m = if keep >= 128 { 0 } else { u128::MAX << keep };
                u128::from(n) & m == u128::from(c) & m
            }
            _ => match (v4(net), v4(client)) {
                (Some(n), Some(c)) => {
                    let keep = 32u32.saturating_sub(bits.min(32));
                    let m = if keep >= 32 { 0 } else { u32::MAX << keep };
                    u32::from_be_bytes(n) & m == u32::from_be_bytes(c) & m
                }
                _ => false,
            },
        };
    }
    if let Ok(ip) = h.parse::<IpAddr>() {
        return match (v4(ip), v4(client)) {
            (Some(a), Some(b)) => a == b,
            _ => ip == client,
        };
    }
    // A partial address: `10.1` or `10.1.`.
    let parts: Vec<&str> = h.trim_end_matches('.').split('.').collect();
    if parts.len() < 4 && parts.iter().all(|p| p.parse::<u8>().is_ok())
        && let Some(c) = v4(client)
    {
        return parts.iter().zip(c).all(|(p, b)| p.parse::<u8>() == Ok(b));
    }
    false
}

/// A `Redirect`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redirect {
    /// The status.
    pub status: u16,
    /// The URL path it covers.
    pub from: String,
    /// Where to, for a 3xx.
    pub to: Option<String>,
}

impl Redirect {
    /// From a directive's arguments.
    pub fn parse(args: &[String]) -> Option<Self> {
        let (status, rest) = match args.first().map(|s| s.to_ascii_lowercase()).as_deref() {
            Some("permanent") => (301, &args[1..]),
            Some("temp") => (302, &args[1..]),
            Some("seeother") => (303, &args[1..]),
            Some("gone") => (410, &args[1..]),
            Some(n) if n.parse::<u16>().is_ok() => (n.parse().ok()?, &args[1..]),
            _ => (302, args),
        };
        let from = rest.first()?.clone();
        let to = rest.get(1).cloned();
        if (300..400).contains(&status) && to.is_none() {
            return None;
        }
        Some(Self { status, from, to })
    }

    /// Where `url` goes, when this covers it.
    pub fn target(&self, url: &str) -> Option<Option<String>> {
        let from = self.from.trim_end_matches('/');
        let rest = if url == self.from || url == from {
            ""
        } else if from.is_empty() || url.starts_with(&format!("{from}/")) {
            &url[from.len()..]
        } else {
            return None;
        };
        Some(self.to.as_ref().map(|t| {
            if rest.is_empty() {
                t.clone()
            } else {
                format!("{}{rest}", t.trim_end_matches('/'))
            }
        }))
    }
}

/// The settings for one path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirConf {
    /// `DirectoryIndex`.
    pub index: Vec<String>,
    /// `Options Indexes`.
    pub indexes: bool,
    /// The access rules.
    pub access: Access,
    /// `AuthType`.
    pub auth_type: Option<String>,
    /// `AuthName`: the realm.
    pub auth_name: String,
    /// `PasswdFile`: `%%SYSTEM%%` or a validation list.
    pub passwd_file: Option<String>,
    /// `Require`.
    pub require: Vec<String>,
    /// `ErrorDocument`: status and what to answer.
    pub error_docs: Vec<(u16, String)>,
    /// `Redirect`s, in order.
    pub redirects: Vec<Redirect>,
    /// `AddType`: media type and extensions.
    pub types: Vec<(String, String)>,
    /// `DefaultType`.
    pub default_type: Option<String>,
}

impl DirConf {
    /// What a fresh server starts with.
    pub fn base() -> Self {
        Self {
            index: vec!["index.html".into()],
            ..Self::default()
        }
    }

    /// Apply the directives of one scope (not its sections); `fresh`
    /// is where the scope's access group starts when it names one.
    pub fn apply(&mut self, items: &[Node]) {
        let names_access = items
            .iter()
            .any(|n| matches!(n, Node::Directive(d) if d.is("Order") || d.is("Allow") || d.is("Deny")));
        if names_access {
            self.access = Access::default();
        }
        for n in items {
            if let Node::Directive(d) = n {
                self.directive(d);
            }
        }
    }

    fn directive(&mut self, d: &Directive) {
        let a = &d.args;
        match d.name.to_ascii_lowercase().as_str() {
            "directoryindex" => self.index.clone_from(a),
            "options" => {
                let absolute = a.iter().all(|o| !o.starts_with(['+', '-']));
                if absolute {
                    self.indexes = false;
                }
                for o in a {
                    let (on, name) = match o.as_bytes().first() {
                        Some(b'-') => (false, &o[1..]),
                        Some(b'+') => (true, &o[1..]),
                        _ => (true, o.as_str()),
                    };
                    if name.eq_ignore_ascii_case("Indexes") || name.eq_ignore_ascii_case("All") {
                        self.indexes = on;
                    } else if name.eq_ignore_ascii_case("None") {
                        self.indexes = false;
                    }
                }
            }
            "order" => {
                self.access.order = match a[0].to_ascii_lowercase().as_str() {
                    "allow,deny" => Order::AllowDeny,
                    "mutual-failure" => Order::MutualFailure,
                    _ => Order::DenyAllow,
                };
            }
            "allow" => self.access.allow.extend(a[1..].iter().cloned()),
            "deny" => self.access.deny.extend(a[1..].iter().cloned()),
            "authtype" => self.auth_type = Some(a[0].clone()),
            "authname" => self.auth_name.clone_from(&a[0]),
            "passwdfile" => self.passwd_file = Some(a[0].clone()),
            "require" => self.require.clone_from(a),
            "errordocument" => {
                if let Ok(code) = a[0].parse::<u16>() {
                    self.error_docs.retain(|(c, _)| *c != code);
                    self.error_docs.push((code, a[1..].join(" ")));
                }
            }
            "redirect" => {
                if let Some(r) = Redirect::parse(a) {
                    self.redirects.push(r);
                }
            }
            "addtype" => {
                for ext in &a[1..] {
                    self.types.push((a[0].clone(), ext.trim_start_matches('.').to_ascii_lowercase()));
                }
            }
            "defaulttype" => self.default_type = Some(a[0].clone()),
            _ => {}
        }
    }

    /// Apply a section and the `<Limit>`s inside it that cover `method`.
    pub fn apply_section(&mut self, s: &Section, method: &str) {
        self.apply(&s.items);
        for n in &s.items {
            if let Node::Section(l) = n {
                let listed = l.args.iter().any(|m| m.eq_ignore_ascii_case(method));
                let covers = match l.name.to_ascii_lowercase().as_str() {
                    "limit" => listed,
                    "limitexcept" => !listed,
                    _ => false,
                };
                if covers {
                    self.apply(&l.items);
                }
            }
        }
    }

    /// The `ErrorDocument` for `status`.
    pub fn error_doc(&self, status: u16) -> Option<&str> {
        self.error_docs.iter().find(|(c, _)| *c == status).map(|(_, t)| t.as_str())
    }
}

/// A path with one trailing slash removed (but `/` kept).
fn trim(p: &str) -> &str {
    if p.len() > 1 { p.trim_end_matches('/') } else { p }
}

/// Whether directory `dir` holds `path` (or is it).
pub fn under(dir: &str, path: &str) -> bool {
    let d = trim(dir);
    d == "/" || path == d || path.starts_with(&format!("{d}/"))
}

/// Whether a simple pattern (`*` and `?`) matches `name`.
pub fn glob(pat: &str, name: &str) -> bool {
    fn go(p: &[u8], n: &[u8]) -> bool {
        match p.split_first() {
            None => n.is_empty(),
            Some((b'*', rest)) => (0..=n.len()).any(|i| go(rest, &n[i..])),
            Some((b'?', rest)) => !n.is_empty() && go(rest, &n[1..]),
            Some((c, rest)) => n.first() == Some(c) && go(rest, &n[1..]),
        }
    }
    go(pat.as_bytes(), name.as_bytes())
}

/// The sections that apply to a request: `<Directory>` holding `fs`
/// shortest first, `<Files>` matching its last name, `<Location>`
/// covering `url`, each in file order within its kind.
pub fn matching<'a>(sections: &[&'a Section], fs: Option<&str>, url: &str) -> Vec<&'a Section> {
    let mut dirs: Vec<&Section> = sections
        .iter()
        .copied()
        .filter(|s| s.name.eq_ignore_ascii_case("Directory"))
        .filter(|s| fs.is_some_and(|f| s.args.first().is_some_and(|d| under(d, f))))
        .collect();
    dirs.sort_by_key(|s| trim(s.args.first().map_or("", String::as_str)).split('/').filter(|p| !p.is_empty()).count());
    let name = fs.and_then(|f| f.rsplit('/').next()).unwrap_or("");
    let files = sections
        .iter()
        .copied()
        .filter(|s| s.name.eq_ignore_ascii_case("Files"))
        .filter(|s| s.args.first().is_some_and(|p| glob(p, name)));
    let locs = sections
        .iter()
        .copied()
        .filter(|s| s.name.eq_ignore_ascii_case("Location"))
        .filter(|s| s.args.first().is_some_and(|l| under(l, url)));
    dirs.into_iter().chain(files).chain(locs).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conf::{Scope as _, parse};

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn hosts() {
        let c = ip("10.1.2.3");
        for yes in ["all", "10.1", "10.1.", "10.1.2.3", "10.0.0.0/8", "10.1.0.0/255.255.0.0"] {
            assert!(host_matches(yes, c), "{yes}");
        }
        for no in ["10.2", "10.1.2.4", "11.0.0.0/8", "example.com", "::1"] {
            assert!(!host_matches(no, c), "{no}");
        }
        assert!(host_matches("::1", ip("::1")));
        assert!(host_matches("fe80::/10", ip("fe80::1")));
        assert!(host_matches("127.0.0.1", ip("::ffff:127.0.0.1")));
    }

    #[test]
    fn orders() {
        let mut a = Access {
            order: Order::AllowDeny,
            allow: vec!["10.0.0.0/8".into()],
            deny: vec!["10.9".into()],
        };
        assert!(a.allows(ip("10.1.1.1")) && !a.allows(ip("10.9.1.1")) && !a.allows(ip("192.168.1.1")));
        a.order = Order::DenyAllow;
        assert!(a.allows(ip("192.168.1.1")) && a.allows(ip("10.9.1.1")));
        a.deny = vec!["all".into()];
        assert!(a.allows(ip("10.1.1.1")) && !a.allows(ip("192.168.1.1")));
        assert!(Access::default().allows(ip("1.2.3.4")));
    }

    #[test]
    fn redirects() {
        let r = Redirect::parse(&["/old".into(), "http://x/new".into()]).unwrap();
        assert_eq!(r.status, 302);
        assert_eq!(r.target("/old/a.html"), Some(Some("http://x/new/a.html".into())));
        assert_eq!(r.target("/older"), None);
        let g = Redirect::parse(&["gone".into(), "/dead".into()]).unwrap();
        assert_eq!((g.status, g.target("/dead")), (410, Some(None)));
        assert!(Redirect::parse(&["permanent".into(), "/x".into()]).is_none());
    }

    #[test]
    fn merging() {
        let c = parse(
            "DirectoryIndex a.html\nOptions -Indexes\n<Directory />\n Order Deny,Allow\n Deny From all\n</Directory>\n\
             <Directory /www/x/htdocs>\n Order Allow,Deny\n Allow From all\n Options +Indexes\n</Directory>\n\
             <Directory /www/x/htdocs/priv>\n AuthType Basic\n AuthName Private\n Require valid-user\n\
             <LimitExcept GET>\n Require user ADMIN\n</LimitExcept>\n</Directory>\n<Files *.cfg>\n Deny From all\n</Files>\n",
        )
        .unwrap();
        let secs: Vec<&Section> = c.items.iter().filter_map(|n| if let Node::Section(s) = n { Some(s) } else { None }).collect();
        let resolve = |fs: &str, method: &str| {
            let mut d = DirConf::base();
            d.apply(&c.items);
            for s in matching(&secs, Some(fs), "/") {
                d.apply_section(s, method);
            }
            d
        };
        let top = resolve("/etc/x", "GET");
        assert!(!top.access.allows(ip("1.2.3.4")));
        assert_eq!(top.index, ["a.html"]);
        let doc = resolve("/www/x/htdocs/a.html", "GET");
        assert!(doc.access.allows(ip("1.2.3.4")) && doc.indexes && doc.require.is_empty());
        let p = resolve("/www/x/htdocs/priv/a.html", "GET");
        assert_eq!((p.auth_name.as_str(), p.require.clone()), ("Private", vec!["valid-user".to_string()]));
        assert_eq!(resolve("/www/x/htdocs/priv/a.html", "POST").require, ["user", "ADMIN"]);
        assert!(!resolve("/www/x/htdocs/s.cfg", "GET").access.allows(ip("1.2.3.4")));
        assert!(c.get("Options").is_some());
        assert!(glob("*.cfg", "s.cfg") && !glob("*.cfg", "s.cfgx") && glob("a?c", "abc"));
    }
}
