// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The LDAP tools (docs/DIRSRV.md §13): `ldapsearch`, `ldapadd`,
//! `ldapmodify`, `ldapdelete`, `ldapmodrdn`, `ldapchangepwd`, `ldapexop`,
//! `ldapcompare` and `ldapdiff`, with IBM's option letters, over the
//! `ldap` crate's client. The tools connect with the job's own sockets
//! (the client is a blocking one over a TCP stream); files and the
//! terminal go through the `Sys` seam as every utility's do. Secure
//! connections (`-Z` LDAPS, `-Y` `StartTLS`) take their configuration
//! from the hook the job sets ([`set_tls`]): the `*SYSTEM` certificate
//! store. What QS/400 refuses (`DIGEST-MD5`, `CRAM-MD5`, `GSSAPI`, their
//! `-G` realm and `-U` user, debug traces, IBM's own controls) gives a
//! message and status 92 (`LDAP_NOT_SUPPORTED`).

pub mod compare;
pub mod delete;
pub mod diff;
pub mod exop;
pub mod modify;
pub mod modrdn;
pub mod passwd;
pub mod search;

use std::sync::{Arc, Mutex};

use ldap::client::{Client, ClientError, Options};
use ldap::control::{Control, ControlValue, ManageDsaIt, ProxiedAuthz};
use qsh::sys::UtilCtx;
use tls::rustls::ClientConfig;

use crate::common::complain;
use crate::common::io::{Lines, read_input, stdin};

/// What a secure connection asks the job for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TlsAsk {
    /// `-K`: the key database (none: the `*SYSTEM` store).
    pub keyfile: Option<String>,
    /// `-P`: its password.
    pub password: Option<String>,
    /// `-N`: the client certificate's label.
    pub label: Option<String>,
}

/// Where the tools get TLS client configurations.
pub type TlsHook = Arc<dyn Fn(&TlsAsk) -> Result<Arc<ClientConfig>, String> + Send + Sync>;

static TLS: Mutex<Option<TlsHook>> = Mutex::new(None);

/// Sets the hook (the job sets it when QSH starts).
pub fn set_tls(h: Option<TlsHook>) {
    *TLS.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = h;
}

fn tls(ask: &TlsAsk) -> Result<Arc<ClientConfig>, String> {
    let h = TLS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    match h {
        Some(f) => f(ask),
        None => Err("secure connections are not available in this job".into()),
    }
}

/// Exit status: a parameter error.
pub const PARAM: i32 = 89;
/// Exit status: refused.
pub const REFUSED: i32 = 92;
/// Exit status: the server could not be reached.
pub const DOWN: i32 = 81;

/// The options every tool takes, and the operands after them.
#[derive(Debug, Clone, Default)]
pub struct Args {
    /// Each option given, in order, with its argument.
    pub opts: Vec<(String, Option<String>)>,
    /// The operands.
    pub operands: Vec<String>,
}

impl Args {
    /// Whether `-name` was given.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.opts.iter().any(|(o, _)| o == name)
    }

    /// The last argument of `-name`.
    #[must_use]
    pub fn value(&self, name: &str) -> Option<&str> {
        self.opts
            .iter()
            .rev()
            .find(|(o, _)| o == name)
            .and_then(|(_, v)| v.as_deref())
    }

    /// Every argument of `-name`.
    #[must_use]
    pub fn values(&self, name: &str) -> Vec<&str> {
        self.opts
            .iter()
            .filter(|(o, _)| o == name)
            .filter_map(|(_, v)| v.as_deref())
            .collect()
    }
}

/// The connection options every tool takes (`name:` takes an argument).
pub const CONNECT: &[&str] = &[
    "h:", "p:", "D:", "w:", "V:", "Z", "Y", "K:", "P:", "N:", "m:", "U:", "G:", "y:", "R", "M",
    "O:", "C:", "d:", "v", "n", "k", "x",
];

/// Parses `args` against `spec` (names, `:` after one that takes an
/// argument). Names longer than a letter match whole (`-op`, `-sh`);
/// letters may be clustered (`-vM`) with an argument attached or next.
/// Parsing stops at the first operand or `--`.
pub fn parse(args: &[String], spec: &[&str]) -> Result<Args, String> {
    let takes = |n: &str| -> Option<bool> {
        spec.iter().find_map(|s| {
            let (name, arg) = s.strip_suffix(':').map_or((*s, false), |n| (n, true));
            (name == n).then_some(arg)
        })
    };
    let mut out = Args::default();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            out.operands.extend(args[i + 1..].iter().cloned());
            break;
        }
        let Some(body) = a.strip_prefix('-').filter(|b| !b.is_empty()) else {
            out.operands.extend(args[i..].iter().cloned());
            break;
        };
        if body.len() > 1
            && let Some(arg) = takes(body)
        {
            if arg {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| format!("Option -{body} needs an argument."))?;
                out.opts.push((body.to_string(), Some(v.clone())));
            } else {
                out.opts.push((body.to_string(), None));
            }
            i += 1;
            continue;
        }
        let chars: Vec<char> = body.chars().collect();
        let mut k = 0;
        while k < chars.len() {
            let c = chars[k].to_string();
            match takes(&c) {
                None => return Err(format!("Option -{c} is not valid.")),
                Some(false) => out.opts.push((c, None)),
                Some(true) => {
                    let rest: String = chars[k + 1..].iter().collect();
                    let v = if rest.is_empty() {
                        i += 1;
                        args.get(i)
                            .cloned()
                            .ok_or_else(|| format!("Option -{c} needs an argument."))?
                    } else {
                        rest
                    };
                    out.opts.push((c, Some(v)));
                    break;
                }
            }
            k += 1;
        }
        i += 1;
    }
    Ok(out)
}

/// The refusals every tool shares: what an option asks for that QS/400
/// does not build. None when nothing is refused.
#[must_use]
pub fn refused(a: &Args) -> Option<String> {
    if a.has("G") {
        return Some(
            "-G (a DIGEST-MD5 realm) is not supported: DIGEST-MD5 is not available.".into(),
        );
    }
    if a.has("U") {
        return Some(
            "-U (a SASL user name) is not supported: only SASL EXTERNAL binds are available."
                .into(),
        );
    }
    if let Some(m) = a.value("m")
        && !m.eq_ignore_ascii_case("EXTERNAL")
    {
        return Some(format!("SASL mechanism {m} is not supported; EXTERNAL is."));
    }
    if a.has("d") {
        return Some(
            "-d (debug traces) is not supported; use the communications trace instead.".into(),
        );
    }
    if a.has("k") {
        return Some("-k (the server administration control) is not supported.".into());
    }
    if a.has("x") {
        return Some("-x is not supported.".into());
    }
    None
}

/// A tool's run: its name, the standard output and status.
pub struct Tool<'c, 'a> {
    /// The context.
    pub cx: &'c UtilCtx<'a>,
    /// The tool's name.
    pub name: &'static str,
}

impl Tool<'_, '_> {
    /// Writes a line to standard output.
    pub fn say(&self, s: &str) {
        let _ = self.cx.out(s);
    }

    /// Reports `text` and returns `status`.
    pub fn fail(&self, status: i32, text: &str) -> i32 {
        complain(self.cx, self.name, text);
        status
    }

    /// Reports a usage error with the tool's usage.
    pub fn usage(&self, text: &str, usage: &str) -> i32 {
        complain(self.cx, self.name, text);
        let _ = self.cx.err(usage);
        PARAM
    }

    /// Verbose output (`-v`) to standard error.
    pub fn verbose(&self, a: &Args, text: &str) {
        if a.has("v") {
            let _ = self.cx.err(&format!("{}: {text}\n", self.name));
        }
    }
}

/// The password `-w` gives: `?` reads a line from standard input.
fn password(cx: &UtilCtx<'_>, a: &Args) -> Option<String> {
    let w = a.value("w")?;
    if w != "?" {
        return Some(w.to_string());
    }
    let _ = cx.err("Password: ");
    let mut l = Lines::new(stdin(cx));
    let line = l.next_line().ok().flatten().unwrap_or_default();
    Some(
        String::from_utf8_lossy(&line)
            .trim_end_matches(['\r', '\n'])
            .to_string(),
    )
}

/// The status of a client error.
#[must_use]
pub fn status_of(e: &ClientError) -> i32 {
    match e {
        ClientError::Result(r) => i32::try_from(r.code.number()).unwrap_or(80),
        ClientError::Io(_) | ClientError::Closed | ClientError::Disconnected(_) => DOWN,
        ClientError::Timeout => 85,
        ClientError::Tls(_) => 0x74,
        ClientError::Unsupported(_) => REFUSED,
        ClientError::HopLimit => 0x61,
        _ => 84,
    }
}

/// The controls every operation carries: `-M` and `-y`.
#[must_use]
pub fn controls(a: &Args) -> Vec<Control> {
    let mut v = Vec::new();
    if a.has("M") {
        v.push(ManageDsaIt.to_control(true));
    }
    if let Some(p) = a.value("y") {
        let id = if p.starts_with("dn:") || p.starts_with("u:") {
            p.to_string()
        } else {
            format!("dn:{p}")
        };
        v.push(ProxiedAuthz(id).to_control(true));
    }
    v
}

/// The `*SYSTEM` store's path; the only key database the tools open.
pub const SYSTEM_STORE: &str = "/QIBM/UserData/ICSS/Cert/Server/DEFAULT.KDB";

/// Connects and binds as the options say; the client, or the status
/// after a message.
pub fn connect(t: &Tool<'_, '_>, a: &Args) -> Result<Client, i32> {
    if let Some(r) = refused(a) {
        return Err(t.fail(REFUSED, &r));
    }
    let host = a.value("h").unwrap_or("localhost").to_string();
    let secure = a.has("Z");
    let port = match a.value("p") {
        Some(p) => p
            .parse::<u16>()
            .map_err(|_| t.fail(PARAM, &format!("The port {p} is not valid.")))?,
        None if secure => 636,
        None => 389,
    };
    let mut opts = Options {
        referrals: !a.has("R"),
        ..Options::default()
    };
    if let Some(v) = a.value("V") {
        opts.version = match v {
            "2" => 2,
            "3" => 3,
            _ => {
                return Err(t.fail(
                    PARAM,
                    &format!("The LDAP version {v} is not valid; give 2 or 3."),
                ));
            }
        };
    }
    if let Some(h) = a.value("O") {
        opts.hop_limit = h
            .parse()
            .map_err(|_| t.fail(PARAM, &format!("The hop limit {h} is not valid.")))?;
    }
    let ask = TlsAsk {
        keyfile: a.value("K").map(str::to_string),
        password: a.value("P").map(str::to_string),
        label: a.value("N").map(str::to_string),
    };
    if let Some(k) = &ask.keyfile
        && !k.eq_ignore_ascii_case(SYSTEM_STORE)
    {
        return Err(t.fail(
            REFUSED,
            &format!("The key database {k} is not supported: QSH tools use the *SYSTEM certificate store."),
        ));
    }
    let cfg =
        if secure || a.has("Y") {
            Some(tls(&ask).map_err(|e| {
                t.fail(0x71, &format!("A secure connection cannot be set up: {e}."))
            })?)
        } else {
            None
        };
    opts.tls.clone_from(&cfg);
    t.verbose(
        a,
        &format!(
            "connecting to {host}:{port}{}",
            if secure { " with TLS" } else { "" }
        ),
    );
    let mut c = match (&cfg, secure) {
        (Some(c), true) => Client::connect_tls(&host, port, c.clone(), opts),
        _ => Client::connect(&host, port, opts),
    }
    .map_err(|e| t.fail(status_of(&e), &format!("{host}:{port}: {e}")))?;
    if a.has("Y") {
        let cfg = cfg.expect("made above");
        c.start_tls(cfg)
            .map_err(|e| t.fail(status_of(&e), &format!("StartTLS: {e}")))?;
    }
    let ctrls = controls(a);
    if a.value("m").is_some() {
        let dn = a.value("D").unwrap_or("");
        let (r, _) = c
            .sasl_bind(dn, "EXTERNAL", None, &ctrls)
            .map_err(|e| t.fail(status_of(&e), &format!("bind: {e}")))?;
        if !r.result.is_success() {
            return Err(t.fail(code(&r.result), &format!("bind: {}", text(&r.result))));
        }
    } else if let Some(dn) = a.value("D") {
        let pw = password(t.cx, a).unwrap_or_default();
        t.verbose(a, &format!("binding as {dn}"));
        let r = c
            .simple_bind(dn, pw.as_bytes())
            .map_err(|e| t.fail(status_of(&e), &format!("bind: {e}")))?;
        if !r.result.is_success() {
            return Err(t.fail(code(&r.result), &format!("bind: {}", text(&r.result))));
        }
    }
    Ok(c)
}

/// A result's code as a status.
#[must_use]
pub fn code(r: &ldap::LdapResult) -> i32 {
    i32::try_from(r.code.number()).unwrap_or(80)
}

/// A result's text: its code's and the server's message.
#[must_use]
pub fn text(r: &ldap::LdapResult) -> String {
    let mut s = r.code.text().to_string();
    if !r.message.is_empty() {
        s.push_str(" (");
        s.push_str(&r.message);
        s.push(')');
    }
    if !r.matched_dn.is_empty() {
        s.push_str(&format!("; matched {}", r.matched_dn));
    }
    s
}

/// Input in a `-C` character set as UTF-8 text.
pub fn decode_charset(a: &Args, b: &[u8]) -> Result<Vec<u8>, String> {
    let Some(cs) = a.value("C") else {
        return Ok(b.to_vec());
    };
    let n = cs.trim().to_ascii_uppercase().replace('_', "-");
    match n.as_str() {
        "UTF-8" | "UTF8" => Ok(b.to_vec()),
        "ISO8859-1" | "ISO-8859-1" | "LATIN1" => Ok(b
            .iter()
            .map(|&c| char::from(c))
            .collect::<String>()
            .into_bytes()),
        _ => {
            let cp = n
                .strip_prefix("IBM-")
                .or_else(|| n.strip_prefix("CP"))
                .unwrap_or(&n)
                .parse::<u16>()
                .map_err(|_| format!("The character set {cs} is not known."))?;
            ccsid::conv::to_string(ccsid::conv::ccsid_of_codepage(cp), b)
                .map(String::into_bytes)
                .map_err(|_| format!("The character set {cs} is not supported."))
        }
    }
}

/// The input of `-f`/`-i` (standard input when neither).
pub fn input(t: &Tool<'_, '_>, a: &Args) -> Result<Vec<u8>, i32> {
    let f = a.value("f").or(a.value("i")).unwrap_or("-");
    let b = read_input(t.cx, f).map_err(|e| t.fail(PARAM, &format!("{f}: {}", e.errno.text())))?;
    decode_charset(a, &b).map_err(|e| t.fail(PARAM, &e))
}

/// Unbinds, ignoring a connection already gone.
pub fn close(c: Client) {
    let _ = c.unbind();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn options_long_and_clustered() {
        let a = parse(
            &v(&["-vMh", "host", "-sh", "a", "-p389", "x", "-v"]),
            &["v", "M", "h:", "sh:", "p:"],
        )
        .unwrap();
        assert!(a.has("v") && a.has("M"));
        assert_eq!(a.value("h"), Some("host"));
        assert_eq!(a.value("sh"), Some("a"));
        assert_eq!(a.value("p"), Some("389"));
        assert_eq!(a.operands, ["x", "-v"]);
        assert!(parse(&v(&["-q"]), &["v"]).is_err());
        assert!(parse(&v(&["-h"]), &["h:"]).is_err());
        let r = parse(&v(&["-m", "DIGEST-MD5"]), CONNECT).unwrap();
        assert!(refused(&r).unwrap().contains("DIGEST-MD5"));
        let ok = parse(&v(&["-m", "external"]), CONNECT).unwrap();
        assert!(refused(&ok).is_none());
    }
}
