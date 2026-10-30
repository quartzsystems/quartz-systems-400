// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 10 QEMU step 64: the TCP servers framework. `STRTCPSVR *HTTP
//! HTTPSVR(*ADMIN)` starts `ADMIN` in `QHTTPSVR`, whose page comes over
//! HTTP on 2001 and HTTPS on 2005; `ENDTCPSVR` takes it down. The
//! telnet server ends, takes `CHGTELNA`, and starts again under the
//! framework; a server this release does not run says so.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};
use crate::tcp::{forward_port, telnet_answers};

/// The guest's own address on QEMU's user network.
const GUEST: &str = "10.0.2.15";

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
fn job_shown(s: &mut Session, name: &str, present: bool) -> Result<String, String> {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let out = s.command("wrkactjob", CMD)?;
        let shown = out
            .lines()
            .any(|l| l.split_whitespace().next() == Some(name));
        if shown == present {
            return Ok(out);
        }
        if Instant::now() > end {
            return Err(format!("{name} {} in:\n{out}", if present { "never shown" } else { "still shown" }));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// One request over `stream`, the whole answer.
fn get(stream: &mut (impl Read + Write)) -> Result<String, String> {
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: qs400\r\n\r\n")
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let _ = stream.read_to_end(&mut out);
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Connect to host port `port`, trying while the server comes up.
fn connect(port: u16) -> Result<TcpStream, String> {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(c) => {
                let _ = c.set_read_timeout(Some(Duration::from_secs(10)));
                return Ok(c);
            }
            Err(e) if Instant::now() > end => return Err(format!("port {port}: {e}")),
            Err(_) => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}

/// The administration page over plain HTTP; slirp accepts before the
/// guest does, so an empty answer is tried again.
fn page(port: u16) -> Result<String, String> {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let answer = get(&mut connect(port)?)?;
        if answer.starts_with("HTTP/1.1 200 OK") {
            return Ok(answer);
        }
        if Instant::now() > end {
            return Err(format!("port {port} answered:\n{answer}"));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// The page over HTTPS, the server's certificate taken as it is.
fn secure_page(port: u16) -> Result<String, String> {
    let policy = tls::policy::TlsPolicy::opsys();
    let cfg = tls::config::client_config(
        &policy,
        tls::rustls::RootCertStore::empty(),
        None,
        tls::config::ServerAuthMode::Passthru,
    )
    .map_err(|e| e.to_string())?;
    let name = tls::config::server_name("qs400").map_err(|e| e.to_string())?;
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let r = tls::stream::TlsStream::connect(cfg.clone(), name.clone(), connect(port)?)
            .map_err(|e| e.to_string())
            .and_then(|mut t| get(&mut t));
        match r {
            Ok(a) if a.starts_with("HTTP/1.1 200 OK") => return Ok(a),
            other if Instant::now() > end => return Err(format!("HTTPS on port {port}: {other:?}")),
            _ => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}

/// Wait until nothing answers HTTP on `port`.
fn page_gone(port: u16) -> Result<(), String> {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        let answered = TcpStream::connect(("127.0.0.1", port))
            .ok()
            .and_then(|mut c| {
                let _ = c.set_read_timeout(Some(Duration::from_secs(3)));
                get(&mut c).ok()
            })
            .is_some_and(|a| a.starts_with("HTTP/"));
        if !answered {
            return Ok(());
        }
        if Instant::now() > end {
            return Err(format!("port {port} still answers after ENDTCPSVR"));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[allow(clippy::unused_self)] // the steps share one signature
impl Suite {
    /// Step 64: `STRTCPSVR` and `ENDTCPSVR` of `*HTTP` and `*TELNET`.
    pub(crate) fn tcp_servers(&self, s: &mut Session) -> Result<String, String> {
        // The HTTP administration instance.
        expect(s, "STRTCPSVR SERVER(*HTTP) HTTPSVR(*ADMIN)", "TCP1A07")?;
        let jobs = job_shown(s, "ADMIN", true)?;
        if !jobs.lines().any(|l| l.trim_start().starts_with("QHTTPSVR")) {
            return Err(format!("no QHTTPSVR subsystem:\n{jobs}"));
        }
        let plain = forward_port(self, GUEST, 2001)?;
        let p = page(plain)?;
        for want in ["not yet available", "QS/400 V1R0M0", "Connection: close"] {
            if !p.contains(want) {
                return Err(format!("the page has no {want:?}:\n{p}"));
            }
        }
        let secure = forward_port(self, GUEST, 2005)?;
        secure_page(secure)?;
        expect(s, "STRTCPSVR SERVER(*HTTP) HTTPSVR(*ADMIN)", "TCP1A08")?;
        expect(s, "STRTCPSVR SERVER(*HTTP) HTTPSVR(WEB1)", "TCP1A06")?;
        expect(s, "ENDTCPSVR SERVER(*HTTP) HTTPSVR(*ADMIN)", "TCP1A0A")?;
        job_shown(s, "ADMIN", false)?;
        page_gone(plain)?;
        expect(s, "ENDTCPSVR SERVER(*HTTP)", "TCP1A09")?;
        // The telnet server under the framework, with CHGTELNA.
        expect(s, "ENDTCPSVR SERVER(*TELNET)", "TCP1A0A")?;
        job_shown(s, "QTVTELNET", false)?;
        expect(s, "CHGTELNA TIMMRKTIMO(600) NBRSVR(4)", "TCP1A10")?;
        // Out of range: the analyzer refuses it (CPD0085).
        let refused = s.command("CHGTELNA NBRFLDS(100)", CMD)?;
        if !refused.contains("CPD0085") || refused.contains("TCP1A10") {
            return Err(format!("CHGTELNA took NBRFLDS(100):\n{refused}"));
        }
        expect(s, "STRTCPSVR SERVER(*TELNET)", "TCP1A07")?;
        job_shown(s, "QTVTELNET", true)?;
        if let Some(port) = self.telnet_port() {
            telnet_answers(port)?;
        }
        expect(s, "STRTCPSVR SERVER(*SNMP)", "TCP1A04")?;
        Ok(format!(
            "ADMIN in QHTTPSVR served the page on 2001 (host {plain}) and 2005 (host {secure}) and ended; QTVTELNET ended, took CHGTELNA and started again"
        ))
    }
}
