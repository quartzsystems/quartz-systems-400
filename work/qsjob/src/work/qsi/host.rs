// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system as the QSI server's families ask it, for one session's
//! thread over a link of its own (`hello_thread`): the storage service
//! for what it keeps (`stg::qsi`), the state file and the reference code
//! history `slic` writes, `slic` itself for function 21, and the console
//! source (`console.rs`) for the console. The keeper of the password and
//! the pins is the service too, over a thread of its own ([`Svc`]).

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Mutex, PoisonError, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use ipc::chan::Chan;
use qsi::base::{
    self, Identity, Keylock, Level, Power, PowerAction, PowerState, RefcodeList, Space, State,
    SystemDump, Time,
};
use qsi::host::console::{Capture, In, Pieces};
use qsi::service;
use qsisess::Note;
use qsisvr::trust::{Attempt, Failure, Keeper};
use qsisvr::{Flow, Host, Opener, Pipe, Refusal, refusal};
use qsitrust::Fingerprint;
use stg::qsi::{QsiOp, QsiReply};
use stg::{Client, Link, Shared};

use super::console;

/// A thread's link of job `number` in process `pid`.
pub fn thread_link(number: u32, pid: u32) -> Result<Shared, String> {
    let chan = Chan::connect(Path::new(stg::SOCKET)).map_err(|e| e.to_string())?;
    let mut link = Link::over(chan);
    link.with(|l| l.hello_thread(number, pid))?;
    Ok(Rc::new(RefCell::new(link)))
}

/// Ask the service.
pub fn ask(link: &Shared, op: QsiOp) -> QsiReply {
    link.borrow_mut().with(|l| l.qsi(op))
}

/// A refusal as the families give it.
pub fn notes(r: QsiReply) -> Refusal {
    match r {
        QsiReply::Refused { notes } => notes
            .into_iter()
            .map(|(id, text)| Note::new(&id, &text))
            .collect(),
        other => refusal("CPF9898", &format!("An answer not expected: {other:?}")),
    }
}

/// The state file `slic` writes at each code.
pub fn status() -> guestagent::status::Status {
    guestagent::status::Status::parse(
        &std::fs::read_to_string(guestagent::status::PATH).unwrap_or_default(),
    )
}

/// This IPL's codes, oldest first.
pub fn codes() -> Vec<base::Refcode> {
    qsisvr::history::parse(&std::fs::read_to_string(qsisvr::history::PATH).unwrap_or_default())
}

/// The state, from the state file and the history.
pub fn state() -> State {
    let s = status();
    let power = match s.state.as_str() {
        "complete" => PowerState::Running,
        "ending" => PowerState::Ending,
        "down" => PowerState::Halted,
        _ => PowerState::Starting,
    };
    let refcode = codes().pop().or_else(|| {
        (!s.src.is_empty()).then(|| base::Refcode {
            seq: 0,
            code: s.src.clone(),
            words: Vec::new(),
            text: s.src_text.clone(),
            time_ms: s.src_time,
        })
    });
    State {
        power,
        refcode,
        release: release::RELEASE.to_string(),
        slot: if s.ipl_type == "D" {
            "D".into()
        } else {
            s.slot.clone()
        },
        keylock: Some(if s.mode == "manual" {
            Keylock::Manual
        } else {
            Keylock::Normal
        }),
    }
}

/// How long `slic` has to answer a function 21.
const PANEL_WAIT: Duration = Duration::from_secs(10);

/// Function 21 through `slic` (`procs::qsipanel` there): the request
/// file, `SIGUSR1`, then its answer.
fn function_21(user: &str) -> Result<(), Refusal> {
    const REQUEST: &str = "/run/qs400/qsipanel";
    const ANSWER: &str = "/run/qs400/qsipanel.answer";
    let _ = std::fs::remove_file(ANSWER);
    std::fs::write(REQUEST, format!("21 {user}\n"))
        .map_err(|e| refusal("CPF9898", &format!("The request to the LIC: {e}")))?;
    rustix::process::kill_process(rustix::process::Pid::INIT, rustix::process::Signal::USR1)
        .map_err(|e| refusal("CPF9898", &format!("The request to the LIC: {e}")))?;
    let until = Instant::now() + PANEL_WAIT;
    while Instant::now() < until {
        if let Ok(a) = std::fs::read_to_string(ANSWER) {
            let _ = std::fs::remove_file(ANSWER);
            let a = a.trim();
            if a == "ok" {
                return Ok(());
            }
            let (id, text) = a.split_once(' ').unwrap_or((a, ""));
            // The partition scope's identifiers as the panel family's.
            let id = match id {
                "QSI0021" => qsi::host::panel::msgid::NEEDS_MANUAL,
                "QSI0022" => qsi::host::panel::msgid::NOT_NOW,
                other => other,
            };
            return Err(refusal(id, text));
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = std::fs::remove_file(REQUEST);
    Err(refusal(
        qsi::host::panel::msgid::NOT_NOW,
        "The Licensed Internal Code did not answer function 21 in time.",
    ))
}

/// The system for one session.
pub struct JobHost {
    link: Shared,
}

impl JobHost {
    /// Over `link`.
    #[must_use]
    pub fn new(link: Shared) -> Self {
        Self { link }
    }
}

impl Host for JobHost {
    fn identity(&mut self) -> Result<Identity, Refusal> {
        match ask(&self.link, QsiOp::Identify) {
            QsiReply::Identify { identity } => Ok(identity),
            r => Err(notes(r)),
        }
    }

    fn state(&mut self) -> Result<State, Refusal> {
        Ok(state())
    }

    fn power(&mut self, power: &Power, user: &str) -> Result<(), Refusal> {
        let op = QsiOp::Power {
            immediate: power.immediate,
            delay: power.delay,
            restart: power.action == PowerAction::Restart,
            user: user.into(),
        };
        match ask(&self.link, op) {
            QsiReply::Done => Ok(()),
            r => Err(notes(r)),
        }
    }

    fn refcodes(&mut self, before: Option<u64>, limit: u32) -> Result<RefcodeList, Refusal> {
        Ok(qsisvr::history::page(&codes(), before, limit))
    }

    fn time(&mut self) -> Result<Time, Refusal> {
        match ask(&self.link, QsiOp::Time) {
            QsiReply::Time { time } => Ok(time),
            r => Err(notes(r)),
        }
    }

    fn fix_level(&mut self) -> Result<Level, Refusal> {
        match ask(&self.link, QsiOp::FixLevel) {
            QsiReply::Level { level } => Ok(level),
            r => Err(notes(r)),
        }
    }

    fn dumps(&mut self) -> Result<Vec<SystemDump>, Refusal> {
        match ask(&self.link, QsiOp::Dumps) {
            QsiReply::Dumps { list } => Ok(list),
            r => Err(notes(r)),
        }
    }

    fn take_dump(&mut self, why: &str, user: &str) -> Result<String, Refusal> {
        let op = QsiOp::TakeDump {
            why: why.into(),
            user: user.into(),
        };
        match ask(&self.link, op) {
            QsiReply::Name { name } => Ok(name),
            r => Err(notes(r)),
        }
    }

    fn delete_dump(&mut self, name: &str, user: &str) -> Result<(), Refusal> {
        let op = QsiOp::DeleteDump {
            name: name.into(),
            user: user.into(),
        };
        match ask(&self.link, op) {
            QsiReply::Done => Ok(()),
            r => Err(notes(r)),
        }
    }

    fn dump_space(&mut self) -> Result<Space, Refusal> {
        match ask(&self.link, QsiOp::DumpSpace) {
            QsiReply::Space { space } => Ok(space),
            r => Err(notes(r)),
        }
    }

    fn problems(
        &mut self,
        request: &service::Request,
        user: &str,
    ) -> Result<service::Reply, Refusal> {
        let op = QsiOp::Service {
            op: qsi::Message::op(request),
            body: qsi::Message::encode(request),
            user: user.into(),
        };
        match ask(&self.link, op) {
            QsiReply::Body { body } => {
                match <service::Reply as qsi::Message>::decode(qsi::Message::op(request), &body) {
                    Ok(Some(r)) => Ok(r),
                    _ => Err(refusal(
                        "CPF9898",
                        "The problem log's answer does not read.",
                    )),
                }
            }
            r => Err(notes(r)),
        }
    }

    fn service_tools(&mut self, user: &str) -> Result<(), Refusal> {
        function_21(user)
    }

    fn screen(&mut self) -> Result<Capture, Refusal> {
        console::screen().map_err(|r| refusal(r.msgid, &r.text))
    }
}

/// The console's viewer as a stream's far end.
struct ConsolePipe {
    viewer: console::Viewer,
    pieces: Pieces,
    over: bool,
}

impl Pipe for ConsolePipe {
    fn poll(&mut self) -> Flow {
        if self.over {
            return Flow::End(None);
        }
        let mut out = Vec::new();
        // What the console has now, a few pieces at a time.
        for _ in 0..8 {
            match self.viewer.recv(Duration::ZERO) {
                Ok(Some(o)) => {
                    let ended = matches!(o, qsi::host::console::Out::Ended(_));
                    if let Ok(p) = o.piece() {
                        out.extend(p);
                    }
                    if ended {
                        self.over = true;
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    self.over = true;
                    break;
                }
            }
        }
        if out.is_empty() {
            if self.over {
                Flow::End(None)
            } else {
                Flow::Idle
            }
        } else {
            Flow::Bytes(out)
        }
    }

    fn put(&mut self, bytes: &[u8]) -> Result<(), Note> {
        self.pieces.push(bytes);
        while let Some(p) = self.pieces.next_piece() {
            let Ok(input) = In::decode(&p) else {
                continue;
            };
            if input.check().is_err() {
                continue;
            }
            self.viewer
                .send(input)
                .map_err(|e| Note::new("QSI0301", &format!("The console went away: {e}")))?;
        }
        Ok(())
    }
}

/// A dump's text read from the service in pieces.
struct DumpPipe {
    link: Shared,
    name: String,
    at: u64,
    total: u64,
}

impl Pipe for DumpPipe {
    fn poll(&mut self) -> Flow {
        if self.at >= self.total {
            return Flow::End(None);
        }
        let op = QsiOp::DumpRead {
            name: self.name.clone(),
            offset: self.at,
            len: 16 * 1024,
        };
        match ask(&self.link, op) {
            QsiReply::Bytes { bytes, .. } if !bytes.is_empty() => {
                self.at += bytes.len() as u64;
                Flow::Bytes(bytes)
            }
            QsiReply::Bytes { .. } => Flow::End(None),
            r => Flow::End(notes(r).into_iter().next()),
        }
    }

    fn put(&mut self, _bytes: &[u8]) -> Result<(), Note> {
        Ok(())
    }
}

/// The streams of one session.
pub struct JobOpener {
    link: Shared,
}

impl JobOpener {
    /// Over `link`.
    #[must_use]
    pub fn new(link: Shared) -> Self {
        Self { link }
    }
}

impl Opener for JobOpener {
    fn console(&mut self, take_over: bool) -> Result<Box<dyn Pipe>, Refusal> {
        let viewer = console::attach(take_over).map_err(|r| refusal(r.msgid, &r.text))?;
        Ok(Box::new(ConsolePipe {
            viewer,
            pieces: Pieces::default(),
            over: false,
        }))
    }

    fn dump(&mut self, name: &str) -> Result<(u64, Box<dyn Pipe>), Refusal> {
        let op = QsiOp::DumpRead {
            name: name.into(),
            offset: 0,
            len: 0,
        };
        match ask(&self.link, op) {
            QsiReply::Bytes { total, .. } => Ok((
                total,
                Box::new(DumpPipe {
                    link: self.link.clone(),
                    name: name.to_ascii_uppercase(),
                    at: 0,
                    total,
                }),
            )),
            r => Err(notes(r)),
        }
    }
}

type Call = Box<dyn FnOnce(&Shared) + Send>;

/// A link to the service on a thread of its own, for what every
/// session's thread shares.
pub struct Svc {
    tx: Mutex<mpsc::Sender<Call>>,
}

impl Svc {
    /// The thread for job `number`.
    ///
    /// # Errors
    /// The link could not be made.
    pub fn start(number: u32) -> Result<Self, String> {
        let pid = std::process::id();
        let (tx, rx) = mpsc::channel::<Call>();
        let (ok_tx, ok_rx) = mpsc::channel();
        thread::Builder::new()
            .name("qsi-keeper".into())
            .spawn(move || {
                let link = match thread_link(number, pid) {
                    Ok(l) => {
                        let _ = ok_tx.send(Ok(()));
                        l
                    }
                    Err(e) => {
                        let _ = ok_tx.send(Err(e));
                        return;
                    }
                };
                while let Ok(call) = rx.recv() {
                    call(&link);
                }
            })
            .map_err(|e| e.to_string())?;
        ok_rx.recv().map_err(|e| e.to_string())??;
        Ok(Self { tx: Mutex::new(tx) })
    }

    /// Ask the service on the thread; `None` when it is gone.
    pub fn ask(&self, op: QsiOp) -> Option<QsiReply> {
        let (tx, rx) = mpsc::channel();
        let call: Call = Box::new(move |l| {
            let _ = tx.send(ask(l, op));
        });
        self.tx
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .send(call)
            .ok()?;
        rx.recv().ok()
    }
}

/// The password and the pins, kept by the service.
pub struct JobKeeper {
    svc: Svc,
}

impl JobKeeper {
    /// Over `svc`.
    #[must_use]
    pub fn new(svc: Svc) -> Self {
        Self { svc }
    }
}

impl Keeper for JobKeeper {
    fn attempt(&self) -> Option<String> {
        match self.svc.ask(QsiOp::Attempt) {
            Some(QsiReply::Attempt { password }) => password,
            _ => None,
        }
    }

    fn failed(&self, attempt: &Attempt, why: Failure) {
        let _ = self.svc.ask(QsiOp::Failed {
            name: attempt.name.clone(),
            fingerprint: attempt.fingerprint.hex(),
            address: attempt.address.clone(),
            why: why.code().into(),
        });
    }

    fn paired(&self, attempt: &Attempt) -> bool {
        matches!(
            self.svc.ask(QsiOp::Paired {
                name: attempt.name.clone(),
                fingerprint: attempt.fingerprint.hex(),
                address: attempt.address.clone(),
            }),
            Some(QsiReply::Flag { yes: true })
        )
    }

    fn contact(&self, fingerprint: &Fingerprint, address: &str) {
        let _ = self.svc.ask(QsiOp::Contact {
            fingerprint: fingerprint.hex(),
            address: address.into(),
        });
    }
}
