// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `TELNET` (`STRTCPTELN`): a 5250 session with another system from
//! this job's display. The client negotiates TN5250 (RFC 2877, with the
//! RFC 4777 password substitute for `RMTUSER`/`RMTPWD`), then passes
//! records both ways untouched: the remote system's to the display, the
//! display's (System Request and Attention included) to the remote.
//! The session ends when the remote system closes the connection (a
//! sign-off); the screen as it was comes back with `TCP2504`.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use cmd::{Resolved, Value};
use mi::Job;
use tn5250::record::Record;
use tn5250::telnet::client::{Client, Config, Encoding};
use wm::ctl::ToJob;

use super::Fail;
use crate::console::host::Host;

fn text(v: &Value) -> String {
    v.source().trim().trim_matches('\'').trim().to_string()
}

fn parm(r: &Resolved, kwd: &str) -> String {
    r.get(kwd).and_then(|b| b.value.as_ref()).map(text).unwrap_or_default()
}

/// What the socket reader hands the session.
enum Net {
    Bytes(Vec<u8>),
    Closed,
}

/// `TELNET RMTSYS() ...`.
pub fn run(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    if !host.has_display() {
        return Err(Fail::no_display(&r.name));
    }
    if parm(r, "SSL").eq_ignore_ascii_case("*YES") {
        // BLOCKED(rel): the TELNET client does not make TLS sessions.
        return Err(Fail::msg("TCP2534", &[]));
    }
    let rmtsys = parm(r, "RMTSYS");
    let target = if rmtsys.eq_ignore_ascii_case("*INTNETADR") {
        parm(r, "INTNETADR")
    } else {
        rmtsys
    };
    let port: u16 = match parm(r, "PORT").as_str() {
        "*DFT" | "" | "0" => 23,
        n => n.parse().unwrap_or(23),
    };
    let wait = match parm(r, "INZWAIT").as_str() {
        "*NOMAX" | "0" => Duration::from_secs(3600),
        n => Duration::from_secs(n.parse().unwrap_or(120)),
    };
    let addrs: Vec<_> = (target.as_str(), port)
        .to_socket_addrs()
        .map(Iterator::collect)
        .unwrap_or_default();
    if addrs.is_empty() {
        let mut f = Fail::msg("TCP101A", &[]);
        f.data = vary(&target);
        return Err(f);
    }
    let stream = addrs
        .iter()
        .find_map(|a| TcpStream::connect_timeout(a, wait.min(Duration::from_secs(30))).ok())
        .ok_or_else(|| {
            let mut f = Fail::msg("TCP10B0", &[]);
            f.data = vary(&target);
            f.data.extend(vary(&addrs[0].ip().to_string()));
            f
        })?;
    session(job, host, r, stream, wait)
}

/// A `*CHAR *VARY 2` substitution value.
fn vary(s: &str) -> Vec<u8> {
    let b = ccsid::to_ebcdic(s).unwrap_or_default();
    let mut out = u16::try_from(b.len()).unwrap_or(0).to_be_bytes().to_vec();
    out.extend(b);
    out
}

fn config(host: &Host, r: &Resolved) -> Config {
    let wide = host.display.borrow().as_ref().is_some_and(|d| d.wide);
    let mut cfg = Config {
        terminal_type: if wide { "IBM-3477-FC" } else { "IBM-3179-2" }.into(),
        ..Config::default()
    };
    let dev = parm(r, "RMTVRTDSP");
    if !dev.is_empty() && !dev.starts_with('*') {
        cfg.device = dev.to_ascii_uppercase();
    }
    let user = parm(r, "RMTUSER");
    if !user.is_empty() && !user.eq_ignore_ascii_case("*NONE") {
        let user = if user.eq_ignore_ascii_case("*CURRENT") {
            host.start.user.clone()
        } else {
            user.to_ascii_uppercase()
        };
        cfg.user = Some(user);
        let pwd = parm(r, "RMTPWD");
        if !pwd.is_empty() && !pwd.eq_ignore_ascii_case("*NONE") {
            cfg.password = Some(pwd);
        }
        cfg.encoding = if parm(r, "RMTPWDENC").eq_ignore_ascii_case("*DES7") {
            Encoding::Des
        } else {
            Encoding::Sha
        };
    }
    cfg
}

fn session(job: &mut Job, host: &Host, r: &Resolved, stream: TcpStream, wait: Duration) -> Result<String, Fail> {
    let mut out = stream.try_clone().map_err(|e| {
        let mut f = Fail::msg("TCP2590", &[]);
        f.data = vary(&e.raw_os_error().unwrap_or(0).to_string());
        f
    })?;
    let mut reader = stream;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 16384];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let _ = tx.send(Net::Closed);
                    return;
                }
                Ok(n) => {
                    if tx.send(Net::Bytes(buf[..n].to_vec())).is_err() {
                        return;
                    }
                }
            }
        }
    });
    let mut client = Client::new(config(host, r));
    // The negotiation, within INZWAIT.
    let start = Instant::now();
    let mut early = Vec::new();
    let mut heard = false;
    while !client.negotiated {
        let left = wait.saturating_sub(start.elapsed());
        match rx.recv_timeout(left) {
            Ok(Net::Bytes(b)) => {
                heard = true;
                let (reply, records) = client.feed(&b);
                if !reply.is_empty() {
                    let _ = out.write_all(&reply);
                }
                early.extend(records);
            }
            Ok(Net::Closed) => return Err(Fail::msg("TCP2534", &[])),
            Err(_) => return Err(Fail::msg(if heard { "TCP2534" } else { "TCP2591" }, &[])),
        }
    }
    let saved = host.display.borrow().as_ref().map(crate::display::Display::snapshot);
    let show = |rec: &[u8]| {
        if let Ok(rec) = Record::decode(rec)
            && let Some(d) = host.display.borrow_mut().as_mut()
        {
            d.write_raw(rec.opcode, rec.data);
        }
    };
    for rec in &early {
        show(rec);
    }
    loop {
        // The remote system's records first.
        let mut closed = false;
        while let Ok(m) = rx.try_recv() {
            match m {
                Net::Bytes(b) => {
                    let (reply, records) = client.feed(&b);
                    if !reply.is_empty() {
                        let _ = out.write_all(&reply);
                    }
                    for rec in &records {
                        show(rec);
                    }
                }
                Net::Closed => closed = true,
            }
        }
        if closed {
            break;
        }
        // Then what the display sent.
        match host.inbox.recv_timeout(Duration::from_millis(50)) {
            Ok(ToJob::Record { flags, opcode, data }) => {
                let rec = Record { flags, opcode, data }.encode();
                if out.write_all(&tn5250::telnet::record(&rec)).is_err() {
                    break;
                }
            }
            Ok(ToJob::DeviceGone) => {
                host.pending.borrow_mut().push_back(ToJob::DeviceGone);
                break;
            }
            Ok(other) => host.pending.borrow_mut().push_back(other),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        host.breaks(job);
    }
    let _ = out.shutdown(std::net::Shutdown::Both);
    if let (Some(s), Some(d)) = (saved, host.display.borrow_mut().as_mut()) {
        d.restore(s);
    }
    super::log(job, "TCP2504", &[], qir::MsgType::Comp, None);
    Ok(String::new())
}
