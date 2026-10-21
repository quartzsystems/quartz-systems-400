// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Program messages from IR: sending to the queues, receiving with
//! substituted text, removing, and escape messages as exceptions.

use mi::{ClearMode, Cond, Program, QueueRef};
use qir::MsgType;

#[test]
fn messages_flow_between_entries() {
    let mut p = Program::from_text(
        r#"
module MSG
static $text: char(80)
static $id: char(7)
static $key: char(4)
static $key2: char(4)
static $sev: i32
static $data: char(10) inz("CUSTOMER")
static $got: i32
proc SUB() {
L0:
  sndpgmmsg msgid("MCH3401") msgf("QCPFMSG") data($data) type(*DIAG) to(*PRV) key($key)
  sndpgmmsg msgid("CPF9897") msgf("QCPFMSG", "QSYS") data("completed") type(*COMP) to(*PRV)
  sndpgmmsg msgid("CPF9897") msgf("QCPFMSG") data("to myself") type(*INFO) to(*SAME)
  sndpgmmsg msgid("CPF9898") msgf("QCPFMSG") data("external") type(*INFO) to(*EXT)
  ret
}
proc MSG() entry {
L0:
  callb SUB()
  %g = rcvmsg pgmq(*SAME) type(*DIAG) msg($text) msgid($id) keyout($key2) sev($sev)
  %g1 = cvt.i32 %g
  store $got, %g1
  dsply $text
  dsply $id
  dsply $sev
  %c = rcvmsg pgmq(*SAME) type(*COMP) rmv msg($text)
  dsply $text
  %none = rcvmsg pgmq(*SAME) type(*COMP) msg($text)
  %n1 = cvt.i32 %none
  dsply %n1
  rmvmsg pgmq(*SAME) key($key)
  %again = rcvmsg pgmq(*SAME) type(*ANY) msg($text)
  %a1 = cvt.i32 %again
  dsply %a1
  ret
}
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    p.call_entry().unwrap();
    let j = p.job();
    assert_eq!(
        j.console,
        vec![
            "Object CUSTOMER was not found.",
            "MCH3401",
            "40",
            "completed",
            "0",
            "0"
        ]
    );
    assert_eq!(
        p.static_bytes("key").unwrap(),
        p.static_bytes("key2").unwrap(),
        "the key sent is the key received"
    );
    let j = p.job();
    assert!(!j.pending());
    assert_eq!(j.ext_msgq.msgs.len(), 1);
    assert_eq!(j.ext_msgq.msgs[0].text, "external");
    assert_eq!(j.ext_msgq.msgs[0].msg_type, MsgType::Info);
    assert!(j.queue(QueueRef::Same).is_none(), "no entry is running");
}

#[test]
fn escape_messages_percolate_to_their_target() {
    let mut p = Program::from_text(
        r#"
module ESC
static $log: char(20)
proc INNER() {
L0:
  monitor M, *ALL -> MINE
  sndpgmmsg msgid("CPF9898") msgf("QCPFMSG") data("fail up") type(*ESCAPE) to(*PRV)
  mov $log, "continued"
  endmon M
  ret
MINE:
  mov $log, "wrongly caught"
  ret
}
proc MIDDLE() {
L0:
  callb INNER()
  mov $log, "middle went on"
  ret
}
proc ESC() entry {
L0:
  monitor OUT, CPF9898 -> CAUGHT
  callb MIDDLE()
  mov $log, "not reached"
  endmon OUT
  ret
CAUGHT:
  mov $log, "caught above"
  ret
}
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    p.call_entry().unwrap();
    assert_eq!(
        ccsid::decode_trimmed(&p.static_bytes("log").unwrap()),
        "caught above"
    );
    let j = p.job();
    assert!(!j.pending());
    assert_eq!(j.handled.len(), 1);
    assert_eq!(j.handled[0].msgid, "CPF9898");
    assert_eq!(j.handled[0].cond, None);
    assert_eq!(j.handled[0].depth, 2, "sent to MIDDLE, two entries deep");
    assert!(!j.handled[0].is_file_error());
}

#[test]
fn machine_conditions_are_escape_messages() {
    let mut p = Program::from_text(
        "module MC\nstatic $p: packed(3,0)\nstatic $text: char(60)\nproc MC() entry {\nL0:\n  monitor M, MCH1211 -> H\n  dec.div $p, 1, 0 prec(3,0)\n  endmon M\n  ret\nH:\n  %g = rcvmsg pgmq(*SAME) type(*ESCAPE) msg($text)\n  dsply $text\n  ret\n}\n",
    )
    .unwrap();
    p.call_entry().unwrap();
    let j = p.job();
    assert_eq!(j.console, vec!["Division by zero."]);
    assert_eq!(j.handled[0].cond, Some(Cond::Mch1211));
    let n = j
        .remove_messages(QueueRef::Ext, None, ClearMode::All)
        .unwrap();
    assert_eq!(n, 0);
}
