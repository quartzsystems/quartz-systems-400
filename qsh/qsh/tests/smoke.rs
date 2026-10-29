// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A few end-to-end checks of the language before the corpus.

mod common;

use common::run;

#[test]
fn echo_and_status() {
    let o = run("echo hello world; false; echo $?");
    assert_eq!(o.out, "hello world\n1\n");
    assert_eq!(o.status, 0);
}

#[test]
fn pipeline_through_utility() {
    let o = run("echo abc | cat | cat");
    assert_eq!(o.out, "abc\n");
}

#[test]
fn command_substitution_and_loops() {
    let o = run("for i in 1 2 3; do x=$(echo $i$i); echo \"$x\"; done");
    assert_eq!(o.out, "11\n22\n33\n");
}

#[test]
fn not_found() {
    let o = run("nosuch");
    assert_eq!(o.status, 127);
    assert!(o.err.contains("001-0014"), "{}", o.err);
}
