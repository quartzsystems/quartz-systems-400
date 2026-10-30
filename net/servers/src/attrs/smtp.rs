// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CHGSMTPA`: the SMTP attributes, the oracle's keywords (V7R5M0
//! `QCDRCMDD`). There is no SMTP server in this release, so every value
//! is kept as given and only the outbound ones act: mail goes to the
//! forwarding mail hub (`FWDHUBSVR`) or, without one, the mail router
//! (`MAILROUTER`). Either may name a port as `host:port`.
//!
//! Beside them the bag keeps three of ours that no command sets:
//! `SMTPTLS` (`*NONE`, `*STARTTLS` to require it, `*IMPLICIT`; by
//! default `STARTTLS` when offered, implicit TLS on port 465), and
//! `AUTHUSER`/`AUTHPWD` for `AUTH`.

use super::{get, is_same};

/// The keywords `CHGSMTPA` changes besides `AUTOSTART`, in its order.
pub const KEYWORDS: &[&str] = &[
    "COLDSTART",
    "DIRTYPE",
    "RTYMIN",
    "RTYDAY",
    "RTYHOUR",
    "RTYRMTSVR",
    "CCSID",
    "JOURNAL",
    "PCTRTGCHR",
    "ETRNSVR",
    "MIME8BIT",
    "NFYDLVRY",
    "SBSD",
    "RBLSVR",
    "ALWRLY",
    "POPWDW",
    "IFCDMN",
    "FTRACN",
    "OVRRJTCNNL",
    "ALWBARELF",
    "VFYID",
    "ALWAUTH",
    "VFYMSFMSG",
    "VFYFROMUSR",
    "FWDHUBSVR",
    "AUTOADD",
    "USRIDPFX",
    "ADDRESS",
    "SYSNAME",
    "TBLTYPE",
    "USRIDDELIM",
    "MAILROUTER",
    "TBLSMTPOUT",
    "TBLSMTPIN",
    "FIREWALL",
    "ALLMAILMSF",
    "DIALSCD",
    "ETRNCLT",
    "ALWIMPSSL",
    "MAXINEMM",
    "MAXOUTEMM",
    "KEEPUNTIL",
    "MAXEMMSIZE",
    "ALIASDMN",
    "OAUTH",
];

/// Our keys beside the command's.
pub const OURS: &[&str] = &["SMTPTLS", "AUTHUSER", "AUTHPWD"];

/// How the client uses TLS with the relay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SmtpTls {
    /// `STARTTLS` when offered (implicit on port 465).
    #[default]
    Offered,
    /// `*NONE`.
    Never,
    /// `*STARTTLS`: required.
    Required,
    /// `*IMPLICIT`.
    Implicit,
}

/// The SMTP attributes: the bag, with the outbound ones read out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SmtpAttrs {
    bag: Vec<(String, String)>,
}

/// The relay mail goes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmartHost {
    /// Host name or address.
    pub host: String,
    /// Port.
    pub port: u16,
    /// Which attribute named it.
    pub from: &'static str,
}

/// `host`, `host:port`, `[v6]:port`.
fn host_port(v: &str) -> Option<(String, u16)> {
    let v = v.trim().trim_matches('\'').trim();
    if v.is_empty() || v.starts_with('*') {
        return None;
    }
    if let Some(rest) = v.strip_prefix('[') {
        let (h, tail) = rest.split_once(']')?;
        let port = match tail.strip_prefix(':') {
            Some(p) => p.parse().ok()?,
            None => 25,
        };
        return Some((h.to_string(), port));
    }
    match v.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') => Some((h.to_string(), p.parse().ok()?)),
        _ => Some((v.to_string(), 25)),
    }
}

impl SmtpAttrs {
    /// Over the server table's bag.
    pub fn from_bag(bag: &[(String, String)]) -> Self {
        Self { bag: bag.to_vec() }
    }

    /// The bag.
    pub fn to_bag(&self) -> Vec<(String, String)> {
        self.bag.clone()
    }

    /// A value as kept; empty when never set.
    pub fn value(&self, kwd: &str) -> String {
        get(&self.bag, kwd).unwrap_or("").to_string()
    }

    /// Keep one value (`*SAME` leaves it); whether it changed.
    pub fn set(&mut self, kwd: &str, v: &str) -> Result<bool, String> {
        let k = kwd.trim().to_ascii_uppercase();
        if !KEYWORDS.contains(&k.as_str()) && !OURS.contains(&k.as_str()) {
            return Err(format!("{k} is not an SMTP attribute"));
        }
        if is_same(v) {
            return Ok(false);
        }
        let v = v.trim();
        let v = if v.starts_with('*') { v.to_ascii_uppercase() } else { v.to_string() };
        if (k == "FWDHUBSVR" || k == "MAILROUTER") && v != "*NONE" && host_port(&v).is_none() {
            return Err(format!("{k}({v}): a host name or address, with :port if not 25"));
        }
        if k == "SMTPTLS" && !matches!(v.as_str(), "*NONE" | "*STARTTLS" | "*IMPLICIT" | "*OFFERED") {
            return Err(format!("SMTPTLS({v}): *NONE, *STARTTLS, *IMPLICIT or *OFFERED"));
        }
        match self.bag.iter_mut().find(|(b, _)| b.eq_ignore_ascii_case(&k)) {
            Some((_, old)) if *old == v => Ok(false),
            Some((_, old)) => {
                *old = v;
                Ok(true)
            }
            None => {
                self.bag.push((k, v));
                Ok(true)
            }
        }
    }

    /// Where mail goes: `FWDHUBSVR`, else `MAILROUTER`; `None` when
    /// neither is set.
    pub fn smart_host(&self) -> Option<SmartHost> {
        for (kwd, from) in [("FWDHUBSVR", "FWDHUBSVR"), ("MAILROUTER", "MAILROUTER")] {
            if let Some((host, port)) = get(&self.bag, kwd).and_then(host_port) {
                return Some(SmartHost { host, port, from });
            }
        }
        None
    }

    /// TLS with the relay.
    pub fn tls(&self) -> SmtpTls {
        match get(&self.bag, "SMTPTLS").map(str::to_ascii_uppercase).as_deref() {
            Some("*NONE") => SmtpTls::Never,
            Some("*STARTTLS") => SmtpTls::Required,
            Some("*IMPLICIT") => SmtpTls::Implicit,
            _ => SmtpTls::Offered,
        }
    }

    /// `AUTH` credentials.
    pub fn auth(&self) -> Option<(String, String)> {
        let user = get(&self.bag, "AUTHUSER")?.trim();
        let pwd = get(&self.bag, "AUTHPWD").unwrap_or("");
        (!user.is_empty() && !user.starts_with('*')).then(|| (user.to_string(), pwd.to_string()))
    }

    /// `CCSID` (819 by default): the code page of text the mail
    /// carries when it is not UTF-8 (`QtmmSendMail` files).
    pub fn ccsid(&self) -> u16 {
        get(&self.bag, "CCSID").and_then(|c| c.trim().parse().ok()).unwrap_or(819)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_smart_host() {
        let mut a = SmtpAttrs::default();
        assert_eq!(a.smart_host(), None);
        assert_eq!(a.set("MAILROUTER", "router.example.com"), Ok(true));
        assert_eq!(
            a.smart_host(),
            Some(SmartHost {
                host: "router.example.com".into(),
                port: 25,
                from: "MAILROUTER"
            })
        );
        assert_eq!(a.set("FWDHUBSVR", "'192.168.77.147:2525'"), Ok(true));
        let h = a.smart_host().unwrap();
        assert_eq!((h.host.as_str(), h.port, h.from), ("192.168.77.147", 2525, "FWDHUBSVR"));
        assert_eq!(a.set("FWDHUBSVR", "*none"), Ok(true));
        assert_eq!(a.smart_host().unwrap().from, "MAILROUTER");
        assert_eq!(host_port("[2001:db8::1]:587"), Some(("2001:db8::1".into(), 587)));
        assert_eq!(host_port("2001:db8::1"), Some(("2001:db8::1".into(), 25)));
        assert!(a.set("MAILROUTER", "x:notaport").is_err());
        assert!(a.set("NOSUCH", "1").is_err());
    }

    #[test]
    fn kept_values_and_ours() {
        let mut a = SmtpAttrs::default();
        assert_eq!(a.set("JOURNAL", "*no"), Ok(true));
        assert_eq!(a.set("JOURNAL", "*NO"), Ok(false));
        assert_eq!(a.set("JOURNAL", "*SAME"), Ok(false));
        assert_eq!(a.value("journal"), "*NO");
        assert_eq!(a.tls(), SmtpTls::Offered);
        a.set("SMTPTLS", "*STARTTLS").unwrap();
        assert_eq!(a.tls(), SmtpTls::Required);
        assert!(a.set("SMTPTLS", "*MAYBE").is_err());
        assert_eq!(a.auth(), None);
        a.set("AUTHUSER", "mailer").unwrap();
        a.set("AUTHPWD", "s3cret").unwrap();
        assert_eq!(a.auth(), Some(("mailer".into(), "s3cret".into())));
        assert_eq!(SmtpAttrs::from_bag(&a.to_bag()), a);
        assert_eq!(a.ccsid(), 819);
    }
}
