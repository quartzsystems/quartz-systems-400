// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the job's `Sys` adds to the utilities: the shell as
//! `/usr/bin/sh` and `/usr/bin/qsh`, `/usr/bin` listing the utilities,
//! programs called in the job, CL through `system`, and `QSH CMD()`'s
//! output modes into the IFS.

mod common;

use common::{run_script, setup, side};
use qsh_job::run::batch;

#[test]
fn the_shell_is_a_program() {
    let mut s = side();
    let r = run_script(&mut s, "/usr/bin/qsh -c 'echo inner $0'; sh -c 'exit 5'; echo $?", "");
    assert_eq!(r.out, "inner /usr/bin/qsh\n5\n");
    assert_eq!(r.status, 0);
}

#[test]
fn usr_bin_lists_the_utilities() {
    let mut s = side();
    let r = run_script(&mut s, "ls /usr/bin | grep -c .; test -x /usr/bin/sed && echo x", "");
    let n: usize = r.out.lines().next().unwrap().parse().unwrap();
    assert!(n > 80, "{}", r.out);
    assert!(r.out.ends_with("x\n"));
    let r = run_script(&mut s, "command -v ls; type cat", "");
    assert_eq!(r.out, "/usr/bin/ls\ncat is /usr/bin/cat\n");
}

#[test]
fn a_program_is_called_in_the_job() {
    let mut s = side();
    let r = run_script(&mut s, "/QSYS.LIB/QGPL.LIB/HELLO.PGM a 'b c'; echo $?", "");
    assert_eq!(r.out, "QGPL/HELLO a b c\n0\n");
    assert_eq!(
        s.calls,
        vec![("QGPL".to_string(), "HELLO".to_string(), vec!["a".to_string(), "b c".to_string()])]
    );
}

#[test]
fn system_runs_cl_in_the_job() {
    let mut s = side();
    let r = run_script(&mut s, "system 'CRTLIB X'; echo $?; system 'BAD'; echo $?", "");
    assert_eq!(s.cl, vec!["CRTLIB X".to_string(), "BAD".to_string()]);
    let lines: Vec<&str> = r.out.lines().collect();
    assert!(lines.contains(&"0"), "{}", r.out);
    assert!(!lines.last().unwrap().starts_with('0'), "{}", r.out);
}

#[test]
fn scripts_run_from_the_ifs() {
    let mut s = side();
    let r = run_script(
        &mut s,
        "printf 'echo from script $1\\n' > s.sh; chmod +x s.sh; ./s.sh arg; cd data && pwd",
        "",
    );
    assert_eq!(r.out, "from script arg\n/home/QSHUSER/data\n", "{}", r.err);
}

fn with_env(pairs: &[(&str, &str)]) -> qsh_job::Setup {
    let mut st = setup();
    for (k, v) in pairs {
        st.env.push(((*k).to_string(), (*v).to_string()));
    }
    st
}

#[test]
fn batch_output_modes() {
    let mut s = side();
    let b = batch(&mut s, setup(), "echo out; echo err >&2; exit 2");
    assert_eq!(b.output, b"out\nerr\n");
    assert_eq!(b.result.message.id, "QSH0005");
    assert_eq!(b.result.message.data, 2);
    assert_eq!(
        b.result.message.kind,
        qsh::session::batch::MsgKind::Completion
    );

    let st = with_env(&[("QIBM_QSH_CMD_ESCAPE_MSG", "Y")]);
    let b = batch(&mut s, st, "exit 2");
    assert_eq!(b.result.message.kind, qsh::session::batch::MsgKind::Escape);

    let st = with_env(&[("QIBM_QSH_CMD_OUTPUT", "NONE")]);
    let b = batch(&mut s, st, "echo gone");
    assert!(b.output.is_empty());

    let st = with_env(&[("QIBM_QSH_CMD_OUTPUT", "FILE=/home/QSHUSER/o.txt")]);
    let b = batch(&mut s, st, "echo one");
    assert!(b.output.is_empty());
    let st = with_env(&[("QIBM_QSH_CMD_OUTPUT", "FILEAPPEND=/home/QSHUSER/o.txt")]);
    let _ = batch(&mut s, st, "echo two");
    let r = run_script(&mut s, "cat /home/QSHUSER/o.txt", "");
    assert_eq!(r.out, "one\ntwo\n");

    let b = batch(&mut s, setup(), "kill -6 $$");
    assert_eq!(b.result.message.id, "QSH0006");
}
