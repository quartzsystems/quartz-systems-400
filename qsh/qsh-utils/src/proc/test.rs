// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `test expression`: the program form of the shell's builtin, for
//! `find -exec` and `xargs`. POSIX's rules by argument count, then
//! `!`, `-a`, `-o` and parentheses. Status 0 true, 1 false, 2 for an
//! error.

use qsh::sys::{Access, FileType, Stat, UtilCtx};

use crate::common::complain;

const NAME: &str = "test";

const UNARY: &[&str] = &[
    "-b", "-c", "-d", "-e", "-f", "-g", "-h", "-k", "-L", "-n", "-p", "-r", "-s", "-S", "-t", "-u",
    "-w", "-x", "-z",
];
const BINARY: &[&str] = &[
    "=", "==", "!=", "<", ">", "-eq", "-ne", "-gt", "-ge", "-lt", "-le", "-nt", "-ot", "-ef",
];

struct T<'c, 'a> {
    cx: &'c UtilCtx<'a>,
}

impl T<'_, '_> {
    fn stat(&self, p: &str) -> Option<Stat> {
        self.cx.sys.stat(&self.cx.path(p)).ok()
    }

    fn unary(&self, op: &str, arg: &str) -> Result<bool, String> {
        let sys = self.cx.sys;
        let path = self.cx.path(arg);
        let is = |t: FileType| self.stat(arg).is_some_and(|s| s.ftype == t);
        let bit = |m: u32| self.stat(arg).is_some_and(|s| s.mode & m != 0);
        Ok(match op {
            "-n" => !arg.is_empty(),
            "-z" => arg.is_empty(),
            "-e" => self.stat(arg).is_some(),
            "-f" => is(FileType::File),
            "-d" => is(FileType::Dir),
            "-b" => is(FileType::Block),
            "-c" => is(FileType::Char),
            "-p" => is(FileType::Fifo),
            "-S" => is(FileType::Socket),
            "-h" | "-L" => sys.lstat(&path).is_ok_and(|s| s.ftype == FileType::Symlink),
            "-s" => self.stat(arg).is_some_and(|s| s.size > 0),
            "-g" => bit(0o2000),
            "-u" => bit(0o4000),
            "-k" => bit(0o1000),
            "-r" => sys.access(&path, Access::Read).is_ok(),
            "-w" => sys.access(&path, Access::Write).is_ok(),
            "-x" => sys.access(&path, Access::Exec).is_ok(),
            "-t" => {
                let n = u32::try_from(int(arg)?).map_err(|_| format!("{arg} is not a number."))?;
                self.cx.fd(n).is_some_and(|f| f.is_terminal())
            }
            _ => return Err(format!("{op} is not a unary operator.")),
        })
    }

    fn binary(&self, l: &str, op: &str, r: &str) -> Result<bool, String> {
        Ok(match op {
            "=" | "==" => l == r,
            "!=" => l != r,
            "<" => l < r,
            ">" => l > r,
            "-eq" => int(l)? == int(r)?,
            "-ne" => int(l)? != int(r)?,
            "-gt" => int(l)? > int(r)?,
            "-ge" => int(l)? >= int(r)?,
            "-lt" => int(l)? < int(r)?,
            "-le" => int(l)? <= int(r)?,
            "-nt" => match (self.stat(l), self.stat(r)) {
                (Some(a), Some(b)) => a.mtime > b.mtime,
                (Some(_), None) => true,
                _ => false,
            },
            "-ot" => match (self.stat(l), self.stat(r)) {
                (Some(a), Some(b)) => a.mtime < b.mtime,
                (None, Some(_)) => true,
                _ => false,
            },
            "-ef" => match (self.stat(l), self.stat(r)) {
                (Some(a), Some(b)) => a.dev == b.dev && a.ino == b.ino,
                _ => false,
            },
            _ => return Err(format!("{op} is not a binary operator.")),
        })
    }

    fn eval(&self, a: &[&str]) -> Result<bool, String> {
        match a.len() {
            0 => Ok(false),
            1 => Ok(!a[0].is_empty()),
            2 => {
                if a[0] == "!" {
                    Ok(a[1].is_empty())
                } else if UNARY.contains(&a[0]) {
                    self.unary(a[0], a[1])
                } else {
                    Err(format!("{} is not a unary operator.", a[0]))
                }
            }
            3 => {
                if BINARY.contains(&a[1]) {
                    self.binary(a[0], a[1], a[2])
                } else if a[1] == "-a" || a[1] == "-o" {
                    let (l, r) = (!a[0].is_empty(), !a[2].is_empty());
                    Ok(if a[1] == "-a" { l && r } else { l || r })
                } else if a[0] == "!" {
                    Ok(!self.eval(&a[1..])?)
                } else if a[0] == "(" && a[2] == ")" {
                    Ok(!a[1].is_empty())
                } else {
                    Err(format!("{} is not a binary operator.", a[1]))
                }
            }
            4 if a[0] == "!" => Ok(!self.eval(&a[1..])?),
            4 if a[0] == "(" && a[3] == ")" => self.eval(&a[1..3]),
            _ => {
                let mut p = P { t: self, a, i: 0 };
                let v = p.or()?;
                if p.i != a.len() {
                    return Err(format!("{} was not expected.", a[p.i]));
                }
                Ok(v)
            }
        }
    }
}

struct P<'t, 'c, 'a, 'x> {
    t: &'t T<'c, 'a>,
    a: &'x [&'x str],
    i: usize,
}

impl P<'_, '_, '_, '_> {
    fn or(&mut self) -> Result<bool, String> {
        let mut v = self.and()?;
        while self.a.get(self.i) == Some(&"-o") {
            self.i += 1;
            let r = self.and()?;
            v = v || r;
        }
        Ok(v)
    }

    fn and(&mut self) -> Result<bool, String> {
        let mut v = self.not()?;
        while self.a.get(self.i) == Some(&"-a") {
            self.i += 1;
            let r = self.not()?;
            v = v && r;
        }
        Ok(v)
    }

    fn not(&mut self) -> Result<bool, String> {
        if self.a.get(self.i) == Some(&"!") && self.i + 1 < self.a.len() {
            self.i += 1;
            return Ok(!self.not()?);
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<bool, String> {
        let Some(&t) = self.a.get(self.i) else {
            return Err("an argument is missing.".to_string());
        };
        if t == "(" {
            self.i += 1;
            let v = self.or()?;
            if self.a.get(self.i) != Some(&")") {
                return Err("the closing ) is missing.".to_string());
            }
            self.i += 1;
            return Ok(v);
        }
        if let (Some(op), Some(r)) = (self.a.get(self.i + 1), self.a.get(self.i + 2))
            && BINARY.contains(op)
        {
            self.i += 3;
            return self.t.binary(t, op, r);
        }
        if UNARY.contains(&t)
            && let Some(arg) = self.a.get(self.i + 1)
        {
            self.i += 2;
            return self.t.unary(t, arg);
        }
        self.i += 1;
        Ok(!t.is_empty())
    }
}

fn int(s: &str) -> Result<i64, String> {
    let t = s.trim();
    let t = t.strip_prefix('+').unwrap_or(t);
    t.parse::<i64>().map_err(|_| format!("{s} is not a number."))
}

/// Run `test`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let args: Vec<&str> = cx.argv[1..].iter().map(String::as_str).collect();
    let t = T { cx };
    match t.eval(&args) {
        Ok(b) => i32::from(!b),
        Err(e) => {
            complain(cx, NAME, &e);
            2
        }
    }
}
