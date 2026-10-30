// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A `MemSys` with every utility registered and a seeded tree under
//! `/home/QSHUSER`, and a runner for scripts.

#![allow(dead_code, clippy::too_many_lines, clippy::format_push_string)]

pub mod services;

use std::sync::Arc;
use std::time::Duration;

use qsh::session::{self, Invocation};
use qsh::sys::{Buffer, Bytes, MemSys, ProcInfo, UtilCtx};

/// Modification time of the seeded data files (2026-05-28 20:26:40 UTC).
pub const DATA_MTIME: i64 = 1_780_000_000;
/// An old file's time (2020-09-13 12:26:40 UTC).
pub const OLD_MTIME: i64 = 1_600_000_000;

/// The seeded system.
pub fn sys() -> MemSys {
    let s = MemSys::new();
    for (name, f) in qsh_utils::UTILITIES {
        s.register(&format!("/usr/bin/{name}"), Arc::new(*f));
    }
    // `sh` for scripts run from utilities (`xargs`, `env`, `find
    // -exec`): the shell on the same system, given the caller's
    // environment and directory as a prologue.
    let me = s.clone();
    s.register(
        "/usr/bin/sh",
        Arc::new(move |cx: &mut UtilCtx<'_>| -> i32 {
            let Ok(mut inv) = Invocation::parse(&cx.argv) else {
                return 2;
            };
            let body = match (&inv.command, &inv.script) {
                (Some(c), _) => c.clone(),
                (None, Some(p)) => match me.read_file(&cx.path(p)) {
                    Some(b) => String::from_utf8_lossy(&b).into_owned(),
                    None => return 127,
                },
                _ => return 2,
            };
            let mut text = String::new();
            for (k, v) in &cx.env {
                text.push_str(&format!("export {k}='{}'\n", v.replace('\'', "'\\''")));
            }
            text.push_str(&format!("cd '{}'\n{body}", cx.cwd));
            inv.command = Some(text);
            inv.script = None;
            session::run(
                Arc::new(me.clone()),
                &inv,
                cx.fd(0).cloned(),
                cx.fd(1).cloned(),
                cx.fd(2).cloned(),
            )
        }),
    );
    let d = "/home/QSHUSER/data";
    let files: &[(&str, &str)] = &[
        ("fruits.txt", "apple\nbanana\ncherry\n"),
        ("nums.txt", "10\n9\n100\n-5\n2.5\n"),
        ("words.txt", "the quick brown fox\njumps over\nthe lazy dog\n"),
        ("empty.txt", ""),
        ("nonl.txt", "no newline"),
        ("tabs.txt", "a\tb\tc\n1\t2\t3\n"),
        ("csv.txt", "x,1,b\ny,2,a\nz,3,c\n"),
        ("dup.txt", "a\na\nb\nc\nc\nc\nd\n"),
        ("people.txt", "bob 25 ny\nalice 30 la\ncarol 25 sf\ndave 19 ny\n"),
        ("mixed.txt", "Apple\nbanana\nCherry\napple\n"),
        ("sub/deep.txt", "deep\n"),
        ("sub/inner/leaf", "leaf\n"),
        (".hidden", "hidden\n"),
    ];
    for (name, text) in files {
        let p = format!("{d}/{name}");
        s.write_file(&p, text);
        s.set_mtime(&p, DATA_MTIME);
    }
    // Raw bytes the shell's printf cannot make (it writes UTF-8).
    let raw: &[(&str, &[u8])] = &[
        ("ff.bin", &[0xff, 0xff, 0x80, 0x00, 0x01]),
        ("one.f8", &[0x3f, 0xf0, 0, 0, 0, 0, 0, 0]),
        ("latin1.txt", &[0xe9, b'\n']),
        ("ebcdic.txt", &[0xc1, 0xc2, 0x25]),
    ];
    for (name, bytes) in raw {
        let p = format!("/home/QSHUSER/raw/{name}");
        s.write_file(&p, bytes);
        s.set_mtime(&p, DATA_MTIME);
    }
    s.write_file(&format!("{d}/old.txt"), "old\n");
    s.set_mtime(&format!("{d}/old.txt"), OLD_MTIME);
    s.write_file("/home/QSHUSER/bin/tool", "");
    s.set_mode("/home/QSHUSER/bin/tool", 0o755);
    s.set_mtime("/home/QSHUSER/bin/tool", DATA_MTIME);
    s.set_mtime("/home/QSHUSER/bin", DATA_MTIME);
    s.set_mtime(d, DATA_MTIME);
    s.set_mtime(&format!("{d}/sub"), DATA_MTIME);
    s.set_mtime(&format!("{d}/sub/inner"), DATA_MTIME);
    s.set_mtime("/home/QSHUSER", DATA_MTIME);
    s.make_symlink("fruits.txt", &format!("{d}/link"));
    s.set_mtime(&format!("{d}/link"), DATA_MTIME);
    s.set_owner(&format!("{d}/old.txt"), 0, 50);
    s.write_file("/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR", "PGM\nENDPGM\n");
    s.set_mtime("/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR", DATA_MTIME);
    s.set_mtime("/QSYS.LIB/QGPL.LIB/QCLSRC.FILE", DATA_MTIME);
    s.make_object("/QSYS.LIB/QGPL.LIB/HELLO.PGM", 4096);
    s.set_mtime("/QSYS.LIB/QGPL.LIB/HELLO.PGM", DATA_MTIME);
    s.make_object("/QSYS.LIB/QGPL.LIB/CFG.DTAARA", 512);
    s.set_mtime("/QSYS.LIB/QGPL.LIB/CFG.DTAARA", DATA_MTIME);
    s.set_mtime("/QSYS.LIB/QGPL.LIB", DATA_MTIME);
    s.write_file("/QOpenSys/etc/profile", "# profile\n");
    s.set_mtime("/QOpenSys/etc/profile", DATA_MTIME);
    s.add_process(ProcInfo {
        pid: 101,
        ppid: 100,
        pgid: 100,
        uid: 100,
        job: "000124/QSHUSER/QP0ZSPWP".to_string(),
        status: "RUN".to_string(),
        function: "PGM-ls".to_string(),
        device: "-".to_string(),
        cpu: Duration::from_secs(65),
        args: vec!["/usr/bin/ls".to_string(), "-l".to_string()],
    });
    services::seed(&s);
    s
}

/// What a script did.
#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    pub out: String,
    pub err: String,
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

/// Run a script on a fresh seeded system.
pub fn run(script: &str) -> Outcome {
    run_on(&sys(), script, "")
}
