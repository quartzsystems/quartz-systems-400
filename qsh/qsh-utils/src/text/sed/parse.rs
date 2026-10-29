// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sed script: addresses, commands, `s` and `y` arguments, text
//! for `a`/`i`/`c`, labels and `w` files, parsed into a flat program
//! with block ends resolved to jumps.

use std::collections::BTreeMap;
use std::sync::Arc;

use qsh::sys::{Fd, UtilCtx};

use crate::common::io::open_write;
use crate::regex::{Regex, Syntax};

/// A regular expression in the script: `None` is the empty one (the
/// last used at run time).
pub type Re = Option<Arc<Regex>>;

/// One address.
#[derive(Clone)]
pub enum Addr {
    /// A line number.
    Line(u64),
    /// `$`.
    Last,
    /// A pattern.
    Re(Re),
}

/// A piece of an `s` replacement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rep {
    /// Literal bytes.
    Lit(Vec<u8>),
    /// `&` or `\n`.
    Group(usize),
}

/// The `s` command.
#[derive(Clone)]
pub struct Subst {
    /// The pattern.
    pub re: Re,
    /// The replacement.
    pub rep: Vec<Rep>,
    /// Replace every match.
    pub global: bool,
    /// Replace the nth match only.
    pub nth: usize,
    /// Print after a replacement.
    pub print: bool,
    /// Write to a file after a replacement.
    pub wfile: Option<usize>,
}

/// A command.
#[derive(Clone)]
pub enum Cmd {
    /// `{`: jump past the block when the address does not match.
    Block(usize),
    /// `}`.
    End,
    /// `=`.
    LineNo,
    /// `a`.
    Append(Vec<u8>),
    /// `b`: target index, or the end.
    Branch(Option<usize>),
    /// `c`.
    Change(Vec<u8>),
    /// `d`.
    Delete,
    /// `D`.
    DeleteFirst,
    /// `g`.
    GetHold,
    /// `G`.
    AppendHold,
    /// `h`.
    Hold,
    /// `H`.
    AppendToHold,
    /// `i`.
    Insert(Vec<u8>),
    /// `l`.
    List,
    /// `n`.
    Next,
    /// `N`.
    NextAppend,
    /// `p`.
    Print,
    /// `P`.
    PrintFirst,
    /// `q`.
    Quit(i32),
    /// `r`.
    ReadFile(String),
    /// `s`.
    Subst(Box<Subst>),
    /// `t`.
    Test(Option<usize>),
    /// `w`: file index.
    Write(usize),
    /// `x`.
    Exchange,
    /// `y`.
    Translate(Box<[u8; 256]>),
    /// `:label` and comments: nothing.
    Nop,
}

/// A command with its addresses.
#[derive(Clone)]
pub struct Line {
    /// First address.
    pub a1: Option<Addr>,
    /// Second address.
    pub a2: Option<Addr>,
    /// `!`.
    pub negate: bool,
    /// The command.
    pub cmd: Cmd,
}

/// The parsed script.
pub struct Script {
    /// The commands.
    pub lines: Vec<Line>,
    /// The `w` files by index.
    pub wfiles: Vec<WFile>,
}

struct P<'a> {
    s: &'a [u8],
    i: usize,
    extended: bool,
    labels: BTreeMap<String, usize>,
    pending: Vec<(usize, String)>,
    wnames: Vec<String>,
}

/// Parse a script.
pub fn parse(cx: &UtilCtx<'_>, script: &[u8], extended: bool) -> Result<Script, String> {
    let mut p = P {
        s: script,
        i: 0,
        extended,
        labels: BTreeMap::new(),
        pending: Vec::new(),
        wnames: Vec::new(),
    };
    let mut lines: Vec<Line> = Vec::new();
    let mut blocks: Vec<usize> = Vec::new();
    loop {
        p.skip_ws_semi();
        if p.i >= p.s.len() {
            break;
        }
        let at = lines.len();
        let line = p.command(at, &mut blocks)?;
        lines.push(line);
    }
    if !blocks.is_empty() {
        return Err("A { has no matching }.".to_string());
    }
    for (idx, label) in &p.pending {
        let target = *p
            .labels
            .get(label)
            .ok_or_else(|| format!("The label {label} is not defined."))?;
        match &mut lines[*idx].cmd {
            Cmd::Branch(t) | Cmd::Test(t) => *t = Some(target),
            _ => {}
        }
    }
    let mut wfiles = Vec::new();
    for name in &p.wnames {
        wfiles.push(match name.as_str() {
            "/dev/stdout" => WFile::Stdout,
            "/dev/stderr" => WFile::Fd(cx.fd(2).cloned().unwrap_or_else(qsh::sys::Null::fd)),
            _ => WFile::Fd(
                open_write(cx, name, false).map_err(|e| format!("{name}: {}", e.errno.text()))?,
            ),
        });
    }
    Ok(Script { lines, wfiles })
}

/// Where a `w` command writes.
pub enum WFile {
    /// `/dev/stdout`, in order with the other output.
    Stdout,
    /// An open file.
    Fd(Fd),
}

impl P<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.i += 1;
        }
    }

    fn skip_ws_semi(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b';')) {
            self.i += 1;
        }
    }

    fn end_of_command(&mut self) -> Result<(), String> {
        self.skip_ws();
        match self.peek() {
            None | Some(b'\n' | b';') => Ok(()),
            Some(b'}') => Ok(()),
            Some(b'#') => {
                self.rest_of_line();
                Ok(())
            }
            Some(c) => Err(format!("Unexpected text after the command: {}.", c as char)),
        }
    }

    fn rest_of_line(&mut self) -> Vec<u8> {
        let start = self.i;
        while let Some(c) = self.peek() {
            if c == b'\n' {
                break;
            }
            self.i += 1;
        }
        let out = self.s[start..self.i].to_vec();
        if self.peek() == Some(b'\n') {
            self.i += 1;
        }
        out
    }

    fn number(&mut self) -> Option<u64> {
        let start = self.i;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.i += 1;
        }
        if start == self.i {
            return None;
        }
        std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok()
    }

    /// Read up to an unescaped `delim`; `\delim` becomes `delim`, `\n`
    /// stays for the regex parser, other escapes are kept.
    fn delimited(&mut self, delim: u8, keep_escapes: bool) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        loop {
            let c = self.peek().ok_or("The command is not complete.")?;
            self.i += 1;
            if c == delim {
                return Ok(out);
            }
            if c == b'\n' && delim != b'\n' {
                return Err("The command is not complete.".to_string());
            }
            if c == b'\\' {
                let e = self.peek().ok_or("The command is not complete.")?;
                self.i += 1;
                if e == delim {
                    out.push(delim);
                } else if e == b'\n' {
                    out.push(b'\n');
                } else {
                    if keep_escapes {
                        out.push(b'\\');
                    }
                    out.push(e);
                }
                continue;
            }
            out.push(c);
        }
    }

    /// A regular expression's text up to `delim`. The delimiter does
    /// not end it inside a bracket expression (`s/[/]/x/`).
    fn regex_text(&mut self, delim: u8) -> Result<Vec<u8>, String> {
        let incomplete = || "The command is not complete.".to_string();
        let mut out = Vec::new();
        loop {
            let c = self.peek().ok_or_else(incomplete)?;
            self.i += 1;
            match c {
                _ if c == delim => return Ok(out),
                b'\n' => return Err(incomplete()),
                b'\\' => {
                    let e = self.peek().ok_or_else(incomplete)?;
                    self.i += 1;
                    if e == delim {
                        out.push(delim);
                    } else if e == b'\n' {
                        out.push(b'\n');
                    } else {
                        out.extend_from_slice(&[b'\\', e]);
                    }
                }
                b'[' => {
                    out.push(c);
                    let mut first = true;
                    if self.peek() == Some(b'^') {
                        out.push(b'^');
                        self.i += 1;
                    }
                    loop {
                        let b = self.peek().ok_or_else(incomplete)?;
                        if b == b'\n' {
                            return Err(incomplete());
                        }
                        self.i += 1;
                        out.push(b);
                        if b == b']' && !first {
                            break;
                        }
                        first = false;
                        if b == b'['
                            && let Some(k @ (b':' | b'.' | b'=')) = self.peek()
                        {
                            // Copy `[:name:]` whole.
                            out.push(k);
                            self.i += 1;
                            while let Some(x) = self.peek() {
                                if x == b'\n' {
                                    break;
                                }
                                self.i += 1;
                                out.push(x);
                                if x == k && self.peek() == Some(b']') {
                                    self.i += 1;
                                    out.push(b']');
                                    break;
                                }
                            }
                        }
                    }
                }
                _ => out.push(c),
            }
        }
    }

    fn regex(&mut self, delim: u8) -> Result<Re, String> {
        let text = self.regex_text(delim)?;
        if text.is_empty() {
            return Ok(None);
        }
        let syntax = if self.extended {
            Syntax::Extended
        } else {
            Syntax::Basic
        };
        Regex::new(&text, syntax, false)
            .map(|r| Some(Arc::new(r)))
            .map_err(|e| e.to_string())
    }

    fn address(&mut self) -> Result<Option<Addr>, String> {
        match self.peek() {
            Some(b'0'..=b'9') => match self.number() {
                Some(0) => Err("Line 0 is not a valid address.".to_string()),
                n => Ok(n.map(Addr::Line)),
            },
            Some(b'$') => {
                self.i += 1;
                Ok(Some(Addr::Last))
            }
            Some(b'/') => {
                self.i += 1;
                Ok(Some(Addr::Re(self.regex(b'/')?)))
            }
            Some(b'\\') => {
                self.i += 1;
                let d = self.peek().ok_or("The address is not complete.")?;
                self.i += 1;
                Ok(Some(Addr::Re(self.regex(d)?)))
            }
            _ => Ok(None),
        }
    }

    /// Text for `a`, `i`, `c`: `\` newline then lines, each ending in
    /// `\` continuing; or the rest of the line.
    fn text(&mut self) -> Vec<u8> {
        self.skip_ws();
        if self.peek() == Some(b'\\') {
            self.i += 1;
            self.skip_ws();
            if self.peek() == Some(b'\n') {
                self.i += 1;
            }
        }
        let mut out = Vec::new();
        while let Some(c) = self.peek() {
            self.i += 1;
            match c {
                b'\\' => match self.peek() {
                    Some(b'\n') => {
                        self.i += 1;
                        out.push(b'\n');
                    }
                    Some(e) => {
                        self.i += 1;
                        out.push(e);
                    }
                    None => {}
                },
                b'\n' => break,
                _ => out.push(c),
            }
        }
        out.push(b'\n');
        out
    }

    /// A label: to the end of the line, or to `;` or `}` so that
    /// `b end; p` and `{...;b}` read as meant.
    fn label(&mut self) -> String {
        self.skip_ws();
        let start = self.i;
        while let Some(c) = self.peek() {
            if matches!(c, b'\n' | b';' | b'}') {
                break;
            }
            self.i += 1;
        }
        let l = self.s[start..self.i].trim_ascii_end();
        if matches!(self.peek(), Some(b'\n' | b';')) {
            self.i += 1;
        }
        String::from_utf8_lossy(l).into_owned()
    }

    fn filename(&mut self) -> Result<String, String> {
        self.skip_ws();
        let l = self.rest_of_line();
        if l.is_empty() {
            return Err("A file name is needed.".to_string());
        }
        Ok(String::from_utf8_lossy(&l).into_owned())
    }

    fn wfile(&mut self, name: String) -> usize {
        if let Some(i) = self.wnames.iter().position(|n| *n == name) {
            return i;
        }
        self.wnames.push(name);
        self.wnames.len() - 1
    }

    fn command(&mut self, at: usize, blocks: &mut Vec<usize>) -> Result<Line, String> {
        let a1 = self.address()?;
        let mut a2 = None;
        if a1.is_some() {
            self.skip_ws();
            if self.peek() == Some(b',') {
                self.i += 1;
                self.skip_ws();
                a2 = self.address()?;
                if a2.is_none() {
                    return Err("The second address is missing.".to_string());
                }
            }
        }
        self.skip_ws();
        let mut negate = false;
        while self.peek() == Some(b'!') {
            negate = true;
            self.i += 1;
            self.skip_ws();
        }
        let c = match self.peek() {
            None | Some(b'\n' | b';') => return Err("A command is missing.".to_string()),
            Some(c) => c,
        };
        self.i += 1;
        let cmd = match c {
            b'{' => {
                blocks.push(at);
                Cmd::Block(0)
            }
            b'}' => {
                if a1.is_some() {
                    return Err("A } takes no address.".to_string());
                }
                let _open = blocks.pop().ok_or("A } has no matching {.")?;
                self.end_of_command()?;
                return Ok(Line {
                    a1,
                    a2,
                    negate,
                    cmd: Cmd::End,
                });
            }
            b'#' => {
                self.rest_of_line();
                Cmd::Nop
            }
            b':' => {
                if a1.is_some() {
                    return Err("A label takes no address.".to_string());
                }
                let l = self.label();
                if l.is_empty() {
                    return Err("A label name is needed.".to_string());
                }
                self.labels.insert(l, at);
                Cmd::Nop
            }
            b'=' => Cmd::LineNo,
            b'a' => Cmd::Append(self.text()),
            b'i' => Cmd::Insert(self.text()),
            b'c' => Cmd::Change(self.text()),
            b'b' | b't' => {
                let l = self.label();
                if !l.is_empty() {
                    self.pending.push((at, l));
                }
                if c == b'b' {
                    Cmd::Branch(None)
                } else {
                    Cmd::Test(None)
                }
            }
            b'd' => Cmd::Delete,
            b'D' => Cmd::DeleteFirst,
            b'g' => Cmd::GetHold,
            b'G' => Cmd::AppendHold,
            b'h' => Cmd::Hold,
            b'H' => Cmd::AppendToHold,
            b'l' => Cmd::List,
            b'n' => Cmd::Next,
            b'N' => Cmd::NextAppend,
            b'p' => Cmd::Print,
            b'P' => Cmd::PrintFirst,
            b'q' => {
                self.skip_ws();
                let n = self.number().unwrap_or(0);
                Cmd::Quit(n as i32)
            }
            b'r' => Cmd::ReadFile(self.filename()?),
            b'w' => {
                let f = self.filename()?;
                Cmd::Write(self.wfile(f))
            }
            b'x' => Cmd::Exchange,
            b's' => self.subst()?,
            b'y' => self.translate()?,
            other => return Err(format!("The command {} is not known.", other as char)),
        };
        let line_end = matches!(
            cmd,
            Cmd::Append(_)
                | Cmd::Insert(_)
                | Cmd::Change(_)
                | Cmd::Branch(_)
                | Cmd::Test(_)
                | Cmd::ReadFile(_)
                | Cmd::Write(_)
                | Cmd::Nop
        );
        if !line_end && !matches!(cmd, Cmd::Block(_)) {
            self.end_of_command()?;
        }
        Ok(Line {
            a1,
            a2,
            negate,
            cmd,
        })
    }

    fn subst(&mut self) -> Result<Cmd, String> {
        let delim = self.peek().ok_or("The s command is not complete.")?;
        if delim == b'\n' || delim == b'\\' {
            return Err("The s delimiter is not valid.".to_string());
        }
        self.i += 1;
        let re = self.regex(delim)?;
        let rep_text = self.delimited(delim, true)?;
        let rep = replacement(&rep_text);
        let mut s = Subst {
            re,
            rep,
            global: false,
            nth: 1,
            print: false,
            wfile: None,
        };
        loop {
            match self.peek() {
                Some(b'g') => {
                    s.global = true;
                    self.i += 1;
                }
                Some(b'p') => {
                    s.print = true;
                    self.i += 1;
                }
                Some(b'0'..=b'9') => {
                    let n = self.number().unwrap_or(1);
                    if n == 0 {
                        return Err("The s count must be above zero.".to_string());
                    }
                    s.nth = n as usize;
                }
                Some(b'w') => {
                    self.i += 1;
                    let f = self.filename()?;
                    s.wfile = Some(self.wfile(f));
                    return Ok(Cmd::Subst(Box::new(s)));
                }
                Some(c) if c.is_ascii_alphabetic() => {
                    return Err(format!("The s flag {} is not valid.", c as char));
                }
                _ => break,
            }
        }
        self.end_of_command()?;
        Ok(Cmd::Subst(Box::new(s)))
    }

    fn translate(&mut self) -> Result<Cmd, String> {
        let delim = self.peek().ok_or("The y command is not complete.")?;
        self.i += 1;
        let a = unescape_y(&self.delimited(delim, true)?);
        let b = unescape_y(&self.delimited(delim, true)?);
        if a.len() != b.len() {
            return Err("The y strings differ in length.".to_string());
        }
        let mut map = [0u8; 256];
        for (i, m) in map.iter_mut().enumerate() {
            *m = i as u8;
        }
        for (x, y) in a.iter().zip(&b) {
            map[*x as usize] = *y;
        }
        self.end_of_command()?;
        Ok(Cmd::Translate(Box::new(map)))
    }
}

fn unescape_y(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'\\' && i + 1 < s.len() {
            out.push(match s[i + 1] {
                b'n' => b'\n',
                b't' => b'\t',
                other => other,
            });
            i += 2;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}

/// Split a replacement into literal runs and group references.
pub fn replacement(s: &[u8]) -> Vec<Rep> {
    let mut out = Vec::new();
    let mut lit = Vec::new();
    let mut i = 0;
    while i < s.len() {
        match s[i] {
            b'&' => {
                if !lit.is_empty() {
                    out.push(Rep::Lit(std::mem::take(&mut lit)));
                }
                out.push(Rep::Group(0));
            }
            b'\\' if i + 1 < s.len() => {
                i += 1;
                match s[i] {
                    d @ b'0'..=b'9' => {
                        if !lit.is_empty() {
                            out.push(Rep::Lit(std::mem::take(&mut lit)));
                        }
                        out.push(Rep::Group(usize::from(d - b'0')));
                    }
                    b'n' => lit.push(b'\n'),
                    b't' => lit.push(b'\t'),
                    other => lit.push(other),
                }
            }
            c => lit.push(c),
        }
        i += 1;
    }
    if !lit.is_empty() {
        out.push(Rep::Lit(lit));
    }
    out
}
