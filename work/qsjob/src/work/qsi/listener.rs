// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSIDSRV`: the QSI listener of a standalone system (docs/QSID.md
//! section 12). It takes the system's identity from the storage service
//! (making it the first time: an Ed25519 key and certificate, kept
//! sealed by the LIC's key slot), binds `CHGQSIA`'s port, and serves
//! each admitted connection on a thread of its own (`qsisvr::serve`).
//! Its main thread keeps the pins and whether a password is out current
//! for the handshakes, pushes the events (codes posted, the state,
//! problems, dumps) into the log every session shares, keeps that log,
//! and writes the sessions' lines into the job log. It runs until
//! `ENDTCPSVR`, `ENDJOB` or its subsystem ends it.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ipc::chan::Chan;
use qsi::{Hello, PeerKind, caps};
use qsisess::{EventLog, Limits};
use qsisvr::conn::{SharedLog, lock};
use qsisvr::pump::{self, SeenProblem};
use qsisvr::serve::{self, Server};
use qsisvr::store::events::{self, Said, Stored};
use qsisvr::trust::SharedPins;
use qsisvr::{Host, Opener};
use qsitrust::{Fingerprint, Identity};
use servers::attrs::qsi::QsiAttrs;
use servers::msg;
use stg::Shared;
use stg::qsi::{QsiOp, QsiReply};
use wm::ctl::{FromJob, Start};

use super::host::{self, JobHost, JobKeeper, JobOpener, Svc, ask};
use crate::work::netsvc::{self, Ender};

const PGM: &str = "QSIDSRV";
/// How often the pins and the events are looked at.
const TURN: Duration = Duration::from_millis(500);
/// How often the problems and dumps are, and the log kept.
const LOOK: Duration = Duration::from_secs(2);

/// Tell `QSYSOPR` and the job log why the server did not start, and end.
fn refuse(mut ctl: Chan, link: &Shared, why: &str) -> ! {
    let text = format!("QSI server could not be started: {why}.");
    netsvc::sysopr(link, msg::QSI_NOT_STARTED, 40, &text);
    netsvc::joblog(link, PGM, msg::QSI_NOT_STARTED, &text);
    crate::start::end(&mut ctl, why)
}

/// What graviola's cryptography needs of the processor.
fn processor_ok() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("avx2")
            && std::arch::is_x86_feature_detected!("adx")
            && std::arch::is_x86_feature_detected!("aes")
            && std::arch::is_x86_feature_detected!("pclmulqdq")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// The identity kept, or one made now and kept.
fn identity(link: &Shared, name: &str, serial: &str) -> Result<Identity, String> {
    match ask(link, QsiOp::Identity) {
        QsiReply::Identity { cert, key } => {
            return Identity::new(cert, key).map_err(|e| e.to_string());
        }
        QsiReply::NoIdentity => {}
        r => return Err(note_text(r)),
    }
    let now = i64::try_from(netsvc::now_micros() / 1_000_000).unwrap_or(0);
    let id = Identity::make(name, serial, now).map_err(|e| e.to_string())?;
    match ask(
        link,
        QsiOp::KeepIdentity {
            cert: id.cert.clone(),
            key: id.key.clone(),
        },
    ) {
        QsiReply::Done => Ok(id),
        r => Err(note_text(r)),
    }
}

fn note_text(r: QsiReply) -> String {
    host::notes(r)
        .first()
        .map_or_else(|| "no answer".into(), |n| format!("{} {}", n.msgid, n.text))
}

/// The pins and the password's state, from the service.
fn refresh(link: &Shared, pins: &SharedPins) {
    let QsiReply::Consoles { list } = ask(link, QsiOp::Consoles) else {
        return;
    };
    let out = matches!(ask(link, QsiOp::Pairing), QsiReply::Pairing { state } if state.out);
    pins.set(
        list.iter()
            .filter_map(|c| Fingerprint::from_hex(&c.fingerprint)),
        out,
    );
}

/// The log kept, and what it last said.
fn load(link: &Shared, limits: &Limits) -> (EventLog, Said) {
    match ask(link, QsiOp::LoadEvents) {
        QsiReply::Bytes { bytes, .. } if !bytes.is_empty() => match events::decode(&bytes) {
            Ok(s) => (s.log(limits), s.said),
            Err(e) => {
                netsvc::joblog(
                    link,
                    PGM,
                    msg::QSI_STARTED,
                    &format!("The event log kept was passed over: {e}."),
                );
                (EventLog::new(limits), Said::default())
            }
        },
        _ => (EventLog::new(limits), Said::default()),
    }
}

/// One look at the system: the events of what changed. With `all`, the
/// problems and dumps too.
fn look(link: &Shared, log: &SharedLog, said: &mut Said, all: bool) -> usize {
    let codes = host::codes();
    let state = host::state();
    let problems: Option<Vec<SeenProblem>> = all
        .then(|| match ask(link, QsiOp::Problems) {
            QsiReply::Problems { list } => Some(
                list.into_iter()
                    .map(|p| SeenProblem {
                        summary: p.summary,
                        notes: p.notes,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .flatten();
    let dumps = all
        .then(|| match ask(link, QsiOp::Dumps) {
            QsiReply::Dumps { list } => Some(list),
            _ => None,
        })
        .flatten();
    let seen = pump::Seen {
        codes: Some(&codes),
        state: Some(&state),
        problems: problems.as_deref(),
        dumps: dumps.as_deref(),
    };
    let now = netsvc::now_micros() / 1_000_000;
    pump::pump(&mut lock(log), said, &seen, now)
}

fn keep(link: &Shared, log: &SharedLog, said: &Said) {
    let bytes = events::encode(&Stored::of(&lock(log), said));
    if let QsiReply::Refused { .. } = ask(link, QsiOp::SaveEvents { bytes }) {
        eprintln!("qsjob: {PGM}: the event log was not kept");
    }
}

/// Run the listener. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let a = QsiAttrs::from_bag(&netsvc::bag(link, "*QSI"));
    if !processor_ok() {
        refuse(
            ctl,
            link,
            "the processor lacks AVX2, ADX, AES or PCLMULQDQ, which QSI's cryptography needs",
        );
    }
    let me = match ask(link, QsiOp::Identify) {
        QsiReply::Identify { identity } => identity,
        r => refuse(ctl, link, &note_text(r)),
    };
    let id = match identity(link, &me.name, &me.serial) {
        Ok(id) => id,
        Err(e) => refuse(ctl, link, &format!("the system's identity: {e}")),
    };
    let limits = Limits::default();
    let (log, mut said) = load(link, &limits);
    let log: SharedLog = Arc::new(Mutex::new(log));
    let pins = SharedPins::default();
    refresh(link, &pins);
    let keeper = match Svc::start(start.number) {
        Ok(svc) => Arc::new(JobKeeper::new(svc)),
        Err(e) => refuse(ctl, link, &e),
    };
    let hello = Hello {
        version: qsi::VERSION,
        caps: caps::STANDALONE,
        kind: PeerKind::Standalone,
        name: me.name.clone(),
        release: me.release.clone(),
        serial: me.serial.clone(),
    };
    let number = start.number;
    let pid = std::process::id();
    let make = Box::new(move || {
        let l = host::thread_link(number, pid)?;
        Ok((
            Box::new(JobHost::new(l.clone())) as Box<dyn Host>,
            Box::new(JobOpener::new(l)) as Box<dyn Opener>,
        ))
    });
    let server = match Server::new(id.clone(), pins.clone(), hello, log.clone(), keeper, make) {
        Ok(s) => Arc::new(s),
        Err(e) => refuse(ctl, link, &e),
    };
    // What happened while the server was away, before any session.
    look(link, &log, &mut said, true);
    keep(link, &log, &said);
    let running = match servers::listen::bind(a.port).and_then(|l| serve::listen(l, server.clone()))
    {
        Ok(r) => r,
        Err(e) => refuse(ctl, link, &format!("port {}: {e}", a.port)),
    };
    let text = format!("QSI server started on port {}.", a.port);
    netsvc::sysopr(link, msg::QSI_STARTED, 0, &text);
    netsvc::joblog(link, PGM, msg::QSI_STARTED, &text);
    netsvc::joblog(
        link,
        PGM,
        msg::QSI_STARTED,
        &format!(
            "The system's QSI fingerprint is {}.",
            id.fingerprint().hex()
        ),
    );
    stg::remote::status(link, wm::job::Status::Selw, "");
    let ender = Ender::new(&ctl, start.number);
    let mut looked = Instant::now();
    loop {
        refresh(link, &pins);
        let all = looked.elapsed() >= LOOK;
        if look(link, &log, &mut said, all) > 0 || all {
            keep(link, &log, &said);
        }
        if all {
            looked = Instant::now();
        }
        for line in server.said() {
            netsvc::joblog(link, PGM, msg::QSI_STARTED, &line);
        }
        if ender.ended(link, TURN) {
            break;
        }
    }
    running.stop(&server);
    // The sessions see the goodbye within a turn of theirs.
    let until = Instant::now() + Duration::from_secs(3);
    while server.sessions() > 0 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
    for line in server.said() {
        netsvc::joblog(link, PGM, msg::QSI_STARTED, &line);
    }
    keep(link, &log, &said);
    netsvc::sysopr(link, msg::QSI_ENDED, 0, "QSI server ended.");
    netsvc::joblog(link, PGM, msg::QSI_ENDED, "QSI server ended.");
    let mut ctl = ctl;
    let _ = ctl.send(&FromJob::Ending {
        reason: "ENDTCPSVR".into(),
    });
    std::process::exit(0)
}
