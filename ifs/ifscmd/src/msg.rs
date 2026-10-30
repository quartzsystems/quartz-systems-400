// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What an IFS command answers: text, the completion and diagnostic
//! messages, the escape that ended it. The identifiers are IBM's
//! (`CPCA0xx`, `CPDA0xx`, `CPFA0xx`) with IBM's substitution layouts
//! (`tools/conformance/oracle/net/pub400/messages-cpf.txt`); a path is
//! IBM's `*CCHAR *VARY 4`: a 4-byte big-endian length, then the text in
//! CCSID 37. The texts are Quartz's, in `QCPFMSG`.

use ifs::Errno;

/// Message type: completion.
pub const COMPLETION: u8 = 4;
/// Message type: diagnostic.
pub const DIAGNOSTIC: u8 = 5;
/// Message type: escape.
pub const ESCAPE: u8 = 7;
/// Message type: informational.
pub const INFO: u8 = 1;

/// One message with its substitution data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Msg {
    /// `AAAnnnn`.
    pub id: String,
    /// Substitution data, laid out as the message's variables say.
    pub data: Vec<u8>,
}

impl Msg {
    /// A message with no data.
    #[must_use]
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            data: Vec::new(),
        }
    }

    /// A message whose first variable is a path.
    #[must_use]
    pub fn path(id: &str, path: &str) -> Self {
        Self::new(id).vary(path)
    }

    /// A message whose variables are paths, in order.
    #[must_use]
    pub fn paths(id: &str, paths: &[&str]) -> Self {
        paths.iter().fold(Self::new(id), |m, p| m.vary(p))
    }

    /// Add a `*CCHAR *VARY 4` (or `*CHAR *VARY 4`) variable.
    #[must_use]
    pub fn vary(mut self, text: &str) -> Self {
        let b = ebcdic(text);
        let n = u32::try_from(b.len()).unwrap_or(u32::MAX);
        self.data.extend_from_slice(&n.to_be_bytes());
        self.data.extend_from_slice(&b);
        self
    }

    /// Add a `*CHAR n` variable, blank padded or cut.
    #[must_use]
    pub fn chars(mut self, text: &str, n: usize) -> Self {
        let mut b = ebcdic(text);
        b.resize(n, 0x40);
        self.data.extend_from_slice(&b);
        self
    }

    /// Add a `*BIN 4` (or `*UBIN 4`) variable.
    #[must_use]
    pub fn bin4(mut self, v: i64) -> Self {
        let v = i32::try_from(v).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX });
        self.data.extend_from_slice(&v.to_be_bytes());
        self
    }

    /// Add a `*UBIN 8` variable.
    #[must_use]
    pub fn u64(mut self, v: u64) -> Self {
        self.data.extend_from_slice(&v.to_be_bytes());
        self
    }

    /// Add a `*BIN 2` variable.
    #[must_use]
    pub fn bin2(mut self, v: i64) -> Self {
        let v = i16::try_from(v).unwrap_or(if v < 0 { i16::MIN } else { i16::MAX });
        self.data.extend_from_slice(&v.to_be_bytes());
        self
    }

    /// The `CPFA0xx` escape for an `errno` on `path`: the path where the
    /// message names the object, the number for `CPFA0D4`.
    #[must_use]
    pub fn errno(e: Errno, path: &str) -> Self {
        let id = e.command_message();
        let m = Self::new(id);
        match id {
            "CPFA0D4" => m.bin4(i64::from(e.code())),
            "CPFA0D0" => m.bin4(0).bin4(0).bin4(0),
            "CPFA0A3" | "CPFA0A7" | "CPFA0AD" | "CPFA0B2" | "CPFA0DB" | "CPFA0DC"
            | "CPFA0DE" | "CPFA0E0" => m.vary(path),
            _ if e.command_message_names_object() => m.vary(path),
            _ => m,
        }
    }
}

/// Text in CCSID 37, characters it lacks as its substitute.
fn ebcdic(text: &str) -> Vec<u8> {
    ccsid::conv::from_str(37, text).unwrap_or_else(|_| vec![0x6F; text.chars().count()])
}

/// What a command answered.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reply {
    /// Output for the display (a text form), possibly empty.
    pub text: String,
    /// Completion, diagnostic and informational messages, in order,
    /// each with its type.
    pub messages: Vec<(u8, Msg)>,
    /// The escape that ended the command.
    pub escape: Option<Msg>,
}

impl Reply {
    /// A command that worked, with its completion message.
    #[must_use]
    pub fn done(m: Msg) -> Self {
        Self {
            messages: vec![(COMPLETION, m)],
            ..Self::default()
        }
    }

    /// A command that ended with an escape.
    #[must_use]
    pub fn fail(m: Msg) -> Self {
        Self {
            escape: Some(m),
            ..Self::default()
        }
    }

    /// The escape for an `errno` on `path`.
    #[must_use]
    pub fn errno(e: Errno, path: &str) -> Self {
        Self::fail(Msg::errno(e, path))
    }

    /// Text only.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    /// With a diagnostic before what is there.
    #[must_use]
    pub fn diag(mut self, m: Msg) -> Self {
        self.messages.push((DIAGNOSTIC, m));
        self
    }

    /// With a message of type `kind` added.
    #[must_use]
    pub fn with(mut self, kind: u8, m: Msg) -> Self {
        self.messages.push((kind, m));
        self
    }

    /// With text added.
    #[must_use]
    pub fn and_text(mut self, text: &str) -> Self {
        self.text.push_str(text);
        self
    }

    /// Whether the command ended with an escape.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.escape.is_some()
    }
}

/// A path quoted for a CL command string: apostrophes around it, an
/// apostrophe inside doubled.
#[must_use]
pub fn quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts() {
        let m = Msg::path("CPCA083", "/a");
        assert_eq!(m.data, vec![0, 0, 0, 2, 0x61, 0x81]);
        let m = Msg::errno(Errno::ENOENT, "/x");
        assert_eq!(m.id, "CPFA0A9");
        assert_eq!(&m.data[..4], &[0, 0, 0, 2]);
        let m = Msg::errno(Errno::EAGAIN, "/x");
        assert_eq!(m.id, "CPFA0D4");
        assert_eq!(m.data, Errno::EAGAIN.code().to_be_bytes().to_vec());
        assert_eq!(quote("it's"), "'it''s'");
    }
}
