// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QEMU step 86: an HTTP server instance. An instance written into the
//! IFS (`/www/stepweb`, its registry entry) starts with `STRTCPSVR
//! SERVER(*HTTP) HTTPSVR(STEPWEB)` as job `STEPWEB` in `QHTTPSVR`,
//! serves its document root on 8186 with the access log written, is
//! refused a second start and an unknown name, and ends controlled with
//! `ENDTCPSVR ... OPTION(*CNTRLD) DELAY(5)`, recorded in the registry.

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};
use crate::tcp::forward_port;

/// The guest's own address on QEMU's user network.
const GUEST: &str = "10.0.2.15";
/// The instance's port, outside the 8000-8100 range step 61 keeps
/// for QTCP.
const PORT: u16 = 8186;

const CONF: &str = "# Step 86\nServerRoot /www/stepweb\nListen *:8186\nDocumentRoot /www/stepweb/htdocs\n\
LogFormat \"%h %u \\\"%r\\\" %>s %b\" short\nCustomLog logs/access_log short\nErrorLog logs/error_log\n\
DirectoryIndex index.html\n<Directory />\n   Order Deny,Allow\n   Deny From all\n</Directory>\n\
<Directory /www/stepweb/htdocs>\n   Order Allow,Deny\n   Allow From all\n</Directory>\n";

const REGISTRY: &str = "{\n  \"version\": 1,\n  \"instances\": [\n    {\n      \"name\": \"STEPWEB\",\n      \
\"description\": \"Step 86\",\n      \"root\": \"/www/stepweb\",\n      \"autostart\": false\n    }\n  ]\n}\n";

fn expect(s: &mut Session, line: &str, want: &str) -> Result<String, String> {
    let out = s.command(line, CMD)?;
    if out.contains(want) {
        Ok(out)
    } else {
        Err(format!("{line}: no {want} in:\n{out}"))
    }
}

/// Write `text` to `path` through the xfer area.
fn put(s: &mut Session, path: &str, name: &str, text: &str) -> Result<(), String> {
    crate::qsh::push_hex(s, name, text.as_bytes())?;
    expect(s, &format!("svc.ifs putx {path} {name}"), "bytes").map(|_| ())
}

fn job_shown(s: &mut Session, name: &str, present: bool) -> Result<String, String> {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let out = s.command("wrkactjob", CMD)?;
        let shown = out.lines().any(|l| l.split_whitespace().next() == Some(name));
        if shown == present {
            return Ok(out);
        }
        if Instant::now() > end {
            return Err(format!("{name} {} in:\n{out}", if present { "never shown" } else { "still shown" }));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// `GET path` on host port `port`: the whole answer, tried while the
/// instance comes up (slirp accepts before the guest does).
fn fetch(port: u16, path: &str) -> Result<String, String> {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let answer = TcpStream::connect(("127.0.0.1", port))
            .map_err(|e| e.to_string())
            .and_then(|mut c| {
                let _ = c.set_read_timeout(Some(Duration::from_secs(10)));
                c.write_all(format!("GET {path} HTTP/1.1\r\nHost: stepweb\r\nConnection: close\r\n\r\n").as_bytes())
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                let _ = c.read_to_end(&mut out);
                Ok(String::from_utf8_lossy(&out).into_owned())
            })
            .unwrap_or_default();
        if answer.starts_with("HTTP/1.1 ") {
            return Ok(answer);
        }
        if Instant::now() > end {
            return Err(format!("port {port} answered {answer:?}"));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Wait until nothing answers on `port`.
fn gone(port: u16) -> Result<(), String> {
    let end = Instant::now() + Duration::from_secs(20);
    while fetch_once(port) {
        if Instant::now() > end {
            return Err(format!("port {port} still answers after ENDTCPSVR"));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Ok(())
}

fn fetch_once(port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", port)).is_ok_and(|mut c| {
        let _ = c.set_read_timeout(Some(Duration::from_secs(3)));
        let _ = c.write_all(b"GET / HTTP/1.1\r\nConnection: close\r\n\r\n");
        let mut out = Vec::new();
        let _ = c.read_to_end(&mut out);
        out.starts_with(b"HTTP/")
    })
}

/// What the guest says about the instance, for a failure's report.
fn diagnostics(s: &mut Session) -> String {
    let mut out = String::new();
    for line in [
        "wrkactjob",
        "svc.ifs cat /www/instances.json",
        "svc.ifs cat /www/stepweb/logs/error_log",
        "svc.ifs ls /www/stepweb",
        "dsplog",
    ] {
        let said = s.command(line, CMD).unwrap_or_else(|e| e);
        let _ = writeln!(out, "--- {line}\n{said}");
    }
    out
}

#[allow(clippy::unused_self)] // the steps share one signature
impl Suite {
    /// Step 86: an HTTP server instance started, serving and ended.
    pub(crate) fn http_instance(&self, s: &mut Session) -> Result<String, String> {
        for d in ["/www", "/www/stepweb", "/www/stepweb/conf", "/www/stepweb/htdocs", "/www/stepweb/logs"] {
            let _ = s.command(&format!("svc.ifs mkdir {d}"), CMD);
        }
        put(s, "/www/stepweb/conf/httpd.conf", "S86CONF", CONF)?;
        put(s, "/www/stepweb/htdocs/index.html", "S86PAGE", "<h1>Step 86 page</h1>\n")?;
        put(s, "/www/instances.json", "S86REG", REGISTRY)?;
        // The instance runs as QTMHHTTP: it reads the tree and writes
        // its logs and registry entry.
        s.command("CHGAUT OBJ('/www') USER(*PUBLIC) DTAAUT(*RWX) OBJAUT(*ALL) SUBTREE(*ALL)", CMD)?;
        expect(s, "STRTCPSVR SERVER(*HTTP) HTTPSVR(STEPWEB)", "TCP1A07")?;
        job_shown(s, "STEPWEB", true)?;
        let port = forward_port(self, GUEST, PORT)?;
        let page = fetch(port, "/").map_err(|e| format!("{e}\n{}", diagnostics(s)))?;
        if !page.starts_with("HTTP/1.1 200 OK") || !page.contains("Step 86 page") {
            return Err(format!("GET / answered:\n{page}"));
        }
        let missing = fetch(port, "/nope.html")?;
        if !missing.starts_with("HTTP/1.1 404 ") {
            return Err(format!("GET /nope.html answered:\n{missing}"));
        }
        expect(s, "STRTCPSVR SERVER(*HTTP) HTTPSVR(STEPWEB)", "TCP1A08")?;
        expect(s, "STRTCPSVR SERVER(*HTTP) HTTPSVR(NOSUCH)", "TCP1A06")?;
        expect(s, "svc.ifs cat /www/stepweb/logs/access_log", "\"GET / HTTP/1.1\" 200")?;
        expect(s, "ENDTCPSVR SERVER(*HTTP) HTTPSVR(STEPWEB) OPTION(*CNTRLD) DELAY(5)", "TCP1A0A")?;
        job_shown(s, "STEPWEB", false)?;
        gone(port)?;
        expect(s, "svc.ifs cat /www/instances.json", "ENDTCPSVR *HTTP, controlled")?;
        expect(s, "ENDTCPSVR SERVER(*HTTP) HTTPSVR(STEPWEB)", "TCP1A09")?;
        Ok("STEPWEB started in QHTTPSVR, served its document root and access log on 8186, refused a second start and an unknown instance, ended controlled".into())
    }
}
