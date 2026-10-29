// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `eval`, `.` (`source`), `exec`, `command`, `builtin`.

use super::{KEYWORDS, fail, opts_or_fail, special_fail};
use crate::msg::Msg;
use crate::shell::exec::Found;
use crate::shell::vars::quote;
use crate::shell::{Flow, MAX_NEST, Res, Shell};

/// `eval [arg...]`.
pub(crate) fn eval(sh: &mut Shell, args: &[String]) -> Res {
    let text = args[1..].join(" ");
    if text.trim().is_empty() {
        return Ok(0);
    }
    if sh.eval_depth >= MAX_NEST {
        return Err(special_fail(
            sh,
            &Msg::Failed("eval".to_string(), "nesting is too deep.".to_string()),
            2,
        ));
    }
    sh.eval_depth += 1;
    let line = sh.lineno;
    let r = sh.run_text(&text, line);
    sh.eval_depth -= 1;
    r
}

/// `. file [arg...]`.
pub(crate) fn dot(sh: &mut Shell, args: &[String]) -> Res {
    let Some(name) = args.get(1) else {
        return Err(special_fail(
            sh,
            &Msg::Usage(args[0].clone(), "a file name is required.".to_string()),
            2,
        ));
    };
    let path = if name.contains('/') {
        Some(name.clone())
    } else {
        let p = sh.var("PATH").unwrap_or_default();
        p.split(':')
            .map(|d| crate::pattern::path::join(if d.is_empty() { "." } else { d }, name))
            .find(|c| {
                let abs = crate::pattern::path::absolute(&sh.cwd, c);
                matches!(sh.sys.stat(&abs), Ok(st) if st.ftype == crate::sys::FileType::File)
            })
    };
    let text = match path.as_deref().map(|p| sh.read_file(p)) {
        Some(Ok(t)) => t,
        Some(Err(e)) => return Err(special_fail(sh, &Msg::Open(name.clone(), e.to_string()), 1)),
        None => return Err(special_fail(sh, &Msg::NotFound(name.clone()), 1)),
    };
    if sh.dot_depth >= MAX_NEST {
        return Err(special_fail(
            sh,
            &Msg::Failed(args[0].clone(), "nesting is too deep.".to_string()),
            2,
        ));
    }
    let saved = (args.len() > 2).then(|| std::mem::replace(&mut sh.args, args[2..].to_vec()));
    sh.dot_depth += 1;
    let r = sh.run_text(&text, 1);
    sh.dot_depth -= 1;
    if let Some(a) = saved {
        sh.args = a;
    }
    match r {
        Err(Flow::Return(n)) => Ok(n),
        r => r,
    }
}

/// `exec [command [arg...]]`: redirections alone stay in effect;
/// with a command, the shell runs it and exits with its status.
pub(crate) fn exec(sh: &mut Shell, args: &[String]) -> Res {
    if args.len() == 1 {
        return Ok(0);
    }
    let argv = &args[1..];
    let found = sh.lookup(&argv[0], true);
    let st = match sh.run_found(found, argv, &[]) {
        Ok(n) | Err(Flow::Fail(n) | Flow::Error(n) | Flow::Return(n)) => n,
        Err(Flow::Exit(n)) => return Err(Flow::Exit(n)),
        Err(Flow::Break(_) | Flow::Continue(_)) => sh.status,
    };
    Err(Flow::Exit(st))
}

/// The `-v` or `-V` description of a name, or `None` when not found.
pub(crate) fn describe(sh: &mut Shell, name: &str, verbose: bool) -> Option<String> {
    if let Some(v) = sh.aliases.get(name) {
        return Some(if verbose {
            format!("{name} is an alias for {v}")
        } else {
            format!("alias {name}={}", quote(v))
        });
    }
    if KEYWORDS.contains(&name) {
        return Some(if verbose {
            format!("{name} is a shell keyword")
        } else {
            name.to_string()
        });
    }
    let found = sh.lookup(name, false);
    match found {
        Found::Special(_) => Some(if verbose {
            format!("{name} is a special shell builtin")
        } else {
            name.to_string()
        }),
        Found::Function(_) => Some(if verbose {
            format!("{name} is a shell function")
        } else {
            name.to_string()
        }),
        Found::Builtin(_) => Some(if verbose {
            format!("{name} is a shell builtin")
        } else {
            name.to_string()
        }),
        Found::External(p) => {
            if name.contains('/') {
                let abs = crate::pattern::path::absolute(&sh.cwd, &p);
                sh.sys.stat(&abs).ok()?;
            }
            Some(if verbose { format!("{name} is {p}") } else { p })
        }
        Found::NotFound => None,
    }
}

/// `command [-p] [-v|-V] name [arg...]`.
pub(crate) fn command(sh: &mut Shell, args: &[String]) -> Res {
    let o = opts_or_fail(sh, args, "pvV", "", false)?;
    if o.operands.is_empty() {
        return Ok(0);
    }
    if o.has('v') || o.has('V') {
        let mut st = 0;
        for name in &o.operands {
            match describe(sh, name, o.has('V')) {
                Some(d) => sh.out(&format!("{d}\n"))?,
                None => {
                    if o.has('V') {
                        sh.report(&Msg::NotFound(name.clone()));
                    }
                    st = 1;
                }
            }
        }
        return Ok(st);
    }
    let saved_path = if o.has('p') {
        let old = sh.vars.get("PATH").map(str::to_string);
        let _ = sh.vars.set("PATH", crate::shell::DEFAULT_PATH);
        Some(old)
    } else {
        None
    };
    let argv = o.operands.clone();
    let found = sh.lookup(&argv[0], true);
    let r = sh.run_found(found, &argv, &[]);
    if let Some(old) = saved_path {
        match old {
            Some(v) => {
                let _ = sh.vars.set("PATH", &v);
            }
            None => {
                let _ = sh.vars.unset("PATH");
            }
        }
    }
    match r {
        Err(Flow::Error(n)) => Ok(n),
        r => r,
    }
}

/// `builtin name [arg...]`.
pub(crate) fn builtin(sh: &mut Shell, args: &[String]) -> Res {
    let Some(name) = args.get(1) else {
        return Ok(0);
    };
    match super::special(name).or_else(|| super::regular(name)) {
        Some(b) => sh.call_builtin(b, &args[1..]),
        None => Err(fail(
            sh,
            &Msg::Usage("builtin".to_string(), format!("{name} is not a builtin.")),
            1,
        )),
    }
}
