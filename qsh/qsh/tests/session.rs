// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The entry points: the interactive session, startup files, `qsh`
//! arguments, the batch `QSH CMD()` path and the host `Sys`.

mod common;

use std::sync::Arc;

use qsh::session::batch::{self, MsgKind};
use qsh::session::{self, Feed, Interactive, Invocation};
use qsh::sys::{Buffer, Bytes, HostSys, MemSys, Sys};

struct Session {
    i: Interactive,
    out: Buffer,
    err: Buffer,
}

fn session_on(s: &MemSys) -> Session {
    let out = Buffer::new();
    let err = Buffer::new();
    let sh = session::shell_for(
        Arc::new(s.clone()),
        &Invocation::session(),
        Some(Bytes::fd(Vec::new())),
        Some(out.fd()),
        Some(err.fd()),
    );
    Session {
        i: Interactive::new(sh),
        out,
        err,
    }
}

fn session() -> Session {
    session_on(&common::sys())
}

#[test]
fn lines_run_as_they_complete() {
    let mut s = session();
    assert_eq!(s.i.prompt(), "$ ");
    assert_eq!(s.i.feed_line("echo hi"), Feed::Ready);
    assert_eq!(s.out.take(), b"hi\n");
    assert_eq!(s.i.feed_line("if true; then"), Feed::More);
    assert_eq!(s.i.prompt(), "> ");
    assert_eq!(s.i.feed_line("  echo inside"), Feed::More);
    assert_eq!(s.i.feed_line("fi"), Feed::Ready);
    assert_eq!(s.out.take(), b"inside\n");
    assert_eq!(s.i.prompt(), "$ ");
}

#[test]
fn here_documents_and_quotes_continue() {
    let mut s = session();
    assert_eq!(s.i.feed_line("cat <<EOF"), Feed::More);
    assert_eq!(s.i.feed_line("body $HOME"), Feed::More);
    assert_eq!(s.i.feed_line("EOF"), Feed::Ready);
    assert_eq!(s.out.text(), "body /home/QSHUSER\n");
    s.out.take();
    assert_eq!(s.i.feed_line("echo 'open"), Feed::More);
    assert_eq!(s.i.feed_line("close'"), Feed::Ready);
    assert_eq!(s.out.text(), "open\nclose\n");
    s.out.take();
    assert_eq!(s.i.feed_line("echo a \\"), Feed::More);
    assert_eq!(s.i.feed_line("b"), Feed::Ready);
    assert_eq!(s.out.text(), "a b\n");
}

#[test]
fn errors_do_not_end_the_session() {
    let mut s = session();
    assert_eq!(s.i.feed_line("echo )"), Feed::Ready);
    assert!(s.err.text().contains("001-0050"));
    assert_eq!(
        s.i.feed_line("echo ${nosuch?gone}; echo not here"),
        Feed::Ready
    );
    assert!(s.err.text().contains("gone"));
    assert_eq!(s.i.feed_line("shift 9"), Feed::Ready);
    assert_eq!(s.i.feed_line("echo $?; echo still"), Feed::Ready);
    assert_eq!(s.out.text(), "1\nstill\n");
}

#[test]
fn exit_ends_with_trap() {
    let mut s = session();
    s.i.feed_line("trap 'echo bye' EXIT");
    assert_eq!(s.i.feed_line("exit 3"), Feed::Exit(3));
    assert_eq!(s.out.text(), "bye\n");
}

#[test]
fn eof_and_ignoreeof() {
    let mut s = session();
    s.i.feed_line("set -o ignoreeof");
    assert_eq!(s.i.feed_eof(), Feed::Ready);
    s.i.feed_line("set +o ignoreeof; false");
    assert_eq!(s.i.feed_eof(), Feed::Exit(1));
}

#[test]
fn prompt_is_expanded_and_interactive_flag_set() {
    let mut s = session();
    s.i.feed_line("cd /tmp; PS1='$PWD ==> '");
    assert_eq!(s.i.prompt(), "/tmp ==> ");
    s.i.feed_line("case $- in *i*) echo interactive;; esac");
    assert_eq!(s.out.text(), "interactive\n");
}

#[test]
fn aliases_apply_on_the_next_line() {
    let mut s = session();
    s.i.feed_line("alias ll='echo listed'");
    s.i.feed_line("ll");
    assert_eq!(s.out.text(), "listed\n");
}

#[test]
fn background_jobs_are_reported() {
    let mut s = session();
    s.i.feed_line("exitwith 2 &");
    s.i.feed_line("wait_for_it() { :; }");
    let mut notes = String::new();
    for _ in 0..5000 {
        notes = s.i.notices();
        if !notes.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(notes, "[1] + Done(2) exitwith 2\n");
}

#[test]
fn startup_files_run_for_login_and_env() {
    let sys = common::sys();
    sys.write_file("/etc/profile", "echo etc; PROFILE=1\n");
    sys.write_file("/home/QSHUSER/.profile", "echo home; ENV=$HOME/.qshrc\n");
    sys.write_file("/home/QSHUSER/.qshrc", "echo env file\n");
    let mut s = session_on(&sys);
    assert_eq!(s.out.text(), "etc\nhome\nenv file\n");
    s.i.feed_line("echo $PROFILE");
    assert_eq!(s.out.text(), "etc\nhome\nenv file\n1\n");
}

#[test]
fn qsh_arguments_run_scripts() {
    let sys = common::sys();
    sys.write_file("/tmp/s.sh", "echo script $0 $1 $#\n");
    let out = Buffer::new();
    let argv: Vec<String> = ["qsh", "/tmp/s.sh", "a", "b"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let inv = Invocation::parse(&argv).unwrap();
    let st = session::run(Arc::new(sys.clone()), &inv, None, Some(out.fd()), None);
    assert_eq!(st, 0);
    assert_eq!(out.text(), "script /tmp/s.sh a 2\n");

    let out = Buffer::new();
    let argv: Vec<String> = ["qsh", "-s", "x"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let inv = Invocation::parse(&argv).unwrap();
    let st = session::run(
        Arc::new(sys),
        &inv,
        Some(Bytes::fd(b"echo from stdin $1\nexit 4\n".to_vec())),
        Some(out.fd()),
        None,
    );
    assert_eq!(st, 4);
    assert_eq!(out.text(), "from stdin x\n");
}

#[test]
fn batch_output_goes_where_the_variable_says() {
    let sys = common::sys();
    sys.set_env("QIBM_QSH_CMD_OUTPUT", "FILE=/tmp/out.txt");
    let job_out = Buffer::new();
    let r = batch::run_cmd(
        Arc::new(sys.clone()),
        "echo to file; nosuch",
        None,
        Some(job_out.fd()),
    );
    assert_eq!(r.status, 127);
    assert_eq!(r.message.id, "QSH0005");
    assert_eq!(r.message.kind, MsgKind::Completion);
    assert_eq!(r.message.data, 127);
    let text = String::from_utf8(sys.read_file("/tmp/out.txt").unwrap()).unwrap();
    assert_eq!(text, "to file\nqsh: 001-0014 Cannot find command nosuch.\n");
    assert!(job_out.contents().is_empty());

    sys.set_env("QIBM_QSH_CMD_OUTPUT", "FILEAPPEND=/tmp/out.txt");
    sys.set_env("QIBM_QSH_CMD_ESCAPE_MSG", "Y");
    let r = batch::run_cmd(Arc::new(sys.clone()), "echo more; exit 2", None, None);
    assert_eq!(r.message.kind, MsgKind::Escape);
    let text = String::from_utf8(sys.read_file("/tmp/out.txt").unwrap()).unwrap();
    assert!(text.ends_with("more\n"));

    sys.set_env("QIBM_QSH_CMD_OUTPUT", "NONE");
    let r = batch::run_cmd(Arc::new(sys.clone()), "echo gone", None, Some(job_out.fd()));
    assert_eq!(r.status, 0);
    assert!(job_out.contents().is_empty());

    sys.unset_env("QIBM_QSH_CMD_OUTPUT");
    let r = batch::run_cmd(
        Arc::new(sys),
        "echo to job; kill $$",
        None,
        Some(job_out.fd()),
    );
    assert_eq!(job_out.text(), "to job\n");
    assert_eq!(r.message.id, "QSH0006");
    assert_eq!(r.message.data, 6);
}

#[test]
fn signals_are_recorded_for_traps() {
    let sys = common::sys();
    let o = common::run_on(&sys, "trap 'echo got' INT; kill -INT $$; trap - INT", "");
    assert_eq!(o.out, "got\n");
    assert_eq!(
        sys.action(qsh::sys::mem::MEM_PID, 4),
        qsh::sys::SigAction::Default
    );
}

#[test]
fn host_sys_runs_against_a_directory() {
    let dir = std::env::temp_dir().join(format!("qsh-host-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("work")).unwrap();
    std::fs::write(dir.join("work/a.txt"), "alpha\n").unwrap();
    std::fs::write(dir.join("work/b.txt"), "beta\n").unwrap();
    let host = HostSys::rooted(&dir);
    host.register(
        "/bin/cat",
        Arc::new(|cx: &mut qsh::sys::UtilCtx<'_>| {
            if cx.argv.len() == 1
                && let Some(fd) = cx.fd(0)
            {
                let data = qsh::sys::stream::read_to_end(&**fd).unwrap_or_default();
                let _ = cx.write(1, &data);
            }
            for f in &cx.argv[1..] {
                let p = cx.path(f);
                if let Ok(fd) = cx.sys.open(
                    &p,
                    qsh::sys::OpenFlags {
                        read: true,
                        ..Default::default()
                    },
                    0,
                ) {
                    let data = qsh::sys::stream::read_to_end(&*fd).unwrap_or_default();
                    let _ = cx.write(1, &data);
                }
            }
            0
        }),
    );
    let sys: Arc<dyn Sys> = Arc::new(host);
    let out = Buffer::new();
    let mut inv = Invocation::command(
        "cd /work; echo *.txt; cat a.txt b.txt; echo made > c.txt; test -f c.txt && echo exists; cat <c.txt",
    );
    inv.arg0 = "qsh".to_string();
    let mut sh = session::shell_for(sys, &inv, None, Some(out.fd()), None);
    sh.run_text("PATH=/bin", 1).unwrap();
    let st = session::run_text_to_end(&mut sh, inv.command.as_deref().unwrap());
    assert_eq!(st, 0);
    assert_eq!(out.text(), "a.txt b.txt\nalpha\nbeta\nexists\nmade\n");
    assert_eq!(
        std::fs::read_to_string(dir.join("work/c.txt")).unwrap(),
        "made\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
