// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `sshd_config`: the subset of OpenSSH's directives the server honors,
//! parsed by our own reader. Keywords are case-insensitive, `Keyword
//! value` or `Keyword=value`; for single-valued keywords the first
//! value wins, as in OpenSSH. Unknown directives are accepted, ignored
//! and noted. `Match` blocks are not supported: the block and the rest
//! of the file are ignored with a note.

pub mod paths;
pub mod pattern;

use std::fmt;

/// `PermitRootLogin`, applied to `QSECOFR`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootLogin {
    /// `yes`.
    Yes,
    /// `no`.
    No,
    /// `prohibit-password` (or `without-password`): keys only.
    ProhibitPassword,
    /// `forced-commands-only`: keys with `command=` only.
    ForcedCommandsOnly,
}

/// `AllowTcpForwarding`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forwarding {
    /// `no`: the QS/400 default.
    No,
    /// `local`: `-L` only.
    Local,
    /// `remote`: `-R` only.
    Remote,
    /// `yes` or `all`.
    All,
}

impl Forwarding {
    /// Local (`direct-tcpip`) forwarding allowed.
    #[must_use]
    pub fn local(self) -> bool {
        matches!(self, Self::Local | Self::All)
    }

    /// Remote (`tcpip-forward`) forwarding allowed.
    #[must_use]
    pub fn remote(self) -> bool {
        matches!(self, Self::Remote | Self::All)
    }
}

/// One `ListenAddress`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listen {
    /// The address (`0.0.0.0`, `::`, a host address).
    pub host: String,
    /// Its port, if given; otherwise every `Port`.
    pub port: Option<u16>,
}

/// Something the parser noticed and did not act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// The line number, from 1.
    pub line: usize,
    /// What.
    pub text: String,
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.text)
    }
}

/// The server's configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Config {
    /// `Port`: default 22.
    pub ports: Vec<u16>,
    /// `ListenAddress`: default every IPv4 and IPv6 address.
    pub listen: Vec<Listen>,
    /// `PermitRootLogin`: default `prohibit-password`.
    pub permit_root_login: RootLogin,
    /// `PasswordAuthentication`: default yes.
    pub password_authentication: bool,
    /// `PubkeyAuthentication`: default yes.
    pub pubkey_authentication: bool,
    /// `PermitEmptyPasswords`: default no (QS/400 profiles always have
    /// a password or `*NONE`; kept for the refusal).
    pub permit_empty_passwords: bool,
    /// `AuthorizedKeysFile`: relative to the home directory unless
    /// absolute; `%h`, `%u` and `%%` expanded.
    pub authorized_keys_files: Vec<String>,
    /// `AllowUsers`.
    pub allow_users: Vec<String>,
    /// `DenyUsers`.
    pub deny_users: Vec<String>,
    /// `AllowGroups`.
    pub allow_groups: Vec<String>,
    /// `DenyGroups`.
    pub deny_groups: Vec<String>,
    /// `Subsystem`: name and command.
    pub subsystems: Vec<(String, String)>,
    /// `ClientAliveInterval` in seconds, 0 off.
    pub client_alive_interval: u32,
    /// `ClientAliveCountMax`: default 3.
    pub client_alive_count_max: u32,
    /// `MaxAuthTries`: default 6.
    pub max_auth_tries: u32,
    /// `MaxSessions`: default 10.
    pub max_sessions: u32,
    /// `LoginGraceTime` in seconds: default 120, 0 none.
    pub login_grace_time: u32,
    /// `Banner`: a path, or `None` for `none`.
    pub banner: Option<String>,
    /// `HostKey`: empty means the three default files.
    pub host_keys: Vec<String>,
    /// `AllowTcpForwarding`: default no.
    pub allow_tcp_forwarding: Forwarding,
    /// `AcceptEnv` patterns.
    pub accept_env: Vec<String>,
    /// What the parser did not act on.
    pub notes: Vec<Note>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ports: vec![22],
            listen: Vec::new(),
            permit_root_login: RootLogin::ProhibitPassword,
            password_authentication: true,
            pubkey_authentication: true,
            permit_empty_passwords: false,
            authorized_keys_files: vec![
                ".ssh/authorized_keys".into(),
                ".ssh/authorized_keys2".into(),
            ],
            allow_users: Vec::new(),
            deny_users: Vec::new(),
            allow_groups: Vec::new(),
            deny_groups: Vec::new(),
            subsystems: Vec::new(),
            client_alive_interval: 0,
            client_alive_count_max: 3,
            max_auth_tries: 6,
            max_sessions: 10,
            login_grace_time: 120,
            banner: None,
            host_keys: Vec::new(),
            allow_tcp_forwarding: Forwarding::No,
            accept_env: Vec::new(),
            notes: Vec::new(),
        }
    }
}

/// Split a line into words, honoring double quotes.
fn words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(cur);
    }
    out
}

fn yes_no(v: &str) -> Option<bool> {
    match v.to_ascii_lowercase().as_str() {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

/// `sshd_config` time: a number with optional `s m h d w` suffixes,
/// summed (`1h30m`).
#[must_use]
pub fn seconds(v: &str) -> Option<u32> {
    let mut total: u64 = 0;
    let mut num = String::new();
    for c in v.chars() {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        let unit = match c.to_ascii_lowercase() {
            's' => 1,
            'm' => 60,
            'h' => 3600,
            'd' => 86_400,
            'w' => 604_800,
            _ => return None,
        };
        total += num.parse::<u64>().ok()? * unit;
        num.clear();
    }
    if !num.is_empty() {
        total += num.parse::<u64>().ok()?;
    }
    u32::try_from(total).ok()
}

fn listen(v: &str) -> Option<Listen> {
    if let Some(rest) = v.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        let port = match after.strip_prefix(':') {
            Some(p) => Some(p.parse().ok()?),
            None if after.is_empty() => None,
            None => return None,
        };
        return Some(Listen {
            host: host.into(),
            port,
        });
    }
    if v.matches(':').count() == 1 {
        let (h, p) = v.split_once(':')?;
        return Some(Listen {
            host: h.into(),
            port: Some(p.parse().ok()?),
        });
    }
    Some(Listen {
        host: v.into(),
        port: None,
    })
}

/// Directives recognized and deliberately without effect here.
const IGNORED: &[&str] = &[
    "protocol",
    "syslogfacility",
    "loglevel",
    "usepam",
    "useprivilegeseparation",
    "printmotd",
    "printlastlog",
    "pidfile",
    "x11forwarding",
    "allowagentforwarding",
    "tcpkeepalive",
    "compression",
    "usedns",
    "strictmodes",
    "ignorerhosts",
    "hostbasedauthentication",
    "kbdinteractiveauthentication",
    "challengeresponseauthentication",
    "gatewayports",
    "permittunnel",
    "maxstartups",
];

impl Config {
    /// Parse a configuration file's text.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn parse(text: &str) -> Self {
        let mut c = Self::default();
        let mut seen: Vec<String> = Vec::new();
        let mut ports_set = false;
        let mut first = |k: &str| {
            if seen.iter().any(|s| s == k) {
                false
            } else {
                seen.push(k.to_string());
                true
            }
        };
        for (i, raw) in text.lines().enumerate() {
            let line_no = i + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (kw, rest) = match line.find(|ch: char| ch.is_whitespace() || ch == '=') {
                Some(p) => {
                    let rest = line[p..].trim_start();
                    let rest = rest.strip_prefix('=').unwrap_or(rest).trim_start();
                    (&line[..p], rest)
                }
                None => (line, ""),
            };
            let kw = kw.to_ascii_lowercase();
            let args = words(rest);
            let note = |c: &mut Self, text: String| {
                c.notes.push(Note {
                    line: line_no,
                    text,
                });
            };
            let Some(v0) = args.first().cloned() else {
                note(&mut c, format!("{kw} has no value; ignored"));
                continue;
            };
            let bad = |c: &mut Self| note(c, format!("{kw}: value {v0} not valid; ignored"));
            match kw.as_str() {
                "match" => {
                    note(&mut c, "Match blocks are not supported; the rest of the file is ignored".into());
                    break;
                }
                "port" => match v0.parse::<u16>() {
                    Ok(p) if p > 0 => {
                        if !ports_set {
                            c.ports.clear();
                            ports_set = true;
                        }
                        c.ports.push(p);
                    }
                    _ => bad(&mut c),
                },
                "listenaddress" => match listen(&v0) {
                    Some(l) => c.listen.push(l),
                    None => bad(&mut c),
                },
                "permitrootlogin" => {
                    let v = match v0.to_ascii_lowercase().as_str() {
                        "yes" => Some(RootLogin::Yes),
                        "no" => Some(RootLogin::No),
                        "prohibit-password" | "without-password" => {
                            Some(RootLogin::ProhibitPassword)
                        }
                        "forced-commands-only" => Some(RootLogin::ForcedCommandsOnly),
                        _ => None,
                    };
                    match v {
                        Some(v) if first(&kw) => c.permit_root_login = v,
                        Some(_) => {}
                        None => bad(&mut c),
                    }
                }
                "passwordauthentication" | "pubkeyauthentication" | "permitemptypasswords" => {
                    match yes_no(&v0) {
                        Some(v) if first(&kw) => match kw.as_str() {
                            "passwordauthentication" => c.password_authentication = v,
                            "pubkeyauthentication" => c.pubkey_authentication = v,
                            _ => c.permit_empty_passwords = v,
                        },
                        Some(_) => {}
                        None => bad(&mut c),
                    }
                }
                "authorizedkeysfile" => {
                    if first(&kw) {
                        c.authorized_keys_files =
                            args.iter().filter(|a| *a != "none").cloned().collect();
                    }
                }
                "allowusers" => c.allow_users.extend(args),
                "denyusers" => c.deny_users.extend(args),
                "allowgroups" => c.allow_groups.extend(args),
                "denygroups" => c.deny_groups.extend(args),
                "acceptenv" => c.accept_env.extend(args),
                "subsystem" => {
                    if args.len() < 2 {
                        bad(&mut c);
                    } else if c.subsystems.iter().any(|(n, _)| *n == v0) {
                        note(&mut c, format!("Subsystem {v0} given twice; the first is used"));
                    } else {
                        c.subsystems.push((v0.clone(), args[1..].join(" ")));
                    }
                }
                "clientaliveinterval" | "logingracetime" => match seconds(&v0) {
                    Some(s) if first(&kw) => {
                        if kw == "logingracetime" {
                            c.login_grace_time = s;
                        } else {
                            c.client_alive_interval = s;
                        }
                    }
                    Some(_) => {}
                    None => bad(&mut c),
                },
                "clientalivecountmax" | "maxauthtries" | "maxsessions" => match v0.parse::<u32>() {
                    Ok(n) if first(&kw) => match kw.as_str() {
                        "clientalivecountmax" => c.client_alive_count_max = n,
                        "maxauthtries" => c.max_auth_tries = n,
                        _ => c.max_sessions = n,
                    },
                    Ok(_) => {}
                    Err(_) => bad(&mut c),
                },
                "banner" => {
                    if first(&kw) {
                        c.banner = (!v0.eq_ignore_ascii_case("none")).then_some(v0);
                    }
                }
                "hostkey" => c.host_keys.push(v0),
                "allowtcpforwarding" => {
                    let v = match v0.to_ascii_lowercase().as_str() {
                        "yes" | "all" => Some(Forwarding::All),
                        "no" => Some(Forwarding::No),
                        "local" => Some(Forwarding::Local),
                        "remote" => Some(Forwarding::Remote),
                        _ => None,
                    };
                    match v {
                        Some(v) if first(&kw) => c.allow_tcp_forwarding = v,
                        Some(_) => {}
                        None => bad(&mut c),
                    }
                }
                k if IGNORED.contains(&k) => {
                    note(&mut c, format!("{k} has no effect on QS/400; ignored"));
                }
                k => note(&mut c, format!("{k} is not supported; ignored")),
            }
        }
        c
    }

    /// The host key files: `HostKey`, or the defaults beside the file.
    #[must_use]
    pub fn host_key_paths(&self) -> Vec<String> {
        if self.host_keys.is_empty() {
            paths::default_host_keys()
        } else {
            self.host_keys.clone()
        }
    }

    /// The `(address, port)` pairs to listen on.
    #[must_use]
    pub fn binds(&self) -> Vec<(String, u16)> {
        let mut out = Vec::new();
        let hosts: Vec<Listen> = if self.listen.is_empty() {
            vec![
                Listen {
                    host: "0.0.0.0".into(),
                    port: None,
                },
                Listen {
                    host: "::".into(),
                    port: None,
                },
            ]
        } else {
            self.listen.clone()
        };
        for l in hosts {
            match l.port {
                Some(p) => out.push((l.host.clone(), p)),
                None => out.extend(self.ports.iter().map(|p| (l.host.clone(), *p))),
            }
        }
        out
    }

    /// The command for a subsystem.
    #[must_use]
    pub fn subsystem(&self, name: &str) -> Option<&str> {
        self.subsystems
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, c)| c.as_str())
    }

    /// The `authorized_keys` paths for a user, absolute.
    #[must_use]
    pub fn authorized_keys_for(&self, user: &str, home: &str) -> Vec<String> {
        self.authorized_keys_files
            .iter()
            .map(|f| {
                let mut out = String::new();
                let mut chars = f.chars();
                while let Some(ch) = chars.next() {
                    if ch == '%' {
                        match chars.next() {
                            Some('h') => out.push_str(home),
                            Some('u') => out.push_str(user),
                            Some('%') => out.push('%'),
                            Some(o) => {
                                out.push('%');
                                out.push(o);
                            }
                            None => out.push('%'),
                        }
                    } else {
                        out.push(ch);
                    }
                }
                if out.starts_with('/') {
                    out
                } else {
                    format!("{}/{out}", home.trim_end_matches('/'))
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directives() {
        let c = Config::parse(
            "# comment\n\
             Port 2222\nPort 2223\n\
             ListenAddress 10.0.0.1\nListenAddress [::1]:22\n\
             PermitRootLogin no\nPermitRootLogin yes\n\
             PasswordAuthentication=no\n\
             AuthorizedKeysFile .ssh/authorized_keys /etc/keys/%u\n\
             AllowUsers qstest bob@10.*\nDenyUsers mallory\nAllowGroups qstestgrp\n\
             Subsystem sftp /QOpenSys/QIBM/ProdData/SC1/OpenSSH/libexec/sftp-server\n\
             ClientAliveInterval 1m30s\nMaxAuthTries 3\nBanner /etc/banner\n\
             UsePAM yes\nFrobnicate 1\nMatch User x\nPort 9\n",
        );
        assert_eq!(c.ports, [2222, 2223]);
        assert_eq!(c.binds()[0], ("10.0.0.1".into(), 2222));
        assert_eq!(c.binds()[2], ("::1".into(), 22));
        assert_eq!(c.permit_root_login, RootLogin::No);
        assert!(!c.password_authentication && c.pubkey_authentication);
        assert_eq!(
            c.authorized_keys_for("QSTEST", "/home/QSTEST"),
            ["/home/QSTEST/.ssh/authorized_keys", "/etc/keys/QSTEST"]
        );
        assert_eq!(c.allow_users, ["qstest", "bob@10.*"]);
        assert_eq!(c.subsystem("sftp"), Some("/QOpenSys/QIBM/ProdData/SC1/OpenSSH/libexec/sftp-server"));
        assert_eq!(c.client_alive_interval, 90);
        assert_eq!(c.max_auth_tries, 3);
        assert_eq!(c.banner.as_deref(), Some("/etc/banner"));
        assert_eq!(c.notes.len(), 3, "{:?}", c.notes);
        assert!(c.notes[1].text.contains("frobnicate"));
        assert!(c.notes[2].text.contains("Match"));
    }

    #[test]
    fn defaults() {
        let c = Config::parse("");
        assert_eq!(c.ports, [22]);
        assert_eq!(c.binds().len(), 2);
        assert_eq!(c.permit_root_login, RootLogin::ProhibitPassword);
        assert_eq!(c.allow_tcp_forwarding, Forwarding::No);
        assert_eq!(c.max_auth_tries, 6);
        assert_eq!(c.host_key_paths().len(), 3);
    }
}
