// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The publishing agents as [`PublishAgent`] over `publish.conf`'s
//! [`AgentConfig`] (docs/DIRSRV.md §10.1): one is the other's view, so
//! the file stays the one truth. A blank server is the file's
//! `localhost`, a blank bind or parent DN its absent one, `tls` and
//! `start_tls` its security; the sealed password is the file's alone.

use super::{AdminError, AgentKind, ErrorKind, PublishAgent};
use crate::config::ConfigError;
use crate::publish::config::{
    self, AgentConfig, Kind, PUBLISHER_MAX, PublishConfig, RESYNC_MIN, RETRY_RANGE, Security,
};

/// The host the file names for this system's default instance.
const LOCAL: &str = "localhost";

/// `GLD0204`: a port not 1 to 65535.
pub const BAD_PORT: &str = "GLD0204";
/// `GLD0239`: a server that is not a host name or address.
pub const BAD_SERVER: &str = "GLD0239";

/// The file's agent of `k`.
#[must_use]
pub const fn kind(k: AgentKind) -> Kind {
    match k {
        AgentKind::Users => Kind::Users,
        AgentKind::Printers => Kind::Printers,
    }
}

/// The administration's agent of `k`.
#[must_use]
pub const fn agent_kind(k: Kind) -> AgentKind {
    match k {
        Kind::Users => AgentKind::Users,
        Kind::Printers => AgentKind::Printers,
    }
}

/// One agent as the administration shows it.
#[must_use]
pub fn agent(a: &AgentConfig) -> PublishAgent {
    PublishAgent {
        kind: agent_kind(a.kind),
        enabled: a.enabled,
        server: if a.host.eq_ignore_ascii_case(LOCAL) {
            String::new()
        } else {
            a.host.clone()
        },
        port: a.port,
        tls: a.security != Security::None,
        bind_dn: a.bind_dn.clone().unwrap_or_default(),
        password_set: a.bind_pw.is_some(),
        parent_dn: a.parent_dn.clone().unwrap_or_default(),
        start_tls: a.security == Security::StartTls,
        create_parent: a.create_parent,
        retry: a.retry,
        resync: a.resync,
        publisher: a.publisher.clone().unwrap_or_default(),
    }
}

/// Both agents, users first.
#[must_use]
pub fn agents(c: &PublishConfig) -> Vec<PublishAgent> {
    c.agents.iter().map(agent).collect()
}

fn refusal(kind: ErrorKind, msgid: &str, data: Vec<String>, text: String) -> AdminError {
    AdminError {
        kind,
        msgid: msgid.into(),
        data,
        text,
    }
}

fn bad_value(attr: &str, value: &str) -> AdminError {
    ConfigError::BadValue {
        attr: attr.into(),
        value: value.into(),
    }
    .into()
}

/// A DN value: blank for none.
fn dn(v: &str) -> Result<Option<String>, AdminError> {
    let v = v.trim();
    if v.is_empty() {
        return Ok(None);
    }
    match ldap::Dn::parse(v) {
        Ok(d) if !d.is_root() => Ok(Some(v.to_string())),
        _ => Err(ConfigError::BadDn(v.into()).into()),
    }
}

/// What `to` asks of an agent checked: `GLD0204`, `GLD0239`, `GLD0224`
/// and `GLD0232` (retry, resync, publisher).
///
/// # Errors
/// The first value not taken.
pub fn check(to: &PublishAgent) -> Result<(), AdminError> {
    if to.port == 0 {
        return Err(refusal(
            ErrorKind::Invalid,
            BAD_PORT,
            Vec::new(),
            "The port must be 1 to 65535.".into(),
        ));
    }
    let server = to.server.trim();
    if server.len() > 255
        || server
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || ".-:".contains(c)))
    {
        return Err(refusal(
            ErrorKind::Invalid,
            BAD_SERVER,
            vec![server.into()],
            format!("Server {server} is not a host name or address."),
        ));
    }
    dn(&to.bind_dn)?;
    dn(&to.parent_dn)?;
    if !(RETRY_RANGE.0..=RETRY_RANGE.1).contains(&to.retry) {
        return Err(bad_value("qs400-pubRetry", &to.retry.to_string()));
    }
    if to.resync != 0 && to.resync < RESYNC_MIN {
        return Err(bad_value("qs400-pubResync", &to.resync.to_string()));
    }
    if to.publisher.trim().len() > PUBLISHER_MAX {
        return Err(bad_value("qs400-pubPublisher", to.publisher.trim()));
    }
    Ok(())
}

/// Change `c`'s agent to `to` (checked first); its sealed password
/// stays unless the bind DN is gone, which drops it.
///
/// # Errors
/// As [`check`]; `GLD0232` when the file would not read back.
pub fn apply(c: &mut PublishConfig, to: &PublishAgent) -> Result<(), AdminError> {
    check(to)?;
    let mut next = c.clone();
    let a = next.agent_mut(kind(to.kind));
    a.enabled = to.enabled;
    a.host = match to.server.trim() {
        "" => LOCAL.into(),
        s => s.into(),
    };
    a.port = to.port;
    a.security = match (to.tls, to.start_tls) {
        (false, _) => Security::None,
        (true, false) => Security::Ssl,
        (true, true) => Security::StartTls,
    };
    a.bind_dn = dn(&to.bind_dn)?;
    if a.bind_dn.is_none() {
        a.bind_pw = None;
    }
    a.parent_dn = dn(&to.parent_dn)?;
    a.create_parent = to.create_parent;
    a.retry = to.retry;
    a.resync = to.resync;
    a.publisher = Some(to.publisher.trim().to_string()).filter(|p| !p.is_empty());
    // The format's own reader is the last word on what the file takes.
    config::parse(config::write(&next).as_bytes()).map_err(|e| bad_value("publish.conf", &e.to_string()))?;
    *c = next;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_and_the_view_agree() {
        let mut c = PublishConfig::default();
        let shipped = agents(&c);
        assert_eq!(shipped[0].server, "");
        assert_eq!((shipped[0].retry, shipped[0].resync), (300, 86_400));
        assert!(shipped[0].create_parent && !shipped[0].tls);
        let to = PublishAgent {
            enabled: true,
            server: "ldap.example.com".into(),
            port: 389,
            tls: true,
            start_tls: true,
            bind_dn: "cn=Administrator".into(),
            parent_dn: "ou=Users,dc=example,dc=com".into(),
            publisher: "HQ".into(),
            ..shipped[0].clone()
        };
        apply(&mut c, &to).unwrap();
        let a = c.agent(Kind::Users);
        assert_eq!(a.security, Security::StartTls);
        assert_eq!(a.host, "ldap.example.com");
        assert_eq!(a.publisher.as_deref(), Some("HQ"));
        assert_eq!(agent(a), to);
        // Back to the local instance: the file says localhost.
        apply(
            &mut c,
            &PublishAgent {
                server: String::new(),
                ..to.clone()
            },
        )
        .unwrap();
        assert_eq!(c.agent(Kind::Users).host, "localhost");
    }

    #[test]
    fn values_not_taken() {
        let c = PublishConfig::default();
        let a = agents(&c)[1].clone();
        let code = |to: PublishAgent| check(&to).unwrap_err().msgid;
        assert_eq!(code(PublishAgent { port: 0, ..a.clone() }), "GLD0204");
        assert_eq!(
            code(PublishAgent {
                server: "a b".into(),
                ..a.clone()
            }),
            "GLD0239"
        );
        assert_eq!(
            code(PublishAgent {
                parent_dn: "nonsense".into(),
                ..a.clone()
            }),
            "GLD0224"
        );
        assert_eq!(code(PublishAgent { retry: 5, ..a.clone() }), "GLD0232");
        assert_eq!(code(PublishAgent { resync: 30, ..a.clone() }), "GLD0232");
        let mut c = c;
        let mut with_pw = c.clone();
        with_pw.agent_mut(Kind::Printers).bind_dn = Some("cn=x".into());
        with_pw.agent_mut(Kind::Printers).bind_pw = Some(vec![1, 2, 3]);
        c = with_pw;
        // No bind DN: the password goes too, so the file still reads.
        apply(&mut c, &a).unwrap();
        assert!(c.agent(Kind::Printers).bind_pw.is_none());
    }
}
