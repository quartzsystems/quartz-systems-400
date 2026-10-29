// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `alias`, `unalias`, `hash`, `type`, `getopts`, `let`.

use super::run::describe;
use super::{fail, opts_or_fail};
use crate::msg::Msg;
use crate::shell::vars::quote;
use crate::shell::{Res, Shell};

/// `alias [name[=value]...]`.
pub(crate) fn alias(sh: &mut Shell, args: &[String]) -> Res {
    if args.len() == 1 {
        let lines: Vec<String> = sh
            .aliases
            .iter()
            .map(|(k, v)| format!("{k}={}\n", quote(v)))
            .collect();
        for l in lines {
            sh.out(&l)?;
        }
        return Ok(0);
    }
    let mut st = 0;
    for a in &args[1..] {
        match a.split_once('=') {
            Some((n, v)) if !n.is_empty() => {
                sh.aliases.insert(n.to_string(), v.to_string());
            }
            _ => match sh.aliases.get(a).cloned() {
                Some(v) => sh.out(&format!("{a}={}\n", quote(&v)))?,
                None => {
                    sh.report(&Msg::Usage(
                        "alias".to_string(),
                        format!("{a} is not an alias."),
                    ));
                    st = 1;
                }
            },
        }
    }
    Ok(st)
}

/// `unalias -a | name...`.
pub(crate) fn unalias(sh: &mut Shell, args: &[String]) -> Res {
    let o = opts_or_fail(sh, args, "a", "", false)?;
    if o.has('a') {
        sh.aliases.clear();
        return Ok(0);
    }
    let mut st = 0;
    for n in &o.operands {
        if sh.aliases.remove(n).is_none() {
            sh.report(&Msg::Usage(
                "unalias".to_string(),
                format!("{n} is not an alias."),
            ));
            st = 1;
        }
    }
    Ok(st)
}

/// `hash [-r] [name...]`.
pub(crate) fn hash(sh: &mut Shell, args: &[String]) -> Res {
    let o = opts_or_fail(sh, args, "r", "", false)?;
    if o.has('r') {
        sh.hash.clear();
    }
    if o.operands.is_empty() {
        if !o.has('r') {
            let lines: Vec<String> = sh.hash.values().map(|p| format!("{p}\n")).collect();
            for l in lines {
                sh.out(&l)?;
            }
        }
        return Ok(0);
    }
    let mut st = 0;
    for n in &o.operands {
        if n.contains('/')
            || super::special(n).is_some()
            || super::regular(n).is_some()
            || sh.funcs.contains_key(n)
        {
            continue;
        }
        sh.hash.remove(n);
        if sh.search_path(n).is_none() {
            sh.report(&Msg::NotFound(n.clone()));
            st = 1;
        }
    }
    Ok(st)
}

/// `type name...`.
pub(crate) fn type_(sh: &mut Shell, args: &[String]) -> Res {
    let mut st = 0;
    for n in &args[1..] {
        match describe(sh, n, true) {
            Some(d) => sh.out(&format!("{d}\n"))?,
            None => {
                sh.report(&Msg::NotFound(n.clone()));
                st = 1;
            }
        }
    }
    Ok(st)
}

/// `getopts optstring name [arg...]`.
pub(crate) fn getopts(sh: &mut Shell, args: &[String]) -> Res {
    if args.len() < 3 {
        return Err(fail(
            sh,
            &Msg::Usage(
                "getopts".to_string(),
                "an option string and a name are required.".to_string(),
            ),
            2,
        ));
    }
    let optstring = args[1].clone();
    let name = args[2].clone();
    if !crate::syntax::is_name(&name) {
        return Err(fail(sh, &Msg::BadName("getopts".to_string(), name), 2));
    }
    let list: Vec<String> = if args.len() > 3 {
        args[3..].to_vec()
    } else {
        sh.args.clone()
    };
    let silent = optstring.starts_with(':');
    let mut optind: usize = sh
        .vars
        .get("OPTIND")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .max(1);
    let mut pos = sh.getopts_pos;
    let set = |sh: &mut Shell, k: &str, v: &str| {
        let _ = sh.vars.set(k, v);
    };
    let done = |sh: &mut Shell, optind: usize, name: &str| {
        let _ = sh.vars.set(name, "?");
        let _ = sh.vars.unset("OPTARG");
        let _ = sh.vars.set("OPTIND", &optind.to_string());
        sh.getopts_pos = 0;
        Ok(1)
    };
    let Some(arg) = list.get(optind - 1) else {
        return done(sh, optind, &name);
    };
    let chars: Vec<char> = arg.chars().collect();
    if pos == 0 {
        if arg == "--" {
            return done(sh, optind + 1, &name);
        }
        if chars.len() < 2 || chars[0] != '-' {
            return done(sh, optind, &name);
        }
        pos = 1;
    }
    let c = chars.get(pos).copied().unwrap_or('?');
    pos += 1;
    let at_end = pos >= chars.len();
    let spec = optstring.trim_start_matches(':');
    let found = (c != ':').then(|| spec.find(c)).flatten();
    let mut advance = at_end;
    match found {
        None => {
            if silent {
                set(sh, "OPTARG", &c.to_string());
            } else {
                sh.report(&Msg::Usage(
                    "getopts".to_string(),
                    format!("option -{c} is not valid."),
                ));
                let _ = sh.vars.unset("OPTARG");
            }
            set(sh, &name, "?");
        }
        Some(i) if spec[i + c.len_utf8()..].starts_with(':') => {
            if !at_end {
                let rest: String = chars[pos..].iter().collect();
                set(sh, "OPTARG", &rest);
                set(sh, &name, &c.to_string());
                advance = true;
            } else if let Some(v) = list.get(optind) {
                set(sh, "OPTARG", v);
                set(sh, &name, &c.to_string());
                optind += 1;
                advance = true;
            } else if silent {
                set(sh, "OPTARG", &c.to_string());
                set(sh, &name, ":");
                advance = true;
            } else {
                sh.report(&Msg::Usage(
                    "getopts".to_string(),
                    format!("option -{c} needs a value."),
                ));
                let _ = sh.vars.unset("OPTARG");
                set(sh, &name, "?");
                advance = true;
            }
        }
        Some(_) => {
            let _ = sh.vars.unset("OPTARG");
            set(sh, &name, &c.to_string());
        }
    }
    if advance {
        optind += 1;
        pos = 0;
    }
    set(sh, "OPTIND", &optind.to_string());
    sh.getopts_pos = pos;
    Ok(0)
}

/// `let expr...`.
pub(crate) fn let_(sh: &mut Shell, args: &[String]) -> Res {
    if args.len() < 2 {
        return Err(fail(
            sh,
            &Msg::Usage("let".to_string(), "an expression is required.".to_string()),
            1,
        ));
    }
    let mut last = 0;
    for a in &args[1..] {
        match crate::arith::eval(a, &mut crate::shell::exec::ArithVars(sh)) {
            Ok(v) => last = v,
            Err(e) => return Err(fail(sh, &Msg::Arith(e.to_string()), 2)),
        }
    }
    Ok(i32::from(last == 0))
}
