// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 2's operations through a client built from `qap-proto`, every
//! status each can answer: `RunCommand`, `CheckCommand`,
//! `DescribeCommand`, `PasswordStatus`, `TelnetTicket`, and the ticket
//! as the Telnet server takes it (single use, expiry, wrong user, plain
//! Telnet, a plain-text password).

mod common;

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use qap_proto::ops::command::{
    CheckReply, CommandDefinition, CommandReply, CommandText, DescribeCommand, ParmKind, Role, Show,
};
use qap_proto::ops::signon::{PasswordStatus, SignOn, TelnetTicketReply};
use qap_proto::types::ObjectName;
use qap_proto::{Envelope, Op, Secret, Status};

use common::{Controls, Run, cfg, start_with};

/// Version 2, signed on as `user`.
fn signed_on(c: &std::sync::Arc<Controls>, user: &str, password: &str) -> Run {
    let mut r = start_with(cfg(), std::sync::Arc::clone(c));
    r.client.hello_range(1, 2);
    let s = r.client.call(
        Op::SignOn,
        &SignOn {
            user: user.into(),
            password: Secret::from(password),
        },
    );
    assert_eq!(s.status, Status::Ok);
    r
}

fn run(r: &mut Run, command: &str) -> Envelope {
    r.client.call(Op::RunCommand, &CommandText { command: command.into() })
}

fn ids(e: &Envelope) -> Vec<String> {
    e.body_as::<CommandReply>().unwrap().messages.into_iter().map(|m| m.id).collect()
}

fn describe(r: &mut Run, lib: &str, name: &str) -> Envelope {
    r.client.call(
        Op::DescribeCommand,
        &DescribeCommand {
            command: ObjectName {
                library: lib.into(),
                name: name.into(),
            },
        },
    )
}

#[test]
fn run_command_statuses() {
    let c = Controls::new();
    let mut r = signed_on(&c, "BOB", "SECRET1");
    // Ok: the messages the command sent, the job it ran in.
    let ok = run(&mut r, "CRTLIB LIB(MYLIB)");
    assert_eq!(ok.status, Status::Ok);
    let reply: CommandReply = ok.body_as().unwrap();
    assert_eq!(reply.job.name, "QAPCNN");
    assert_eq!(reply.messages[0].id, "CPC2102");
    assert!(!reply.messages[0].help.is_empty(), "second level included");
    // Error: the escape is the message, the body still the reply.
    let again = run(&mut r, "CRTLIB LIB(MYLIB)");
    assert_eq!(again.status, Status::Error);
    assert_eq!(again.message.as_ref().unwrap().id, "CPF2111");
    assert_eq!(ids(&again), ["CPF2111"]);
    let missing = run(&mut r, "CRTLIB");
    assert_eq!((missing.status, ids(&missing)), (Status::Error, vec!["CPD0071".into(), "CPF0006".into()]));
    // Interactive only: Error with its escape.
    let seu = run(&mut r, "STRSEU");
    assert_eq!((seu.status, seu.message.unwrap().id.as_str()), (Status::Error, "CPF0006"));
    assert_eq!(ids(&run(&mut r, "STRSEU"))[0], "CPD0031");
    // NotFound, NotAuthorized (to the command, and to what it names).
    assert_eq!(run(&mut r, "NOSUCH X(1)").status, Status::NotFound);
    assert_eq!(run(&mut r, "DLTUSRPRF USRPRF(ANN)").status, Status::NotAuthorized);
    assert_eq!(run(&mut r, "DSPOBJD OBJ(QSYS/SECRET) OBJTYPE(*FILE)").status, Status::NotAuthorized);
}

#[test]
fn cancel_ends_a_running_command() {
    let c = Controls::new();
    let mut r = signed_on(&c, "BOB", "SECRET1");
    let id = r.client.request(Op::RunCommand, &CommandText { command: "DLYJOB DLY(30)".into() });
    std::thread::sleep(Duration::from_millis(100));
    let at = Instant::now();
    r.client.send(&Envelope::cancel(id));
    let e = r.client.reply(id);
    assert_eq!(e.status, Status::Cancelled);
    assert!(at.elapsed() < Duration::from_secs(5), "ended where it was");
    // Started, so the command's own reply: the job and the end message.
    assert_eq!(e.message.as_ref().unwrap().id, "CPF1907");
    let reply: CommandReply = e.body_as().unwrap();
    assert_eq!(reply.job.name, "QAPCNN");
    let end = reply.messages.last().unwrap();
    assert_eq!(end.id, "CPF1907");
    assert!(!end.help.is_empty(), "second level included");
    // The next command is not cancelled by the old request.
    assert_eq!(run(&mut r, "DLYJOB DLY(1)").status, Status::Ok);
}

#[test]
fn check_command_statuses() {
    let c = Controls::new();
    let mut r = signed_on(&c, "BOB", "SECRET1");
    let check = |r: &mut Run, cmd: &str| r.client.call(Op::CheckCommand, &CommandText { command: cmd.into() });
    let ok = check(&mut r, "CRTLIB LIB(X)");
    assert_eq!(ok.status, Status::Ok);
    assert!(ok.body_as::<CheckReply>().unwrap().messages.is_empty());
    assert!(c.libraries.lock().unwrap().is_empty(), "nothing ran");
    let bad = check(&mut r, "CRTLIB");
    assert_eq!(bad.status, Status::Error);
    assert_eq!(bad.body_as::<CheckReply>().unwrap().messages[0].id, "CPD0071");
    assert_eq!(check(&mut r, "NOSUCH").status, Status::NotFound);
}

#[test]
fn describe_command_statuses() {
    let c = Controls::new();
    let mut r = signed_on(&c, "BOB", "SECRET1");
    // Listed values.
    let d: CommandDefinition = describe(&mut r, "*LIBL", "crtlib").body_as().unwrap();
    assert_eq!((d.name.library.as_str(), d.name.name.as_str()), ("QSYS", "CRTLIB"));
    assert_eq!(d.nodes[1].values, ["*PROD", "*TEST"]);
    // Qualifiers under their owner, a list of listed values.
    let d: CommandDefinition = describe(&mut r, "QSYS", "DSPOBJD").body_as().unwrap();
    assert_eq!((d.nodes[1].role, d.nodes[1].parent), (Role::Qualifier, Some(0)));
    assert_eq!(d.nodes[2].default.as_deref(), Some("*LIBL"));
    assert!(d.nodes[3].max_values > 1 && !d.nodes[3].values.is_empty());
    // An element list, under Additional Parameters.
    let d: CommandDefinition = describe(&mut r, "*LIBL", "CRTPF").body_as().unwrap();
    let elems: Vec<_> = d.nodes.iter().filter(|n| n.parent == Some(3)).collect();
    assert_eq!(elems.len(), 3);
    assert!(elems.iter().all(|n| n.role == Role::Element && n.kind == ParmKind::Integer && n.show == Show::Additional));
    assert!(d.nodes.iter().all(|n| n.parent.is_none_or(|p| p < n.index)));
    // NotFound, NotAuthorized.
    let nf = describe(&mut r, "*LIBL", "NOSUCH");
    assert_eq!((nf.status, nf.message.unwrap().id.as_str()), (Status::NotFound, "CPD0030"));
    assert_eq!(describe(&mut r, "OTHERLIB", "CRTLIB").status, Status::NotFound);
    assert_eq!(describe(&mut r, "*LIBL", "DLTUSRPRF").status, Status::NotAuthorized);
}

#[test]
fn password_status() {
    let c = Controls::new();
    let mut r = signed_on(&c, "WARN", "WARNPW1");
    let s: PasswordStatus = r.client.call(Op::PasswordStatus, &()).body_as().unwrap();
    let now_ms = i64::try_from(c.now()).unwrap() * 1000;
    let left_days = (s.expires.unwrap() - now_ms) / 86_400_000;
    assert!(left_days > 0 && left_days <= i64::from(s.warning_days), "{left_days} of {}", s.warning_days);
    let mut b = signed_on(&c, "BOB", "SECRET1");
    let s: PasswordStatus = b.client.call(Op::PasswordStatus, &()).body_as().unwrap();
    assert_eq!((s.expires, s.warning_days), (None, 7));
}

fn hex(t: &qap_proto::Token) -> String {
    use std::fmt::Write as _;
    t.0.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn ticket(r: &mut Run) -> TelnetTicketReply {
    let e = r.client.call(Op::TelnetTicket, &());
    assert_eq!(e.status, Status::Ok);
    e.body_as().unwrap()
}

#[test]
fn telnet_tickets() {
    let c = Controls::new();
    let mut r = signed_on(&c, "BOB", "SECRET1");
    // Single use.
    let t = ticket(&mut r);
    assert_eq!(t.expires_secs, 60);
    assert!(c.redeem_ticket("BOB", hex(&t.ticket).as_bytes(), true));
    assert!(!c.redeem_ticket("BOB", hex(&t.ticket).as_bytes(), true));
    // Expiry.
    let t = ticket(&mut r);
    c.advance(61);
    assert!(!c.redeem_ticket("BOB", hex(&t.ticket).as_bytes(), true));
    // The wrong user uses it up.
    let t = ticket(&mut r);
    assert!(!c.redeem_ticket("ANN", hex(&t.ticket).as_bytes(), true));
    assert!(!c.redeem_ticket("BOB", hex(&t.ticket).as_bytes(), true));
    // Plain Telnet: refused, and used up all the same.
    let t = ticket(&mut r);
    assert!(!c.redeem_ticket("BOB", hex(&t.ticket).as_bytes(), false));
    assert!(!c.redeem_ticket("BOB", hex(&t.ticket).as_bytes(), true));
    // A plain-text password is never a ticket.
    assert!(!c.redeem_ticket("BOB", b"SECRET1", true));
    // Issue and use are audited; the value never is.
    let audit = c.audit.lock().unwrap().join("\n");
    assert!(audit.contains("PS P BOB") && audit.contains("PS V BOB"));
    assert!(!audit.contains(&hex(&t.ticket)));
}

#[test]
fn telnet_ticket_not_authorized() {
    let c = Controls::new();
    let mut r = signed_on(&c, "BOB", "SECRET1");
    c.auto_signon.store(false, Ordering::SeqCst);
    let e = r.client.call(Op::TelnetTicket, &());
    assert_eq!((e.status, e.message.unwrap().id.as_str()), (Status::NotAuthorized, "TCP1A55"));
    c.auto_signon.store(true, Ordering::SeqCst);
    c.edit("BOB", |u| u.disabled = true);
    assert_eq!(r.client.call(Op::TelnetTicket, &()).status, Status::NotAuthorized);
}

#[test]
fn version_2_operations_need_sign_on() {
    let c = Controls::new();
    let mut r = start_with(cfg(), c);
    r.client.hello_range(1, 2);
    for op in [Op::RunCommand, Op::CheckCommand, Op::DescribeCommand, Op::TelnetTicket, Op::PasswordStatus] {
        let id = r.client.request(op, &CommandText::default());
        assert_eq!(r.client.reply(id).status, Status::NotSignedOn, "{op:?}");
    }
}
