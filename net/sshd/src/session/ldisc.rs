// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The terminal line discipline for a session with `pty-req`: the
//! client sends keystrokes and expects the far end to echo and edit
//! (canonical mode, `ECHO`, `ICRNL`, `ONLCR`). Erase (`DEL`, `BS`),
//! kill (`^U`), interrupt (`^C`) and end of file (`^D` on an empty
//! line) are honored; escape sequences (arrow keys) are dropped.

use std::io::{self, Write};

/// What the keystrokes produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// A completed line, without its end.
    Line(String),
    /// `^C`.
    Interrupt,
    /// `^D` on an empty line.
    Eof,
}

/// The line being typed.
#[derive(Debug, Default)]
pub struct LineDiscipline {
    line: String,
    pending: Vec<u8>,
    after_cr: bool,
    escape: u8,
}

impl LineDiscipline {
    /// Feed keystrokes; `echo` receives what the terminal shows.
    pub fn feed(&mut self, bytes: &[u8], echo: &mut Vec<u8>) -> Vec<Input> {
        let mut out = Vec::new();
        for &b in bytes {
            if self.escape > 0 {
                // ESC [ ... final byte, or ESC and one byte.
                if self.escape == 1 {
                    self.escape = if b == b'[' || b == b'O' { 2 } else { 0 };
                } else if (0x40..=0x7e).contains(&b) {
                    self.escape = 0;
                }
                continue;
            }
            let after_cr = std::mem::take(&mut self.after_cr);
            match b {
                b'\n' if after_cr => {}
                b'\r' | b'\n' => {
                    self.after_cr = b == b'\r';
                    echo.extend_from_slice(b"\r\n");
                    self.pending.clear();
                    out.push(Input::Line(std::mem::take(&mut self.line)));
                }
                0x7f | 0x08 => {
                    if self.line.pop().is_some() {
                        echo.extend_from_slice(b"\x08 \x08");
                    }
                }
                0x15 => {
                    for _ in self.line.chars() {
                        echo.extend_from_slice(b"\x08 \x08");
                    }
                    self.line.clear();
                }
                0x03 => {
                    echo.extend_from_slice(b"^C\r\n");
                    self.line.clear();
                    out.push(Input::Interrupt);
                }
                0x04 => {
                    if self.line.is_empty() {
                        out.push(Input::Eof);
                    }
                }
                0x1b => self.escape = 1,
                b'\t' => {
                    self.line.push('\t');
                    echo.push(b'\t');
                }
                b if b < 0x20 => {}
                b => {
                    self.pending.push(b);
                    if let Ok(s) = std::str::from_utf8(&self.pending) {
                        self.line.push_str(s);
                        echo.extend_from_slice(&self.pending);
                        self.pending.clear();
                    } else if self.pending.len() >= 4 {
                        self.pending.clear();
                    }
                }
            }
        }
        out
    }
}

/// `ONLCR`: a writer that turns `\n` into `\r\n`.
pub struct Onlcr<W: Write>(pub W);

impl<W: Write> Write for Onlcr<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let mut out = Vec::with_capacity(data.len() + 8);
        for &b in data {
            if b == b'\n' {
                out.push(b'\r');
            }
            out.push(b);
        }
        self.0.write_all(&out)?;
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing() {
        let mut l = LineDiscipline::default();
        let mut echo = Vec::new();
        let got = l.feed(b"ecx\x7fho hi\r\n\x1b[Als\r", &mut echo);
        assert_eq!(got, [Input::Line("echo hi".into()), Input::Line("ls".into())]);
        assert_eq!(echo, b"ecx\x08 \x08ho hi\r\nls\r\n");
        let got = l.feed(b"abc\x03\x04", &mut Vec::new());
        assert_eq!(got, [Input::Interrupt, Input::Eof]);
        let mut w = Onlcr(Vec::new());
        w.write_all(b"a\nb\n").unwrap();
        assert_eq!(w.0, b"a\r\nb\r\n");
    }
}
