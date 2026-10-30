// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `locale [-a | -m]`, `locale [-ck] name...`: the locale settings.
//!
//! Without operands: `LANG=value`, each category as `LC_x=value` (in
//! double quotes when implied by `LC_ALL` or `LANG` rather than set),
//! then `LC_ALL=value`. `-a` lists `C`, `POSIX` and the `*LOCALE`
//! objects in `QSYS` (as `/QSYS.LIB/NAME.LOCALE`, the form `LANG`
//! takes); `-m` the coded character sets. A name is a category (all its
//! keywords) or a keyword; `-c` writes the category's name first, `-k`
//! writes `keyword=value` (strings quoted). The keyword values are the
//! POSIX locale's.

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::{complain, usage};

const NAME: &str = "locale";

/// The categories in output order.
pub const CATEGORIES: &[&str] = &[
    "LC_CTYPE",
    "LC_COLLATE",
    "LC_MONETARY",
    "LC_NUMERIC",
    "LC_TIME",
    "LC_MESSAGES",
];

/// A keyword's value.
#[derive(Clone, Copy)]
enum V {
    S(&'static str),
    N(i32),
}

const KEYWORDS: &[(&str, &str, V)] = &[
    ("LC_CTYPE", "charmap", V::S("ANSI_X3.4-1968")),
    ("LC_NUMERIC", "decimal_point", V::S(".")),
    ("LC_NUMERIC", "thousands_sep", V::S("")),
    ("LC_NUMERIC", "grouping", V::N(-1)),
    ("LC_MONETARY", "int_curr_symbol", V::S("")),
    ("LC_MONETARY", "currency_symbol", V::S("")),
    ("LC_MONETARY", "mon_decimal_point", V::S("")),
    ("LC_MONETARY", "mon_thousands_sep", V::S("")),
    ("LC_MONETARY", "mon_grouping", V::N(-1)),
    ("LC_MONETARY", "positive_sign", V::S("")),
    ("LC_MONETARY", "negative_sign", V::S("")),
    ("LC_MONETARY", "int_frac_digits", V::N(-1)),
    ("LC_MONETARY", "frac_digits", V::N(-1)),
    ("LC_MONETARY", "p_cs_precedes", V::N(-1)),
    ("LC_MONETARY", "p_sep_by_space", V::N(-1)),
    ("LC_MONETARY", "n_cs_precedes", V::N(-1)),
    ("LC_MONETARY", "n_sep_by_space", V::N(-1)),
    ("LC_MONETARY", "p_sign_posn", V::N(-1)),
    ("LC_MONETARY", "n_sign_posn", V::N(-1)),
    ("LC_TIME", "abday", V::S("Sun;Mon;Tue;Wed;Thu;Fri;Sat")),
    (
        "LC_TIME",
        "day",
        V::S("Sunday;Monday;Tuesday;Wednesday;Thursday;Friday;Saturday"),
    ),
    (
        "LC_TIME",
        "abmon",
        V::S("Jan;Feb;Mar;Apr;May;Jun;Jul;Aug;Sep;Oct;Nov;Dec"),
    ),
    (
        "LC_TIME",
        "mon",
        V::S("January;February;March;April;May;June;July;August;September;October;November;December"),
    ),
    ("LC_TIME", "d_t_fmt", V::S("%a %b %e %H:%M:%S %Y")),
    ("LC_TIME", "d_fmt", V::S("%m/%d/%y")),
    ("LC_TIME", "t_fmt", V::S("%H:%M:%S")),
    ("LC_TIME", "am_pm", V::S("AM;PM")),
    ("LC_TIME", "t_fmt_ampm", V::S("%I:%M:%S %p")),
    ("LC_MESSAGES", "yesexpr", V::S("^[yY]")),
    ("LC_MESSAGES", "noexpr", V::S("^[nN]")),
];

fn value_text(v: V, keyword: Option<&str>) -> String {
    match (v, keyword) {
        (V::S(s), Some(k)) => format!("{k}=\"{s}\""),
        (V::N(n), Some(k)) => format!("{k}={n}"),
        (V::S(s), None) => s.to_string(),
        (V::N(n), None) => n.to_string(),
    }
}

fn env<'c>(cx: &'c UtilCtx<'_>, name: &str) -> Option<&'c str> {
    cx.getenv(name).filter(|v| !v.is_empty())
}

fn settings(cx: &UtilCtx<'_>, out: &mut Out) {
    let lang = env(cx, "LANG");
    let all = env(cx, "LC_ALL");
    out.text(&format!("LANG={}\n", lang.unwrap_or("")));
    for c in CATEGORIES {
        let line = match (all, env(cx, c)) {
            (Some(a), _) => format!("{c}=\"{a}\""),
            (None, Some(v)) => format!("{c}={v}"),
            (None, None) => format!("{c}=\"{}\"", lang.unwrap_or("C")),
        };
        out.text(&line);
        out.text("\n");
    }
    out.text(&format!("LC_ALL={}\n", all.unwrap_or("")));
}

fn locales(cx: &UtilCtx<'_>, out: &mut Out) {
    let mut names = vec!["C".to_string(), "POSIX".to_string()];
    let mut objs = Vec::new();
    for dir in ["/QSYS.LIB", "/QSYS.LIB/QSYS.LIB"] {
        for n in cx.sys.read_dir(dir).unwrap_or_default() {
            let up = n.to_ascii_uppercase();
            if up.ends_with(".LOCALE") {
                let p = format!("/QSYS.LIB/{up}");
                if !objs.contains(&p) {
                    objs.push(p);
                }
            }
        }
    }
    objs.sort();
    names.extend(objs);
    for n in names {
        out.text(&n);
        out.text("\n");
    }
}

/// Run `locale`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "amck") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    let list = p.has('a') || p.has('m');
    if (p.has('a') && p.has('m')) || (list && (p.has('c') || p.has('k'))) {
        return usage(cx, NAME, "Options -a and -m stand alone.");
    }
    if list && !p.operands.is_empty() {
        return usage(cx, NAME, "Options -a and -m take no operands.");
    }
    let mut out = Out::stdout(cx);
    if p.has('a') {
        locales(cx, &mut out);
        return out.finish(cx, NAME, 0);
    }
    if p.has('m') {
        for c in ccsid::conv::supported() {
            out.text(&format!("IBMCCSID{c:05}\n"));
        }
        return out.finish(cx, NAME, 0);
    }
    if p.operands.is_empty() {
        settings(cx, &mut out);
        return out.finish(cx, NAME, 0);
    }
    let mut status = 0;
    let kw = p.has('k');
    for name in &p.operands {
        if CATEGORIES.contains(&name.as_str()) {
            if p.has('c') {
                out.text(&format!("{name}\n"));
            }
            for (_, k, v) in KEYWORDS.iter().filter(|(c, _, _)| c == name) {
                out.text(&value_text(*v, kw.then_some(*k)));
                out.text("\n");
            }
            continue;
        }
        match KEYWORDS.iter().find(|(_, k, _)| k == name) {
            Some((c, k, v)) => {
                if p.has('c') {
                    out.text(&format!("{c}\n"));
                }
                out.text(&value_text(*v, kw.then_some(*k)));
                out.text("\n");
            }
            None => {
                out.flush();
                complain(cx, NAME, &format!("{name} is not a category or keyword."));
                status = 1;
            }
        }
    }
    out.finish(cx, NAME, status)
}
