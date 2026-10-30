// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QTOTNTP` in `QSYSWRK`, as `QNTP`: the SNTP client and server
//! (`STRTCPSVR *NTP`). The client polls `CHGNTPA`'s servers every
//! `POLLITV` minutes and adjusts the system clock (`platform/clock.rs`)
//! when `QTIMADJ` lets it, naming itself there while it runs. The
//! server answers on UDP 123 from the system clock. Network work runs
//! on threads; the job's link stays on this one, which logs, tells
//! `QSYSOPR`, and watches for the end.

use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ipc::chan::Chan;
use ntp::adjust::{Decision, Policy};
use ntp::poll::{Polled, poll};
use ntp::server::{self, Shared as Upstream};
use servers::attrs::ntp::NtpAttrs;
use servers::msg;
use stg::{Client, Shared};
use wm::ctl::{FromJob, Start};

use super::netsvc::{self, Ender};
use crate::platform::clock::SystemClock;

/// What `QTIMADJ` says while the client adjusts the clock (IBM's
/// identifier for its SNTP client).
pub const ADJUSTER: &str = "QIBM_OS400_SNTP";
/// How long one server may take to answer.
const TIMEOUT: Duration = Duration::from_secs(2);
const PGM: &str = "QTOTNTP";

/// What the threads tell the job.
enum Event {
    Polled(Polled),
    Served(SocketAddr, bool),
}

/// `QTIMADJ`, trimmed.
fn qtimadj(link: &Shared) -> String {
    link.borrow_mut()
        .with(|l| l.sysval("QTIMADJ".into()))
        .map(|v| ccsid::from_ebcdic(&v.data).trim().to_string())
        .unwrap_or_default()
}

fn set_qtimadj(link: &Shared, value: &str) {
    let cmd = format!("CHGSYSVAL SYSVAL(QTIMADJ) VALUE('{value}')");
    let r = link.borrow_mut().with(|l| l.exec(cmd));
    if r.text.contains("CPF") {
        eprintln!("qsjob: QTOTNTP: QTIMADJ: {}", r.text.trim());
    }
}

/// Claim `QTIMADJ` for the client; whether the client may adjust.
fn claim(link: &Shared) -> bool {
    let now = qtimadj(link);
    if now.eq_ignore_ascii_case(ADJUSTER) {
        return true;
    }
    if now.is_empty() || now.eq_ignore_ascii_case("*NONE") {
        set_qtimadj(link, ADJUSTER);
        return true;
    }
    netsvc::sysopr(
        link,
        msg::NTP_OTHER_ADJUSTER,
        30,
        &format!("SNTP client not adjusting the clock: QTIMADJ names {now}."),
    );
    false
}

#[allow(clippy::cast_precision_loss)]
fn ms(ns: i64) -> String {
    format!("{:.3}", ns as f64 / 1e6)
}

#[allow(clippy::cast_precision_loss)]
fn secs(ns: i64) -> String {
    format!("{:.3}", ns as f64 / 1e9)
}

/// The servers to poll, resolved now (the host table may change).
fn targets(link: &Shared, a: &NtpAttrs) -> Vec<(String, SocketAddr)> {
    a.rmtsys
        .iter()
        .flat_map(|h| {
            netsvc::resolve(link, h, ntp::PORT)
                .into_iter()
                .take(1)
                .map(move |addr| (h.clone(), addr))
        })
        .collect()
}

fn policy(a: &NtpAttrs) -> Policy {
    Policy::of(a.minadj, a.maxadj, a.adjthld)
}

/// Start one poll on a thread.
fn dispatch(link: &Shared, a: &NtpAttrs, may_adjust: bool, up: &Upstream, tx: &Sender<Event>) {
    let servers = targets(link, a);
    let policy = if may_adjust {
        policy(a)
    } else {
        // Only measure: everything is beyond the limit.
        Policy {
            max: 0,
            ..policy(a)
        }
    };
    let up = up.clone();
    let tx = tx.clone();
    std::thread::spawn(move || {
        let p = poll(&servers, &SystemClock, &policy, TIMEOUT, Some(&up));
        let _ = tx.send(Event::Polled(p));
    });
}

/// Report one poll as `ACTLOG` and IBM's rules for the operator ask.
fn report(link: &Shared, a: &NtpAttrs, p: &Polled, may_adjust: bool, quiet: &mut bool) {
    match p {
        Polled::Sample {
            sample,
            decision,
            applied,
        } => {
            *quiet = false;
            if a.actlog == "*POLL" {
                netsvc::joblog(
                    link,
                    PGM,
                    msg::NTP_POLLED,
                    &format!(
                        "SNTP poll of {}: offset {} ms, round trip {} ms.",
                        sample.server,
                        ms(sample.offset),
                        ms(sample.delay)
                    ),
                );
            }
            let secs = secs(sample.offset);
            match (decision, applied) {
                (Decision::Slew(n) | Decision::Step(n), Ok(())) if a.actlog != "*NONE" => {
                    netsvc::joblog(
                        link,
                        PGM,
                        msg::NTP_ADJUSTED,
                        &format!("SNTP client adjusted the clock by {} milliseconds.", ms(*n)),
                    );
                }
                (Decision::TooFar(_), _) if may_adjust => netsvc::sysopr(
                    link,
                    msg::NTP_TOO_FAR,
                    40,
                    &format!("Clock not adjusted: {} differs by {secs} seconds, more than MAXADJ.", sample.server),
                ),
                (Decision::Slew(_) | Decision::Step(_), Err(e)) => {
                    eprintln!("qsjob: QTOTNTP: adjusting the clock: {e}");
                }
                _ => {}
            }
            if matches!((decision, applied), (Decision::Step(_), Ok(()))) {
                netsvc::sysopr(
                    link,
                    msg::NTP_STEPPED,
                    10,
                    &format!("Clock set at once: {} differed by {secs} seconds.", sample.server),
                );
            }
        }
        Polled::Failed(why) => {
            let text = if why.is_empty() {
                "no remote system is configured or none resolved".to_string()
            } else {
                why.iter().map(|(h, e)| format!("{h}: {e}")).collect::<Vec<_>>().join("; ")
            };
            let said = format!("No time server answered: {text}.");
            netsvc::joblog(link, PGM, msg::NTP_NO_SERVER, &said);
            // The operator hears once, not at every poll.
            if !*quiet {
                netsvc::sysopr(link, msg::NTP_NO_SERVER, 20, &said);
                *quiet = true;
            }
        }
    }
}

/// The server on UDP `port`, its requests reported back when logged.
fn start_server(a: &NtpAttrs, up: &Upstream, tx: &Sender<Event>) -> Result<server::Running, String> {
    let sock = UdpSocket::bind(("0.0.0.0", a.port)).map_err(|e| format!("UDP port {}: {e}", a.port))?;
    let log: Option<server::Log> = if a.svractlog == "*NONE" {
        None
    } else {
        let all = a.svractlog == "*ALL";
        let tx = Mutex::new(tx.clone());
        Some(Box::new(move |peer, answered| {
            if all || !answered {
                let _ = tx.lock().map(|t| t.send(Event::Served(peer, answered)));
            }
        }))
    };
    server::serve(sock, Arc::new(ntp::clock::ReadOnly), up.clone(), a.syncrqd, log).map_err(|e| e.to_string())
}

/// Run `QTOTNTP`. Never returns.
pub fn run(mut ctl: Chan, link: &Shared, start: &Start) -> ! {
    let (client, serve_time) = NtpAttrs::parts(&start.rqsdta);
    let attrs = NtpAttrs::from_bag(&netsvc::bag(link, "*NTP"));
    let up: Upstream = Arc::new(Mutex::new(None));
    let (tx, rx): (Sender<Event>, Receiver<Event>) = mpsc::channel();
    let running = if serve_time {
        match start_server(&attrs, &up, &tx) {
            Ok(r) => {
                eprintln!("qsjob: QTOTNTP answering on UDP port {}", attrs.port);
                Some(r)
            }
            Err(e) if !client => crate::start::end(&mut ctl, &e),
            Err(e) => {
                eprintln!("qsjob: QTOTNTP: {e}");
                None
            }
        }
    } else {
        None
    };
    // QTIMADJ is claimed at the first poll that has servers.
    let mut may_adjust: Option<bool> = None;
    let ender = Ender::new(&ctl, start.number);
    stg::remote::status(link, wm::job::Status::Selw, "");
    let mut next = Instant::now();
    let mut in_flight = false;
    let mut quiet = false;
    loop {
        if ender.ended(link, Duration::from_millis(500)) {
            break;
        }
        while let Ok(ev) = rx.try_recv() {
            let a = NtpAttrs::from_bag(&netsvc::bag(link, "*NTP"));
            match ev {
                Event::Polled(p) => {
                    in_flight = false;
                    report(link, &a, &p, may_adjust == Some(true), &mut quiet);
                    next = Instant::now() + Duration::from_secs(u64::from(a.pollitv.max(1)) * 60);
                }
                Event::Served(peer, answered) => netsvc::joblog(
                    link,
                    PGM,
                    msg::NTP_SERVED,
                    &format!("SNTP request from {peer} {}.", if answered { "answered" } else { "not answered" }),
                ),
            }
        }
        if client && !in_flight && Instant::now() >= next {
            // Attribute changes take effect at the next poll; without
            // servers the client waits for CHGNTPA RMTSYS.
            let a = NtpAttrs::from_bag(&netsvc::bag(link, "*NTP"));
            if a.rmtsys.is_empty() {
                next = Instant::now() + Duration::from_secs(60);
                continue;
            }
            let adjust = *may_adjust.get_or_insert_with(|| claim(link));
            dispatch(link, &a, adjust, &up, &tx);
            in_flight = true;
        }
    }
    if let Some(r) = running {
        r.stop();
    }
    if may_adjust == Some(true) && qtimadj(link).eq_ignore_ascii_case(ADJUSTER) {
        set_qtimadj(link, "*NONE");
    }
    let _ = ctl.send(&FromJob::Ending {
        reason: "ENDTCPSVR".into(),
    });
    std::process::exit(0)
}
