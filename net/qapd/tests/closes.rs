// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every close is logged once at `info`: the connection, its job, the
//! reason (the I/O error's text, a panic on any of the connection's
//! threads) and the requests outstanding. A long request is not
//! idleness. The tests run one at a time: [`qapd::exiting`] logs every
//! live connection of the process.

mod common;

use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read, Write};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use qap_proto::ops::command::CommandText;
use qap_proto::ops::connection::Hello;
use qap_proto::{Envelope, Op, Status};
use qapd::frames::frame;
use qapd::testing::{Crash, MemHost, System, identity, next_job};
use qapd::{Closed, Config, Ending, Transport};

use common::log::{Line, capture, lines};
use common::{Run, cfg, start};

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// Capture, and one test at a time.
fn serial() -> MutexGuard<'static, ()> {
    capture(None);
    ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Connection `job`'s one close line.
fn line(job: u32) -> Line {
    let l = lines(job);
    assert_eq!(l.len(), 1, "one close line: {l:?}");
    let l = l.into_iter().next().unwrap();
    assert_eq!(l["message"], "QAP connection closed");
    assert_eq!((l["level"].as_str(), l["target"].as_str()), ("INFO", "qapd"));
    assert_eq!(l["job"], format!("{job:06}/QUSER/QAPCNN"));
    l
}

fn text(c: &str) -> CommandText {
    CommandText { command: c.into() }
}

fn signed_on(cfg: Config, version: u16) -> Run {
    let mut r = start(cfg);
    r.client.hello_range(1, version);
    r.client.sign_on("BOB", "SECRET1");
    r
}

#[test]
fn a_client_close_names_what_was_outstanding() {
    let _one = serial();
    let mut r = signed_on(cfg(), 1);
    r.c.gate.set(false);
    let waiting = r.client.request(Op::SystemInfo, &());
    thread::sleep(Duration::from_millis(100));
    drop(r.client);
    assert_eq!(r.server.join().unwrap(), Closed::Peer);
    r.c.gate.set(true);
    let l = line(r.job);
    assert_eq!(l["reason"], "client closed");
    assert_eq!(l["outstanding"], format!("[{waiting}]"));
    assert_eq!(l["profile"], "BOB");
    assert!(l["peer"].starts_with("127.0.0.1:"), "{l:?}");
}

/// A command running longer than the idle limit keeps the connection:
/// before, the idle clock counted only frames read, so the client saw
/// the connection close under its request without a word.
#[test]
fn a_long_command_is_not_idleness() {
    let _one = serial();
    let mut r = signed_on(
        Config {
            idle_close: Duration::from_millis(300),
            ..cfg()
        },
        2,
    );
    let id = r.client.request(Op::RunCommand, &text("DLYJOB DLY(1)"));
    let rep = r.client.recv().expect("the reply, not a close");
    assert_eq!((rep.request_id, rep.status), (id, Status::Ok));
    // Idle once nothing is outstanding.
    let since = Instant::now();
    assert!(r.client.closed());
    assert!(since.elapsed() >= Duration::from_millis(200));
    assert_eq!(r.server.join().unwrap(), Closed::Idle);
    let l = line(r.job);
    assert_eq!((l["reason"].as_str(), l["outstanding"].as_str()), ("idle timeout", "[]"));
}

#[test]
fn protocol_errors_and_ends_are_logged() {
    let _one = serial();
    let mut r = start(cfg());
    r.client.hello();
    r.client.raw(&u32::try_from(qap_proto::MAX_FRAME + 1).unwrap().to_le_bytes());
    assert_eq!(r.server.join().unwrap(), Closed::BadFrame);
    assert_eq!(line(r.job)["reason"], "protocol error: bad frame");

    let mut r = start(cfg());
    r.client.hello();
    r.client.set_next_id(1);
    r.client.call(Op::Hello, &Hello::default());
    assert_eq!(r.server.join().unwrap(), Closed::Refused);
    assert_eq!(line(r.job)["reason"], "protocol error: Hello refused");

    for (how, reason) in [(Ending::Server, "server ending"), (Ending::Operator, "job ended by an operator")] {
        let mut r = signed_on(cfg(), 1);
        r.c.end(how);
        // The event, then the close.
        while r.client.recv().is_some() {}
        r.server.join().unwrap();
        assert_eq!(line(r.job)["reason"], reason);
    }
}

/// What a scripted stream does on each read.
enum Step {
    Bytes(Vec<u8>),
    Fail(&'static str),
    Panic(&'static str),
}

/// A stream that reads a script and swallows what is written.
struct Script(VecDeque<Step>);

impl Read for Script {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.pop_front() {
            Some(Step::Bytes(b)) => {
                buf[..b.len()].copy_from_slice(&b);
                Ok(b.len())
            }
            Some(Step::Fail(e)) => Err(io::Error::other(e)),
            Some(Step::Panic(p)) => panic!("{p}"),
            None => {
                thread::sleep(Duration::from_millis(5));
                Err(ErrorKind::WouldBlock.into())
            }
        }
    }
}

impl Write for Script {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Transport for Script {}

fn hello_bytes() -> Vec<u8> {
    let h = Hello {
        min_version: 1,
        max_version: 1,
        ..Hello::default()
    };
    frame(&Envelope::request(1, Op::Hello, &h)).unwrap()
}

/// Serve `steps` as a new connection job; why it closed, and its job.
fn scripted(steps: Vec<Step>) -> (Closed, u32) {
    let job = next_job();
    let mut host = MemHost::new(System::new(), job);
    let why = qapd::serve(Script(steps.into()), &mut host, cfg(), identity(job, None));
    (why, job)
}

#[test]
fn an_io_error_gives_its_text() {
    let _one = serial();
    let (why, job) = scripted(vec![Step::Bytes(hello_bytes()), Step::Fail("wire cut")]);
    assert_eq!(why, Closed::Broken("read: wire cut".into()));
    let l = line(job);
    assert_eq!(l["reason"], "I/O error: read: wire cut");
    assert_eq!(l["peer"], "-");
}

#[test]
fn a_panic_in_the_pump_is_the_reason() {
    let _one = serial();
    let (why, job) = scripted(vec![Step::Bytes(hello_bytes()), Step::Panic("stream exploded")]);
    let Closed::Panic(p) = &why else {
        panic!("{why:?}");
    };
    assert!(p.starts_with("thread qap-pump: stream exploded at "), "{p}");
    assert_eq!(line(job)["reason"], format!("panic: {p}"));
}

/// A panic where `crash` says, on the request `op` makes: the
/// connection closes, its reason the panic, the request outstanding.
fn crash(crash: Crash, version: u16, op: Op, body: &CommandText, thread: &str) {
    let mut r = signed_on(cfg(), version);
    *r.c.crash.lock().unwrap() = Some(crash);
    let id = if op == Op::RunCommand {
        r.client.request(op, body)
    } else {
        r.client.request(op, &())
    };
    assert!(r.client.closed(), "no reply after a panic");
    let why = r.server.join().unwrap();
    let Closed::Panic(p) = &why else {
        panic!("{why:?}");
    };
    assert!(p.contains(thread) && p.contains(&format!("test crash at {crash:?}")), "{p}");
    let l = line(r.job);
    assert_eq!(l["reason"], format!("panic: {p}"));
    assert_eq!(l["outstanding"], format!("[{id}]"));
}

#[test]
fn a_panic_on_any_thread_is_the_reason() {
    let _one = serial();
    let none = text("");
    crash(Crash::SystemInfo, 1, Op::SystemInfo, &none, "thread qap-worker-");
    crash(Crash::Ping, 1, Op::Ping, &none, "thread qap-lanes");
    crash(Crash::Command, 2, Op::RunCommand, &text("DLYJOB DLY(0)"), "thread ");
}

#[test]
fn a_process_ending_logs_its_connections_once() {
    let _one = serial();
    let mut r = signed_on(cfg(), 1);
    qapd::exiting(&Closed::Exit("storage service lost".into()));
    assert_eq!(line(r.job)["reason"], "job ending: storage service lost");
    // The process would be gone; here the connection goes on, and its
    // close is not logged again.
    assert_eq!(r.client.call(Op::Ping, &()).status, Status::Ok);
    drop(r.client);
    assert_eq!(r.server.join().unwrap(), Closed::Peer);
    assert_eq!(lines(r.job).len(), 1);
}
