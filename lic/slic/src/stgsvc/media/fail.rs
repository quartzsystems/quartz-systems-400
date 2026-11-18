// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A media operation's refusal as the escape message its command sends:
//! the identifier, the data as the catalogue entry lays it out, and the
//! cause underneath (`CPF9897` before the escape). Callers that still
//! take `msgid text` strings get one through `String::from`.

use std::fmt;

use medio::msg::{Field, Msg};
use stg::proto::{CmdMsg, CmdReply};

use crate::cpp::msgout;

/// The escape a media command ends with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fail {
    /// The message identifier.
    pub msgid: String,
    /// Its data, as the entry declares it.
    pub data: Vec<u8>,
    /// The cause underneath; empty when the message says it all.
    pub detail: String,
}

impl Fail {
    /// Escape `msgid` with `fields`.
    pub(crate) fn new(msgid: &str, fields: &[Field<'_>]) -> Self {
        Self {
            msgid: msgid.into(),
            data: medio::msg::data(fields),
            detail: String::new(),
        }
    }

    /// With the cause underneath.
    #[must_use]
    pub(crate) fn detail(mut self, d: impl fmt::Display) -> Self {
        self.detail = d.to_string();
        self
    }

    /// Whether this is message `id`.
    pub(crate) fn is(&self, id: &str) -> bool {
        self.msgid == id
    }

    /// The command's reply: the cause, then the escape.
    pub(crate) fn reply(&self) -> CmdReply {
        let mut r = msgout::escape(&self.msgid, &[], &self.detail);
        if let Some(e) = r.escape.as_mut() {
            e.data.clone_from(&self.data);
        }
        r
    }
}

impl From<Msg> for Fail {
    fn from(m: Msg) -> Self {
        Self {
            msgid: m.id.into(),
            data: m.data,
            detail: m.detail,
        }
    }
}

/// A refusal still made as `msgid text`: the identifier it leads with
/// (`CPF6708` when none) and the rest as the cause. The data is empty.
impl From<String> for Fail {
    fn from(e: String) -> Self {
        let e = e.trim();
        match crate::cpp::leading_msgid(e) {
            Some(id) => Self {
                msgid: id.into(),
                data: Vec::new(),
                detail: e[id.len()..].trim_start_matches([' ', ':']).trim().into(),
            },
            None => Self::new("CPF6708", &[]).detail(e),
        }
    }
}

/// `msgid text`: the catalogue's text with the data, then the cause.
impl fmt::Display for Fail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = crate::stgsvc::hist::message_text(&self.msgid, &self.data);
        let text = text.trim();
        let text = text.strip_prefix(self.msgid.as_str()).unwrap_or(text).trim();
        write!(f, "{} {text}", self.msgid)?;
        if !self.detail.is_empty() {
            write!(f, " ({})", self.detail)?;
        }
        Ok(())
    }
}

impl From<Fail> for String {
    fn from(f: Fail) -> Self {
        f.to_string()
    }
}

/// A `msgid text` refusal without its identifier: the cause it names.
pub(crate) fn cause(e: &str) -> &str {
    let e = e.trim();
    crate::cpp::leading_msgid(e).map_or(e, |id| e[id.len()..].trim_start_matches([' ', ':']))
}

/// A completion message with its data, and no text of the command's.
pub(crate) fn done(m: &Msg) -> CmdReply {
    CmdReply {
        messages: vec![CmdMsg {
            msgid: m.id.into(),
            kind: 4,
            data: m.data.clone(),
        }],
        ..CmdReply::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_is_the_escape_with_its_data() {
        let f = Fail::new("CPF6768", &[Field::Char("TAP01", 10)]);
        let r = f.reply();
        assert!(r.text.is_empty() && r.messages.is_empty());
        let e = r.escape.expect("escape");
        assert_eq!((e.msgid.as_str(), e.kind), ("CPF6768", 7));
        assert_eq!(e.data, medio::msg::data(&[Field::Char("TAP01", 10)]));
        let r = Fail::new("CPF6708", &[]).detail("I/O error").reply();
        assert_eq!(r.messages[0].msgid, "CPF9897");
    }

    #[test]
    fn strings_both_ways() {
        let f = Fail::from("CPF6760 device TAP01 not ready: no volume".to_string());
        assert!(f.is("CPF6760"));
        assert_eq!(f.detail, "device TAP01 not ready: no volume");
        let f = Fail::from("no identifier".to_string());
        assert!(f.is("CPF6708"));
        let s = String::from(Fail::new("CPF6768", &[Field::Char("TAP01", 10)]));
        assert!(s.starts_with("CPF6768 ") && s.contains("TAP01"), "{s}");
    }
}
