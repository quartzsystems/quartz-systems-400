// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the server accepts: every directive and section it knows, how
//! many arguments each takes, where it may appear, and the values the
//! few with fixed choices allow. Anything else is a configuration error
//! (`HTP8100`) at its line and column.

use super::ConfError;

/// Where a directive may appear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scope(u8);

impl Scope {
    /// The server, outside any section.
    pub const SERVER: Scope = Scope(1);
    /// `<VirtualHost>`.
    pub const VHOST: Scope = Scope(2);
    /// `<Directory>`, `<Location>`, `<Files>` and their `Match` forms.
    pub const DIR: Scope = Scope(4);
    /// Everywhere.
    pub const ALL: Scope = Scope(7);
    /// Server and virtual host.
    pub const SV: Scope = Scope(3);

    fn has(self, s: Scope) -> bool {
        self.0 & s.0 != 0
    }
}

/// A known directive.
#[derive(Clone, Copy, Debug)]
pub struct Rule {
    /// Its name as documented.
    pub name: &'static str,
    /// Fewest arguments.
    pub min: usize,
    /// Most arguments (`usize::MAX` for any number).
    pub max: usize,
    /// Where it may appear.
    pub scope: Scope,
}

const fn r(name: &'static str, min: usize, max: usize, scope: Scope) -> Rule {
    Rule {
        name,
        min,
        max,
        scope,
    }
}

const ANY: usize = usize::MAX;

/// Every directive the server knows.
pub const DIRECTIVES: &[Rule] = &[
    r("AccessFileName", 1, ANY, Scope::SV),
    r("AddCharset", 2, ANY, Scope::ALL),
    r("AddLanguage", 2, ANY, Scope::ALL),
    r("AddType", 2, ANY, Scope::ALL),
    r("Alias", 2, 2, Scope::SV),
    r("AliasMatch", 2, 2, Scope::SV),
    r("Allow", 2, ANY, Scope::DIR),
    r("AllowOverride", 1, ANY, Scope::DIR),
    r("AuthName", 1, 1, Scope::DIR),
    r("AuthType", 1, 1, Scope::DIR),
    r("CustomLog", 2, 3, Scope::SV),
    r("DefaultFsCCSID", 1, 1, Scope::SERVER),
    r("DefaultNetCCSID", 1, 1, Scope::SERVER),
    r("DefaultType", 1, 1, Scope::ALL),
    r("Deny", 2, ANY, Scope::DIR),
    r("DirectoryIndex", 1, ANY, Scope::ALL),
    r("DocumentRoot", 1, 1, Scope::SV),
    r("ErrorDocument", 2, ANY, Scope::ALL),
    r("ErrorLog", 1, 1, Scope::SV),
    r("HostNameLookups", 1, 1, Scope::ALL),
    r("KeepAlive", 1, 1, Scope::SV),
    r("KeepAliveTimeout", 1, 1, Scope::SV),
    r("Listen", 1, 2, Scope::SERVER),
    r("LogCCSID", 1, 1, Scope::SV),
    r("LogFormat", 1, 2, Scope::SV),
    r("LogLevel", 1, 1, Scope::SV),
    r("LogMaint", 3, 4, Scope::SV),
    r("MaxKeepAliveRequests", 1, 1, Scope::SV),
    r("NameVirtualHost", 1, 1, Scope::SERVER),
    r("Options", 1, ANY, Scope::ALL),
    r("Order", 1, 1, Scope::DIR),
    r("PasswdFile", 1, 1, Scope::DIR),
    r("Redirect", 1, 3, Scope::ALL),
    r("Require", 1, ANY, Scope::DIR),
    r("ScriptAlias", 2, 2, Scope::SV),
    r("ScriptAliasMatch", 2, 2, Scope::SV),
    r("ServerAdmin", 1, 1, Scope::SV),
    r("ServerAlias", 1, ANY, Scope::VHOST),
    r("ServerName", 1, 1, Scope::SV),
    r("ServerRoot", 1, 1, Scope::SERVER),
    r("ServerSignature", 1, 1, Scope::ALL),
    r("ServerTokens", 1, 1, Scope::SERVER),
    r("ServerUserID", 1, 1, Scope::SV),
    r("SSLAppName", 1, 1, Scope::SV),
    r("SSLCipherSpec", 1, ANY, Scope::SV),
    r("SSLClientAuth", 1, 2, Scope::SV),
    r("SSLEngine", 1, 1, Scope::SV),
    r("SSLProtocolDisable", 1, ANY, Scope::SV),
    r("ThreadsPerChild", 1, 1, Scope::SERVER),
    r("TimeOut", 1, 1, Scope::SV),
    r("UseCanonicalName", 1, 1, Scope::ALL),
];

/// Every section the server knows and where it may open.
pub const SECTIONS: &[(&str, Scope, usize)] = &[
    ("Directory", Scope::SV, 1),
    ("DirectoryMatch", Scope::SV, 1),
    ("Files", Scope::ALL, 1),
    ("FilesMatch", Scope::ALL, 1),
    ("IfModule", Scope::ALL, 1),
    ("Limit", Scope::DIR, 1),
    ("LimitExcept", Scope::DIR, 1),
    ("Location", Scope::SV, 1),
    ("LocationMatch", Scope::SV, 1),
    ("VirtualHost", Scope::SERVER, 1),
];

/// The rule for directive `name` (case-blind).
pub fn rule(name: &str) -> Option<&'static Rule> {
    DIRECTIVES
        .iter()
        .find(|r| r.name.eq_ignore_ascii_case(name))
}

fn scope_of(parent: Option<&str>) -> Scope {
    match parent.map(str::to_ascii_lowercase).as_deref() {
        None | Some("ifmodule") => Scope::SERVER,
        Some("virtualhost") => Scope::VHOST,
        Some(_) => Scope::DIR,
    }
}

/// Check a section start.
pub fn check_section(
    name: &str,
    args: &[String],
    line: u32,
    col: u32,
    parent: Option<&str>,
) -> Result<(), ConfError> {
    let Some((n, scope, min)) = SECTIONS
        .iter()
        .find(|(n, _, _)| n.eq_ignore_ascii_case(name))
    else {
        return Err(ConfError::new(
            line,
            col,
            &format!("<{name}> is not a section this server knows."),
        ));
    };
    if args.len() < *min {
        return Err(ConfError::new(
            line,
            col,
            &format!("<{n}> needs an argument."),
        ));
    }
    if !scope.has(scope_of(parent)) {
        return Err(ConfError::new(
            line,
            col,
            &format!("<{n}> is not allowed inside <{}>.", parent.unwrap_or("")),
        ));
    }
    if n.eq_ignore_ascii_case("VirtualHost") {
        for a in args {
            listen_addr(a).map_err(|e| ConfError::new(line, col, &e))?;
        }
    }
    Ok(())
}

/// Check a directive and its arguments.
#[allow(clippy::too_many_lines)]
pub fn check_directive(
    name: &str,
    args: &[String],
    line: u32,
    col: u32,
    parent: Option<&str>,
) -> Result<(), ConfError> {
    let Some(rule) = rule(name) else {
        return Err(ConfError::new(
            line,
            col,
            &format!("{name} is not a directive this server knows."),
        ));
    };
    let here = scope_of(parent);
    if !rule.scope.has(here) {
        let place = parent.map_or("outside a section".to_string(), |p| format!("inside <{p}>"));
        return Err(ConfError::new(
            line,
            col,
            &format!("{} is not allowed {place}.", rule.name),
        ));
    }
    if args.len() < rule.min || args.len() > rule.max {
        let want = match (rule.min, rule.max) {
            (a, b) if a == b => format!("{a}"),
            (a, ANY) => format!("at least {a}"),
            (a, b) => format!("{a} to {b}"),
        };
        return Err(ConfError::new(
            line,
            col,
            &format!(
                "{} takes {want} argument(s), not {}.",
                rule.name,
                args.len()
            ),
        ));
    }
    let bad = |m: String| ConfError::new(line, col, &m);
    match rule.name {
        "Listen" => {
            listen_addr(&args[0]).map_err(bad)?;
        }
        "ThreadsPerChild"
        | "TimeOut"
        | "KeepAliveTimeout"
        | "MaxKeepAliveRequests"
        | "DefaultFsCCSID"
        | "DefaultNetCCSID"
        | "LogCCSID" => {
            if args[0].parse::<u32>().is_err() {
                return Err(bad(format!(
                    "{} needs a number, not {}.",
                    rule.name, args[0]
                )));
            }
        }
        "SSLEngine" | "KeepAlive" | "HostNameLookups" | "UseCanonicalName" | "ServerSignature" => {
            one_of(rule.name, &args[0], &["On", "Off"]).map_err(bad)?;
        }
        "LogLevel" => {
            one_of(
                rule.name,
                &args[0],
                &[
                    "emerg", "alert", "crit", "error", "warn", "notice", "info", "debug",
                ],
            )
            .map_err(bad)?;
        }
        "Order" => {
            one_of(
                rule.name,
                &args[0],
                &["Allow,Deny", "Deny,Allow", "Mutual-failure"],
            )
            .map_err(bad)?;
        }
        "Allow" | "Deny" => {
            if !args[0].eq_ignore_ascii_case("from") {
                return Err(bad(format!("{} must be followed by From.", rule.name)));
            }
        }
        "AuthType" => {
            one_of(rule.name, &args[0], &["Basic"]).map_err(bad)?;
        }
        // LogMaint path keep-files size-MB [HH:MM]: size 0 rotates daily,
        // at the time given (00:00 by default).
        "LogMaint" => {
            for a in &args[1..3] {
                if a.parse::<u32>().is_err() {
                    return Err(bad(format!("LogMaint needs a number, not {a}.")));
                }
            }
            if let Some(t) = args.get(3)
                && !is_time(t)
            {
                return Err(bad(format!(
                    "LogMaint needs a time of day as HH:MM, not {t}."
                )));
            }
        }
        "SSLProtocolDisable" => {
            for a in args {
                one_of(
                    rule.name,
                    a,
                    &["SSLv2", "SSLv3", "TLSv1", "TLSv1.1", "TLSv1.2", "TLSv1.3"],
                )
                .map_err(bad)?;
            }
        }
        "SSLClientAuth" => {
            one_of(
                rule.name,
                &args[0],
                &["None", "0", "Optional", "1", "Required", "2"],
            )
            .map_err(bad)?;
        }
        _ => {}
    }
    Ok(())
}

/// Whether `t` is a time of day, `HH:MM`.
pub fn is_time(t: &str) -> bool {
    t.split_once(':').is_some_and(|(h, m)| {
        h.len() == 2
            && m.len() == 2
            && h.parse::<u8>().is_ok_and(|h| h < 24)
            && m.parse::<u8>().is_ok_and(|m| m < 60)
    })
}

fn one_of(name: &str, v: &str, allowed: &[&str]) -> Result<(), String> {
    if allowed.iter().any(|a| a.eq_ignore_ascii_case(v)) {
        Ok(())
    } else {
        Err(format!(
            "{name} does not accept {v}; use {}.",
            allowed.join(", ")
        ))
    }
}

/// The address and port of a `Listen` or `<VirtualHost>` argument:
/// `80`, `*:80`, `10.0.0.1:80`, `[::1]:80`. The address is `None` for
/// every address.
pub fn listen_addr(s: &str) -> Result<(Option<String>, u16), String> {
    let (addr, port) = if let Some(rest) = s.strip_prefix('[') {
        let (a, p) = rest
            .split_once("]:")
            .ok_or_else(|| format!("{s} is not an address and port."))?;
        (Some(a.to_string()), p)
    } else if let Some((a, p)) = s.rsplit_once(':') {
        (if a == "*" { None } else { Some(a.to_string()) }, p)
    } else {
        (None, s)
    };
    let port: u16 = port
        .parse()
        .ok()
        .filter(|p| *p > 0)
        .ok_or_else(|| format!("{s} does not name a port from 1 to 65535."))?;
    if let Some(a) = &addr
        && a.parse::<std::net::IpAddr>().is_err()
    {
        return Err(format!("{a} is not an IP address."));
    }
    Ok((addr, port))
}
