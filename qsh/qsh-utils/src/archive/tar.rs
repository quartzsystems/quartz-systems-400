// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `tar`: create (`c`), append (`r`), update (`u`), list (`t`) and
//! extract (`x`) ustar archives. Key letters come with or without a
//! leading `-`, in one word or several; `b` and `f` (and `C`) take the
//! next arguments in order.

use qsh::sys::UtilCtx;

use super::arcio::{read_all, write_out};
use super::build::Build;
use super::codec::{self, Format};
use super::gather::Gather;
use super::listing::Lister;
use super::member::{Kind, Member};
use super::restore::{Done, Restore, Restorer};
use super::select::Select;
use crate::common::io::{Out, confirm};
use crate::common::{complain, complain_sys, usage};

const NAME: &str = "tar";

#[derive(Default)]
struct Keys {
    func: Option<char>,
    block: Option<usize>,
    file: Option<String>,
    dir: Option<String>,
    follow: bool,
    links: bool,
    no_mtime: bool,
    old: bool,
    preserve: bool,
    verbose: bool,
    confirm: bool,
}

fn parse(args: &[String]) -> Result<(Keys, Vec<String>), String> {
    let mut k = Keys::default();
    let mut i = 0;
    let mut pending: Vec<char> = Vec::new();
    let mut first = true;
    let mut rest: Vec<String> = Vec::new();
    while i < args.len() {
        let a = &args[i];
        i += 1;
        let cluster = if first {
            Some(a.strip_prefix('-').unwrap_or(a))
        } else if a.len() > 1 && a.starts_with('-') && a != "--" {
            Some(&a[1..])
        } else if a == "--" {
            rest.extend(args[i..].iter().cloned());
            break;
        } else {
            None
        };
        first = false;
        let Some(cluster) = cluster else {
            if let Some(c) = pending.first().copied() {
                pending.remove(0);
                match c {
                    'b' => {
                        let n: usize = a.parse().map_err(|_| format!("The blocking factor {a} is not valid."))?;
                        if n == 0 || n > 20 {
                            return Err(format!("The blocking factor {a} is not valid."));
                        }
                        k.block = Some(n * 512);
                    }
                    'f' => k.file = Some(a.clone()),
                    _ => k.dir = Some(a.clone()),
                }
            } else {
                rest.push(a.clone());
                rest.extend(args[i..].iter().cloned());
                break;
            }
            continue;
        };
        for c in cluster.chars() {
            match c {
                'c' | 'r' | 't' | 'u' | 'x' => {
                    if k.func.is_some_and(|f| f != c) {
                        return Err("Only one of c, r, t, u and x may be given.".to_string());
                    }
                    k.func = Some(c);
                }
                'b' | 'f' | 'C' => pending.push(c),
                'h' | 'L' => k.follow = true,
                'l' => k.links = true,
                'm' => k.no_mtime = true,
                'o' => k.old = true,
                'p' => k.preserve = true,
                'v' => k.verbose = true,
                'w' => k.confirm = true,
                _ => return Err(format!("Key letter {c} is not valid.")),
            }
        }
    }
    if let Some(c) = pending.first() {
        return Err(format!("Key letter {c} needs an argument."));
    }
    if k.func.is_none() {
        return Err("One of c, r, t, u or x is needed.".to_string());
    }
    Ok((k, rest))
}

/// Run `tar`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let (k, ops) = match parse(&cx.argv[1..]) {
        Ok(x) => x,
        Err(t) => return usage(cx, NAME, &t),
    };
    let file = k
        .file
        .clone()
        .or_else(|| cx.getenv("TAPE").map(ToString::to_string))
        .map(|f| if f == "-" { f } else { cx.path(&f) });
    if let Some(d) = &k.dir {
        let abs = cx.path(d);
        if let Err(e) = cx.sys.chdir(&abs) {
            complain_sys(cx, NAME, d, e);
            return 1;
        }
        cx.cwd = abs;
    }
    match k.func {
        Some('c') => create(cx, &k, file.as_deref(), &ops, false),
        Some('r' | 'u') => create(cx, &k, file.as_deref(), &ops, true),
        Some('t') => list_or_extract(cx, &k, file.as_deref(), &ops, false),
        _ => list_or_extract(cx, &k, file.as_deref(), &ops, true),
    }
}

fn create(cx: &UtilCtx<'_>, k: &Keys, file: Option<&str>, ops: &[String], append: bool) -> i32 {
    if ops.is_empty() {
        return usage(cx, NAME, "No files were named.");
    }
    let mut old: Vec<Member> = Vec::new();
    let mut format = if k.old { Format::Tar } else { Format::Ustar };
    if append {
        let Some(f) = file.filter(|f| *f != "-") else {
            return usage(cx, NAME, "Appending needs an archive file named with f.");
        };
        match read_all(cx, Some(f)).and_then(|d| codec::read(&d)) {
            Ok((fm, ms)) if matches!(fm, Format::Ustar | Format::Tar) => {
                format = fm;
                old = ms;
            }
            Ok(_) => {
                complain(cx, NAME, &format!("{f}: The archive is not a tar archive."));
                return 1;
            }
            // A missing archive starts empty.
            Err(_) if cx.sys.stat(f).is_err() => {}
            Err(t) => {
                complain(cx, NAME, &t);
                return 1;
            }
        }
    }
    let build = Build {
        util: NAME,
        format,
        gather: Gather {
            follow_root: k.follow,
            follow_all: k.follow,
            ..Gather::default()
        },
        subs: &[],
        verbose: k.verbose,
        skip: file
            .filter(|f| *f != "-")
            .and_then(|f| cx.sys.realpath(f).ok()),
    };
    let mut bytes = Vec::new();
    let quiet = Build {
        util: NAME,
        format,
        gather: build.gather,
        subs: &[],
        verbose: false,
        skip: None,
    };
    for m in &old {
        quiet.put(cx, m, &mut bytes);
    }
    let update = k.func == Some('u');
    let mut counts: Vec<((u64, u64), u32, u32, String)> = Vec::new();
    let mut status = build.add(cx, ops, &mut bytes, &mut |found, name| {
        if update && old.iter().any(|m| m.name == name && m.mtime >= found.st.mtime) {
            return false;
        }
        if k.confirm && !confirm(cx, &format!("add {name}? ")) {
            return false;
        }
        if found.st.nlink > 1 && found.st.ftype == qsh::sys::FileType::File {
            let key = (found.st.dev, found.st.ino);
            match counts.iter_mut().find(|c| c.0 == key) {
                Some(c) => c.2 += 1,
                None => counts.push((key, found.st.nlink, 1, name.to_string())),
            }
        }
        true
    });
    if k.links {
        for (_, want, got, name) in &counts {
            if got < want {
                complain(cx, NAME, &format!("{name}: Not every link to the file was archived."));
            }
        }
    }
    let bytes = codec::finish(bytes, format, k.block.unwrap_or(10240));
    if let Err(t) = write_out(cx, file, &bytes) {
        complain(cx, NAME, &t);
        status = 1;
    }
    status
}

fn list_or_extract(cx: &UtilCtx<'_>, k: &Keys, file: Option<&str>, ops: &[String], extract: bool) -> i32 {
    let members = match read_all(cx, file).and_then(|d| codec::read(&d)) {
        Ok((_, m)) => m,
        Err(t) => {
            complain(cx, NAME, &t);
            return 1;
        }
    };
    let mut sel = Select::new(ops);
    sel.literal = true;
    let lister = Lister::new(cx);
    let mut out = Out::stdout(cx);
    let mut rest = Restorer::new(Restore {
        mode: k.preserve,
        mtime: !k.no_mtime,
        ..Restore::default()
    });
    let mut status = 0;
    for m in &members {
        if !sel.take(&m.name) {
            continue;
        }
        if !extract {
            let line = if k.verbose {
                lister.line(m, &m.name)
            } else {
                m.name.clone()
            };
            out.text(&line);
            out.text("\n");
            continue;
        }
        if k.confirm && !confirm(cx, &format!("extract {}? ", m.name)) {
            continue;
        }
        let link = if m.kind == Kind::Hardlink {
            cx.path(&m.link)
        } else {
            String::new()
        };
        match rest.put(cx, m, &cx.path(&m.name), &link) {
            Ok(Done::Made) if k.verbose => {
                let _ = cx.err(&format!("{}\n", m.name));
            }
            Ok(_) => {}
            Err(e) => {
                complain_sys(cx, NAME, &m.name, e);
                status = 1;
            }
        }
    }
    for (p, e) in rest.finish(cx) {
        complain_sys(cx, NAME, &p, e);
        status = 1;
    }
    for name in sel.unmatched() {
        complain(cx, NAME, &format!("{name}: The archive has no such member."));
        status = 1;
    }
    out.finish(cx, NAME, status)
}
