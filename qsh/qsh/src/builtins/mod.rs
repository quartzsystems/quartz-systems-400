// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shell's builtins: the POSIX special builtins and Qshell's
//! regular builtins. Utilities (`ls`, `cat`, ...) are not here; they
//! live in the `qsh-utils` crates behind `Sys::spawn`.

mod dir;
mod flow;
mod io;
mod names;
mod printf;
mod proc;
mod run;
mod test;
mod vars;

use crate::msg::Msg;
use crate::shell::{Flow, Res, Shell};

/// A builtin: the shell and the arguments, `argv[0]` first.
pub type Builtin = fn(&mut Shell, &[String]) -> Res;

/// The special builtins: errors in them end a non-interactive shell
/// and their assignments persist.
pub const SPECIAL: &[(&str, Builtin)] = &[
    (":", flow::colon),
    (".", run::dot),
    ("break", flow::break_),
    ("continue", flow::continue_),
    ("eval", run::eval),
    ("exec", run::exec),
    ("exit", flow::exit),
    ("export", vars::export),
    ("readonly", vars::readonly),
    ("return", flow::return_),
    ("set", vars::set),
    ("shift", vars::shift),
    ("times", proc::times),
    ("trap", proc::trap),
    ("unset", vars::unset),
];

/// The regular builtins.
pub const REGULAR: &[(&str, Builtin)] = &[
    ("[", test::bracket),
    ("alias", names::alias),
    ("bg", proc::bg),
    ("builtin", run::builtin),
    ("cd", dir::cd),
    ("command", run::command),
    ("declare", vars::declare),
    ("echo", io::echo),
    ("false", flow::false_),
    ("fg", proc::fg),
    ("getopts", names::getopts),
    ("hash", names::hash),
    ("jobs", proc::jobs),
    ("kill", proc::kill),
    ("let", names::let_),
    ("local", vars::local),
    ("print", io::print),
    ("printf", printf::printf),
    ("pwd", dir::pwd),
    ("read", io::read),
    ("source", run::dot),
    ("test", test::test),
    ("true", flow::true_),
    ("type", names::type_),
    ("typeset", vars::declare),
    ("ulimit", proc::ulimit),
    ("umask", dir::umask),
    ("unalias", names::unalias),
    ("wait", proc::wait),
];

/// Reserved words, for `type` and `command -v`.
pub const KEYWORDS: &[&str] = &[
    "!", "{", "}", "case", "do", "done", "elif", "else", "esac", "fi", "for", "if", "in", "then",
    "until", "while",
];

/// A special builtin by name.
pub fn special(name: &str) -> Option<Builtin> {
    SPECIAL.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
}

/// A regular builtin by name.
pub fn regular(name: &str) -> Option<Builtin> {
    REGULAR.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
}

/// Report a builtin's error; the flow that fails it with `status`.
pub(crate) fn fail(sh: &mut Shell, m: &Msg, status: i32) -> Flow {
    sh.report(m);
    Flow::Fail(status)
}

/// Report a special builtin's error: a non-interactive shell exits.
pub(crate) fn special_fail(sh: &mut Shell, m: &Msg, status: i32) -> Flow {
    sh.report(m);
    if sh.interactive {
        Flow::Fail(status)
    } else {
        Flow::Error(status)
    }
}

/// A builtin's options and operands.
pub(crate) struct Opts {
    /// Letters in order, each with its value when it takes one.
    pub flags: Vec<(char, Option<String>)>,
    /// What follows the options.
    pub operands: Vec<String>,
}

impl Opts {
    /// Was the letter given?
    pub fn has(&self, c: char) -> bool {
        self.flags.iter().any(|(f, _)| *f == c)
    }

    /// The last value given for a letter.
    pub fn value(&self, c: char) -> Option<&str> {
        self.flags
            .iter()
            .rev()
            .find(|(f, _)| *f == c)
            .and_then(|(_, v)| v.as_deref())
    }
}

/// Parse leading options: `allowed` letters, of which `with_arg` take a
/// value. The error is the message detail.
pub(crate) fn parse_opts(args: &[String], allowed: &str, with_arg: &str) -> Result<Opts, String> {
    let mut flags = Vec::new();
    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            i += 1;
            break;
        }
        if !a.starts_with('-') || a.len() < 2 {
            break;
        }
        let chars: Vec<char> = a[1..].chars().collect();
        let mut j = 0;
        while j < chars.len() {
            let c = chars[j];
            if with_arg.contains(c) {
                let rest: String = chars[j + 1..].iter().collect();
                let v = if rest.is_empty() {
                    i += 1;
                    match args.get(i) {
                        Some(v) => v.clone(),
                        None => return Err(format!("option -{c} needs a value.")),
                    }
                } else {
                    rest
                };
                flags.push((c, Some(v)));
                break;
            }
            if !allowed.contains(c) {
                return Err(format!("option -{c} is not valid."));
            }
            flags.push((c, None));
            j += 1;
        }
        i += 1;
    }
    Ok(Opts {
        flags,
        operands: args[i.min(args.len())..].to_vec(),
    })
}

/// Parse options or fail the builtin with a usage message.
pub(crate) fn opts_or_fail(
    sh: &mut Shell,
    args: &[String],
    allowed: &str,
    with_arg: &str,
    special: bool,
) -> Result<Opts, Flow> {
    parse_opts(args, allowed, with_arg).map_err(|d| {
        let m = Msg::Usage(args[0].clone(), d);
        if special {
            special_fail(sh, &m, 2)
        } else {
            fail(sh, &m, 2)
        }
    })
}
