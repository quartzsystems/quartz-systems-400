// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `find [-H | -L] path... [expression]`: walk trees and evaluate the
//! POSIX expression language (`-name -path -type -size -mtime -newer
//! -user -group -perm -links -prune -print -exec -ok -depth -xdev`,
//! `!`, `-a`, `-o`, parentheses) at each path.

use qsh::pattern;
use qsh::sys::{FileType, Stat, UtilCtx};

use crate::common::exec;
use crate::common::io::{Out, confirm};
use crate::common::walk::{Control, Entry, Visit, Walker};
use crate::common::{complain, complain_sys, mode, usage};

#[derive(Clone, Copy, Debug)]
enum Num {
    Exact(u64),
    More(u64),
    Less(u64),
}

impl Num {
    fn parse(s: &str) -> Option<Self> {
        let (kind, digits) = match s.as_bytes().first() {
            Some(b'+') => (1, &s[1..]),
            Some(b'-') => (2, &s[1..]),
            _ => (0, s),
        };
        let n: u64 = digits.parse().ok()?;
        Some(match kind {
            1 => Self::More(n),
            2 => Self::Less(n),
            _ => Self::Exact(n),
        })
    }

    fn test(self, v: u64) -> bool {
        match self {
            Self::Exact(n) => v == n,
            Self::More(n) => v > n,
            Self::Less(n) => v < n,
        }
    }
}

enum Prim {
    Name(String),
    Path(String),
    Type(char),
    Size(Num, bool),
    Mtime(Num),
    Newer(i64),
    User(u32),
    Group(u32),
    Nouser,
    Nogroup,
    Perm(u32, bool),
    Links(Num),
    Prune,
    Print,
    Exec(Vec<String>, bool),
    Ok(Vec<String>),
    True,
    False,
}

enum Expr {
    Prim(Prim),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

struct Parser<'c, 'a> {
    cx: &'c UtilCtx<'a>,
    args: Vec<String>,
    i: usize,
    depth: bool,
    xdev: bool,
    has_action: bool,
}

impl Parser<'_, '_> {
    fn peek(&self) -> Option<&str> {
        self.args.get(self.i).map(String::as_str)
    }

    fn take(&mut self) -> Option<String> {
        let a = self.args.get(self.i).cloned();
        self.i += 1;
        a
    }

    fn arg(&mut self, prim: &str) -> Result<String, String> {
        self.take()
            .ok_or_else(|| format!("{prim} needs an argument."))
    }

    fn or(&mut self) -> Result<Expr, String> {
        let mut left = self.and()?;
        while self.peek() == Some("-o") {
            self.i += 1;
            let right = self.and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Expr, String> {
        let mut left = self.unary()?;
        loop {
            match self.peek() {
                Some("-a") => {
                    self.i += 1;
                }
                Some("-o" | ")") | None => return Ok(left),
                _ => {}
            }
            let right = self.unary()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some("!") => {
                self.i += 1;
                Ok(Expr::Not(Box::new(self.unary()?)))
            }
            Some("(") => {
                self.i += 1;
                let e = self.or()?;
                if self.take().as_deref() != Some(")") {
                    return Err("A ( has no matching ).".to_string());
                }
                Ok(e)
            }
            Some(_) => self.primary().map(Expr::Prim),
            None => Err("The expression is not complete.".to_string()),
        }
    }

    fn command(&mut self, prim: &str) -> Result<(Vec<String>, bool), String> {
        let mut argv = Vec::new();
        loop {
            let a = self.arg(prim)?;
            if a == ";" {
                return Ok((argv, false));
            }
            if a == "+" && argv.last().is_some_and(|l| l == "{}") {
                argv.pop();
                return Ok((argv, true));
            }
            argv.push(a);
        }
    }

    fn primary(&mut self) -> Result<Prim, String> {
        let p = self.take().unwrap_or_default();
        let now = i64::try_from(self.cx.sys.now().as_secs()).unwrap_or(0);
        let _ = now;
        Ok(match p.as_str() {
            "-name" => Prim::Name(self.arg(&p)?),
            "-path" => Prim::Path(self.arg(&p)?),
            "-type" => {
                let t = self.arg(&p)?;
                let c = t.chars().next().unwrap_or(' ');
                if t.len() != 1 || !"bcdflps".contains(c) {
                    return Err(format!("The type {t} is not valid."));
                }
                Prim::Type(c)
            }
            "-size" => {
                let s = self.arg(&p)?;
                let (num, bytes) = match s.strip_suffix('c') {
                    Some(n) => (n, true),
                    None => (s.as_str(), false),
                };
                Prim::Size(Num::parse(num).ok_or(format!("The size {s} is not valid."))?, bytes)
            }
            "-mtime" | "-atime" | "-ctime" => {
                let s = self.arg(&p)?;
                Prim::Mtime(Num::parse(&s).ok_or(format!("The number {s} is not valid."))?)
            }
            "-links" => {
                let s = self.arg(&p)?;
                Prim::Links(Num::parse(&s).ok_or(format!("The number {s} is not valid."))?)
            }
            "-newer" => {
                let f = self.arg(&p)?;
                let st = self
                    .cx
                    .sys
                    .stat(&self.cx.path(&f))
                    .map_err(|e| format!("{f}: {}", e.errno.text()))?;
                Prim::Newer(st.mtime)
            }
            "-user" => {
                let u = self.arg(&p)?;
                let id = self
                    .cx
                    .sys
                    .user_id(&u)
                    .or_else(|| u.parse().ok())
                    .ok_or(format!("The user {u} is not known."))?;
                Prim::User(id)
            }
            "-group" => {
                let g = self.arg(&p)?;
                let id = self
                    .cx
                    .sys
                    .group_id(&g)
                    .or_else(|| g.parse().ok())
                    .ok_or(format!("The group {g} is not known."))?;
                Prim::Group(id)
            }
            "-nouser" => Prim::Nouser,
            "-nogroup" => Prim::Nogroup,
            "-perm" => {
                let m = self.arg(&p)?;
                let (spec, all) = match m.strip_prefix('-') {
                    Some(s) => (s, true),
                    None => (m.as_str(), false),
                };
                let bits = mode::parse(spec, 0, false, 0)
                    .ok_or(format!("The mode {m} is not valid."))?;
                Prim::Perm(bits, all)
            }
            "-prune" => Prim::Prune,
            "-print" => {
                self.has_action = true;
                Prim::Print
            }
            "-exec" => {
                self.has_action = true;
                let (argv, plus) = self.command(&p)?;
                Prim::Exec(argv, plus)
            }
            "-ok" => {
                self.has_action = true;
                let (argv, _) = self.command(&p)?;
                Prim::Ok(argv)
            }
            "-depth" => {
                self.depth = true;
                Prim::True
            }
            "-xdev" => {
                self.xdev = true;
                Prim::True
            }
            "-true" => Prim::True,
            "-false" => Prim::False,
            other => return Err(format!("The expression {other} is not known.")),
        })
    }
}

struct Run<'c, 'a> {
    cx: &'c UtilCtx<'a>,
    now: i64,
    out: Out,
    status: i32,
    prune: bool,
    batches: Vec<(Vec<String>, Vec<String>)>,
}

impl Run<'_, '_> {
    fn eval(&mut self, e: &Expr, shown: &str, ent: &Entry) -> bool {
        match e {
            Expr::Not(x) => !self.eval(x, shown, ent),
            Expr::And(a, b) => self.eval(a, shown, ent) && self.eval(b, shown, ent),
            Expr::Or(a, b) => self.eval(a, shown, ent) || self.eval(b, shown, ent),
            Expr::Prim(p) => self.prim(p, shown, &ent.st),
        }
    }

    fn prim(&mut self, p: &Prim, shown: &str, st: &Stat) -> bool {
        match p {
            Prim::Name(pat) => {
                let base = crate::file::basename(shown);
                pattern::matches(&pattern::plain(pat), base, false)
            }
            Prim::Path(pat) => pattern::matches(&pattern::plain(pat), shown, false),
            Prim::Type(c) => {
                let t = match st.ftype {
                    FileType::Block => 'b',
                    FileType::Char => 'c',
                    FileType::Dir => 'd',
                    FileType::File | FileType::Other => 'f',
                    FileType::Symlink => 'l',
                    FileType::Fifo => 'p',
                    FileType::Socket => 's',
                };
                t == *c
            }
            Prim::Size(n, bytes) => {
                let v = if *bytes { st.size } else { st.size.div_ceil(512) };
                n.test(v)
            }
            Prim::Mtime(n) => {
                let days = (self.now - st.mtime).max(0) / 86400;
                n.test(days as u64)
            }
            Prim::Newer(t) => st.mtime > *t,
            Prim::User(u) => st.uid == *u,
            Prim::Group(g) => st.gid == *g,
            Prim::Nouser => self.cx.sys.user_name(st.uid).is_none(),
            Prim::Nogroup => self.cx.sys.group_name(st.gid).is_none(),
            Prim::Perm(bits, all) => {
                if *all {
                    st.mode & bits == *bits
                } else {
                    st.mode & 0o7777 == *bits
                }
            }
            Prim::Links(n) => n.test(u64::from(st.nlink)),
            Prim::Prune => {
                self.prune = true;
                true
            }
            Prim::Print => {
                self.out.text(shown);
                self.out.text("\n");
                true
            }
            Prim::Exec(argv, plus) => {
                if *plus {
                    match self.batches.iter_mut().find(|(a, _)| a == argv) {
                        Some((_, paths)) => paths.push(shown.to_string()),
                        None => self.batches.push((argv.clone(), vec![shown.to_string()])),
                    }
                    return true;
                }
                self.out.flush();
                let argv: Vec<String> = argv.iter().map(|a| a.replace("{}", shown)).collect();
                match exec::run(self.cx, &argv, None, None) {
                    Ok(0) => true,
                    Ok(_) => false,
                    Err(e) => {
                        complain(self.cx, "find", &format!("{}: {}", argv[0], e.text()));
                        self.status = 1;
                        false
                    }
                }
            }
            Prim::Ok(argv) => {
                self.out.flush();
                let argv: Vec<String> = argv.iter().map(|a| a.replace("{}", shown)).collect();
                if !confirm(self.cx, &format!("{} ? ", argv.join(" "))) {
                    return false;
                }
                match exec::run(self.cx, &argv, None, None) {
                    Ok(0) => true,
                    Ok(_) => false,
                    Err(e) => {
                        complain(self.cx, "find", &format!("{}: {}", argv[0], e.text()));
                        self.status = 1;
                        false
                    }
                }
            }
            Prim::True => true,
            Prim::False => false,
        }
    }

    fn run_batches(&mut self) {
        self.out.flush();
        for (argv, paths) in std::mem::take(&mut self.batches) {
            let mut full = argv.clone();
            full.extend(paths);
            match exec::run(self.cx, &full, None, None) {
                Ok(0) => {}
                Ok(_) => self.status = 1,
                Err(e) => {
                    complain(self.cx, "find", &format!("{}: {}", full[0], e.text()));
                    self.status = 1;
                }
            }
        }
    }
}

/// Run `find`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "find";
    let mut args: Vec<String> = cx.argv[1..].to_vec();
    let mut follow_root = false;
    let mut follow_all = false;
    while let Some(a) = args.first() {
        match a.as_str() {
            "-H" => {
                follow_root = true;
                follow_all = false;
            }
            "-L" => {
                follow_all = true;
                follow_root = false;
            }
            _ => break,
        }
        args.remove(0);
    }
    let mut paths = Vec::new();
    while let Some(a) = args.first() {
        if a.starts_with('-') || a == "!" || a == "(" {
            break;
        }
        paths.push(args.remove(0));
    }
    if paths.is_empty() {
        return usage(cx, name, "A path is needed.");
    }
    let mut parser = Parser {
        cx,
        args,
        i: 0,
        depth: false,
        xdev: false,
        has_action: false,
    };
    let expr = if parser.peek().is_none() {
        Expr::Prim(Prim::True)
    } else {
        match parser.or() {
            Ok(e) => e,
            Err(e) => return usage(cx, name, &e),
        }
    };
    if parser.i < parser.args.len() {
        return usage(cx, name, "The expression has text after its end.");
    }
    let expr = if parser.has_action {
        expr
    } else {
        Expr::And(Box::new(expr), Box::new(Expr::Prim(Prim::Print)))
    };
    let depth_first = parser.depth;
    let w = Walker {
        follow_root,
        follow_all,
        one_fs: parser.xdev,
    };
    let mut r = Run {
        cx,
        now: i64::try_from(cx.sys.now().as_secs()).unwrap_or(0),
        out: Out::stdout(cx),
        status: 0,
        prune: false,
        batches: Vec::new(),
    };
    for op in &paths {
        let abs = cx.path(op);
        let prefix = abs.len();
        let shown_of = |path: &str| format!("{op}{}", &path[prefix.min(path.len())..]);
        let mut errors = Vec::new();
        w.walk(cx.sys, &abs, &mut |v| match v {
            Visit::Pre(e) => {
                if depth_first {
                    if e.st.ftype != FileType::Dir {
                        r.eval(&expr, &shown_of(&e.path), e);
                    }
                    return Control::Continue;
                }
                r.prune = false;
                r.eval(&expr, &shown_of(&e.path), e);
                if r.prune {
                    Control::Prune
                } else {
                    Control::Continue
                }
            }
            Visit::Post(e) => {
                if depth_first {
                    r.eval(&expr, &shown_of(&e.path), e);
                }
                Control::Continue
            }
            Visit::Fail(path, e) => {
                errors.push((shown_of(path), e));
                Control::Continue
            }
        });
        for (path, e) in errors {
            r.out.flush();
            complain_sys(cx, name, &path, e);
            r.status = 1;
        }
    }
    r.run_batches();
    let status = r.status;
    r.out.finish(cx, name, status)
}
