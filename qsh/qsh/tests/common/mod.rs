// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Running scripts against a `MemSys` with a few test-only programs.

#![allow(dead_code)]

use std::fmt::Write as _;
use std::sync::Arc;

use qsh::session::{self, Invocation};
use qsh::sys::stream::{read_to_end, write_all};
use qsh::sys::{Buffer, Bytes, MemSys, UtilCtx};

/// `cat [file...]`: copy files or standard input.
fn cat(cx: &mut UtilCtx<'_>) -> i32 {
    let files: Vec<String> = cx.argv[1..].to_vec();
    let mut st = 0;
    let srcs: Vec<Option<String>> = if files.is_empty() {
        vec![None]
    } else {
        files.into_iter().map(Some).collect()
    };
    for f in srcs {
        let data = match f {
            None => match cx.fd(0) {
                Some(fd) => read_to_end(&**fd).unwrap_or_default(),
                None => Vec::new(),
            },
            Some(p) => {
                let path = cx.path(&p);
                match cx.sys.open(
                    &path,
                    qsh::sys::OpenFlags {
                        read: true,
                        ..Default::default()
                    },
                    0,
                ) {
                    Ok(fd) => read_to_end(&*fd).unwrap_or_default(),
                    Err(e) => {
                        let _ = cx.err(&format!("cat: {p}: {e}\n"));
                        st = 1;
                        continue;
                    }
                }
            }
        };
        if let Some(out) = cx.fd(1)
            && write_all(&**out, &data).is_err()
        {
            return 1;
        }
    }
    st
}

/// `args arg...`: each argument in angle brackets.
fn args(cx: &mut UtilCtx<'_>) -> i32 {
    let mut s = String::new();
    for a in &cx.argv[1..] {
        let _ = write!(s, "<{a}>");
    }
    s.push('\n');
    let _ = cx.out(&s);
    0
}

/// `env`: the environment, sorted.
fn env(cx: &mut UtilCtx<'_>) -> i32 {
    let mut e = cx.env.clone();
    e.sort();
    let mut s = String::new();
    for (k, v) in e {
        let _ = writeln!(s, "{k}={v}");
    }
    let _ = cx.out(&s);
    0
}

/// `exitwith n`.
fn exitwith(cx: &mut UtilCtx<'_>) -> i32 {
    cx.argv.get(1).and_then(|s| s.parse().ok()).unwrap_or(0)
}

/// `yes [word]`: repeat until the pipe closes.
fn yes(cx: &mut UtilCtx<'_>) -> i32 {
    let w = format!("{}\n", cx.argv.get(1).map_or("y", String::as_str));
    loop {
        if cx.out(&w).is_err() {
            return 141;
        }
    }
}

/// `head1`: copy the first line of standard input and stop.
fn head1(cx: &mut UtilCtx<'_>) -> i32 {
    let Some(fd) = cx.fd(0).cloned() else {
        return 1;
    };
    let mut line = Vec::new();
    let mut b = [0u8; 1];
    while let Ok(1) = fd.read(&mut b) {
        line.push(b[0]);
        if b[0] == b'\n' {
            break;
        }
    }
    let _ = cx.write(1, &line);
    0
}

/// `pwdx`: the working directory the program was given.
fn pwdx(cx: &mut UtilCtx<'_>) -> i32 {
    let s = format!("{}\n", cx.cwd);
    let _ = cx.out(&s);
    0
}

/// A memory system with the test programs in `/usr/bin`.
pub fn sys() -> MemSys {
    let s = MemSys::new();
    s.register("/usr/bin/cat", Arc::new(cat));
    s.register("/usr/bin/args", Arc::new(args));
    s.register("/usr/bin/env", Arc::new(env));
    s.register("/usr/bin/exitwith", Arc::new(exitwith));
    s.register("/usr/bin/yes", Arc::new(yes));
    s.register("/usr/bin/head1", Arc::new(head1));
    s.register("/usr/bin/pwdx", Arc::new(pwdx));
    let fsc = s.clone();
    s.register(
        "/usr/bin/chmod",
        Arc::new(move |cx: &mut UtilCtx<'_>| {
            let mode =
                u32::from_str_radix(cx.argv.get(1).map_or("0", String::as_str), 8).unwrap_or(0);
            for a in cx.argv.iter().skip(2) {
                fsc.set_mode(&cx.path(a), mode);
            }
            0
        }),
    );
    let fs = s.clone();
    s.register(
        "/usr/bin/mkdir",
        Arc::new(move |cx: &mut UtilCtx<'_>| {
            for a in cx.argv[1..].iter().filter(|a| !a.starts_with('-')) {
                fs.mkdir_p(&cx.path(a));
            }
            0
        }),
    );
    s
}

/// What a script did.
#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    /// Standard output.
    pub out: String,
    /// Standard error.
    pub err: String,
    /// Exit status.
    pub status: i32,
}

/// Run a script as `qsh -c` on a system, with standard input.
pub fn run_on(s: &MemSys, script: &str, stdin: &str) -> Outcome {
    let out = Buffer::new();
    let err = Buffer::new();
    let inv = Invocation::command(script);
    let status = session::run(
        Arc::new(s.clone()),
        &inv,
        Some(Bytes::fd(stdin.as_bytes().to_vec())),
        Some(out.fd()),
        Some(err.fd()),
    );
    Outcome {
        out: out.text(),
        err: err.text(),
        status,
    }
}

/// Run a script on a fresh system.
pub fn run(script: &str) -> Outcome {
    run_on(&sys(), script, "")
}
