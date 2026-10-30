// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSH Command Entry against the pub400 goldens: the display a new
//! session shows, and the one after `ls /` with the output scrolled to
//! its end. The listing is pub400's root, fed in as the shell's output.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use qshterm::{Key, Term};

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/golden/phase10/misc")
}

fn golden(name: &str) -> Vec<String> {
    let p = golden_dir().join(format!("{name}.txt"));
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(str::to_string)
        .collect()
}

fn compare(name: &str, t: &Term) {
    let want = golden(name);
    let got = t.screen().text_rows();
    let mut diff = String::new();
    for (i, w) in want.iter().enumerate() {
        let g = got.get(i).map_or("", String::as_str);
        if w.trim_end() != g.trim_end() {
            let _ = writeln!(
                diff,
                "row {}:\n  want |{}|\n  got  |{}|",
                i + 1,
                w.trim_end(),
                g.trim_end()
            );
        }
    }
    assert!(diff.is_empty(), "{name}:\n{diff}");
}

#[test]
fn a_new_session() {
    let mut t = Term::new();
    t.output(b"$\n");
    compare("qsh-start", &t);
}

#[test]
fn after_ls_of_the_root() {
    let mut t = Term::new();
    t.output(b"$\n");
    assert_eq!(t.key(Key::Enter, "ls /"), qshterm::Action::Run("ls /".into()));
    let listing = [
        "> ls /",
        "_welcome.txt",
        "bin",
        "dev",
        "etc",
        "fb400temp",
        "home",
        "imgclg",
        "instancecache",
        "lib",
        "lib64",
        "qmgtools",
        "QDLS",
        "QFileSvr.400",
        "QIBM",
        "QNTC",
        "QOpenSys",
        "QOPT",
        "QSR",
        "QSYS.LIB",
        "QTCPTMM",
        "QXDALISTEN",
        "Snap.20261001.140145.18.0003.trc",
        "temp",
        "tempdir",
        "tmp",
        "usr",
        "var",
        "www",
        "wwwsrv",
        "$",
    ];
    for l in listing {
        t.output(format!("{l}\n").as_bytes());
    }
    compare("qsh-session", &t);
    // F17 shows the start; F18 the end again.
    t.key(Key::F(17), "");
    assert_eq!(t.screen().text_rows()[2].trim(), "$");
    t.key(Key::F(18), "");
    compare("qsh-session", &t);
}
