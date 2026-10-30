// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `catsplf [-ep] [-j job] name [number]` and `catsplf -a [-ep] [-j
//! job]`: write spooled files to standard output. The job is
//! `number/user/name` (the shell's own by default); the number is a
//! spooled file number, `*LAST` or `*ONLY` (the default). `-a` writes
//! every spooled file of the job in turn. `-e` puts a form feed between
//! pages; `-p` starts each line with its ASA control character (`1` on
//! a page's first line, blank otherwise).

use qsh::sys::{Errno, SplfNumber, Spooled, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::{complain, usage};

const NAME: &str = "catsplf";

/// A spooled file number operand.
pub fn number(s: &str) -> Option<SplfNumber> {
    match s.to_ascii_uppercase().as_str() {
        "*LAST" => Some(SplfNumber::Last),
        "*ONLY" => Some(SplfNumber::Only),
        n => n
            .parse::<u32>()
            .ok()
            .filter(|n| *n > 0)
            .map(SplfNumber::Number),
    }
}

fn write(out: &mut Out, f: &Spooled, eject: bool, asa: bool) {
    for (i, page) in f.pages.iter().enumerate() {
        if eject && i > 0 {
            out.text("\x0c");
        }
        for (j, line) in page.iter().enumerate() {
            if asa {
                out.text(if j == 0 { "1" } else { " " });
            }
            out.text(line);
            out.text("\n");
        }
    }
}

fn why(e: Errno, job: &str) -> String {
    match e {
        Errno::ESRCH => format!("The job {job} is not found."),
        Errno::ENOENT => "The spooled file is not found.".to_string(),
        Errno::EINVAL => "More than one spooled file has the name; give its number.".to_string(),
        _ => e.text().to_string(),
    }
}

/// Run `catsplf`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "aepj:") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    let job = p.value('j').unwrap_or("*").to_ascii_uppercase();
    if job != "*" && job.split('/').count() != 3 {
        return usage(cx, NAME, &format!("The job {job} is not number/user/name."));
    }
    let (eject, asa) = (p.has('e'), p.has('p'));
    let mut out = Out::stdout(cx);
    let files: Vec<(String, SplfNumber)> = if p.has('a') {
        if !p.operands.is_empty() {
            return usage(cx, NAME, "No spooled file is named with -a.");
        }
        match cx.sys.spooled_files(&job) {
            Ok(v) => v
                .into_iter()
                .map(|i| (i.name, SplfNumber::Number(i.number)))
                .collect(),
            Err(e) => {
                complain(cx, NAME, &why(e.errno, &job));
                return 1;
            }
        }
    } else {
        match p.operands.as_slice() {
            [n] => vec![(n.to_ascii_uppercase(), SplfNumber::Only)],
            [n, num] => match number(num) {
                Some(k) => vec![(n.to_ascii_uppercase(), k)],
                None => {
                    return usage(cx, NAME, &format!("The spooled file number {num} is not valid."));
                }
            },
            _ => return usage(cx, NAME, "A spooled file name and number are taken."),
        }
    };
    let mut status = 0;
    for (name, num) in files {
        match cx.sys.spooled_file(&job, &name, num) {
            Ok(f) => write(&mut out, &f, eject, asa),
            Err(e) => {
                out.flush();
                complain(cx, NAME, &format!("{name}: {}", why(e.errno, &job)));
                status = 1;
            }
        }
    }
    out.finish(cx, NAME, status)
}
