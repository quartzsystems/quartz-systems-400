// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 156: Access for Web's Files and Printer output from live state.
//! `HTTPSVR(ACCESSWEB)` starts; a new profile with a home directory of
//! its own signs on over HTTP on 2003, Browse files lists the home
//! directory, a multipart upload writes a file the system says the
//! profile owns (not `QTMHHTTP`), the download gives it back whole and
//! from an offset (`Range`, `206`). Then `QSECOFR` lists printer
//! output, opens a spooled file in the viewer, holds it (`QAD6002`,
//! `WRKSPLF` shows it held) and deletes it after the confirmation.

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use crate::httpadm::{Reply, enc, http_at, secofr_password, session_cookie, want};
use crate::phase5::SECOFR_PASSWORD;
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};
use crate::tcp::{GUEST, forward_port};

const PREFIX: &str = "/qs400/accessweb";
const USER: &str = "AFW156";
const PASSWORD: &str = "AFW156PW";
const HOME: &str = "/home/AFW156";
const CONTENT: &str = "Hello from step 156.\nSecond line.\n";

fn afw(
    port: u16,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    form: Option<&str>,
) -> Result<Reply, String> {
    http_at(PREFIX, port, method, path, cookie, form)
}

/// The form field every POST carries.
fn csrf_in(body: &str) -> Option<String> {
    let at = body.find("name=\"csrf\" value=\"")? + "name=\"csrf\" value=\"".len();
    let end = body[at..].find('"')?;
    Some(body[at..at + end].to_string())
}

fn sign_on(port: u16, user: &str, password: &str) -> Result<String, String> {
    let form = format!("user={user}&password={}", enc(password));
    let ok = afw(port, "POST", "/signon", None, Some(&form))?;
    if ok.status != 303 {
        return Err(format!("signing on as {user}:\n{}", ok.brief()));
    }
    session_cookie(&ok).ok_or_else(|| format!("no session cookie:\n{}", ok.brief()))
}

/// One request of raw bytes: status, head, body bytes.
fn raw(port: u16, req: &[u8]) -> Result<(u16, String, Vec<u8>), String> {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let got = TcpStream::connect(("127.0.0.1", port))
            .ok()
            .and_then(|mut c| {
                let _ = c.set_read_timeout(Some(Duration::from_secs(20)));
                c.write_all(req).ok()?;
                let mut out = Vec::new();
                let _ = c.read_to_end(&mut out);
                let at = out.windows(4).position(|w| w == b"\r\n\r\n")?;
                let head = String::from_utf8_lossy(&out[..at]).into_owned();
                let status = head.split_whitespace().nth(1)?.parse().ok()?;
                Some((status, head, out[at + 4..].to_vec()))
            });
        match got {
            Some(r) => return Ok(r),
            None if Instant::now() > end => return Err(format!("no answer on host port {port}")),
            None => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}

/// A multipart upload of one file.
fn upload(
    port: u16,
    sid: &str,
    csrf: &str,
    name: &str,
    data: &str,
) -> Result<(u16, String), String> {
    let b = "QS400step156boundary";
    let mut body = String::new();
    for (k, v) in [
        ("csrf", csrf),
        ("mode", "bin"),
        ("ex", "Replace"),
        ("dm", "bin"),
        ("dle", "LF"),
    ] {
        let _ = write!(
            body,
            "--{b}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n"
        );
    }
    let _ = write!(
        body,
        "--{b}\r\nContent-Disposition: form-data; name=\"f\"; filename=\"{name}\"\r\nContent-Type: text/plain\r\n\r\n{data}\r\n--{b}--\r\n"
    );
    let req = format!(
        "POST {PREFIX}/files?path={HOME} HTTP/1.1\r\nHost: qs400:2003\r\nConnection: close\r\nCookie: QS400SID={sid}\r\nContent-Type: multipart/form-data; boundary={b}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let (status, head, _) = raw(port, req.as_bytes())?;
    Ok((status, head))
}

/// A GET with a `Range` header.
fn ranged(
    port: u16,
    sid: &str,
    path: &str,
    range: Option<&str>,
) -> Result<(u16, String, Vec<u8>), String> {
    let mut req = format!(
        "GET {PREFIX}{path} HTTP/1.1\r\nHost: qs400:2003\r\nConnection: close\r\nCookie: QS400SID={sid}\r\n"
    );
    if let Some(r) = range {
        let _ = write!(req, "Range: {r}\r\n");
    }
    req.push_str("\r\n");
    raw(port, req.as_bytes())
}

/// The first spooled file's id on a Printer output page.
fn first_splf(body: &str) -> Option<String> {
    let at = body.find("name=\"s0\" value=\"")? + "name=\"s0\" value=\"".len();
    let end = body[at..].find('"')?;
    Some(body[at..at + end].to_string())
}

/// Files as the new profile.
fn files(s: &mut Session, port: u16) -> Result<String, String> {
    let sid = sign_on(port, USER, PASSWORD)?;
    let page = afw(port, "GET", "/files", Some(&sid), None)?;
    want(
        &page,
        200,
        &[
            "<h1>Browse files</h1>",
            &format!("value=\"{HOME}\""),
            "Transfer queue",
        ],
        "Browse files",
    )?;
    let csrf = csrf_in(&page.body).ok_or_else(|| format!("no form field:\n{}", page.brief()))?;
    let (status, head) = upload(port, &sid, &csrf, "hello156.txt", CONTENT)?;
    if status != 303 {
        return Err(format!("the upload: status {status}\n{head}"));
    }
    let after = afw(port, "GET", "/files", Some(&sid), None)?;
    want(
        &after,
        200,
        &["XWA0040", "hello156.txt"],
        "the upload's message and entry",
    )?;
    // The system's view: the user owns it.
    let aut = s.command(&format!("DSPAUT OBJ('{HOME}/hello156.txt')"), CMD)?;
    if !aut.contains(USER) || aut.contains("QTMHHTTP") {
        return Err(format!("DSPAUT of the uploaded file:\n{aut}"));
    }
    let path = format!("/files/download?path={HOME}/hello156.txt");
    let (st, h, body) = ranged(port, &sid, &path, None)?;
    if st != 200 || body != CONTENT.as_bytes() || !h.contains("Accept-Ranges: bytes") {
        return Err(format!(
            "the download: {st}\n{h}\n{}",
            String::from_utf8_lossy(&body)
        ));
    }
    let (st, h, body) = ranged(port, &sid, &path, Some("bytes=6-"))?;
    let want_part = &CONTENT.as_bytes()[6..];
    let total = CONTENT.len();
    if st != 206
        || body != want_part
        || !h.contains(&format!("Content-Range: bytes 6-{}/{total}", total - 1))
    {
        return Err(format!(
            "the ranged download: {st}\n{h}\n{}",
            String::from_utf8_lossy(&body)
        ));
    }
    let edit = afw(
        port,
        "GET",
        &format!("/files/edit?path={HOME}/hello156.txt"),
        Some(&sid),
        None,
    )?;
    want(&edit, 200, &["Second line.", "name=\"save\""], "the editor")?;
    Ok(format!(
        "{USER} uploaded hello156.txt (DSPAUT: owner {USER}), downloaded it whole and from byte 6 (206)"
    ))
}

/// Printer output as QSECOFR: a spooled file listed, viewed, held,
/// deleted.
fn print(s: &mut Session, port: u16) -> Result<String, String> {
    s.command("DSPLIBL OUTPUT(*PRINT)", CMD)?;
    let sid = sign_on(port, "QSECOFR", SECOFR_PASSWORD)?;
    let list = afw(port, "GET", "/print?u=*ALL", Some(&sid), None)?;
    want(
        &list,
        200,
        &["<h1>Printer output</h1>", "name=\"hold\""],
        "Printer output",
    )?;
    let id = first_splf(&list.body)
        .ok_or_else(|| format!("no spooled file listed:\n{}", list.brief()))?;
    let file = id.split('-').nth(3).unwrap_or("").to_string();
    let view = afw(port, "GET", &format!("/print/view/{id}"), Some(&sid), None)?;
    want(
        &view,
        200,
        &[
            "<pre class=\"out out132\">",
            &format!("Spooled file: {file}"),
        ],
        "the viewer",
    )?;
    let csrf = csrf_in(&list.body).ok_or_else(|| format!("no form field:\n{}", list.brief()))?;
    let form = format!("csrf={}&s0={}&hold=Hold&hw=now", enc(&csrf), enc(&id));
    let r = afw(port, "POST", "/print?u=*ALL", Some(&sid), Some(&form))?;
    want(&r, 303, &[], "Hold")?;
    let held = afw(port, "GET", "/print?u=*ALL", Some(&sid), None)?;
    want(
        &held,
        200,
        &["QAD6002", &format!("Spooled file {file} held.")],
        "the hold's message",
    )?;
    let wrk = s.command("WRKSPLF SELECT(*ALL)", CMD)?;
    if !wrk.lines().any(|l| l.contains(&file) && l.contains("HLD")) {
        return Err(format!("WRKSPLF after the hold:\n{wrk}"));
    }
    let confirm = afw(
        port,
        "GET",
        &format!("/print/confirm?s={}", enc(&id)),
        Some(&sid),
        None,
    )?;
    want(
        &confirm,
        200,
        &["Confirm Delete", &file],
        "Delete's confirmation",
    )?;
    let csrf =
        csrf_in(&confirm.body).ok_or_else(|| format!("no form field:\n{}", confirm.brief()))?;
    let form = format!("csrf={}&ok=OK", enc(&csrf));
    let r = afw(
        port,
        "POST",
        &format!("/print/confirm?s={}", enc(&id)),
        Some(&sid),
        Some(&form),
    )?;
    want(&r, 303, &[], "Delete")?;
    let gone = afw(port, "GET", "/print?u=*ALL", Some(&sid), None)?;
    want(&gone, 200, &["QAD6004"], "the delete's message")?;
    Ok(format!(
        "QSECOFR viewed, held (WRKSPLF HLD) and deleted {file}"
    ))
}

impl Suite {
    /// Step 156: Access for Web's Files and Printer output.
    pub(crate) fn access_web_files(&self, s: &mut Session) -> Result<String, String> {
        secofr_password(s)?;
        s.command(
            &format!("CRTUSRPRF USRPRF({USER}) PASSWORD({PASSWORD}) USRCLS(*USER) PWDEXP(*NO) HOMEDIR('{HOME}') TEXT('Step 156')"),
            CMD,
        )?;
        let made = s.command(&format!("MKDIR DIR('{HOME}')"), CMD)?;
        if made.contains("CPF") && !made.contains("CPFA0A0") {
            return Err(format!("MKDIR {HOME}: {made}"));
        }
        let own = s.command(&format!("CHGOWN OBJ('{HOME}') NEWOWN({USER})"), CMD)?;
        if own.contains("CPF") {
            return Err(format!("CHGOWN {HOME}: {own}"));
        }
        let end = Instant::now() + Duration::from_secs(60);
        loop {
            let started = s.command("STRTCPSVR SERVER(*HTTP) HTTPSVR(ACCESSWEB)", CMD)?;
            if started.contains("TCP1A07")
                || started.contains("TCP1A0B")
                || (started.contains("TCP1A08") && Instant::now() > end)
            {
                break;
            }
            if !started.contains("TCP1A08") {
                return Err(format!("STRTCPSVR ACCESSWEB:\n{started}"));
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        let port = forward_port(self, GUEST, 2003)?;
        let f = files(s, port)?;
        let p = print(s, port)?;
        let ended = s.command("ENDTCPSVR SERVER(*HTTP) HTTPSVR(ACCESSWEB)", CMD)?;
        if !ended.contains("TCP1A0A") {
            return Err(format!("ENDTCPSVR ACCESSWEB:\n{ended}"));
        }
        s.command(&format!("DLTUSRPRF USRPRF({USER}) OWNOBJOPT(*DLT)"), CMD)?;
        Ok(format!("{f}; {p}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_fields() {
        assert_eq!(
            first_splf("<input type=\"checkbox\" name=\"s0\" value=\"1-A-B-QPRTLIBL-1\">")
                .as_deref(),
            Some("1-A-B-QPRTLIBL-1")
        );
        assert_eq!(csrf_in("name=\"csrf\" value=\"x\"").as_deref(), Some("x"));
    }
}
