// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sed cycle: the pattern and hold spaces, addresses and ranges,
//! the append queue, branching and every command.

use std::sync::Arc;

use qsh::sys::stream::write_all;
use qsh::sys::UtilCtx;

use super::parse::{Addr, Cmd, Line, Rep, Script, WFile};
use crate::common::complain_sys;
use crate::common::io::{Lines, Out, input, inputs, read_input};
use crate::regex::Regex;

fn write_w(st: &mut State, w: &WFile, line: &[u8]) {
    match w {
        WFile::Stdout => {
            st.out.write(line);
        }
        WFile::Fd(fd) => {
            let _ = write_all(&**fd, line);
        }
    }
}

/// Input lines across every operand, with one line of lookahead so
/// `$` is known.
struct Input<'c, 'a> {
    cx: &'c UtilCtx<'a>,
    name: &'c str,
    files: Vec<String>,
    next: usize,
    cur: Option<Lines>,
    ahead: Option<(Vec<u8>, bool)>,
    status: i32,
}

impl Input<'_, '_> {
    fn open_next(&mut self) -> bool {
        while self.next < self.files.len() {
            let f = self.files[self.next].clone();
            self.next += 1;
            match input(self.cx, &f) {
                Ok(fd) => {
                    self.cur = Some(Lines::new(fd));
                    return true;
                }
                Err(e) => {
                    complain_sys(self.cx, self.name, &f, e);
                    self.status = 2;
                }
            }
        }
        false
    }

    fn raw_next(&mut self) -> Option<(Vec<u8>, bool)> {
        loop {
            if self.cur.is_none() && !self.open_next() {
                return None;
            }
            if let Some(rd) = &mut self.cur {
                match rd.next_text() {
                    Ok(Some(l)) => return Some(l),
                    Ok(None) | Err(_) => self.cur = None,
                }
            }
        }
    }

    fn fill(&mut self) {
        if self.ahead.is_none() {
            self.ahead = self.raw_next();
        }
    }

    /// The next line (without newline) and whether it ended in one.
    fn line(&mut self) -> Option<(Vec<u8>, bool)> {
        self.fill();
        self.ahead.take()
    }

    fn is_last(&mut self) -> bool {
        self.fill();
        self.ahead.is_none()
    }
}

#[derive(Clone, Copy, Default)]
struct Range {
    active: bool,
}

enum Queued {
    Text(Vec<u8>),
    File(String),
}

struct State {
    ps: Vec<u8>,
    hs: Vec<u8>,
    lineno: u64,
    had_nl: bool,
    quiet: bool,
    tflag: bool,
    queue: Vec<Queued>,
    ranges: Vec<Range>,
    last_re: Option<Arc<Regex>>,
    out: Out,
    quit: Option<i32>,
}

impl State {
    fn emit(&mut self, b: &[u8], nl: bool) {
        self.out.write(b);
        if nl {
            self.out.write(b"\n");
        }
    }

    fn flush_queue(&mut self, cx: &UtilCtx<'_>) {
        for q in std::mem::take(&mut self.queue) {
            match q {
                Queued::Text(t) => {
                    self.out.write(&t);
                }
                Queued::File(f) => {
                    if let Ok(d) = read_input(cx, &f) {
                        self.out.write(&d);
                    }
                }
            }
        }
    }

    fn resolve(&mut self, re: Option<&Arc<Regex>>) -> Option<Arc<Regex>> {
        match re {
            Some(r) => {
                self.last_re = Some(Arc::clone(r));
                Some(Arc::clone(r))
            }
            None => self.last_re.clone(),
        }
    }

    fn matches_addr(&mut self, a: &Addr, last: bool) -> bool {
        match a {
            Addr::Line(n) => *n == self.lineno,
            Addr::Last => last,
            Addr::Re(re) => match self.resolve(re.as_ref()) {
                Some(r) => r.is_match(&self.ps),
                None => false,
            },
        }
    }

    fn selected(&mut self, idx: usize, line: &Line, last: bool) -> bool {
        let hit = match (&line.a1, &line.a2) {
            (None, _) => true,
            (Some(a1), None) => self.matches_addr(a1, last),
            (Some(a1), Some(a2)) => {
                let r = self.ranges[idx];
                if r.active {
                    let done = match a2 {
                        Addr::Line(n) => self.lineno >= *n,
                        _ => self.matches_addr(a2, last),
                    };
                    if done {
                        self.ranges[idx].active = false;
                    }
                    true
                } else if self.matches_addr(a1, last) {
                    let one = match a2 {
                        Addr::Line(n) => *n <= self.lineno,
                        _ => false,
                    };
                    if !one {
                        self.ranges[idx].active = true;
                    }
                    true
                } else {
                    false
                }
            }
        };
        hit != line.negate
    }
}

fn list(ps: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut col = 0;
    let mut push = |s: &[u8], out: &mut Vec<u8>| {
        if col + s.len() > 69 {
            out.extend_from_slice(b"\\\n");
            col = 0;
        }
        out.extend_from_slice(s);
        col += s.len();
    };
    for &b in ps {
        let esc: Vec<u8> = match b {
            b'\\' => b"\\\\".to_vec(),
            7 => b"\\a".to_vec(),
            8 => b"\\b".to_vec(),
            12 => b"\\f".to_vec(),
            b'\n' => b"\\n".to_vec(),
            b'\r' => b"\\r".to_vec(),
            b'\t' => b"\\t".to_vec(),
            11 => b"\\v".to_vec(),
            b if (0x20..0x7f).contains(&b) => vec![b],
            b => format!("\\{b:03o}").into_bytes(),
        };
        push(&esc, &mut out);
    }
    out.extend_from_slice(b"$\n");
    out
}

fn substitute(st: &mut State, re: &Regex, rep: &[Rep], global: bool, nth: usize) -> bool {
    let text = st.ps.clone();
    let mut out = Vec::with_capacity(text.len());
    let mut pos = 0;
    let mut copied = 0;
    let mut count = 0;
    let mut done = false;
    let mut last_end: Option<usize> = None;
    while pos <= text.len() {
        let Some(m) = re.find_at(&text, pos) else {
            break;
        };
        // An empty match right after a match is not a match.
        if m.start == m.end && last_end == Some(m.start) {
            if m.start < text.len() {
                out.extend_from_slice(&text[copied..=m.start]);
                copied = m.start + 1;
            }
            pos = m.start + 1;
            continue;
        }
        if m.end > m.start {
            last_end = Some(m.end);
        }
        count += 1;
        if count >= nth {
            out.extend_from_slice(&text[copied..m.start]);
            for r in rep {
                match r {
                    Rep::Lit(l) => out.extend_from_slice(l),
                    Rep::Group(g) => {
                        if let Some(s) = m.group(&text, *g) {
                            out.extend_from_slice(s);
                        }
                    }
                }
            }
            copied = m.end;
            done = true;
            if !global {
                break;
            }
        }
        if m.end == m.start {
            if m.end < text.len() {
                out.extend_from_slice(&text[copied..=m.end]);
                copied = m.end + 1;
            }
            pos = m.end + 1;
        } else {
            pos = m.end;
        }
    }
    if !done {
        return false;
    }
    if copied < text.len() {
        out.extend_from_slice(&text[copied..]);
    }
    st.ps = out;
    true
}

/// Run a script over the operands.
pub fn run(cx: &mut UtilCtx<'_>, name: &str, prog: Script, quiet: bool, operands: &[String]) -> i32 {
    let mut lines = prog.lines;
    // Resolve block jumps.
    let mut stack = Vec::new();
    for i in 0..lines.len() {
        match lines[i].cmd {
            Cmd::Block(_) => stack.push(i),
            Cmd::End => {
                if let Some(open) = stack.pop() {
                    lines[open].cmd = Cmd::Block(i);
                }
            }
            _ => {}
        }
    }
    let wfiles: Vec<WFile> = prog.wfiles;
    let mut input = Input {
        cx,
        name,
        files: inputs(operands),
        next: 0,
        cur: None,
        ahead: None,
        status: 0,
    };
    let mut st = State {
        ps: Vec::new(),
        hs: Vec::new(),
        lineno: 0,
        had_nl: true,
        quiet,
        tflag: false,
        queue: Vec::new(),
        ranges: vec![Range::default(); lines.len()],
        last_re: None,
        out: Out::stdout(cx),
        quit: None,
    };
    let mut restart = false;
    'cycle: loop {
        if !restart {
            let Some((l, nl)) = input.line() else {
                break;
            };
            st.ps = l;
            st.had_nl = nl;
            st.lineno += 1;
            st.tflag = false;
        }
        restart = false;
        let mut pc = 0;
        let mut autoprint = !st.quiet;
        while pc < lines.len() {
            let line = lines[pc].clone();
            let last = input.is_last();
            if !st.selected(pc, &line, last) {
                pc = match line.cmd {
                    Cmd::Block(end) => end + 1,
                    _ => pc + 1,
                };
                continue;
            }
            pc += 1;
            match line.cmd {
                Cmd::Block(_) | Cmd::End | Cmd::Nop => {}
                Cmd::LineNo => {
                    let s = format!("{}\n", st.lineno);
                    st.out.write(s.as_bytes());
                }
                Cmd::Append(t) => st.queue.push(Queued::Text(t)),
                Cmd::Insert(t) => {
                    st.out.write(&t);
                }
                Cmd::Change(t) => {
                    // With a range, the text comes at the range's end.
                    let in_range = line.a2.is_some() && !line.negate;
                    let range_done = !st.ranges[pc - 1].active;
                    if !in_range || range_done {
                        st.out.write(&t);
                    }
                    st.flush_queue(cx);
                    continue 'cycle;
                }
                Cmd::Branch(t) => pc = t.unwrap_or(lines.len()),
                Cmd::Test(t) => {
                    if st.tflag {
                        st.tflag = false;
                        pc = t.unwrap_or(lines.len());
                    }
                }
                Cmd::Delete => {
                    st.flush_queue(cx);
                    continue 'cycle;
                }
                Cmd::DeleteFirst => {
                    st.flush_queue(cx);
                    if let Some(i) = st.ps.iter().position(|&b| b == b'\n') {
                        st.ps.drain(..=i);
                        restart = true;
                    }
                    continue 'cycle;
                }
                Cmd::GetHold => st.ps.clone_from(&st.hs),
                Cmd::AppendHold => {
                    st.ps.push(b'\n');
                    let hs = st.hs.clone();
                    st.ps.extend(hs);
                }
                Cmd::Hold => st.hs.clone_from(&st.ps),
                Cmd::AppendToHold => {
                    st.hs.push(b'\n');
                    let ps = st.ps.clone();
                    st.hs.extend(ps);
                }
                Cmd::Exchange => std::mem::swap(&mut st.ps, &mut st.hs),
                Cmd::List => {
                    let l = list(&st.ps);
                    st.out.write(&l);
                }
                Cmd::Next => {
                    if input.is_last() {
                        break;
                    }
                    if !st.quiet {
                        let ps = st.ps.clone();
                        st.emit(&ps, st.had_nl);
                    }
                    st.flush_queue(cx);
                    if let Some((l, nl)) = input.line() {
                        st.ps = l;
                        st.had_nl = nl;
                        st.lineno += 1;
                        // Reading a line clears the t flag (POSIX).
                        st.tflag = false;
                    }
                }
                Cmd::NextAppend => {
                    if input.is_last() {
                        // POSIX: quit without printing.
                        autoprint = false;
                        st.quit = Some(0);
                        break;
                    }
                    if let Some((l, nl)) = input.line() {
                        st.ps.push(b'\n');
                        st.ps.extend(l);
                        st.had_nl = nl;
                        st.lineno += 1;
                        st.tflag = false;
                    }
                }
                Cmd::Print => {
                    let ps = st.ps.clone();
                    st.emit(&ps, true);
                }
                Cmd::PrintFirst => {
                    let end = st.ps.iter().position(|&b| b == b'\n').unwrap_or(st.ps.len());
                    let first = st.ps[..end].to_vec();
                    st.emit(&first, true);
                }
                Cmd::Quit(n) => {
                    st.quit = Some(n);
                    break;
                }
                Cmd::ReadFile(f) => st.queue.push(Queued::File(f)),
                Cmd::Write(w) => {
                    let mut l = st.ps.clone();
                    l.push(b'\n');
                    write_w(&mut st, &wfiles[w], &l);
                }
                Cmd::Subst(s) => {
                    let Some(re) = st.resolve(s.re.as_ref()) else {
                        st.out.finish(cx, name, 0);
                        crate::common::complain(cx, name, "No previous regular expression.");
                        return 1;
                    };
                    if substitute(&mut st, &re, &s.rep, s.global, s.nth) {
                        st.tflag = true;
                        if s.print {
                            let ps = st.ps.clone();
                            st.emit(&ps, true);
                        }
                        if let Some(w) = s.wfile {
                            let mut l = st.ps.clone();
                            l.push(b'\n');
                            write_w(&mut st, &wfiles[w], &l);
                        }
                    }
                }
                Cmd::Translate(map) => {
                    for b in &mut st.ps {
                        *b = map[*b as usize];
                    }
                }
            }
        }
        if autoprint {
            let ps = st.ps.clone();
            let nl = st.had_nl || !input.is_last();
            st.emit(&ps, nl);
        }
        st.flush_queue(cx);
        if let Some(code) = st.quit {
            let status = st.out.finish(cx, name, code);
            return status.max(input.status);
        }
    }
    let status = st.out.finish(cx, name, 0);
    status.max(input.status)
}
