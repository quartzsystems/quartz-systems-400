// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `pax`: list (no `-r`/`-w`), read (`-r`), write (`-w`) and copy
//! (`-rw`) archives in the ustar, tar, cpio, sv4cpio and sv4crc
//! formats, with shell-pattern selection and `-s` renames.

use qsh::sys::{FileType, UtilCtx};

use super::arcio::{block_size, names_from_stdin, read_all, write_out};
use super::build::Build;
use super::codec::{self, Format};
use super::gather::{Gather, Links, member, member_name};
use super::listing::Lister;
use super::member::{Kind, Member};
use super::restore::{Done, Restore, Restorer, mkdirs};
use super::select::Select;
use super::subst::{self, Subst, rename};
use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::{complain, complain_sys, usage};

const NAME: &str = "pax";

struct Opts {
    read: bool,
    write: bool,
    append: bool,
    block: Option<usize>,
    complement: bool,
    no_descend: bool,
    file: Option<String>,
    follow_root: bool,
    follow_all: bool,
    keep: bool,
    link: bool,
    first_only: bool,
    restore: Restore,
    subs: Vec<Subst>,
    update: bool,
    verbose: bool,
    format: Option<Format>,
    one_fs: bool,
}

fn parse_p(s: &str, r: &mut Restore) -> bool {
    for c in s.chars() {
        match c {
            'a' => {}
            'e' => {
                r.mode = true;
                r.owner = true;
                r.mtime = true;
            }
            'm' => r.mtime = false,
            'o' => r.owner = true,
            'p' => r.mode = true,
            _ => return false,
        }
    }
    true
}

/// Run `pax`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "rwab:cdf:HikLlno:p:s:tuvx:X") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    if p.has('i') {
        return usage(cx, NAME, "Option -i is not supported: there is no terminal to ask.");
    }
    let mut o = Opts {
        read: p.has('r'),
        write: p.has('w'),
        append: p.has('a'),
        block: None,
        complement: p.has('c'),
        no_descend: p.has('d'),
        file: p.value('f').map(ToString::to_string),
        follow_root: p.has('H'),
        follow_all: p.has('L'),
        keep: p.has('k'),
        link: p.has('l'),
        first_only: p.has('n'),
        restore: Restore {
            mtime: true,
            ..Restore::default()
        },
        subs: Vec::new(),
        update: p.has('u'),
        verbose: p.has('v'),
        format: None,
        one_fs: p.has('X'),
    };
    o.restore.keep = o.keep;
    o.restore.newer_only = o.update;
    for v in p.values('p') {
        if !parse_p(v, &mut o.restore) {
            return usage(cx, NAME, &format!("The -p argument {v} is not valid."));
        }
    }
    for v in p.values('s') {
        match subst::parse(v) {
            Ok(s) => o.subs.push(s),
            Err(t) => return usage(cx, NAME, &t),
        }
    }
    if let Some(b) = p.value('b') {
        match block_size(b) {
            Some(n) if n % 512 == 0 && n <= 32256 => o.block = Some(n),
            _ => return usage(cx, NAME, &format!("The block size {b} is not valid.")),
        }
    }
    if let Some(x) = p.value('x') {
        match Format::parse(x) {
            Some(f) => o.format = Some(f),
            None => return usage(cx, NAME, &format!("The format {x} is not known.")),
        }
    }
    let ops = p.operands.clone();
    match (o.read, o.write) {
        (false | true, false) if o.append || o.block.is_some() || o.format.is_some() => {
            usage(cx, NAME, "Options -a, -b and -x go with -w only.")
        }
        (false, false) => list_or_read(cx, &o, &ops, false),
        (true, false) => list_or_read(cx, &o, &ops, true),
        (false, true) => write(cx, &o, &ops),
        (true, true) => copy(cx, &o, &ops),
    }
}

fn gather_of(o: &Opts) -> Gather {
    Gather {
        follow_root: o.follow_root,
        follow_all: o.follow_all,
        no_descend: o.no_descend,
        one_fs: o.one_fs,
    }
}

fn list_or_read(cx: &UtilCtx<'_>, o: &Opts, pats: &[String], extract: bool) -> i32 {
    let data = match read_all(cx, o.file.as_deref()) {
        Ok(d) => d,
        Err(t) => {
            complain(cx, NAME, &t);
            return 1;
        }
    };
    let members = match codec::read(&data) {
        Ok((_, m)) => m,
        Err(t) => {
            complain(cx, NAME, &t);
            return 1;
        }
    };
    let mut sel = Select::new(pats);
    sel.complement = o.complement;
    sel.first_only = o.first_only;
    let mut out = Out::stdout(cx);
    let lister = Lister::new(cx);
    let mut rest = Restorer::new(o.restore);
    let mut status = 0;
    for m in &members {
        if !sel.take(&m.name) {
            continue;
        }
        let (name, note) = rename(&o.subs, &m.name);
        if let Some(n) = note {
            let _ = cx.err(&n);
        }
        if name.is_empty() {
            continue;
        }
        if !extract {
            let line = if o.verbose {
                lister.line(m, &name)
            } else {
                name.clone()
            };
            out.text(&line);
            out.text("\n");
            continue;
        }
        let link = if m.kind == Kind::Hardlink {
            rename(&o.subs, &m.link).0
        } else {
            m.link.clone()
        };
        match rest.put(cx, m, &cx.path(&name), &cx.path(&link)) {
            Ok(Done::Made) => {
                if o.verbose {
                    let _ = cx.err(&format!("{name}\n"));
                }
            }
            Ok(Done::Skipped) => {}
            Err(e) => {
                complain_sys(cx, NAME, &name, e);
                status = 1;
            }
        }
    }
    for (p, e) in rest.finish(cx) {
        complain_sys(cx, NAME, &p, e);
        status = 1;
    }
    for pat in sel.unmatched() {
        complain(cx, NAME, &format!("{pat}: No member matched the pattern."));
        status = 1;
    }
    out.finish(cx, NAME, status)
}

fn write(cx: &UtilCtx<'_>, o: &Opts, ops: &[String]) -> i32 {
    if o.append && o.file.as_deref().is_none_or(|f| f == "-") {
        return usage(cx, NAME, "Option -a needs an archive file named with -f.");
    }
    let ops = if ops.is_empty() {
        names_from_stdin(cx)
    } else {
        ops.to_vec()
    };
    let mut old: Vec<Member> = Vec::new();
    let mut format = o.format;
    if o.append {
        let file = o.file.as_deref().unwrap_or("-");
        let data = match read_all(cx, Some(file)) {
            Ok(d) => d,
            Err(t) => {
                complain(cx, NAME, &t);
                return 1;
            }
        };
        match codec::read(&data) {
            Ok((f, ms)) => {
                if format.is_some_and(|g| g != f) {
                    complain(cx, NAME, "The format differs from the archive's.");
                    return 1;
                }
                format = Some(f);
                old = ms;
            }
            Err(t) => {
                complain(cx, NAME, &format!("{file}: {t}"));
                return 1;
            }
        }
    }
    let format = format.unwrap_or(Format::Ustar);
    let build = Build {
        util: NAME,
        format,
        gather: gather_of(o),
        subs: &o.subs,
        verbose: o.verbose,
        skip: o
            .file
            .as_deref()
            .filter(|f| *f != "-")
            .and_then(|f| cx.sys.realpath(&cx.path(f)).ok()),
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
    let update = o.update;
    let mut status = build.add(cx, &ops, &mut bytes, &mut |found, name| {
        !update
            || !old
                .iter()
                .any(|m| m.name == name && m.mtime >= found.st.mtime)
    });
    let bytes = codec::finish(bytes, format, o.block.unwrap_or(format.default_block()));
    if let Err(t) = write_out(cx, o.file.as_deref(), &bytes) {
        complain(cx, NAME, &t);
        status = 1;
    }
    status
}

fn copy(cx: &UtilCtx<'_>, o: &Opts, ops: &[String]) -> i32 {
    let Some((dest, files)) = ops.split_last() else {
        return usage(cx, NAME, "Copying needs a target directory.");
    };
    let dest_abs = cx.path(dest);
    match cx.sys.stat(&dest_abs) {
        Ok(st) if st.ftype == FileType::Dir => {}
        Ok(_) => {
            complain_sys(cx, NAME, dest, qsh::sys::Errno::ENOTDIR.into());
            return 1;
        }
        Err(e) => {
            complain_sys(cx, NAME, dest, e);
            return 1;
        }
    }
    let dest_real = cx.sys.realpath(&dest_abs).unwrap_or(dest_abs.clone());
    let files = if files.is_empty() {
        names_from_stdin(cx)
    } else {
        files.to_vec()
    };
    let join = |n: &str| {
        let n = n.trim_start_matches('/');
        if dest_abs.ends_with('/') {
            format!("{dest_abs}{n}")
        } else {
            format!("{dest_abs}/{n}")
        }
    };
    let mut rest = Restorer::new(o.restore);
    let mut links = Links::default();
    let mut status = 0;
    for op in &files {
        gather_of(o).walk(cx, op, &mut |r| {
            let found = match r {
                Ok(f) => f,
                Err((p, e)) => {
                    complain_sys(cx, NAME, &p, e);
                    status = 1;
                    return true;
                }
            };
            let real = cx.sys.realpath(&found.abs).unwrap_or_default();
            if real == dest_real || real.starts_with(&format!("{dest_real}/")) {
                return true;
            }
            let (name, note) = rename(&o.subs, &member_name(&found.name));
            if let Some(n) = note {
                let _ = cx.err(&n);
            }
            if name.is_empty() {
                return true;
            }
            let target = join(&name);
            if o.link && found.st.ftype == FileType::File {
                let fresh = match cx.sys.lstat(&target) {
                    Ok(old) => {
                        if o.keep || (o.update && old.mtime >= found.st.mtime) {
                            return true;
                        }
                        cx.sys.unlink(&target).is_ok()
                    }
                    Err(_) => true,
                };
                let parent = target.rsplit_once('/').map_or("/", |(a, _)| a).to_string();
                if fresh && mkdirs(cx, &parent).is_ok() && cx.sys.link(&found.abs, &target).is_ok() {
                    if o.verbose {
                        let _ = cx.err(&format!("{name}\n"));
                    }
                    return true;
                }
            }
            match member(cx, &found, &name, &mut links) {
                Ok(m) => {
                    let link_abs = join(&m.link);
                    match rest.put(cx, &m, &target, &link_abs) {
                        Ok(Done::Made) if o.verbose => {
                            let _ = cx.err(&format!("{name}\n"));
                        }
                        Ok(_) => {}
                        Err(e) => {
                            complain_sys(cx, NAME, &name, e);
                            status = 1;
                        }
                    }
                }
                Err(e) => {
                    complain_sys(cx, NAME, &found.name, e);
                    status = 1;
                }
            }
            true
        });
    }
    for (p, e) in rest.finish(cx) {
        complain_sys(cx, NAME, &p, e);
        status = 1;
    }
    status
}
