// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Basic authentication: `AuthType Basic`, `AuthName`, `PasswdFile` and
//! `Require`. The job checks users through [`Authenticator`]:
//! `PasswdFile %%SYSTEM%%` (the default) means the system's user
//! profiles, anything else names a validation list. [`MemAuth`] is the
//! one for tests.

use std::collections::HashMap;

use super::dirs::DirConf;

/// `PasswdFile`'s value for the system's user profiles.
pub const SYSTEM: &str = "%%SYSTEM%%";

/// Why a user and password were not accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Wrong user or password, or a profile that may not sign on.
    Bad,
    /// The source cannot be checked (a validation list this system
    /// does not hold); the text says why.
    Unavailable(String),
}

/// Checks users for the server: the job answers it.
pub trait Authenticator: Send + Sync {
    /// Check `user` and `password` against `source` (`%%SYSTEM%%` or a
    /// validation list `LIB/NAME`). Gives the name to log: a profile's
    /// upper case name.
    fn check(&self, source: &str, user: &str, password: &str) -> Result<String, Refused>;
}

/// Users in memory, by source.
#[derive(Debug, Default)]
pub struct MemAuth {
    users: HashMap<(String, String), String>,
}

impl MemAuth {
    /// No users.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add `user` with `password` to `source`.
    #[must_use]
    pub fn with(mut self, source: &str, user: &str, password: &str) -> Self {
        self.users.insert(
            (source.to_ascii_uppercase(), user.to_ascii_uppercase()),
            password.to_string(),
        );
        self
    }
}

impl Authenticator for MemAuth {
    fn check(&self, source: &str, user: &str, password: &str) -> Result<String, Refused> {
        let key = (source.to_ascii_uppercase(), user.to_ascii_uppercase());
        if !self.users.keys().any(|(s, _)| *s == key.0) {
            return Err(Refused::Unavailable(format!("{source} is not known.")));
        }
        match self.users.get(&key) {
            Some(p) if p == password => Ok(key.1),
            _ => Err(Refused::Bad),
        }
    }
}

/// Never accepts anyone: for an instance with no way to check users.
#[derive(Debug, Default)]
pub struct NoAuth;

impl Authenticator for NoAuth {
    fn check(&self, source: &str, _: &str, _: &str) -> Result<String, Refused> {
        Err(Refused::Unavailable(format!(
            "{source} cannot be checked by this server."
        )))
    }
}

/// Decode standard base64 (padding optional).
pub fn base64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for c in s.trim().bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xFF).unwrap_or(0));
        }
    }
    Some(out)
}

/// The user and password of an `Authorization: Basic` header.
pub fn credentials(header: &str) -> Option<(String, String)> {
    let (scheme, rest) = header.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let text = String::from_utf8(base64(rest)?).ok()?;
    let (u, p) = text.split_once(':')?;
    Some((u.to_string(), p.to_string()))
}

/// What the authentication rules say about a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gate {
    /// In; the user, if one signed on.
    Open(Option<String>),
    /// 401 with this realm.
    Challenge(String),
    /// 403 (in the error log with the reason, when there is one).
    Forbidden(Option<String>),
}

/// Apply `d`'s rules to a request with `authorization` header.
pub fn gate(d: &DirConf, authorization: Option<&str>, auth: &dyn Authenticator) -> Gate {
    if d.require.is_empty() {
        return Gate::Open(None);
    }
    if !d
        .auth_type
        .as_deref()
        .is_some_and(|t| t.eq_ignore_ascii_case("basic"))
    {
        return Gate::Forbidden(Some("Require is set but AuthType Basic is not.".into()));
    }
    let realm = if d.auth_name.is_empty() {
        "Restricted".to_string()
    } else {
        d.auth_name.clone()
    };
    let Some((user, password)) = authorization.and_then(credentials) else {
        return Gate::Challenge(realm);
    };
    let source = d.passwd_file.as_deref().unwrap_or(SYSTEM);
    let name = match auth.check(source, &user, &password) {
        Ok(n) => n,
        Err(Refused::Bad) => return Gate::Challenge(realm),
        Err(Refused::Unavailable(why)) => return Gate::Forbidden(Some(why)),
    };
    let what = d.require[0].to_ascii_lowercase();
    match what.as_str() {
        "valid-user" => Gate::Open(Some(name)),
        "user" if d.require[1..].iter().any(|u| u.eq_ignore_ascii_case(&name)) => {
            Gate::Open(Some(name))
        }
        "user" => Gate::Challenge(realm),
        other => Gate::Forbidden(Some(format!("Require {other} is not supported."))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_gates() {
        assert_eq!(base64("Ym9iOnNlY3JldA==").unwrap(), b"bob:secret");
        assert_eq!(base64("YQ").unwrap(), b"a");
        assert!(base64("a$b").is_none());
        assert_eq!(
            credentials("Basic Ym9iOnNlY3JldA=="),
            Some(("bob".into(), "secret".into()))
        );
        let auth = MemAuth::new()
            .with(SYSTEM, "BOB", "secret")
            .with("QUSRSYS/WEB", "ann", "pw");
        let mut d = DirConf {
            require: vec!["valid-user".into()],
            auth_type: Some("Basic".into()),
            auth_name: "Shop".into(),
            ..DirConf::default()
        };
        assert_eq!(gate(&d, None, &auth), Gate::Challenge("Shop".into()));
        assert_eq!(
            gate(&d, Some("Basic Ym9iOnNlY3JldA=="), &auth),
            Gate::Open(Some("BOB".into()))
        );
        assert_eq!(
            gate(&d, Some("Basic Ym9iOndyb25n"), &auth),
            Gate::Challenge("Shop".into())
        );
        d.require = vec!["user".into(), "ann".into()];
        assert_eq!(
            gate(&d, Some("Basic Ym9iOnNlY3JldA=="), &auth),
            Gate::Challenge("Shop".into())
        );
        d.passwd_file = Some("QUSRSYS/NONE".into());
        assert!(matches!(
            gate(&d, Some("Basic Ym9iOnNlY3JldA=="), &auth),
            Gate::Forbidden(Some(_))
        ));
        d.auth_type = None;
        assert!(matches!(gate(&d, None, &auth), Gate::Forbidden(_)));
        assert_eq!(gate(&DirConf::default(), None, &NoAuth), Gate::Open(None));
    }
}
