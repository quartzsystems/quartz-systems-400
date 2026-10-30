// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Replies: a three-digit code and one or more lines. A multi-line
//! reply puts `code-` before every line but the last, as IBM's server
//! does; a feature listing (`FEAT`) sends its middle lines bare, as
//! RFC 2389 asks.

use std::fmt::Write as _;

/// One reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    /// The code.
    pub code: u16,
    /// The lines, without the code.
    pub lines: Vec<String>,
    /// Middle lines go without the `code-` prefix.
    pub bare: bool,
}

impl Reply {
    /// A one-line reply.
    pub fn new(code: u16, text: impl Into<String>) -> Self {
        Self {
            code,
            lines: vec![text.into()],
            bare: false,
        }
    }

    /// A multi-line reply.
    pub fn lines(code: u16, lines: Vec<String>) -> Self {
        let lines = if lines.is_empty() { vec![String::new()] } else { lines };
        Self {
            code,
            lines,
            bare: false,
        }
    }

    /// A multi-line reply whose middle lines are bare.
    pub fn bare(code: u16, lines: Vec<String>) -> Self {
        Self {
            bare: true,
            ..Self::lines(code, lines)
        }
    }

    /// Whether the code says the command did not go through.
    pub fn failed(&self) -> bool {
        self.code >= 400
    }

    /// The reply as it goes on the wire, each line ended by CRLF.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let last = self.lines.len() - 1;
        for (i, l) in self.lines.iter().enumerate() {
            if i == last {
                let _ = write!(out, "{} {l}\r\n", self.code);
            } else if self.bare && i > 0 {
                let _ = write!(out, "{l}\r\n");
            } else {
                let _ = write!(out, "{}-{l}\r\n", self.code);
            }
        }
        out
    }
}

/// Read one reply from lines as they arrive: `None` until the last line
/// of the reply is in.
#[derive(Debug, Default)]
pub struct ReplyReader {
    code: Option<u16>,
    lines: Vec<String>,
}

impl ReplyReader {
    /// Take one line (without its line end); the whole reply when this
    /// line ends it.
    pub fn push(&mut self, line: &str) -> Option<Reply> {
        let code = line
            .get(..3)
            .filter(|c| c.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|c| c.parse::<u16>().ok());
        let sep = line.as_bytes().get(3).copied();
        match self.code {
            None => {
                let c = code?;
                if sep == Some(b'-') {
                    self.code = Some(c);
                    self.lines.push(line[4..].to_string());
                    None
                } else {
                    Some(Reply::new(c, line.get(4..).unwrap_or("")))
                }
            }
            Some(c) => {
                if code == Some(c) && sep != Some(b'-') {
                    self.lines.push(line.get(4..).unwrap_or("").to_string());
                    self.code = None;
                    Some(Reply::lines(c, std::mem::take(&mut self.lines)))
                } else {
                    let text = if code == Some(c) { &line[4..] } else { line };
                    self.lines.push(text.to_string());
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendering() {
        assert_eq!(Reply::new(200, "OK.").render(), "200 OK.\r\n");
        let r = Reply::lines(214, vec!["a".into(), "b".into()]);
        assert_eq!(r.render(), "214-a\r\n214 b\r\n");
        let f = Reply::bare(211, vec!["Features:".into(), " SIZE".into(), "End".into()]);
        assert_eq!(f.render(), "211-Features:\r\n SIZE\r\n211 End\r\n");
    }

    #[test]
    fn reading() {
        let mut r = ReplyReader::default();
        assert_eq!(r.push("220-hello"), None);
        assert_eq!(r.push(" bare line"), None);
        let got = r.push("220 ready").unwrap();
        assert_eq!(got.code, 220);
        assert_eq!(got.lines, ["hello", " bare line", "ready"]);
        assert_eq!(r.push("200 OK").unwrap().lines, ["OK"]);
    }
}
