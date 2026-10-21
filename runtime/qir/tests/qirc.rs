// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `qirc` binary: assemble, disassemble, check and format.

use std::path::PathBuf;
use std::process::Command;

fn qirc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_qirc"))
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qirc-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn asm_dis_check_fmt() {
    let src = include_str!("programs/stor.qir");
    let text = scratch("stor.qir");
    std::fs::write(&text, src).unwrap();

    let out = qirc().arg("asm").arg(&text).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let bin = text.with_extension("qirm");
    assert!(std::fs::read(&bin).unwrap().starts_with(b"QIRM"));

    let out = qirc().arg("check").arg(&bin).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("module STOR verified"));

    let dis = qirc().arg("dis").arg(&bin).output().unwrap();
    assert!(dis.status.success());
    let fmt = qirc().arg("fmt").arg(&text).output().unwrap();
    assert!(fmt.status.success());
    assert_eq!(dis.stdout, fmt.stdout, "disassembly is the canonical text");
    assert_eq!(qir::print(&qir::parse(src).unwrap()).as_bytes(), fmt.stdout);
}

#[test]
fn asm_rejects_a_bad_module() {
    let text = scratch("bad.qir");
    std::fs::write(
        &text,
        "module BAD\nproc P() {\nL0:\n  %x = const.i32 1\n}\n",
    )
    .unwrap();
    let out = qirc().arg("asm").arg(&text).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("terminator"));
    assert!(!text.with_extension("qirm").exists());

    let out = qirc()
        .arg("asm")
        .arg("--no-verify")
        .arg(&text)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(text.with_extension("qirm").exists());
}
