// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 10 QEMU step 69: the small servers. `STRTCPSVR *LPD` starts
//! `QTLPD00001`; a file sent from the host with `work/print`'s LPR
//! client through a port forward becomes a spooled file on an output
//! queue, displayed with its text; an unknown queue is refused.
//! `CHGNTPA` lets the SNTP server answer unsynchronized; `STRTCPSVR
//! *NTP` starts `QTOTNTP`, which answers an SNTP query from the host
//! over a UDP forward with the guest's clock. `ENDTCPSVR` ends both.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use print::job::PrintJob;
use print::sink::Sink;
use print::sink::lpr::LprSink;
use print::transform::{Format, Output};

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};
use crate::tcp::forward_port;

/// The guest's own address on QEMU's user network.
const GUEST: &str = "10.0.2.15";
/// The text the host prints.
const TEXT: &str = "Hello from the harness, line one\nand line two\n";

/// A console command whose output must hold `want`.
fn expect(s: &mut Session, line: &str, want: &str) -> Result<String, String> {
    let out = s.command(line, CMD)?;
    if out.contains(want) {
        Ok(out)
    } else {
        Err(format!("{line}: no {want} in:\n{out}"))
    }
}

/// Wait until `WRKACTJOB` does (`present`) or does not show a job.
fn job_shown(s: &mut Session, name: &str, present: bool) -> Result<(), String> {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let out = s.command("wrkactjob", CMD)?;
        let shown = out.lines().any(|l| l.split_whitespace().next() == Some(name));
        if shown == present {
            return Ok(());
        }
        if Instant::now() > end {
            return Err(format!("{name} {} in:\n{out}", if present { "never shown" } else { "still shown" }));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Send `TEXT` to `queue` at host port `port`, trying while slirp
/// accepts before the guest listens.
fn lpr(port: u16, queue: &str) -> Result<String, String> {
    let mut sink = LprSink::new("127.0.0.1", queue, "harness");
    sink.endpoint.port = port;
    sink.seppage = false;
    let job = PrintJob {
        file: "HARNESS".into(),
        job: ("LPR".into(), "tester".into(), 1),
        ..PrintJob::default()
    };
    let doc = Output {
        data: TEXT.as_bytes().to_vec(),
        format: Format::Text,
    };
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        match sink.send(&job, &doc) {
            Ok(r) => return Ok(r.detail),
            Err(e) if e.detail.contains("refused") => return Err(e.detail),
            Err(e) if Instant::now() > end => return Err(e.detail),
            Err(_) => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}

/// Forward a fresh host UDP port to the guest's `guest` port.
#[cfg(unix)]
fn forward_udp(suite: &Suite, guest: u16) -> Result<u16, String> {
    let port = std::net::UdpSocket::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| e.to_string())?;
    let mut q = crate::qemu::qmp::Qmp::connect(&suite.launch().qmp)?;
    let out = q.hmp(&format!("hostfwd_add n0 udp:127.0.0.1:{port}-{GUEST}:{guest}"))?;
    if !out.trim().is_empty() {
        return Err(format!("hostfwd_add: {out}"));
    }
    Ok(port)
}

#[cfg(not(unix))]
fn forward_udp(_: &Suite, _: u16) -> Result<u16, String> {
    Err("QMP needs a Unix host".into())
}

/// The host's clock for the query.
struct HostClock;

impl ntp::Clock for HostClock {
    fn now(&self) -> i64 {
        ntp::clock::system_now()
    }
    fn slew(&self, _: i64) -> Result<(), String> {
        Ok(())
    }
    fn step(&self, _: i64) -> Result<(), String> {
        Ok(())
    }
}

/// One SNTP sample from the guest, trying while the server comes up.
fn sntp(port: u16) -> Result<ntp::Sample, String> {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        match ntp::client::best_of("qs400", addr, &HostClock, Duration::from_secs(2), 2) {
            Ok(s) => return Ok(s),
            Err(e) if Instant::now() > end => return Err(format!("SNTP from the guest: {e}")),
            Err(_) => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}

#[allow(clippy::unused_self)] // the steps share one signature
impl Suite {
    /// Step 69: `*LPD` and `*NTP`.
    pub(crate) fn small_servers(&self, s: &mut Session) -> Result<String, String> {
        // LPD into an output queue.
        s.command("crtoutq QGPL/LPDQ TEXT('LPD harness queue')", CMD)?;
        expect(s, "CHGLPDA NBRSVR(3)", "TCP1A10")?;
        expect(s, "STRTCPSVR SERVER(*LPD)", "TCP1A07")?;
        job_shown(s, "QTLPD00001", true)?;
        expect(s, "STRTCPSVR SERVER(*LPD)", "TCP1A08")?;
        let port = forward_port(self, GUEST, lpd::PORT)?;
        let sent = lpr(port, "QGPL/LPDQ")?;
        let end = Instant::now() + Duration::from_secs(20);
        let queue = loop {
            let q = s.command("wrkoutq QGPL/LPDQ", CMD)?;
            if q.contains("HARNESS") {
                break q;
            }
            if Instant::now() > end {
                return Err(format!("no HARNESS on QGPL/LPDQ:\n{q}"));
            }
            std::thread::sleep(Duration::from_millis(500));
        };
        let shown = s.command("dspsplf HARNESS JOB(QTLPD00001)", CMD)?;
        for want in ["Hello from the harness, line one", "and line two"] {
            if !shown.contains(want) {
                return Err(format!("the spooled file lacks {want:?}:\n{shown}"));
            }
        }
        match lpr(port, "NOSUCHQ") {
            Err(e) if e.contains("refused") => {}
            other => return Err(format!("an unknown queue was not refused: {other:?}")),
        }
        // SNTP: the server answers from the guest's clock.
        expect(s, "CHGNTPA SYNCRQD(*NO) SVRACTLOG(*ALL)", "TCP1A10")?;
        expect(s, "STRTCPSVR SERVER(*NTP)", "TCP1A07")?;
        job_shown(s, "QTOTNTP", true)?;
        let udp = forward_udp(self, ntp::PORT)?;
        let sample = sntp(udp)?;
        if sample.stratum != ntp::server::LOCAL_STRATUM || &sample.reference_id != b"LOCL" {
            return Err(format!("unexpected answer: {sample:?}"));
        }
        // The guest's clock came from the RTC, which QEMU sets from the host.
        if sample.offset.abs() > 300 * 1_000_000_000 {
            return Err(format!("the guest's clock is {} s off", sample.offset / 1_000_000_000));
        }
        expect(s, "ENDTCPSVR SERVER(*NTP)", "TCP1A0A")?;
        job_shown(s, "QTOTNTP", false)?;
        expect(s, "ENDTCPSVR SERVER(*LPD)", "TCP1A0A")?;
        job_shown(s, "QTLPD00001", false)?;
        expect(s, "ENDTCPSVR SERVER(*LPD)", "TCP1A09")?;
        let row = queue.lines().find(|l| l.contains("HARNESS")).unwrap_or("").trim().to_string();
        Ok(format!(
            "LPR from the host ({sent}) spooled [{row}] on QGPL/LPDQ; SNTP answered at stratum {} with offset {} ms; both ended",
            sample.stratum,
            sample.offset / 1_000_000
        ))
    }
}
