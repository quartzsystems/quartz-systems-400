// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `test` and `[`: POSIX's rules by argument count, then a parser for
//! `!`, `-a`, `-o` and parentheses.

use crate::msg::Msg;
use crate::pattern::path::absolute;
use crate::shell::{Res, Shell};
use crate::sys::{Access, FileType, Stat};

const UNARY: &[&str] = &[
    "-b", "-c", "-d", "-e", "-f", "-g", "-h", "-k", "-L", "-n", "-p", "-r", "-s", "-S", "-t", "-u",
    "-w", "-x", "-z",
];
const BINARY: &[&str] = &[
    "=", "==", "!=", "<", ">", "-eq", "-ne", "-gt", "-ge", "-lt", "-le", "-nt", "-ot", "-ef",
];

/// `test expr`.
pub(crate) fn test(sh: &mut Shell, args: &[String]) -> Res {
    finish(sh, &args[1..])
}

/// `[ expr ]`.
pub(crate) fn bracket(sh: &mut Shell, args: &[String]) -> Res {
    if args.last().is_none_or(|a| a != "]") {
        sh.report(&Msg::Usage(
            "[".to_string(),
            "the closing ] is missing.".to_string(),
        ));
        return Ok(2);
    }
    finish(sh, &args[1..args.len() - 1])
}

fn finish(sh: &mut Shell, a: &[String]) -> Res {
    let a: Vec<&str> = a.iter().map(String::as_str).collect();
    match eval(sh, &a) {
        Ok(b) => Ok(i32::from(!b)),
        Err(d) => {
            sh.report(&Msg::Usage("test".to_string(), d));
            Ok(2)
        }
    }
}

fn eval(sh: &mut Shell, a: &[&str]) -> Result<bool, String> {
    match a.len() {
        0 => Ok(false),
        1 => Ok(!a[0].is_empty()),
        2 => {
            if a[0] == "!" {
                Ok(a[1].is_empty())
            } else if UNARY.contains(&a[0]) {
                unary(sh, a[0], a[1])
            } else {
                Err(format!("{} is not a unary operator.", a[0]))
            }
        }
        3 => {
            if BINARY.contains(&a[1]) {
                binary(sh, a[0], a[1], a[2])
            } else if a[1] == "-a" || a[1] == "-o" {
                let (l, r) = (!a[0].is_empty(), !a[2].is_empty());
                Ok(if a[1] == "-a" { l && r } else { l || r })
            } else if a[0] == "!" {
                Ok(!eval(sh, &a[1..])?)
            } else if a[0] == "(" && a[2] == ")" {
                Ok(!a[1].is_empty())
            } else {
                Err(format!("{} is not a binary operator.", a[1]))
            }
        }
        4 if a[0] == "!" => Ok(!eval(sh, &a[1..])?),
        4 if a[0] == "(" && a[3] == ")" => eval(sh, &a[1..3]),
        _ => {
            let mut p = P { a, i: 0 };
            let v = p.or(sh)?;
            if p.i != a.len() {
                return Err(format!("{} was not expected.", a[p.i]));
            }
            Ok(v)
        }
    }
}

struct P<'a> {
    a: &'a [&'a str],
    i: usize,
}

impl P<'_> {
    fn or(&mut self, sh: &mut Shell) -> Result<bool, String> {
        let mut v = self.and(sh)?;
        while self.a.get(self.i) == Some(&"-o") {
            self.i += 1;
            let r = self.and(sh)?;
            v = v || r;
        }
        Ok(v)
    }

    fn and(&mut self, sh: &mut Shell) -> Result<bool, String> {
        let mut v = self.not(sh)?;
        while self.a.get(self.i) == Some(&"-a") {
            self.i += 1;
            let r = self.not(sh)?;
            v = v && r;
        }
        Ok(v)
    }

    fn not(&mut self, sh: &mut Shell) -> Result<bool, String> {
        if self.a.get(self.i) == Some(&"!") && self.i + 1 < self.a.len() {
            self.i += 1;
            return Ok(!self.not(sh)?);
        }
        self.primary(sh)
    }

    fn primary(&mut self, sh: &mut Shell) -> Result<bool, String> {
        let Some(&t) = self.a.get(self.i) else {
            return Err("an argument is missing.".to_string());
        };
        if t == "(" {
            self.i += 1;
            let v = self.or(sh)?;
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
            return binary(sh, t, op, r);
        }
        if UNARY.contains(&t)
            && let Some(arg) = self.a.get(self.i + 1)
        {
            self.i += 2;
            return unary(sh, t, arg);
        }
        self.i += 1;
        Ok(!t.is_empty())
    }
}

fn int(s: &str) -> Result<i64, String> {
    let t = s.trim();
    let t = t.strip_prefix('+').unwrap_or(t);
    t.parse::<i64>()
        .map_err(|_| format!("{s} is not a number."))
}

fn stat(sh: &Shell, p: &str) -> Option<Stat> {
    sh.sys.stat(&absolute(&sh.cwd, p)).ok()
}

fn unary(sh: &mut Shell, op: &str, arg: &str) -> Result<bool, String> {
    let path = || absolute(&sh.cwd, arg);
    Ok(match op {
        "-n" => !arg.is_empty(),
        "-z" => arg.is_empty(),
        "-e" => stat(sh, arg).is_some(),
        "-f" => stat(sh, arg).is_some_and(|s| s.ftype == FileType::File),
        "-d" => stat(sh, arg).is_some_and(|s| s.ftype == FileType::Dir),
        "-b" => stat(sh, arg).is_some_and(|s| s.ftype == FileType::Block),
        "-c" => stat(sh, arg).is_some_and(|s| s.ftype == FileType::Char),
        "-p" => stat(sh, arg).is_some_and(|s| s.ftype == FileType::Fifo),
        "-S" => stat(sh, arg).is_some_and(|s| s.ftype == FileType::Socket),
        "-h" | "-L" => sh
            .sys
            .lstat(&path())
            .is_ok_and(|s| s.ftype == FileType::Symlink),
        "-s" => stat(sh, arg).is_some_and(|s| s.size > 0),
        "-g" => stat(sh, arg).is_some_and(|s| s.mode & 0o2000 != 0),
        "-u" => stat(sh, arg).is_some_and(|s| s.mode & 0o4000 != 0),
        "-k" => stat(sh, arg).is_some_and(|s| s.mode & 0o1000 != 0),
        "-r" => sh.sys.access(&path(), Access::Read).is_ok(),
        "-w" => sh.sys.access(&path(), Access::Write).is_ok(),
        "-x" => sh.sys.access(&path(), Access::Exec).is_ok(),
        "-t" => {
            let n = u32::try_from(int(arg)?).map_err(|_| format!("{arg} is not a number."))?;
            sh.fds.get(n).is_some_and(|f| f.is_terminal())
        }
        _ => return Err(format!("{op} is not a unary operator.")),
    })
}

fn binary(sh: &mut Shell, l: &str, op: &str, r: &str) -> Result<bool, String> {
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
        "-nt" => match (stat(sh, l), stat(sh, r)) {
            (Some(a), Some(b)) => a.mtime > b.mtime,
            (Some(_), None) => true,
            _ => false,
        },
        "-ot" => match (stat(sh, l), stat(sh, r)) {
            (Some(a), Some(b)) => a.mtime < b.mtime,
            (None, Some(_)) => true,
            _ => false,
        },
        "-ef" => match (stat(sh, l), stat(sh, r)) {
            (Some(a), Some(b)) => a.dev == b.dev && a.ino == b.ino,
            _ => false,
        },
        _ => return Err(format!("{op} is not a binary operator.")),
    })
}
