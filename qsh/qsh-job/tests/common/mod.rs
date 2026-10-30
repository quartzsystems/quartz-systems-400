// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A job side over an in-memory IFS seeded like the utilities' corpus,
//! a fake CL runner, and the corpus file format.

#![allow(dead_code)]

use std::sync::Arc;

use ifs::backend::mem::MemBackend;
use ifs::client::{IfsClient, LocalIfs};
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_CREAT, O_TRUNC, O_WRONLY};
use qsh::session::{self, Invocation};
use qsh::sys::{
    Buffer, Bytes, ClMessage, ClOutcome, ClRequest, Credentials, MsgKind, ProcInfo, SysError,
};
use qsh_job::run::with_shell;
use qsh_job::{Ids, ProgramRun, Setup, Side};
use sysapi::ifs::memory::MemIfs;

/// Modification time of the seeded data files.
pub const DATA_MTIME: i64 = 1_780_000_000;

/// The test job.
pub struct TestSide {
    /// Its IFS.
    pub ifs: LocalIfs<MemBackend>,
    /// The CL commands it ran.
    pub cl: Vec<String>,
    /// The programs it called.
    pub calls: Vec<(String, String, Vec<String>)>,
}

impl Side for TestSide {
    fn ifs(&self) -> &dyn IfsClient {
        &self.ifs
    }

    fn run_cl(&mut self, req: &ClRequest) -> Result<ClOutcome, SysError> {
        self.cl.push(req.command.clone());
        let failed = req.command.to_ascii_uppercase().starts_with("BAD");
        Ok(ClOutcome {
            messages: vec![ClMessage {
                id: if failed { "CPF0001" } else { "CPC0000" }.into(),
                kind: if failed {
                    MsgKind::Escape
                } else {
                    MsgKind::Completion
                },
                text: format!("ran {}", req.command),
            }],
            spooled: Vec::new(),
            failed,
        })
    }

    fn call_program(
        &mut self,
        lib: &str,
        pgm: &str,
        args: &[String],
    ) -> Result<ProgramRun, SysError> {
        self.calls
            .push((lib.to_string(), pgm.to_string(), args.to_vec()));
        Ok(ProgramRun {
            status: 0,
            output: format!("{lib}/{pgm} {}\n", args.join(" ")),
        })
    }

    fn credentials(&mut self) -> Result<Credentials, SysError> {
        Ok(Credentials {
            user: "QSTEST".into(),
            uid: 110,
            gid: 0,
            groups: Vec::new(),
        })
    }

    fn profiles(&mut self) -> Result<Vec<Ids>, SysError> {
        Ok(vec![
            Ids {
                name: "QSYS".into(),
                uid: 0,
                gid: 0,
                home: "/home/QSYS".into(),
            },
            Ids {
                name: "QSECOFR".into(),
                uid: 1,
                gid: 0,
                home: "/home/QSECOFR".into(),
            },
            Ids {
                name: "QSTEST".into(),
                uid: 110,
                gid: 0,
                home: "/home/QSTEST".into(),
            },
        ])
    }

    fn processes(&mut self) -> Result<Vec<ProcInfo>, SysError> {
        Ok(Vec::new())
    }
}

fn put(c: &dyn IfsClient, path: &str, data: &[u8]) {
    let fd = c
        .open(path, OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC).mode(0o644))
        .unwrap_or_else(|e| panic!("{path}: {e}"));
    ifs::client::write_all(c, fd, data).expect("written");
    c.close(fd).expect("closed");
    let t = DATA_MTIME * 1_000_000_000;
    c.utime(path, Some((t, t))).expect("time set");
}

fn mkdirs(c: &dyn IfsClient, path: &str) {
    let mut at = String::new();
    for part in path.split('/').filter(|p| !p.is_empty()) {
        at.push('/');
        at.push_str(part);
        let _ = c.mkdir(&at, 0o755);
    }
}

/// A new job over a freshly seeded IFS.
#[must_use]
pub fn side() -> TestSide {
    let m = MemIfs::new();
    let admin = m.client("QSECOFR", 6, 37);
    {
        let c: &dyn IfsClient = &admin;
        let d = "/home/QSHUSER/data";
        mkdirs(c, &format!("{d}/sub/inner"));
        mkdirs(c, "/home/QSHUSER/raw");
        mkdirs(c, "/home/QSHUSER/bin");
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
            ("old.txt", "old\n"),
        ];
        for (name, text) in files {
            put(c, &format!("{d}/{name}"), text.as_bytes());
        }
        let raw: &[(&str, &[u8])] = &[
            ("ff.bin", &[0xff, 0xff, 0x80, 0x00, 0x01]),
            ("one.f8", &[0x3f, 0xf0, 0, 0, 0, 0, 0, 0]),
            ("latin1.txt", &[0xe9, b'\n']),
            ("ebcdic.txt", &[0xc1, 0xc2, 0x25]),
        ];
        for (name, bytes) in raw {
            put(c, &format!("/home/QSHUSER/raw/{name}"), bytes);
        }
        put(c, "/home/QSHUSER/bin/tool", b"");
        c.chmod("/home/QSHUSER/bin/tool", 0o755).expect("mode");
        c.symlink("fruits.txt", &format!("{d}/link")).expect("link");
        // The tree is the test user's, who has no special authority.
        let tree = c
            .walk("/home/QSHUSER", ifs::ops::WalkOpts::default(), 1000)
            .expect("walked");
        for e in tree {
            c.chgown(&e.path, "QSTEST", true).expect("owner");
        }
    }
    admin.release();
    let ifs = m.client("QSTEST", 7, 37);
    TestSide {
        ifs,
        cl: Vec::new(),
        calls: Vec::new(),
    }
}

/// The setup the corpus expects: `QSHUSER`'s home as the directory.
#[must_use]
pub fn setup() -> Setup {
    Setup {
        env: vec![
            ("HOME".into(), "/home/QSHUSER".into()),
            ("LOGNAME".into(), "QSHUSER".into()),
        ],
        cwd: "/home/QSHUSER".into(),
        pid: 100,
        nodename: "QS400".into(),
    }
}

/// One corpus case, or what a script did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Case {
    /// The name.
    pub name: String,
    /// The script.
    pub script: String,
    /// Standard input.
    pub stdin: String,
    /// Standard output.
    pub out: String,
    /// Standard error.
    pub err: String,
    /// Exit status.
    pub status: i32,
}

/// Run a script as `qsh -c` in the job.
pub fn run_script(side: &mut TestSide, script: &str, stdin: &str) -> Case {
    let (script, stdin) = (script.to_string(), stdin.as_bytes().to_vec());
    with_shell(side, setup(), move |sys| {
        let out = Buffer::new();
        let err = Buffer::new();
        let inv = Invocation::command(&script);
        let status = session::run(
            Arc::new(sys),
            &inv,
            Some(Bytes::fd(stdin)),
            Some(out.fd()),
            Some(err.fd()),
        );
        Case {
            out: out.text(),
            err: err.text(),
            status,
            ..Case::default()
        }
    })
}

fn body(lines: &[&str], no_newline: bool) -> String {
    let mut s = lines.join("\n");
    if !lines.is_empty() && !no_newline {
        s.push('\n');
    }
    s
}

/// The cases of a corpus file (`### name`, then `--- in|out|err|status`).
#[must_use]
pub fn parse(text: &str) -> Vec<Case> {
    let mut cases = Vec::new();
    let mut cur: Option<Case> = None;
    let mut section = String::new();
    let mut lines: Vec<&str> = Vec::new();
    let mut no_nl = false;
    let flush = |cur: &mut Option<Case>, section: &str, lines: &mut Vec<&str>, no_nl: bool| {
        if let Some(c) = cur.as_mut() {
            let b = body(lines, no_nl);
            match section {
                "script" => c.script = b,
                "in" => c.stdin = b,
                "out" => c.out = b,
                "err" => c.err = b,
                "status" => c.status = b.trim().parse().expect("status"),
                other => panic!("unknown section {other}"),
            }
        }
        lines.clear();
    };
    let text = text.replace("\r\n", "\n");
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("### ") {
            if lines.last() == Some(&"") {
                lines.pop();
            }
            flush(&mut cur, &section, &mut lines, no_nl);
            if let Some(c) = cur.take() {
                cases.push(c);
            }
            cur = Some(Case {
                name: name.trim().to_string(),
                ..Case::default()
            });
            section = "script".to_string();
            no_nl = false;
        } else if let Some(h) = line.strip_prefix("--- ") {
            flush(&mut cur, &section, &mut lines, no_nl);
            let mut it = h.split_whitespace();
            section = it.next().unwrap_or_default().to_string();
            no_nl = it.next() == Some("-n");
        } else if cur.is_some() {
            lines.push(line);
        }
    }
    flush(&mut cur, &section, &mut lines, no_nl);
    if let Some(c) = cur.take() {
        cases.push(c);
    }
    cases
}
