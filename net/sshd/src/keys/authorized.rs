// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `authorized_keys`: one key per line, optional options first.
//! Honored: `from=`, `command=`, `no-pty`, `no-port-forwarding`,
//! `restrict`; accepted without effect: `no-agent-forwarding`,
//! `no-X11-forwarding`, `no-user-rc`, `environment=`. A line with any
//! other option is skipped, as OpenSSH does.

use super::public::PublicKey;
use crate::wire::unb64;

/// What a key line allows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyOptions {
    /// `from="pattern-list"`: client addresses the key works from.
    pub from: Option<String>,
    /// `command="..."`: the command forced on every session.
    pub command: Option<String>,
    /// `no-pty` or `restrict`.
    pub no_pty: bool,
    /// `no-port-forwarding` or `restrict`.
    pub no_port_forwarding: bool,
}

/// One usable line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedKey {
    /// The key.
    pub key: PublicKey,
    /// Its options.
    pub options: KeyOptions,
    /// The comment, if any.
    pub comment: String,
}

const TYPES: &[&str] = &["ssh-ed25519", "ecdsa-sha2-nistp256", "ssh-rsa"];

/// Split the options field off a line: (options, rest).
fn split_options(line: &str) -> (&str, &str) {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ' ' | '\t' if !quoted => return (&line[..i], line[i..].trim_start()),
            _ => {}
        }
    }
    (line, "")
}

fn parse_options(text: &str) -> Option<KeyOptions> {
    let mut o = KeyOptions::default();
    let mut items = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                cur.push(c);
            }
            ',' if !quoted => items.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    items.push(cur);
    for item in items {
        let (name, value) = match item.split_once('=') {
            Some((n, v)) => (n.to_ascii_lowercase(), Some(v.trim_matches('"').to_string())),
            None => (item.to_ascii_lowercase(), None),
        };
        match (name.as_str(), value) {
            ("from", Some(v)) => o.from = Some(v),
            ("command", Some(v)) => o.command = Some(v.replace("\\\"", "\"")),
            ("no-pty", None) => o.no_pty = true,
            ("no-port-forwarding", None) => o.no_port_forwarding = true,
            ("restrict", None) => {
                o.no_pty = true;
                o.no_port_forwarding = true;
            }
            ("no-agent-forwarding" | "no-x11-forwarding" | "no-user-rc", None)
            | ("environment", Some(_)) => {}
            _ => return None,
        }
    }
    Some(o)
}

/// Parse one line; `None` for blanks, comments and lines not usable.
#[must_use]
pub fn parse_line(line: &str) -> Option<AuthorizedKey> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let first = line.split_whitespace().next()?;
    let (options, rest) = if TYPES.contains(&first) {
        (KeyOptions::default(), line)
    } else {
        let (opts, rest) = split_options(line);
        (parse_options(opts)?, rest)
    };
    let mut parts = rest.splitn(3, char::is_whitespace);
    let kind = parts.next()?;
    let blob = unb64(parts.next()?)?;
    let key = PublicKey::from_blob(&blob)?;
    if key.kind() != kind {
        return None;
    }
    Some(AuthorizedKey {
        key,
        options,
        comment: parts.next().unwrap_or("").trim().to_string(),
    })
}

/// Every usable line of a file.
#[must_use]
pub fn parse(text: &str) -> Vec<AuthorizedKey> {
    text.lines().filter_map(parse_line).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::host::{KeyKind, PrivateKey};

    #[test]
    fn lines_with_and_without_options() {
        let k = PrivateKey::generate(KeyKind::Ed25519).unwrap().public();
        let plain = format!("{} me@pc", k.openssh());
        let a = parse_line(&plain).unwrap();
        assert_eq!(a.key, k);
        assert_eq!(a.comment, "me@pc");
        let opts = format!(
            "from=\"10.0.0.*\",command=\"echo hi there\",no-pty {}",
            k.openssh()
        );
        let b = parse_line(&opts).unwrap();
        assert_eq!(b.options.from.as_deref(), Some("10.0.0.*"));
        assert_eq!(b.options.command.as_deref(), Some("echo hi there"));
        assert!(b.options.no_pty);
        assert!(parse_line(&format!("tunnel=\"1\" {}", k.openssh())).is_none());
        assert!(parse_line("# comment").is_none());
        assert!(parse_line("ssh-ed25519 notbase64!").is_none());
    }
}
