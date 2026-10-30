// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An exit program built from IR, registered on `QIBM_QZRC_RMT`, called
//! by `call_exit` in a job: it sees the `CZRC0100` structure at IBM's
//! offsets and its return code decides. A missing program and one that
//! ends in an exception refuse; a point with no programs allows.

use ile::{BindInput, BindOptions, CreateData, ModuleObject, PgmKind};
use mi::{Job, MemSource};
use regfac::formats::host::{self, Remote};
use regfac::formats::tcp::{self, Validate, app, op};
use regfac::{AddOpts, ExitProgram, ExitRequest, MemRegistry, call_exit};

const EXIT: &str = r#"
module RMTEXIT lang(QIR) ccsid(37) actgrp(*CALLER)
type Czrc = struct(6056) { user: char(10) @0, server: char(10) @10, fmt: char(8) @20, func: i32 @28, obj: char(10) @32, lib: char(10) @42, len: i32 @52, other: char(6000) @56 }
proc RMTEXIT($rc: char(1), $info: Czrc) entry {
  auto $cmd: char(8)
L0:
  dsply $info.user
  dsply $info.server
  dsply $info.fmt
  %f = load $info.func
  dsply %f
  %n = load $info.len
  dsply %n
  str.subst $cmd, $info.other, 1, 8
  dsply $cmd
  %deny = str.cmp.eq $info.user, "DENYME"
  brif %deny, NO, CHK
CHK:
  %pc = cmp.eq %f, 4099
  brif %pc, PGM, YES
PGM:
  dsply $info.obj
  %bad = str.cmp.eq $info.obj, "DANGER"
  brif %bad, NO, YES
YES:
  mov $rc, "1"
  ret
NO:
  mov $rc, "0"
  ret
}
"#;

const FAILS: &str = r#"
module BADEXIT lang(QIR) ccsid(37) actgrp(*CALLER)
proc BADEXIT($rc: char(1), $info: char(56)) entry {
L0:
  %s = const.i32 7
  %z = const.i32 0
  %q = div %s, %z
  dsply %q
  mov $rc, x"F1"
  ret
}
"#;

const VALIDATE: &str = r"
module FTPEXIT lang(QIR) ccsid(37) actgrp(*CALLER)
proc FTPEXIT($app: i32, $op: i32, $user: char(10), $ip: char(15), $iplen: i32, $info: char(64), $infolen: i32, $allow: i32) entry {
L0:
  %a = load $app
  dsply %a
  %o = load $op
  dsply %o
  dsply $user
  %l = load $iplen
  dsply %l
  %del = cmp.eq %o, 5
  brif %del, NO, YES
YES:
  store $allow, 1
  ret
NO:
  store $allow, 0
  ret
}
";

fn program(text: &str, name: &str) -> ile::ProgramObject {
    let m = ModuleObject::compile(qir::parse(text).unwrap(), CreateData::default(), 0)
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    let (object, _) = ile::bind(BindInput {
        modules: vec![(m, "QSTEST".into())],
        bnddirs: Vec::new(),
        lookup: &|_| None,
        options: BindOptions::new(name, PgmKind::Pgm),
    })
    .unwrap();
    object
}

fn job() -> Box<Job> {
    let mut src = MemSource::default();
    src.add("QSTEST", program(EXIT, "RMTEXIT"));
    src.add("QSTEST", program(FAILS, "BADEXIT"));
    src.add("QSTEST", program(VALIDATE, "FTPEXIT"));
    Job::boxed(Box::new(src))
}

fn registered(point: &str, format: &str, pgm: &str) -> MemRegistry {
    let mut reg = MemRegistry::shipped();
    reg.registry
        .add_program(
            point,
            format,
            ExitProgram {
                number: 1,
                pgm: pgm.into(),
                lib: "QSTEST".into(),
                ..ExitProgram::default()
            },
            &AddOpts::default(),
        )
        .unwrap();
    reg
}

fn command(user: &str, cmd: &str) -> ExitRequest {
    let r = Remote::Command {
        ccsid: 37,
        text: regfac::text::ebcdic(cmd),
    };
    ExitRequest::host("CZRC0100", host::czrc0100(user, &r))
}

#[test]
fn the_remote_command_exit_program_sees_czrc0100_and_decides() {
    let mut j = job();
    let mut reg = registered("QIBM_QZRC_RMT", "CZRC0100", "RMTEXIT");

    let a = call_exit(
        &mut reg,
        &mut *j,
        "QIBM_QZRC_RMT",
        &command("QSTEST", "DSPLIB QGPL"),
    );
    assert!(a.allow, "{:?}", a.called);
    assert_eq!(a.called.len(), 1);
    assert_eq!(a.called[0].rc, "1");
    assert_eq!(
        j.console,
        vec!["QSTEST", "*RMTSRV", "CZRC0100", "4098", "11", "DSPLIB Q"],
        "the program read the structure at IBM's offsets"
    );
    let info = &a.outputs[1];
    assert_eq!(info.len(), host::CZRC0100_DECLARED);
    assert_eq!(&info[32..36], &37i32.to_be_bytes(), "the command's CCSID");

    j.console.clear();
    let d = call_exit(
        &mut reg,
        &mut *j,
        "QIBM_QZRC_RMT",
        &command("DENYME", "DLTLIB QGPL"),
    );
    assert!(!d.allow);
    assert_eq!(d.called[0].rc, "0");
    assert_eq!(reg.refusals.len(), 1, "a refusal is reported for the audit");
    assert_eq!(reg.refusals[0].pgm, "RMTEXIT");

    // A program call: the object name is at 32.
    j.console.clear();
    let call = |obj: &str| {
        ExitRequest::host(
            "CZRC0100",
            host::czrc0100(
                "QSTEST",
                &Remote::Program {
                    program: (obj.into(), "QGPL".into()),
                    parms: Vec::new(),
                },
            ),
        )
    };
    assert!(call_exit(&mut reg, &mut *j, "QIBM_QZRC_RMT", &call("SAFE")).allow);
    assert!(!call_exit(&mut reg, &mut *j, "QIBM_QZRC_RMT", &call("DANGER")).allow);
    assert!(j.console.contains(&"DANGER".to_string()));
    assert!(j.console.contains(&"4099".to_string()));
}

#[test]
fn missing_and_failing_programs_refuse() {
    let mut j = job();
    let mut missing = registered("QIBM_QZRC_RMT", "CZRC0100", "NOSUCHPGM");
    let a = call_exit(
        &mut missing,
        &mut *j,
        "QIBM_QZRC_RMT",
        &command("QSTEST", "X"),
    );
    assert!(!a.allow);
    assert!(a.called[0].failed.is_some());
    assert_eq!(missing.refusals.len(), 1);

    let mut failing = registered("QIBM_QZRC_RMT", "CZRC0100", "BADEXIT");
    let b = call_exit(
        &mut failing,
        &mut *j,
        "QIBM_QZRC_RMT",
        &command("QSTEST", "X"),
    );
    assert!(!b.allow, "an exception in the exit program refuses");
    let why = b.called[0].failed.clone().unwrap_or_default();
    assert!(why.contains("exception"), "{why}");
    assert!(!j.pending(), "the exception does not escape to the server");

    // No program: allowed, nothing called.
    let mut none = MemRegistry::shipped();
    let c = call_exit(&mut none, &mut *j, "QIBM_QZRC_RMT", &command("QSTEST", "X"));
    assert!(c.allow && c.called.is_empty());
}

#[test]
fn the_ftp_request_exit_program_gets_vlrq0100() {
    let mut j = job();
    let mut reg = registered("QIBM_QTMF_SERVER_REQ", "VLRQ0100", "FTPEXIT");
    let v = |o: i32| {
        ExitRequest::Validate(Validate {
            app: app::FTP_SERVER,
            op: o,
            user: "QSTEST".into(),
            remote: "10.1.2.3".into(),
            info: regfac::text::ebcdic("/home/qstest/a.txt"),
        })
    };
    let a = call_exit(&mut reg, &mut *j, "QIBM_QTMF_SERVER_REQ", &v(op::SEND_FILE));
    assert!(a.allow);
    assert_eq!(j.console, vec!["1", "6", "QSTEST", "8"]);
    assert_eq!(tcp::VLRQ0100.parms.len(), a.outputs.len());
    let d = call_exit(
        &mut reg,
        &mut *j,
        "QIBM_QTMF_SERVER_REQ",
        &v(op::DELETE_FILE),
    );
    assert!(!d.allow);
    assert_eq!(d.called[0].rc, "0");
}
