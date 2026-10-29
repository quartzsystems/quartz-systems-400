// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `export`, `readonly`, `unset`, `local`, `declare`/`typeset`, `set`,
//! `shift`.

use super::{fail, opts_or_fail, special_fail};
use crate::msg::Msg;
use crate::shell::options::{OPTIONS, Opt, Options};
use crate::shell::vars::quote;
use crate::shell::{Flow, Res, Shell};
use crate::syntax::is_name;

/// Split `name=value`.
fn name_value(a: &str) -> (&str, Option<&str>) {
    match a.split_once('=') {
        Some((n, v)) => (n, Some(v)),
        None => (a, None),
    }
}

/// `export` and `readonly`: mark names, optionally assigning.
fn mark(sh: &mut Shell, args: &[String], readonly: bool) -> Res {
    let o = opts_or_fail(sh, args, "p", "", true)?;
    if o.operands.is_empty() {
        let word = if readonly { "readonly" } else { "export" };
        let lines: Vec<String> = sh
            .vars
            .iter()
            .filter(|(_, v)| if readonly { v.readonly } else { v.exported })
            .map(|(k, v)| match &v.value {
                Some(val) => format!("{word} {k}={}\n", quote(val)),
                None => format!("{word} {k}\n"),
            })
            .collect();
        for l in lines {
            sh.out(&l)?;
        }
        return Ok(0);
    }
    let mut st = 0;
    for a in &o.operands {
        let (n, v) = name_value(a);
        if !is_name(n) {
            sh.report(&Msg::BadName(args[0].clone(), n.to_string()));
            st = 1;
            continue;
        }
        if let Some(v) = v
            && let Err(Flow::Error(s)) = sh.assign(n, v)
        {
            return Err(if sh.interactive {
                Flow::Fail(s)
            } else {
                Flow::Error(s)
            });
        }
        let e = sh.vars.entry(n);
        if readonly {
            e.readonly = true;
        } else {
            e.exported = true;
        }
    }
    if st != 0 && !sh.interactive {
        return Err(Flow::Error(st));
    }
    Ok(st)
}

/// `export [-p] [name[=value]...]`.
pub(crate) fn export(sh: &mut Shell, args: &[String]) -> Res {
    mark(sh, args, false)
}

/// `readonly [-p] [name[=value]...]`.
pub(crate) fn readonly(sh: &mut Shell, args: &[String]) -> Res {
    mark(sh, args, true)
}

/// `unset [-f|-v] name...`.
pub(crate) fn unset(sh: &mut Shell, args: &[String]) -> Res {
    let o = opts_or_fail(sh, args, "fv", "", true)?;
    let mut st = 0;
    for n in &o.operands {
        if o.has('f') {
            sh.funcs.remove(n);
            continue;
        }
        if !is_name(n) {
            sh.report(&Msg::BadName("unset".to_string(), n.clone()));
            st = 1;
            continue;
        }
        if sh.vars.unset(n).is_err() {
            sh.report(&Msg::ReadOnly(n.clone()));
            st = 1;
            continue;
        }
        if matches!(n.as_str(), "RANDOM" | "SECONDS" | "LINENO") {
            sh.dyn_off.insert(n.clone());
        }
        if n == "OPTIND" {
            sh.getopts_pos = 0;
        }
    }
    Ok(st)
}

/// Make a variable local to the running function (saved once per call).
fn make_local(sh: &mut Shell, name: &str) {
    let old = sh.vars.var(name).cloned();
    if let Some(frame) = sh.locals.last_mut()
        && !frame.iter().any(|(n, _)| n == name)
    {
        frame.push((name.to_string(), old));
    }
}

/// `local name[=value]...`.
pub(crate) fn local(sh: &mut Shell, args: &[String]) -> Res {
    if sh.func_depth == 0 {
        return Err(fail(
            sh,
            &Msg::Usage("local".to_string(), "not in a function.".to_string()),
            1,
        ));
    }
    let mut st = 0;
    for a in &args[1..] {
        let (n, v) = name_value(a);
        if !is_name(n) {
            sh.report(&Msg::BadName("local".to_string(), n.to_string()));
            st = 1;
            continue;
        }
        make_local(sh, n);
        if let Some(v) = v {
            sh.assign(n, v).map_err(|_| Flow::Fail(1))?;
        }
    }
    Ok(st)
}

/// `declare` / `typeset [-irxpf] [name[=value]...]`; local in a
/// function.
pub(crate) fn declare(sh: &mut Shell, args: &[String]) -> Res {
    let mut on = String::new();
    let mut off = String::new();
    let mut i = 1;
    while let Some(a) = args.get(i) {
        let (set, rest) = match a.as_bytes().first() {
            Some(b'-') if a.len() > 1 && a != "--" => (true, &a[1..]),
            Some(b'+') if a.len() > 1 => (false, &a[1..]),
            _ => break,
        };
        for c in rest.chars() {
            if !"irxpf".contains(c) {
                return Err(fail(
                    sh,
                    &Msg::Usage(args[0].clone(), format!("option {c} is not valid.")),
                    2,
                ));
            }
            if set { on.push(c) } else { off.push(c) }
        }
        i += 1;
    }
    if args.get(i).is_some_and(|a| a == "--") {
        i += 1;
    }
    let names = &args[i..];
    if on.contains('f') {
        let mut fs: Vec<_> = sh.funcs.values().cloned().collect();
        fs.sort_by(|a, b| a.name.cmp(&b.name));
        for f in fs {
            if names.is_empty() || names.contains(&f.name) {
                sh.out(&format!("{}\n", f.text))?;
            }
        }
        return Ok(0);
    }
    if names.is_empty() {
        let lines: Vec<String> = sh
            .vars
            .iter()
            .filter(|(_, v)| {
                (!on.contains('i') || v.integer)
                    && (!on.contains('r') || v.readonly)
                    && (!on.contains('x') || v.exported)
            })
            .map(|(k, v)| {
                let mut flags = String::new();
                if v.integer {
                    flags.push('i');
                }
                if v.readonly {
                    flags.push('r');
                }
                if v.exported {
                    flags.push('x');
                }
                let f = if flags.is_empty() {
                    "--".to_string()
                } else {
                    format!("-{flags}")
                };
                match &v.value {
                    Some(val) => format!("declare {f} {k}={}\n", quote(val)),
                    None => format!("declare {f} {k}\n"),
                }
            })
            .collect();
        for l in lines {
            sh.out(&l)?;
        }
        return Ok(0);
    }
    let mut st = 0;
    for a in names {
        let (n, v) = name_value(a);
        if !is_name(n) {
            sh.report(&Msg::BadName(args[0].clone(), n.to_string()));
            st = 1;
            continue;
        }
        if on.contains('p') {
            match sh.vars.var(n).cloned() {
                Some(var) => {
                    let val = var
                        .value
                        .map_or(String::new(), |v| format!("={}", quote(&v)));
                    sh.out(&format!("declare -- {n}{val}\n"))?;
                }
                None => st = 1,
            }
            continue;
        }
        if sh.func_depth > 0 {
            make_local(sh, n);
        }
        {
            let e = sh.vars.entry(n);
            if on.contains('i') {
                e.integer = true;
            }
            if off.contains('i') {
                e.integer = false;
            }
            if on.contains('x') {
                e.exported = true;
            }
            if off.contains('x') {
                e.exported = false;
            }
        }
        if let Some(v) = v
            && sh.assign(n, v).is_err()
        {
            st = 1;
            continue;
        }
        if on.contains('r') {
            sh.vars.entry(n).readonly = true;
        }
    }
    Ok(st)
}

/// `set` with no operands: every variable, quoted for re-input.
fn list_vars(sh: &mut Shell) -> Res {
    let lines: Vec<String> = sh
        .vars
        .iter()
        .filter_map(|(k, v)| v.value.as_ref().map(|val| format!("{k}={}\n", quote(val))))
        .collect();
    for l in lines {
        sh.out(&l)?;
    }
    Ok(0)
}

/// `set -o` and `set +o` listings.
fn list_options(sh: &mut Shell, plus: bool) -> Res {
    let mut text = String::new();
    for (o, name, _) in OPTIONS {
        let on = sh.opts.get(*o);
        if plus {
            text.push_str(&format!("set {}o {name}\n", if on { '-' } else { '+' }));
        } else {
            text.push_str(&format!("{name:<16}{}\n", if on { "on" } else { "off" }));
        }
    }
    sh.out(&text)?;
    Ok(0)
}

/// `set [-+abCefmnuvx] [-+o name] [--] [arg...]`.
pub(crate) fn set(sh: &mut Shell, args: &[String]) -> Res {
    if args.len() == 1 {
        return list_vars(sh);
    }
    let mut i = 1;
    let mut new_args = None;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            new_args = Some(args[i + 1..].to_vec());
            break;
        }
        if a == "-" {
            sh.opts.set(Opt::XTrace, false);
            sh.opts.set(Opt::Verbose, false);
            new_args = Some(args[i + 1..].to_vec());
            break;
        }
        let on = match a.as_bytes().first() {
            Some(b'-') => true,
            Some(b'+') => false,
            _ => {
                new_args = Some(args[i..].to_vec());
                break;
            }
        };
        for c in a[1..].chars() {
            if c == 'o' {
                match args.get(i + 1) {
                    None => return list_options(sh, !on),
                    Some(name) => {
                        let Some(o) = Options::by_name(name) else {
                            return Err(special_fail(
                                sh,
                                &Msg::Usage(
                                    "set".to_string(),
                                    format!("option {name} is not valid."),
                                ),
                                2,
                            ));
                        };
                        sh.opts.set(o, on);
                        i += 1;
                    }
                }
                continue;
            }
            match Options::by_letter(c) {
                Some(Opt::Interactive | Opt::Stdin) | None => {
                    return Err(special_fail(
                        sh,
                        &Msg::Usage("set".to_string(), format!("option {c} is not valid.")),
                        2,
                    ));
                }
                Some(o) => sh.opts.set(o, on),
            }
        }
        i += 1;
    }
    if let Some(a) = new_args {
        sh.args = a;
        sh.getopts_pos = 0;
        let _ = sh.vars.set("OPTIND", "1");
    }
    Ok(0)
}

/// `shift [n]`.
pub(crate) fn shift(sh: &mut Shell, args: &[String]) -> Res {
    let n = match args.get(1) {
        None => 1,
        Some(a) => match a.parse::<usize>() {
            Ok(n) => n,
            Err(_) => {
                return Err(special_fail(
                    sh,
                    &Msg::NotNumber("shift".to_string(), a.clone()),
                    1,
                ));
            }
        },
    };
    if n > sh.args.len() {
        return Err(special_fail(
            sh,
            &Msg::Usage(
                "shift".to_string(),
                format!("cannot shift {n} positional parameters."),
            ),
            1,
        ));
    }
    sh.args.drain(..n);
    Ok(0)
}
