// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Messages: the row under a page's title, and the second-level text
//! behind every message ID link.

/// What a message row says about the request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// Yellow: information.
    #[default]
    Info,
    /// Green: completion.
    Done,
    /// Red: error.
    Error,
}

impl Kind {
    /// The cell's class.
    pub fn class(self) -> &'static str {
        match self {
            Kind::Info => "info",
            Kind::Done => "done",
            Kind::Error => "error",
        }
    }
}

/// A message with its ID and first-level text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Message {
    /// Information, completion or error.
    pub kind: Kind,
    /// `HTP8001`, `CPF2204`, ...
    pub id: String,
    /// The first-level text, substitutions made.
    pub text: String,
}

impl Message {
    /// A message.
    pub fn new(kind: Kind, id: &str, text: &str) -> Self {
        Self {
            kind,
            id: id.to_string(),
            text: text.to_string(),
        }
    }
}

/// A message's second-level text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecondLevel {
    /// The message ID.
    pub id: String,
    /// The first-level text.
    pub text: String,
    /// What happened.
    pub cause: String,
    /// What to do.
    pub recovery: String,
    /// 00 to 99.
    pub severity: u8,
}

/// Where second-level texts come from (a message file, a table).
pub trait MessageText: Send + Sync {
    /// The text of `id`, substitution values `data` filled in.
    fn lookup(&self, id: &str, data: &[&str]) -> Option<SecondLevel>;
}

/// Fill `&1`, `&2`, ... in `template` from `data`.
pub fn substitute(template: &str, data: &[&str]) -> String {
    let mut out = String::with_capacity(template.len());
    let b = template.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'&' && i + 1 < b.len() && b[i + 1].is_ascii_digit() {
            let mut j = i + 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let n: usize = template[i + 1..j].parse().unwrap_or(0);
            out.push_str(n.checked_sub(1).and_then(|k| data.get(k)).copied().unwrap_or(""));
            i = j;
        } else {
            let c = template[i..].chars().next().unwrap_or(' ');
            out.push(c);
            i += c.len_utf8();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitution() {
        assert_eq!(substitute("Instance &1 started on &2.", &["API", "*:9443"]), "Instance API started on *:9443.");
        assert_eq!(substitute("&3 & &x", &["a"]), " & &x");
        assert_eq!(Kind::Done.class(), "done");
    }
}
