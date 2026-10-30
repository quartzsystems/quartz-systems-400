// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `compress [-cdfv] [-b bits] [file ...]`, `uncompress [-cfv] [file
//! ...]` and `zcat [file ...]`: `.Z` files. A file is replaced by its
//! compressed form (mode and time kept) unless `-c` writes standard
//! output; a file that would not shrink is left alone without `-f`
//! (status 2).

use qsh::sys::{FileType, UtilCtx};

use super::lzw;
use super::restore::write_file;
use crate::common::args::getopt;
use crate::common::io::read_input;
use crate::common::{argv0, complain, complain_sys, number, usage};

struct Opts {
    stdout: bool,
    force: bool,
    verbose: bool,
    bits: u32,
}

/// Run `compress`.
pub fn run_compress(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "b:cdfv") {
        Ok(p) => p,
        Err(e) => return usage(cx, "compress", &e.to_string()),
    };
    let bits = match p.value('b') {
        None => 16,
        Some(b) => match number(b) {
            Some(n @ 9..=16) => n as u32,
            _ => return usage(cx, "compress", &format!("The code size {b} is not between 9 and 16.")),
        },
    };
    let o = Opts {
        stdout: p.has('c'),
        force: p.has('f'),
        verbose: p.has('v'),
        bits,
    };
    if p.has('d') {
        return expand_all(cx, "compress", &o, &p.operands);
    }
    if p.operands.is_empty() {
        return to_stdout(cx, "compress", "-", |d| Ok(lzw::compress(d, o.bits)));
    }
    let mut status = 0;
    for f in &p.operands {
        status = status.max(compress_one(cx, &o, f));
    }
    status
}

/// Run `uncompress`.
pub fn run_uncompress(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "cfv") {
        Ok(p) => p,
        Err(e) => return usage(cx, "uncompress", &e.to_string()),
    };
    let o = Opts {
        stdout: p.has('c'),
        force: p.has('f'),
        verbose: p.has('v'),
        bits: 16,
    };
    expand_all(cx, "uncompress", &o, &p.operands)
}

/// Run `zcat`.
pub fn run_zcat(cx: &mut UtilCtx<'_>) -> i32 {
    let name = argv0(cx);
    let p = match getopt(&cx.argv[1..], "") {
        Ok(p) => p,
        Err(e) => return usage(cx, &name, &e.to_string()),
    };
    let o = Opts {
        stdout: true,
        force: true,
        verbose: false,
        bits: 16,
    };
    expand_all(cx, &name, &o, &p.operands)
}

fn to_stdout(
    cx: &UtilCtx<'_>,
    name: &str,
    file: &str,
    f: impl Fn(&[u8]) -> Result<Vec<u8>, String>,
) -> i32 {
    let data = match read_input(cx, file) {
        Ok(d) => d,
        Err(e) => {
            complain_sys(cx, name, file, e);
            return 1;
        }
    };
    let out = match f(&data) {
        Ok(o) => o,
        Err(t) => {
            let shown = if file == "-" { "standard input" } else { file };
            complain(cx, name, &format!("{shown}: {t}"));
            return 1;
        }
    };
    match cx.write(1, &out) {
        Ok(()) => 0,
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => 1,
        Err(e) => {
            complain(cx, name, &format!("standard output: {}", crate::common::io::errno_text(&e)));
            1
        }
    }
}

fn regular(cx: &UtilCtx<'_>, name: &str, f: &str) -> Option<qsh::sys::Stat> {
    match cx.sys.stat(&cx.path(f)) {
        Ok(st) if st.ftype == FileType::File => Some(st),
        Ok(_) => {
            complain(cx, name, &format!("{f}: Not a regular file; left alone."));
            None
        }
        Err(e) => {
            complain_sys(cx, name, f, e);
            None
        }
    }
}

/// Write `data` as `to` with the mode and time of `st`, then remove
/// `from`.
fn replace(cx: &UtilCtx<'_>, name: &str, from: &str, to: &str, data: &[u8], st: &qsh::sys::Stat) -> bool {
    let to_abs = cx.path(to);
    if let Err(e) = write_file(cx, &to_abs, data) {
        complain_sys(cx, name, to, e);
        return false;
    }
    let _ = cx.sys.chmod(&to_abs, st.mode & 0o7777);
    let _ = cx.sys.utimes(&to_abs, st.mtime);
    if let Err(e) = cx.sys.unlink(&cx.path(from)) {
        complain_sys(cx, name, from, e);
        return false;
    }
    true
}

fn compress_one(cx: &UtilCtx<'_>, o: &Opts, f: &str) -> i32 {
    let name = "compress";
    if o.stdout {
        return to_stdout(cx, name, f, |d| Ok(lzw::compress(d, o.bits)));
    }
    // The suffix is `.Z` exactly; `.z` is another format's.
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    if f.ends_with(".Z") {
        complain(cx, name, &format!("{f}: The name already ends in .Z; left alone."));
        return 1;
    }
    let Some(st) = regular(cx, name, f) else {
        return 1;
    };
    let data = match read_input(cx, f) {
        Ok(d) => d,
        Err(e) => {
            complain_sys(cx, name, f, e);
            return 1;
        }
    };
    let z = lzw::compress(&data, o.bits);
    if z.len() >= data.len() && !o.force {
        if o.verbose {
            complain(cx, name, &format!("{f}: Compressing would not make it smaller; left alone."));
        }
        return 2;
    }
    let target = format!("{f}.Z");
    if cx.sys.lstat(&cx.path(&target)).is_ok() && !o.force {
        complain(cx, name, &format!("{target}: The file exists; {f} left alone."));
        return 1;
    }
    if !replace(cx, name, f, &target, &z, &st) {
        return 1;
    }
    if o.verbose {
        let pct = if data.is_empty() {
            0.0
        } else {
            (data.len() as f64 - z.len() as f64) * 100.0 / data.len() as f64
        };
        let change = if pct < 0.0 {
            format!("{:.2}% larger", -pct)
        } else {
            format!("{pct:.2}% smaller")
        };
        complain(cx, name, &format!("{f}: {change}, replaced with {target}"));
    }
    0
}

fn expand_all(cx: &UtilCtx<'_>, name: &str, o: &Opts, ops: &[String]) -> i32 {
    let decode = |d: &[u8]| lzw::decompress(d).map_err(|e| e.to_string());
    if ops.is_empty() {
        return to_stdout(cx, name, "-", decode);
    }
    let mut status = 0;
    for f in ops {
        let (zname, plain) = match f.strip_suffix(".Z") {
            Some(base) => (f.clone(), base.to_string()),
            None => (format!("{f}.Z"), f.clone()),
        };
        if o.stdout {
            status = status.max(to_stdout(cx, name, &zname, decode));
            continue;
        }
        let Some(st) = regular(cx, name, &zname) else {
            status = 1;
            continue;
        };
        let data = match read_input(cx, &zname).map_err(|e| e.errno.text().to_string()).and_then(|d| decode(&d)) {
            Ok(d) => d,
            Err(t) => {
                complain(cx, name, &format!("{zname}: {t}"));
                status = 1;
                continue;
            }
        };
        if cx.sys.lstat(&cx.path(&plain)).is_ok() && !o.force {
            complain(cx, name, &format!("{plain}: The file exists; {zname} left alone."));
            status = 1;
            continue;
        }
        if !replace(cx, name, &zname, &plain, &data, &st) {
            status = 1;
            continue;
        }
        if o.verbose {
            complain(cx, name, &format!("{zname}: replaced with {plain}"));
        }
    }
    status
}
