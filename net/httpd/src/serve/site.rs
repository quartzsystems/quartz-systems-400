// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An instance's configuration compiled for serving: the listeners and
//! their TLS, the main server and each `<VirtualHost>` as a [`Site`]
//! (a virtual host starts from the main server's settings and replaces
//! what it names), and how a request finds its site.

use std::net::{IpAddr, SocketAddr};

use super::dirs::{DirConf, Redirect};
use super::log::Level;
use crate::conf::{Conf, Directive, Node, Scope as _, Section, rules};
use crate::instance::resolve;

/// The user profile an instance runs under unless `ServerUserID` names
/// another.
pub const DEFAULT_USER: &str = "QTMHHTTP";

/// A port's TLS: what the job asks the certificate store for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TlsSpec {
    /// `SSLAppName`, or `QS400_HTTP_SERVER_<instance>`.
    pub app_id: String,
    /// `SSLProtocolDisable`: `TLSv1.2`, ...
    pub disabled: Vec<String>,
    /// `SSLClientAuth`: 0 none, 1 optional, 2 required; `None` keeps
    /// the application's own setting.
    pub client_auth: Option<u8>,
}

/// A socket to listen on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listener {
    /// The address; `None` for every one.
    pub addr: Option<IpAddr>,
    /// The port.
    pub port: u16,
    /// TLS from the first byte.
    pub tls: Option<TlsSpec>,
}

impl Listener {
    /// The socket address to bind.
    pub fn socket(&self) -> SocketAddr {
        SocketAddr::new(self.addr.unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)), self.port)
    }
}

/// An access log: where and in what format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessLog {
    /// The file.
    pub path: String,
    /// The format string.
    pub format: String,
}

/// `LogMaint path files size-MB [HH:MM]`: how a log rotates and how
/// many rotated files stay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Maint {
    /// The log file.
    pub path: String,
    /// Rotated files kept, newest first; 0 keeps them all.
    pub files: u32,
    /// Rotate when the file would pass this many bytes; 0 rotates
    /// daily at [`Maint::at`].
    pub size: u64,
    /// Seconds after midnight (UTC) of the daily rotation.
    pub at: u64,
}

impl Maint {
    /// From a `LogMaint` directive's arguments, `path` resolved.
    pub fn parse(path: String, args: &[String]) -> Self {
        let num = |i: usize| args.get(i).and_then(|a| a.parse::<u64>().ok()).unwrap_or(0);
        let at = args
            .get(3)
            .and_then(|t| t.split_once(':'))
            .and_then(|(h, m)| Some(h.parse::<u64>().ok()? * 3600 + m.parse::<u64>().ok()? * 60))
            .unwrap_or(0);
        Self {
            path,
            files: u32::try_from(num(1)).unwrap_or(u32::MAX),
            size: num(2) * 1024 * 1024,
            at,
        }
    }
}

/// The main server or one virtual host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Site {
    /// `ServerName`.
    pub server_name: Option<String>,
    /// `ServerAlias`.
    pub aliases: Vec<String>,
    /// The `<VirtualHost>` addresses; empty for the main server.
    pub addrs: Vec<(Option<IpAddr>, u16)>,
    /// `DocumentRoot`.
    pub doc_root: String,
    /// `Alias`: URL prefix and path.
    pub alias: Vec<(String, String)>,
    /// `ScriptAlias`: URL prefix and path.
    pub script_alias: Vec<(String, String)>,
    /// `KeepAlive`.
    pub keep_alive: bool,
    /// `KeepAliveTimeout`, seconds.
    pub keep_alive_timeout: u32,
    /// `MaxKeepAliveRequests`; 0 for no limit.
    pub max_keep_alive: u32,
    /// `TimeOut`, seconds.
    pub timeout: u32,
    /// `CustomLog`s.
    pub access_logs: Vec<AccessLog>,
    /// `ErrorLog`.
    pub error_log: Option<String>,
    /// `LogLevel`.
    pub level: Level,
    /// `SSLEngine On`: this site's TLS.
    pub tls: Option<TlsSpec>,
    /// The site's own directives, applied before any section.
    pub base: DirConf,
    /// Its sections: the main server's, then its own.
    pub sections: Vec<Section>,
}

const COMMON: &str = "%h %l %u %t \"%r\" %>s %b";
const COMBINED: &str = "%h %l %u %t \"%r\" %>s %b \"%{Referer}i\" \"%{User-Agent}i\"";

/// A `LogFormat` nickname's format.
fn format_of(nicks: &[(String, String)], v: &str) -> String {
    if let Some((_, f)) = nicks.iter().rev().find(|(n, _)| n == v) {
        return f.clone();
    }
    match v {
        "common" => COMMON.into(),
        "combined" => COMBINED.into(),
        "extended" => format!("{COMBINED} %D"),
        f if f.contains('%') => f.to_string(),
        _ => COMMON.into(),
    }
}

fn on(d: &Directive) -> bool {
    d.args.first().is_some_and(|a| a.eq_ignore_ascii_case("on"))
}

fn num(d: &Directive) -> u32 {
    d.args.first().and_then(|a| a.parse().ok()).unwrap_or(0)
}

fn level(v: &str) -> Level {
    Level::parse(v).unwrap_or(Level::Warn)
}

impl Site {
    /// The main server of `conf`, rooted at `root`.
    fn main(conf: &Conf, root: &str, instance: &str) -> Self {
        let mut s = Site {
            server_name: None,
            aliases: Vec::new(),
            addrs: Vec::new(),
            doc_root: resolve(root, "htdocs"),
            alias: Vec::new(),
            script_alias: Vec::new(),
            keep_alive: true,
            keep_alive_timeout: 300,
            max_keep_alive: 100,
            timeout: 300,
            access_logs: Vec::new(),
            error_log: Some(resolve(root, "logs/error_log")),
            level: Level::Warn,
            tls: None,
            base: DirConf::base(),
            sections: Vec::new(),
        };
        s.take(&conf.items, root, instance, &[]);
        s
    }

    /// Apply a scope's directives: the main server's or a virtual
    /// host's. `nicks` are the `LogFormat`s of the scopes around.
    fn take(&mut self, items: &[Node], root: &str, instance: &str, outer: &[(String, String)]) {
        let mut nicks: Vec<(String, String)> = outer.to_vec();
        let mut tls_on = None;
        let mut app = None;
        let mut disabled = Vec::new();
        let mut client_auth = None;
        let mut logs_named = false;
        for n in items {
            match n {
                Node::Directive(d) => match d.name.to_ascii_lowercase().as_str() {
                    "servername" => self.server_name = d.args.first().cloned(),
                    "serveralias" => self.aliases.extend(d.args.iter().cloned()),
                    "documentroot" => self.doc_root = resolve(root, &d.args[0]),
                    "alias" => self.alias.push((d.args[0].clone(), resolve(root, &d.args[1]))),
                    "scriptalias" => self.script_alias.push((d.args[0].clone(), resolve(root, &d.args[1]))),
                    "keepalive" => self.keep_alive = on(d),
                    "keepalivetimeout" => self.keep_alive_timeout = num(d),
                    "maxkeepaliverequests" => self.max_keep_alive = num(d),
                    "timeout" => self.timeout = num(d).max(1),
                    "logformat" if d.args.len() == 2 => nicks.push((d.args[1].clone(), d.args[0].clone())),
                    "customlog" => {
                        if !logs_named {
                            // A scope that names its own logs replaces the outer ones.
                            self.access_logs.clear();
                            logs_named = true;
                        }
                        self.access_logs.push(AccessLog {
                            path: resolve(root, &d.args[0]),
                            format: format_of(&nicks, &d.args[1]),
                        });
                    }
                    "errorlog" => {
                        self.error_log = if d.args[0].eq_ignore_ascii_case("off") {
                            None
                        } else {
                            Some(resolve(root, &d.args[0]))
                        };
                    }
                    "loglevel" => self.level = level(&d.args[0]),
                    "sslengine" => tls_on = Some(on(d)),
                    "sslappname" => app = d.args.first().cloned(),
                    "sslprotocoldisable" => disabled.extend(d.args.iter().cloned()),
                    "sslclientauth" => {
                        client_auth = Some(match d.args[0].to_ascii_lowercase().as_str() {
                            "optional" | "1" => 1,
                            "required" | "2" => 2,
                            _ => 0,
                        });
                    }
                    _ => {}
                },
                Node::Section(s) if !s.name.eq_ignore_ascii_case("VirtualHost") => {
                    if s.name.eq_ignore_ascii_case("IfModule") {
                        self.take(&s.items, root, instance, &nicks);
                    } else {
                        self.sections.push(s.clone());
                    }
                }
                _ => {}
            }
        }
        self.base.apply(items);
        match tls_on {
            Some(true) => {
                self.tls = Some(TlsSpec {
                    app_id: app.unwrap_or_else(|| format!("QS400_HTTP_SERVER_{}", instance.to_ascii_uppercase())),
                    disabled,
                    client_auth,
                });
            }
            Some(false) => self.tls = None,
            None => {
                if let Some(t) = &mut self.tls {
                    if let Some(a) = app {
                        t.app_id = a;
                    }
                    t.disabled.extend(disabled);
                    t.client_auth = client_auth.or(t.client_auth);
                }
            }
        }
    }

    /// Whether this site answers to `host` (the `Host` header without
    /// its port).
    pub fn named(&self, host: &str) -> bool {
        self.server_name
            .iter()
            .chain(&self.aliases)
            .any(|n| n.split(':').next().unwrap_or("").eq_ignore_ascii_case(host))
    }

    /// Whether this virtual host covers a connection to `local`.
    fn covers(&self, local: SocketAddr) -> bool {
        self.addrs
            .iter()
            .any(|(a, p)| *p == local.port() && a.is_none_or(|a| a == local.ip() || same_v4(a, local.ip())))
    }
}

fn same_v4(a: IpAddr, b: IpAddr) -> bool {
    let m = |x: IpAddr| match x {
        IpAddr::V4(v) => Some(v),
        IpAddr::V6(v) => v.to_ipv4_mapped(),
    };
    m(a).is_some() && m(a) == m(b)
}

/// The `LogMaint` directives of a scope and the `<VirtualHost>`s in it.
fn maints(items: &[Node], root: &str, out: &mut Vec<Maint>) {
    for n in items {
        match n {
            Node::Directive(d) if d.is("LogMaint") => out.push(Maint::parse(resolve(root, &d.args[0]), &d.args)),
            Node::Section(s) if s.name.eq_ignore_ascii_case("VirtualHost") || s.name.eq_ignore_ascii_case("IfModule") => {
                maints(&s.items, root, out);
            }
            _ => {}
        }
    }
}

/// An instance's configuration, compiled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The instance name.
    pub instance: String,
    /// `ServerRoot`.
    pub root: String,
    /// `ServerUserID`.
    pub user: String,
    /// `Listen` sockets.
    pub listeners: Vec<Listener>,
    /// The main server.
    pub main: Site,
    /// The virtual hosts, in file order.
    pub vhosts: Vec<Site>,
    /// `LogMaint`.
    pub maint: Vec<Maint>,
}

impl Plan {
    /// Compile `conf` of instance `instance` whose registry root is
    /// `root` (`ServerRoot` overrides it).
    pub fn new(instance: &str, root: &str, conf: &Conf) -> Self {
        let root = conf.get("ServerRoot").and_then(|d| d.args.first().cloned()).unwrap_or_else(|| root.to_string());
        let main = Site::main(conf, &root, instance);
        let outer: Vec<(String, String)> = conf
            .all("LogFormat")
            .into_iter()
            .filter(|d| d.args.len() == 2)
            .map(|d| (d.args[1].clone(), d.args[0].clone()))
            .collect();
        let vhosts: Vec<Site> = conf
            .sections("VirtualHost")
            .into_iter()
            .map(|v| {
                let mut s = main.clone();
                s.server_name = None;
                s.aliases.clear();
                s.addrs = v
                    .args
                    .iter()
                    .filter_map(|a| rules::listen_addr(a).ok())
                    .map(|(a, p)| (a.and_then(|a| a.parse().ok()), p))
                    .collect();
                s.take(&v.items, &root, instance, &outer);
                if s.server_name.is_none() {
                    s.server_name.clone_from(&main.server_name);
                }
                s
            })
            .collect();
        let listeners = conf
            .all("Listen")
            .into_iter()
            .filter_map(|d| rules::listen_addr(d.args.first()?).ok())
            .map(|(a, port)| {
                let addr = a.and_then(|a| a.parse::<IpAddr>().ok());
                let probe = SocketAddr::new(addr.unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)), port);
                let tls = vhosts
                    .iter()
                    .find(|v| v.addrs.iter().any(|(_, p)| *p == port) && (addr.is_none() || v.covers(probe)))
                    .map_or(main.tls.clone(), |v| v.tls.clone());
                Listener { addr, port, tls }
            })
            .collect();
        let mut maint = Vec::new();
        maints(&conf.items, &root, &mut maint);
        Self {
            instance: instance.to_ascii_uppercase(),
            user: conf
                .get("ServerUserID")
                .and_then(|d| d.args.first().cloned())
                .map_or_else(|| DEFAULT_USER.to_string(), |u| u.to_ascii_uppercase()),
            root,
            listeners,
            main,
            vhosts,
            maint,
        }
    }

    /// The site a request on a connection to `local` with `Host` header
    /// `host` belongs to: among the virtual hosts covering the socket,
    /// the one named `host`, else the first; else the main server.
    pub fn site(&self, local: SocketAddr, host: Option<&str>) -> &Site {
        let host = host.map(|h| {
            if let Some(rest) = h.strip_prefix('[') {
                rest.split(']').next().unwrap_or("")
            } else {
                h.split(':').next().unwrap_or("")
            }
        });
        let covering: Vec<&Site> = self.vhosts.iter().filter(|v| v.covers(local)).collect();
        host.and_then(|h| covering.iter().find(|v| v.named(h)).copied())
            .or_else(|| covering.first().copied())
            .unwrap_or(&self.main)
    }

    /// Every log file the instance writes.
    pub fn log_files(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in std::iter::once(&self.main).chain(&self.vhosts) {
            for p in s.access_logs.iter().map(|l| l.path.clone()).chain(s.error_log.clone()) {
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
        out
    }

    /// The `LogMaint` of `path`.
    pub fn maint_of(&self, path: &str) -> Option<&Maint> {
        self.maint.iter().find(|m| m.path == path)
    }
}

/// The redirect covering `url`, if any.
pub fn redirect<'a>(d: &'a DirConf, url: &str) -> Option<(&'a Redirect, Option<String>)> {
    d.redirects.iter().find_map(|r| r.target(url).map(|t| (r, t)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conf::parse;

    const CONF: &str = "ServerRoot /www/shop\nListen *:8080\nListen 8443\nServerName shop.example\n\
        DocumentRoot htdocs\nLogFormat \"%h %u\" short\nCustomLog logs/access_log short\nLogMaint logs/access_log 7 0 03:30\n\
        KeepAlive Off\nAlias /icons/ /www/shop/icons/\n\
        <VirtualHost *:8443>\n   ServerName secure.example\n   SSLEngine On\n   SSLProtocolDisable TLSv1.2\n   SSLClientAuth Optional\n   DocumentRoot /www/shop/secure\n</VirtualHost>\n\
        <VirtualHost *:8080>\n   ServerName other.example\n   DocumentRoot /www/shop/other\n   CustomLog logs/other_log common\n</VirtualHost>\n";

    #[test]
    fn compiles_sites_and_listeners() {
        let p = Plan::new("shop", "/www/x", &parse(CONF).unwrap());
        assert_eq!((p.instance.as_str(), p.root.as_str(), p.user.as_str()), ("SHOP", "/www/shop", "QTMHHTTP"));
        assert_eq!(p.listeners.len(), 2);
        assert!(p.listeners[0].tls.is_none());
        let t = p.listeners[1].tls.as_ref().unwrap();
        assert_eq!((t.app_id.as_str(), t.disabled.clone(), t.client_auth), ("QS400_HTTP_SERVER_SHOP", vec!["TLSv1.2".to_string()], Some(1)));
        assert_eq!(p.main.doc_root, "/www/shop/htdocs");
        assert_eq!(p.main.access_logs[0].format, "%h %u");
        assert!(!p.main.keep_alive);
        let local: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        assert_eq!(p.site(local, Some("other.example:8080")).doc_root, "/www/shop/other");
        assert_eq!(p.site(local, Some("nobody")).doc_root, "/www/shop/other");
        let sec = p.site("127.0.0.1:8443".parse().unwrap(), None);
        assert_eq!(sec.doc_root, "/www/shop/secure");
        assert_eq!(sec.alias, [("/icons/".to_string(), "/www/shop/icons/".to_string())]);
        assert_eq!(sec.access_logs[0].path, "/www/shop/logs/access_log");
        assert_eq!(p.vhosts[1].access_logs.len(), 1);
        assert_eq!(p.log_files(), ["/www/shop/logs/access_log", "/www/shop/logs/error_log", "/www/shop/logs/other_log"]);
        let m = p.maint_of("/www/shop/logs/access_log").unwrap();
        assert_eq!((m.files, m.size, m.at), (7, 0, 3 * 3600 + 1800));
        let none = Plan::new("x", "/www/x", &parse("Listen 80\n").unwrap());
        assert_eq!(none.site("10.0.0.1:80".parse().unwrap(), Some("a")).doc_root, "/www/x/htdocs");
    }
}
