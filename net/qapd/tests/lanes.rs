// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The lanes of docs/QAP.md, Multiplexing: commands one at a time in
//! arrival order, everything else beside them without waiting.

mod common;

use std::time::{Duration, Instant};

use qap_proto::ops::command::{CommandReply, CommandText};
use qap_proto::{Kind, Op, Status};

use common::{cfg, start};

fn text(c: &str) -> CommandText {
    CommandText { command: c.into() }
}

#[test]
fn a_long_command_holds_up_nothing_else() {
    let mut r = start(cfg());
    r.client.hello_range(1, 2);
    r.client.sign_on("BOB", "SECRET1");
    let at = Instant::now();
    let long = r.client.request(Op::RunCommand, &text("DLYJOB DLY(3)"));
    std::thread::sleep(Duration::from_millis(50));
    for op in [Op::Ping, Op::SystemInfo, Op::PasswordStatus] {
        let e = r.client.call(op, &());
        assert_eq!(e.status, Status::Ok, "{op:?}");
    }
    assert!(
        at.elapsed() < Duration::from_secs(2),
        "answered while the command ran"
    );
    let e = r.client.reply(long);
    assert_eq!(e.status, Status::Ok);
    assert!(at.elapsed() >= Duration::from_secs(3));
}

#[test]
fn commands_run_in_arrival_order() {
    let mut r = start(cfg());
    r.client.hello_range(1, 2);
    r.client.sign_on("BOB", "SECRET1");
    let first = r.client.request(Op::RunCommand, &text("DLYJOB DLY(1)"));
    let second = r.client.request(Op::RunCommand, &text("CRTLIB LIB(AFTER)"));
    // The second waits for the first: its reply comes after.
    let a = r.client.recv().unwrap();
    assert_eq!(
        (a.kind, a.request_id, a.status),
        (Kind::Reply, first, Status::Ok)
    );
    let b = r.client.reply(second);
    assert_eq!(b.status, Status::Ok);
    let reply: CommandReply = b.body_as().unwrap();
    assert_eq!(reply.messages[0].id, "CPC2102");
    assert_eq!(*r.c.libraries.lock().unwrap(), ["AFTER"]);
}
