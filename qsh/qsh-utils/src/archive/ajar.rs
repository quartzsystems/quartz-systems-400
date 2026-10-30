// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ajar {c|t|x|u}[vf0Mm] [archive] [manifest] [-C dir] file ...`: zip
//! and jar archives without a JVM. Key letters come with or without a
//! leading `-`; `f` and `m` take the next arguments in the order they
//! appear. Creating adds `META-INF/MANIFEST.MF` unless `M`; entries are
//! deflated unless `0` (or when deflating does not help).

use qsh::sys::{FileType, UtilCtx};

use super::arcio::{read_all, write_out};
use super::gather::{Found, Gather};
use super::restore::{mkdirs, write_file};
use super::zip::{self, Entry};
use crate::common::io::{Out, read_input};
use crate::common::time::{Zone, strftime};
use crate::common::{complain, complain_sys, usage};

const NAME: &str = "ajar";
/// The manifest's name.
pub const MANIFEST: &str = "META-INF/MANIFEST.MF";

#[derive(Default)]
struct Keys {
    func: Option<char>,
    verbose: bool,
    store: bool,
    no_manifest: bool,
    file: Option<String>,
    manifest: Option<String>,
}

/// One file operand and the directory it is named from (`-C`).
struct Operand {
    dir: Option<String>,
    name: String,
}

fn parse(args: &[String]) -> Result<(Keys, Vec<Operand>), String> {
    let mut k = Keys::default();
    let mut pending = Vec::new();
    let mut i = 0;
    let mut first = true;
    while i < args.len() {
        let a = &args[i];
        let cluster = if first {
            a.strip_prefix('-').unwrap_or(a)
        } else if a.len() > 1 && a.starts_with('-') && a != "-C" {
            &a[1..]
        } else {
            break;
        };
        first = false;
        i += 1;
        for c in cluster.chars() {
            match c {
                'c' | 't' | 'x' | 'u' => {
                    if k.func.is_some_and(|f| f != c) {
                        return Err("Only one of c, t, x and u may be given.".to_string());
                    }
                    k.func = Some(c);
                }
                'v' => k.verbose = true,
                '0' => k.store = true,
                'M' => k.no_manifest = true,
                'f' | 'm' => pending.push(c),
                _ => return Err(format!("Key letter {c} is not valid.")),
            }
        }
    }
    for c in pending {
        let v = args.get(i).ok_or(format!("Key letter {c} needs an argument."))?;
        i += 1;
        if c == 'f' {
            k.file = Some(v.clone());
        } else {
            k.manifest = Some(v.clone());
        }
    }
    if k.func.is_none() {
        return Err("One of c, t, x or u is needed.".to_string());
    }
    let mut ops = Vec::new();
    let mut dir = None;
    while i < args.len() {
        if args[i] == "-C" {
            dir = Some(args.get(i + 1).ok_or("Option -C needs a directory.")?.clone());
            i += 2;
            continue;
        }
        ops.push(Operand {
            dir: dir.clone(),
            name: args[i].clone(),
        });
        i += 1;
    }
    Ok((k, ops))
}

/// Run `ajar`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let (k, ops) = match parse(&cx.argv[1..]) {
        Ok(x) => x,
        Err(t) => return usage(cx, NAME, &t),
    };
    match k.func {
        Some('c') => create(cx, &k, &ops, Vec::new()),
        Some('u') => {
            let Some(f) = k.file.as_deref().filter(|f| *f != "-") else {
                return usage(cx, NAME, "Updating needs an archive named with f.");
            };
            match read_all(cx, Some(f)).and_then(|d| zip::read(&d).map_err(|t| format!("{f}: {t}"))) {
                Ok(es) => create(cx, &k, &ops, es),
                Err(t) => {
                    complain(cx, NAME, &t);
                    1
                }
            }
        }
        Some('t') => list(cx, &k, &ops),
        _ => extract(cx, &k, &ops),
    }
}

fn entry_name(name: &str, dir: bool) -> String {
    let mut n = name;
    while let Some(r) = n.strip_prefix("./") {
        n = r;
    }
    let mut n = n.trim_start_matches('/').trim_end_matches('/').to_string();
    if n == "." {
        n.clear();
    }
    if dir && !n.is_empty() {
        n.push('/');
    }
    n
}

fn manifest(cx: &UtilCtx<'_>, k: &Keys) -> Result<Vec<u8>, String> {
    let mut text = match &k.manifest {
        None => String::new(),
        Some(m) => {
            let d = read_input(cx, m).map_err(|e| format!("{m}: {}", e.errno.text()))?;
            String::from_utf8_lossy(&d).replace("\r\n", "\n")
        }
    };
    if !text.lines().any(|l| l.starts_with("Manifest-Version:")) {
        text = format!("Manifest-Version: 1.0\n{text}");
    }
    if !text.lines().any(|l| l.starts_with("Created-By:")) {
        let at = text.find('\n').map_or(text.len(), |i| i + 1);
        text.insert_str(at, "Created-By: QS/400 ajar\n");
    }
    let text = text.trim_end_matches('\n').to_string() + "\n\n";
    Ok(text.replace('\n', "\r\n").into_bytes())
}

fn create(cx: &UtilCtx<'_>, k: &Keys, ops: &[Operand], mut entries: Vec<Entry>) -> i32 {
    let zone = Zone::of(cx);
    let now = i64::try_from(cx.sys.now().as_secs()).unwrap_or(0);
    let to_stdout = k.file.as_deref().is_none_or(|f| f == "-");
    let note = |t: String| {
        let _ = if to_stdout { cx.err(&t) } else { cx.out(&t) };
    };
    let mut status = 0;
    let put = |entries: &mut Vec<Entry>, e: Entry| match entries.iter_mut().find(|x| x.name == e.name) {
        Some(old) => *old = e,
        None => entries.push(e),
    };
    let have_manifest = entries.iter().any(|e| e.name == MANIFEST);
    if !k.no_manifest && (!have_manifest || k.manifest.is_some()) {
        match manifest(cx, k) {
            Ok(m) => {
                let dos = zip::dos_time(now, &zone);
                if !entries.iter().any(|e| e.name == "META-INF/") {
                    entries.insert(0, Entry::new("META-INF/", b"", false, dos, Some(0o755)));
                }
                let e = Entry::new(MANIFEST, &m, !k.store, dos, Some(0o644));
                match entries.iter().position(|x| x.name == MANIFEST) {
                    Some(i) => entries[i] = e,
                    None => entries.insert(1, e),
                }
                if k.verbose {
                    note(format!("added manifest {MANIFEST}\n"));
                }
            }
            Err(t) => {
                complain(cx, NAME, &t);
                return 1;
            }
        }
    }
    let archive = k
        .file
        .as_deref()
        .filter(|f| *f != "-")
        .and_then(|f| cx.sys.realpath(&cx.path(f)).ok());
    for op in ops {
        let sub = UtilCtx {
            argv: Vec::new(),
            env: cx.env.clone(),
            cwd: op.dir.as_deref().map_or_else(|| cx.cwd.clone(), |d| cx.path(d)),
            umask: cx.umask,
            fds: cx.fds.clone(),
            sys: cx.sys,
        };
        Gather::default().walk(&sub, &op.name, &mut |r: Result<Found, _>| {
            let found = match r {
                Ok(f) => f,
                Err((p, e)) => {
                    complain_sys(cx, NAME, &p, e);
                    status = 1;
                    return true;
                }
            };
            if archive.as_deref().is_some_and(|a| cx.sys.realpath(&found.abs).is_ok_and(|r| r == a)) {
                return true;
            }
            let dir = found.st.ftype == FileType::Dir;
            let name = entry_name(&found.name, dir);
            if name.is_empty() {
                return true;
            }
            let dos = zip::dos_time(found.st.mtime, &zone);
            let mode = Some(found.st.mode & 0o7777);
            let e = if dir {
                Entry::new(&name, b"", false, dos, mode)
            } else {
                match read_input(cx, &found.abs) {
                    Ok(d) => Entry::new(&name, &d, !k.store, dos, mode),
                    Err(e) => {
                        complain_sys(cx, NAME, &found.name, e);
                        status = 1;
                        return true;
                    }
                }
            };
            if k.verbose {
                let how = if dir {
                    String::new()
                } else if e.method == zip::DEFLATED {
                    let pct = 100 - (e.data.len() as u64 * 100 / e.size.max(1));
                    format!(" ({} bytes, deflated {pct}%)", e.size)
                } else {
                    format!(" ({} bytes, stored)", e.size)
                };
                note(format!("added {name}{how}\n"));
            }
            put(&mut entries, e);
            true
        });
    }
    if let Err(t) = write_out(cx, k.file.as_deref(), &zip::write(&entries)) {
        complain(cx, NAME, &t);
        status = 1;
    }
    status
}

fn load(cx: &UtilCtx<'_>, k: &Keys) -> Result<Vec<Entry>, i32> {
    read_all(cx, k.file.as_deref())
        .and_then(|d| zip::read(&d))
        .map_err(|t| {
            complain(cx, NAME, &t);
            1
        })
}

fn wanted(ops: &[Operand], name: &str) -> bool {
    ops.is_empty()
        || ops.iter().any(|o| {
            let p = entry_name(&o.name, false);
            name.trim_end_matches('/') == p || name.starts_with(&format!("{p}/"))
        })
}

fn list(cx: &UtilCtx<'_>, k: &Keys, ops: &[Operand]) -> i32 {
    let entries = match load(cx, k) {
        Ok(e) => e,
        Err(s) => return s,
    };
    let zone = Zone::of(cx);
    let mut out = Out::stdout(cx);
    for e in entries.iter().filter(|e| wanted(ops, &e.name)) {
        if k.verbose {
            let t = zip::from_dos(e.dos, &zone);
            let date = strftime("%a %b %d %H:%M:%S %Z %Y", &zone.local(t));
            out.text(&format!("{:>6} {date} {}\n", e.size, e.name));
        } else {
            out.text(&format!("{}\n", e.name));
        }
    }
    out.finish(cx, NAME, 0)
}

fn extract(cx: &UtilCtx<'_>, k: &Keys, ops: &[Operand]) -> i32 {
    let entries = match load(cx, k) {
        Ok(e) => e,
        Err(s) => return s,
    };
    let zone = Zone::of(cx);
    let mut out = Out::stdout(cx);
    let mut status = 0;
    for e in entries.iter().filter(|e| wanted(ops, &e.name)) {
        let name = entry_name(&e.name, false);
        if name.is_empty() || name.split('/').any(|c| c == "..") {
            complain(cx, NAME, &format!("{}: The name is not safe to extract.", e.name));
            status = 1;
            continue;
        }
        let abs = cx.path(&name);
        let made = if e.is_dir() {
            mkdirs(cx, &abs).map(|()| "created")
        } else {
            match e.plain() {
                Ok(d) => {
                    let parent = abs.rsplit_once('/').map_or("/", |(a, _)| if a.is_empty() { "/" } else { a });
                    mkdirs(cx, parent)
                        .and_then(|()| write_file(cx, &abs, &d))
                        .map(|()| "extracted")
                }
                Err(t) => {
                    complain(cx, NAME, &format!("{}: {t}", e.name));
                    status = 1;
                    continue;
                }
            }
        };
        match made {
            Ok(what) => {
                if let Some(m) = e.mode {
                    let _ = cx.sys.chmod(&abs, m & 0o777 & !cx.umask);
                }
                let _ = cx.sys.utimes(&abs, zip::from_dos(e.dos, &zone));
                if k.verbose {
                    out.text(&format!("{what} {}\n", e.name));
                }
            }
            Err(err) => {
                complain_sys(cx, NAME, &e.name, err);
                status = 1;
            }
        }
    }
    out.finish(cx, NAME, status)
}
