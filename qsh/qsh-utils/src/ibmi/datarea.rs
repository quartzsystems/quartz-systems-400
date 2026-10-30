// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `datarea [-r | -w] [-s start[:length]] name [value ...]`: read a data
//! area (the default) or write one. The name is `*LDA`, `*GDA`, `*PDA`
//! or a path to a `*DTAARA` object. `-s` picks a substring of a
//! character data area, positions counted from 1; a value written
//! without it replaces the whole area, padded with blanks. Several
//! value operands are joined by blanks.

use qsh::sys::{DtaaraType, Errno, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::{complain, usage};

const NAME: &str = "datarea";

/// `start[:length]` as (offset from 0, length if given).
pub fn substring(s: &str) -> Option<(usize, Option<usize>)> {
    let (a, b) = match s.split_once(':') {
        Some((a, b)) => (a, Some(b)),
        None => (s, None),
    };
    let start: usize = a.parse().ok().filter(|n| *n > 0)?;
    let len = match b {
        Some(b) => Some(b.parse().ok().filter(|n: &usize| *n > 0)?),
        None => None,
    };
    Some((start - 1, len))
}

fn target(cx: &UtilCtx<'_>, name: &str) -> String {
    if name.starts_with('*') {
        name.to_ascii_uppercase()
    } else {
        cx.path(name)
    }
}

fn why(e: Errno) -> &'static str {
    match e {
        Errno::ENOENT => "The data area is not found.",
        Errno::EINVAL => "The value is not valid for the data area.",
        _ => e.text(),
    }
}

/// Run `datarea`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "rws:") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    if p.has('r') && p.has('w') {
        return usage(cx, NAME, "Use -r or -w, not both.");
    }
    let Some((name, values)) = p.operands.split_first() else {
        return usage(cx, NAME, "A data area is needed.");
    };
    let part = match p.value('s') {
        Some(s) => match substring(s) {
            Some(x) => Some(x),
            None => return usage(cx, NAME, &format!("The substring {s} is not valid.")),
        },
        None => None,
    };
    let path = target(cx, name);
    if p.has('w') {
        if values.is_empty() {
            return usage(cx, NAME, "A value to write is needed.");
        }
        let value = values.join(" ");
        let part = part.map(|(off, len)| (off, len.unwrap_or_else(|| value.chars().count())));
        if let Err(e) = cx.sys.write_data_area(&path, part, &value) {
            complain(cx, NAME, &format!("{name}: {}", why(e.errno)));
            return 1;
        }
        return 0;
    }
    if !values.is_empty() {
        return usage(cx, NAME, "A value is taken only with -w.");
    }
    let area = match cx.sys.read_data_area(&path) {
        Ok(a) => a,
        Err(e) => {
            complain(cx, NAME, &format!("{name}: {}", why(e.errno)));
            return 1;
        }
    };
    let text = match (part, area.kind) {
        (None, _) => area.value,
        (Some((off, len)), DtaaraType::Char) => {
            let chars: Vec<char> = area.value.chars().collect();
            let end = len.map_or(chars.len(), |l| off + l);
            if off >= chars.len() || end > chars.len() {
                complain(
                    cx,
                    NAME,
                    &format!("{name}: The substring is past the end of the data area."),
                );
                return 1;
            }
            chars[off..end].iter().collect()
        }
        (Some(_), _) => {
            complain(
                cx,
                NAME,
                &format!("{name}: A substring is taken only for a character data area."),
            );
            return 1;
        }
    };
    let mut out = Out::stdout(cx);
    out.text(&text);
    out.text("\n");
    out.finish(cx, NAME, 0)
}
