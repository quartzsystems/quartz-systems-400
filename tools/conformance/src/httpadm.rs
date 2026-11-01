// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 87: HTTP Server Administration from live state. `ADMIN` is
//! started; from the host, over HTTP on 2001: the sign-on page, a
//! wrong password refused, `QSECOFR` signed on, Server instances
//! listing `ADMIN` running in the job `DSPJOB` knows, a message's
//! second-level text from `QHTTPSVR/QZHBMSG`, a POST without the form
//! field refused, sign-off; then the sign-on page over TLS on 2010.

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use crate::phase5::SECOFR_PASSWORD;
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};
use crate::tcp::{GUEST, forward_port};

/// The product's URLs.
const PREFIX: &str = "/qs400/httpadmin";

/// An answer: status, head, body.
pub(crate) struct Reply {
    pub(crate) status: u16,
    pub(crate) head: String,
    pub(crate) body: String,
}

impl Reply {
    fn parse(raw: &[u8]) -> Option<Self> {
        let text = String::from_utf8_lossy(raw).into_owned();
        let (head, body) = text.split_once("\r\n\r\n")?;
        let status = head.split_whitespace().nth(1)?.parse().ok()?;
        Some(Self {
            status,
            head: head.to_string(),
            body: body.to_string(),
        })
    }

    /// A header's value (the first).
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.head.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim().eq_ignore_ascii_case(name).then(|| v.trim())
        })
    }

    pub(crate) fn brief(&self) -> String {
        format!("{}\n{}", self.head, self.body.chars().take(2000).collect::<String>())
    }
}

/// The request's bytes.
fn request_bytes(method: &str, path: &str, cookie: Option<&str>, form: Option<&str>) -> Vec<u8> {
    let mut r = format!("{method} {path} HTTP/1.1\r\nHost: qs400:2001\r\nConnection: close\r\n");
    if let Some(c) = cookie {
        let _ = write!(r, "Cookie: QS400SID={c}\r\n");
    }
    if let Some(f) = form {
        let _ = write!(
            r,
            "Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{f}",
            f.len()
        );
    } else {
        r.push_str("\r\n");
    }
    r.into_bytes()
}

/// One exchange over `stream`.
fn exchange(stream: &mut (impl Read + Write), req: &[u8]) -> Option<Reply> {
    stream.write_all(req).ok()?;
    let mut out = Vec::new();
    let _ = stream.read_to_end(&mut out);
    Reply::parse(&out)
}

fn connect(port: u16) -> Result<TcpStream, String> {
    let c = TcpStream::connect(("127.0.0.1", port)).map_err(|e| format!("port {port}: {e}"))?;
    let _ = c.set_read_timeout(Some(Duration::from_secs(20)));
    Ok(c)
}

/// One request to the administration server over plain HTTP.
fn http(port: u16, method: &str, path: &str, cookie: Option<&str>, form: Option<&str>) -> Result<Reply, String> {
    http_at(PREFIX, port, method, path, cookie, form)
}

/// One request under `prefix` over plain HTTP, tried for a while: slirp accepts
/// before the guest does.
pub(crate) fn http_at(
    prefix: &str,
    port: u16,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    form: Option<&str>,
) -> Result<Reply, String> {
    let end = Instant::now() + Duration::from_secs(30);
    let req = request_bytes(method, &format!("{prefix}{path}"), cookie, form);
    loop {
        let r = connect(port).ok().and_then(|mut c| exchange(&mut c, &req));
        match r {
            Some(r) => return Ok(r),
            None if Instant::now() > end => return Err(format!("{method} {path}: no answer on host port {port}")),
            None => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}

/// A GET of `path` over TLS, the certificate taken as it is.
pub(crate) fn tls_get(port: u16, path: &str) -> Result<Reply, String> {
    let policy = tls::policy::TlsPolicy::opsys();
    let cfg = tls::config::client_config(
        &policy,
        tls::rustls::RootCertStore::empty(),
        None,
        tls::config::ServerAuthMode::Passthru,
    )
    .map_err(|e| e.to_string())?;
    let name = tls::config::server_name("qs400").map_err(|e| e.to_string())?;
    let req = request_bytes("GET", path, None, None);
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let r = connect(port).and_then(|c| {
            let mut t = tls::stream::TlsStream::connect(cfg.clone(), name.clone(), c).map_err(|e| e.to_string())?;
            exchange(&mut t, &req).ok_or_else(|| "no answer".to_string())
        });
        match r {
            Ok(a) => return Ok(a),
            Err(e) if Instant::now() > end => return Err(format!("HTTPS on host port {port}: {e}")),
            Err(_) => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}

pub(crate) fn want(r: &Reply, status: u16, needles: &[&str], what: &str) -> Result<(), String> {
    if r.status != status {
        return Err(format!("{what}: status {} rather than {status}:\n{}", r.status, r.brief()));
    }
    for n in needles {
        if !r.body.contains(n) && !r.head.contains(n) {
            return Err(format!("{what}: no {n:?} in:\n{}", r.brief()));
        }
    }
    Ok(())
}

/// The session cookie a sign-on set.
pub(crate) fn session_cookie(r: &Reply) -> Option<String> {
    r.head.lines().find_map(|l| {
        let v = l.strip_prefix("Set-Cookie: QS400SID=")?;
        let id = v.split(';').next()?.trim();
        (!id.is_empty()).then(|| id.to_string())
    })
}

/// `nnnnnn/QTMHHTTP/ADMIN` in a page.
fn admin_job(body: &str) -> Option<String> {
    let at = body.find("/QTMHHTTP/ADMIN")?;
    let number = body.get(at.checked_sub(6)?..at)?;
    number
        .bytes()
        .all(|b| b.is_ascii_digit())
        .then(|| format!("{number}/QTMHHTTP/ADMIN"))
}

/// Set `QSECOFR`'s password, so a step needs no earlier one.
pub(crate) fn secofr_password(s: &mut Session) -> Result<(), String> {
    let set = s.command(&format!("CHGUSRPRF USRPRF(QSECOFR) PASSWORD('{SECOFR_PASSWORD}') PWDEXP(*NO)"), CMD)?;
    if set.trim_start().starts_with("CPF") {
        return Err(format!("CHGUSRPRF QSECOFR: {set}"));
    }
    Ok(())
}

/// Form-encode a value.
pub(crate) fn enc(v: &str) -> String {
    v.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'*' => char::from(b).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

impl Suite {
    /// Step 87: HTTP Server Administration over HTTP and HTTPS.
    pub(crate) fn http_admin(&self, s: &mut Session) -> Result<String, String> {
        // The IPL makes /www for the ADMIN job, whatever step 86 did.
        let www = s.command("svc.ifs stat /www", CMD)?;
        if !www.contains("QTMHHTTP") {
            return Err(format!("/www is not QTMHHTTP's:\n{www}"));
        }
        secofr_password(s)?;
        let started = s.command("STRTCPSVR SERVER(*HTTP) HTTPSVR(*ADMIN)", CMD)?;
        if !started.contains("TCP1A07") && !started.contains("TCP1A08") {
            return Err(format!("STRTCPSVR *ADMIN:\n{started}"));
        }
        let plain = forward_port(self, GUEST, 2001)?;
        let page = http(plain, "GET", "/signon", None, None)?;
        want(&page, 200, &["QS/400 HTTP Server Administration", "name=\"password\""], "the sign-on page")?;
        // A wrong password: the page again with the system's refusal.
        let bad = http(plain, "POST", "/signon", None, Some("user=QSECOFR&password=NOT1RIGHT"))?;
        want(&bad, 200, &["user"], "a wrong password")?;
        if !bad.body.contains("CPF1107") && !bad.body.contains("CPF1392") {
            return Err(format!("a wrong password was not refused:
{}", bad.brief()));
        }
        let form = format!("user=QSECOFR&password={}", enc(SECOFR_PASSWORD));
        let ok = http(plain, "POST", "/signon", None, Some(&form))?;
        if ok.status != 303 || !ok.header("Location").is_some_and(|l| l.ends_with("/instances")) {
            return Err(format!("signing on as QSECOFR:\n{}", ok.brief()));
        }
        let sid = session_cookie(&ok).ok_or_else(|| format!("no session cookie:\n{}", ok.brief()))?;
        // Server instances: ADMIN running, in a job the system knows.
        let list = http(plain, "GET", "/instances", Some(&sid), None)?;
        want(&list, 200, &["ADMIN", "Running"], "Server instances")?;
        let job = admin_job(&list.body).ok_or_else(|| format!("no ADMIN job on Server instances:\n{}", list.brief()))?;
        let shown = s.command(&format!("dspjob JOB({job})"), CMD)?;
        if !shown.contains("ADMIN") || shown.contains("CPF1070") {
            return Err(format!("dspjob of the page's job {job}:\n{shown}"));
        }
        // A message's second-level text, from the message file.
        let msg = http(plain, "GET", "/msg/HTP8001?d=WEB1&d=%2A%3A80", Some(&sid), None)?;
        want(&msg, 200, &["HTP8001", "WEB1", "*:80"], "message HTP8001")?;
        // A POST without the form's field is refused.
        let forged = http(plain, "POST", "/msg/HTP8001", Some(&sid), Some("Close=Close"))?;
        want(&forged, 403, &["out of date"], "a POST without the form field")?;
        // Sign off: the session is gone.
        let off = http(plain, "GET", "/signoff", Some(&sid), None)?;
        want(&off, 303, &["Max-Age=0"], "sign-off")?;
        let after = http(plain, "GET", "/instances", Some(&sid), None)?;
        if after.status != 303 || !after.header("Location").is_some_and(|l| l.ends_with("/signon")) {
            return Err(format!("the session outlived sign-off:\n{}", after.brief()));
        }
        // TLS on 2010.
        let secure = forward_port(self, GUEST, 2010)?;
        let t = tls_get(secure, &format!("{PREFIX}/signon"))?;
        want(&t, 200, &["name=\"password\""], "the sign-on page over TLS")?;
        let ended = s.command("ENDTCPSVR SERVER(*HTTP) HTTPSVR(*ADMIN)", CMD)?;
        if !ended.contains("TCP1A0A") {
            return Err(format!("ENDTCPSVR *ADMIN:\n{ended}"));
        }
        Ok(format!(
            "signed on as QSECOFR over 2001 (host {plain}), ADMIN running in {job}, HTP8001 from QZHBMSG, a forged POST refused, signed off; the sign-on page over TLS on 2010 (host {secure})"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_and_jobs() {
        let r = Reply::parse(b"HTTP/1.1 303 See Other\r\nLocation: /qs400/httpadmin/instances\r\nSet-Cookie: QS400SID=abc123; Path=/qs400/httpadmin; HttpOnly\r\n\r\n").unwrap();
        assert_eq!(r.status, 303);
        assert_eq!(r.header("location"), Some("/qs400/httpadmin/instances"));
        assert_eq!(session_cookie(&r).as_deref(), Some("abc123"));
        assert_eq!(
            admin_job("<a href=\"x\">012345/QTMHHTTP/ADMIN</a>").as_deref(),
            Some("012345/QTMHHTTP/ADMIN")
        );
        assert_eq!(admin_job("x/QTMHHTTP/ADMIN"), None);
        assert_eq!(enc("Quartz 20#26"), "Quartz+20%2326");
    }
}
