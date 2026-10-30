// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Mailbox addresses: `local@domain`, optionally `Name <local@domain>`.
//! The check is RFC 5321's mailbox without quoted local parts or
//! address literals, which no `SNDSMTPEMM` user needs.

use thiserror::Error;

/// An address that is not one.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("{0} is not a valid mail address")]
pub struct BadAddress(pub String);

/// One mailbox.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    /// The display name, empty for none.
    pub name: String,
    /// `local@domain`.
    pub mailbox: String,
}

fn atom_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-/=?^_`{|}~".contains(c)
}

fn valid_local(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.split('.').all(|p| !p.is_empty() && p.chars().all(atom_char))
}

fn valid_domain(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 255
        && s.split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

impl Address {
    /// Parse `local@domain` or `Name <local@domain>`.
    pub fn parse(s: &str) -> Result<Self, BadAddress> {
        let t = s.trim();
        let (name, mailbox) = match (t.rfind('<'), t.ends_with('>')) {
            (Some(at), true) => (t[..at].trim().trim_matches('"').to_string(), &t[at + 1..t.len() - 1]),
            _ => (String::new(), t),
        };
        let mailbox = mailbox.trim();
        match mailbox.rsplit_once('@') {
            Some((l, d)) if valid_local(l) && valid_domain(d) => Ok(Self {
                name,
                mailbox: mailbox.to_string(),
            }),
            _ => Err(BadAddress(s.trim().to_string())),
        }
    }

    /// A bare mailbox.
    #[must_use]
    pub fn mailbox(m: &str) -> Self {
        Self {
            name: String::new(),
            mailbox: m.to_string(),
        }
    }

    /// As a header shows it: `Name <a@b>` (the name encoded when it is
    /// not plain ASCII) or `a@b`.
    #[must_use]
    pub fn header(&self) -> String {
        if self.name.is_empty() {
            self.mailbox.clone()
        } else if self.name.chars().all(|c| c.is_ascii_alphanumeric() || c == ' ') {
            format!("{} <{}>", self.name, self.mailbox)
        } else if self.name.is_ascii() && !self.name.contains(['"', '\\']) {
            format!("\"{}\" <{}>", self.name, self.mailbox)
        } else {
            format!("{} <{}>", crate::encode::word(&self.name), self.mailbox)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        assert_eq!(Address::parse(" anne@example.com ").unwrap().mailbox, "anne@example.com");
        let a = Address::parse("Anne Smith <anne.smith@mail.example.com>").unwrap();
        assert_eq!((a.name.as_str(), a.mailbox.as_str()), ("Anne Smith", "anne.smith@mail.example.com"));
        assert_eq!(a.header(), "Anne Smith <anne.smith@mail.example.com>");
        for bad in ["", "anne", "@x.com", "a@", "a@-x.com", "a b@x.com", "a..b@x.com", "a@x..com"] {
            assert!(Address::parse(bad).is_err(), "{bad}");
        }
        let o = Address {
            name: "O'Brien, Pat".into(),
            mailbox: "p@x.ie".into(),
        };
        assert_eq!(o.header(), "\"O'Brien, Pat\" <p@x.ie>");
        let j = Address {
            name: "Jürgen".into(),
            mailbox: "j@x.de".into(),
        };
        assert!(j.header().starts_with("=?utf-8?B?"));
    }
}
