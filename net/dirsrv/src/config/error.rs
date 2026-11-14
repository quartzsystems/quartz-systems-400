// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What is wrong with an instance or its configuration, each as the
//! `QGLDMSG` message QS/400 sends for it (docs/DIRSRV.md §2.1).

/// A message variable: `*CHAR n` or `*CHAR *VARY 4`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Var {
    /// `*CHAR n`, blank padded.
    Chars(String, usize),
    /// `*CHAR *VARY 4`.
    Vary(String),
}

/// A configuration problem.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// The file is missing or not LDIF.
    #[error("configuration file {file} cannot be used: {why}")]
    File {
        /// The path.
        file: String,
        /// Why.
        why: String,
    },
    /// An attribute or entry QS/400 does not honour, not at IBM's
    /// default.
    #[error("{what} in {file} is not supported")]
    Unsupported {
        /// `attribute: value` or the entry.
        what: String,
        /// The path.
        file: String,
    },
    /// A value QS/400 does not take.
    #[error("value {value} of {attr} is not valid")]
    BadValue {
        /// The attribute.
        attr: String,
        /// The value.
        value: String,
    },
    /// No such instance (`CHGDIRSVRA`).
    #[error("directory server instance {0} not found")]
    NoInstance(String),
    /// No such instance (`STRTCPSVR`, `ENDTCPSVR`).
    #[error("directory server instance {0} not found")]
    NoServerInstance(String),
    /// The instance names no update DN.
    #[error("instance {0} has no update distinguished name")]
    NoUpdateDn(String),
    /// Not a distinguished name.
    #[error("distinguished name {0} is not valid")]
    BadDn(String),
    /// A distinguished name with no password.
    #[error("a password is required")]
    NoPassword,
}

impl ConfigError {
    /// The message identifier.
    #[must_use]
    pub fn msgid(&self) -> &'static str {
        match self {
            Self::File { .. } => "GLD0127",
            Self::Unsupported { .. } => "GLD0126",
            Self::BadValue { .. } => "GLD0232",
            Self::NoInstance(_) => "GLD0201",
            Self::NoServerInstance(_) => "GLD0203",
            Self::NoUpdateDn(_) => "GLD0215",
            Self::BadDn(_) => "GLD0224",
            Self::NoPassword => "GLD0202",
        }
    }

    /// The message data, in the message's variables' order and forms.
    #[must_use]
    pub fn vars(&self) -> Vec<Var> {
        match self {
            Self::File { file, .. } => vec![Var::Vary(file.clone())],
            Self::Unsupported { what, file } => {
                vec![Var::Vary(what.clone()), Var::Vary(file.clone())]
            }
            Self::BadValue { attr, value } => {
                vec![Var::Vary(attr.clone()), Var::Vary(value.clone())]
            }
            Self::NoInstance(n) | Self::NoUpdateDn(n) => vec![Var::Chars(n.clone(), 8)],
            Self::NoServerInstance(n) => vec![Var::Chars(n.clone(), 32)],
            Self::BadDn(d) => vec![Var::Vary(d.clone())],
            Self::NoPassword => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each identifier's variables are the ones `QGLDMSG` declares
    /// (`runtime/msgf/msgs/qgldmsg.toml`).
    #[test]
    fn identifiers_and_layouts() {
        let src = include_str!("../../../../runtime/msgf/msgs/qgldmsg.toml");
        let cases = [
            ConfigError::File {
                file: "f".into(),
                why: String::new(),
            },
            ConfigError::Unsupported {
                what: "a".into(),
                file: "f".into(),
            },
            ConfigError::BadValue {
                attr: "a".into(),
                value: "v".into(),
            },
            ConfigError::NoInstance("X".into()),
            ConfigError::NoServerInstance("X".into()),
            ConfigError::NoUpdateDn("X".into()),
            ConfigError::BadDn("d".into()),
            ConfigError::NoPassword,
        ];
        for e in cases {
            let at = src
                .find(&format!("id = \"{}\"", e.msgid()))
                .unwrap_or_else(|| panic!("{}", e.msgid()));
            let block = &src[at..];
            let block = &block[..block.find("[[msg]]").unwrap_or(block.len())];
            let want: Vec<String> = e
                .vars()
                .iter()
                .map(|v| match v {
                    Var::Chars(_, n) => format!("\"*CHAR {n}\""),
                    Var::Vary(_) => "\"*CHAR *VARY 4\"".to_string(),
                })
                .collect();
            let have = block
                .lines()
                .find(|l| l.starts_with("vars = "))
                .map_or(String::new(), |l| l["vars = ".len()..].to_string());
            assert_eq!(
                have,
                if want.is_empty() {
                    String::new()
                } else {
                    format!("[{}]", want.join(", "))
                },
                "{}",
                e.msgid()
            );
        }
    }
}
