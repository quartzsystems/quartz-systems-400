// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 154: the 5250 session of QS/400 Access for Web against the live
//! system. `HTTPSVR(ACCESSWEB)` starts; a new profile signs on to Access
//! for Web over 2003, saves a session to this system (the store under
//! `/www/accessweb/sessions`), starts it: the `ACCESSWEB` job's TN5250E
//! client reaches the system's own Telnet server and the page draws its
//! sign-on display (`screen.png`, `screen.txt`). The profile signs on at
//! that display through the page's no-script controls (Paste, Field
//! Exit, Enter on the keypad) and reaches the Main Menu; the WebSocket
//! answers its handshake with the screen; Print Screen is a PDF; the
//! `.qsd` downloads and reads; Disconnect, Delete, and the instance ends.

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use crate::httpadm::{Reply, enc, http_at, secofr_password, session_cookie, want};
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};
use crate::tcp::{GUEST, forward_port};

const PREFIX: &str = "/qs400/accessweb";
const USER: &str = "AFW154";
const PASSWORD: &str = "AFW154PW";

fn afw(
    port: u16,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    form: Option<&str>,
) -> Result<Reply, String> {
    http_at(PREFIX, port, method, path, cookie, form)
}

fn csrf_in(body: &str) -> Option<String> {
    let at = body.find("name=\"csrf\" value=\"")? + "name=\"csrf\" value=\"".len();
    let end = body[at..].find('"')?;
    Some(body[at..at + end].to_string())
}

fn system_in(body: &str) -> Option<String> {
    let at = body.find("System: <b>")? + "System: <b>".len();
    let end = body[at..].find("</b>")?;
    Some(body[at..at + end].trim().to_string()).filter(|n| !n.is_empty())
}

/// POST `fields` (urlencoded) with the page's form field.
fn post(
    port: u16,
    page: &str,
    path: &str,
    sid: &str,
    fields: &[(&str, &str)],
) -> Result<Reply, String> {
    let p = afw(port, "GET", page, Some(sid), None)?;
    let t = csrf_in(&p.body).ok_or_else(|| format!("no form field on {page}:\n{}", p.brief()))?;
    let mut form = format!("csrf={}", enc(&t));
    for (k, v) in fields {
        let _ = write!(form, "&{k}={}", enc(v));
    }
    afw(port, "POST", path, Some(sid), Some(&form))
}

/// The display's text, until `ok` holds or `secs` pass.
fn screen_until(
    port: u16,
    sid: &str,
    id: u32,
    secs: u64,
    ok: impl Fn(&str) -> bool,
) -> Result<String, String> {
    let end = Instant::now() + Duration::from_secs(secs);
    loop {
        let r = afw(
            port,
            "GET",
            &format!("/5250/session/{id}/screen.txt"),
            Some(sid),
            None,
        )?;
        if r.status == 200 && ok(&r.body) {
            return Ok(r.body);
        }
        if Instant::now() > end {
            return Err(format!(
                "the display did not get there; it shows:\n{}",
                r.brief()
            ));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// The WebSocket's handshake and its first message.
fn websocket(port: u16, sid: &str, id: u32) -> Result<String, String> {
    let mut c =
        TcpStream::connect(("127.0.0.1", port)).map_err(|e| format!("WebSocket connect: {e}"))?;
    let _ = c.set_read_timeout(Some(Duration::from_secs(10)));
    let req = format!(
        "GET {PREFIX}/5250/ws/{id} HTTP/1.1\r\nHost: qs400:2003\r\nCookie: QS400SID={sid}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    c.write_all(req.as_bytes())
        .map_err(|e| format!("WebSocket send: {e}"))?;
    let mut got = Vec::new();
    let mut buf = [0u8; 16384];
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end {
        match c.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => got.extend_from_slice(&buf[..n]),
        }
        let t = String::from_utf8_lossy(&got);
        if t.contains("\"t\":\"screen\"") && t.contains("\"cells\"") && t.contains("]]}") {
            break;
        }
    }
    let t = String::from_utf8_lossy(&got).into_owned();
    if !t.starts_with("HTTP/1.1 101")
        || !t.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=")
        || !t.contains("\"t\":\"screen\"")
    {
        return Err(format!(
            "the WebSocket answered:\n{}",
            t.chars().take(1500).collect::<String>()
        ));
    }
    Ok(t)
}

impl Suite {
    /// Step 154: the web 5250 session against the system's Telnet server.
    pub(crate) fn access_web_5250(&self, s: &mut Session) -> Result<String, String> {
        secofr_password(s)?;
        let _ = s.command(&format!("DLTUSRPRF USRPRF({USER})"), CMD);
        s.command(
            &format!("CRTUSRPRF USRPRF({USER}) PASSWORD({PASSWORD}) USRCLS(*USER) PWDEXP(*NO) INLMNU(MAIN) TEXT('Step 154')"),
            CMD,
        )?;
        let _ = s.command("STRTCPSVR SERVER(*TELNET)", CMD);
        let end = Instant::now() + Duration::from_secs(60);
        loop {
            let started = s.command("STRTCPSVR SERVER(*HTTP) HTTPSVR(ACCESSWEB)", CMD)?;
            if started.contains("TCP1A07") || (started.contains("TCP1A08") && Instant::now() > end)
            {
                break;
            }
            if !started.contains("TCP1A08") {
                return Err(format!("STRTCPSVR ACCESSWEB:\n{started}"));
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        let port = forward_port(self, GUEST, 2003)?;
        let form = format!("user={USER}&password={}", enc(PASSWORD));
        let ok = afw(port, "POST", "/signon", None, Some(&form))?;
        let sid = session_cookie(&ok)
            .ok_or_else(|| format!("signing on to Access for Web:\n{}", ok.brief()))?;
        let page = afw(port, "GET", "/5250", Some(&sid), None)?;
        want(
            &page,
            200,
            &[
                "<h2>Saved sessions</h2>",
                "You have no saved sessions.",
                "name=\"sys\"",
            ],
            "5250 session",
        )?;
        let system =
            system_in(&page.body).ok_or_else(|| format!("no system name:\n{}", page.brief()))?;
        // Save a session to this system, then start it.
        let saved = post(
            port,
            "/5250",
            "/5250/session/new",
            &sid,
            &[
                ("n", "Step 154"),
                ("sys", &format!("{system} (this system)")),
                ("sz", "24x80"),
                ("cp", "37 - USA, Canada"),
                ("dev", "*DFT"),
                ("col", "console"),
                ("kp", "on"),
                ("ac", "on"),
                ("h1", "on"),
                ("h2", "on"),
                ("km", "Default"),
                ("save", "Save session"),
            ],
        )?;
        want(&saved, 303, &[], "Save session")?;
        let list = afw(port, "GET", "/5250", Some(&sid), None)?;
        want(
            &list,
            200,
            &["XWA0010", "<td>Step 154</td>", "/5250/session/1\""],
            "the saved session",
        )?;
        let qsd = afw(port, "GET", "/5250/1/download", Some(&sid), None)?;
        want(
            &qsd,
            200,
            &["[qsaccess]", "kind = \"display\"", "title = \"Step 154\""],
            "the .qsd file",
        )?;
        if qsd.body.contains("[accessweb]") {
            return Err(format!(
                "the download carries the server's own table:\n{}",
                qsd.brief()
            ));
        }
        let display = afw(port, "GET", "/5250/session/1", Some(&sid), None)?;
        want(
            &display,
            200,
            &[
                "5250 display: Step 154",
                &format!("{system}: Connected"),
                "/5250/script.js",
                "value=\"F24\"",
            ],
            "the display",
        )?;
        let first = screen_until(port, &sid, 1, 30, |t| t.contains("Sign On"))?;
        let png = afw(port, "GET", "/5250/session/1/screen.png", Some(&sid), None)?;
        if png.status != 200
            || png.header("Content-Type").is_none_or(|c| c != "image/png")
            || !png.body.contains("PNG")
        {
            return Err(format!("the picture:\n{}", png.head));
        }
        let ws = websocket(port, &sid, 1)?;
        if !ws.contains("Sign On") {
            return Err(format!(
                "the WebSocket's screen is not the display's:\n{}",
                ws.chars().take(1500).collect::<String>()
            ));
        }
        // Sign on at the display, as a browser without script does.
        let page1 = "/5250/session/1/paste";
        let typed = post(port, page1, page1, &sid, &[("t", USER), ("ok", "Paste")])?;
        want(&typed, 303, &[], "Paste the user")?;
        let fe = post(
            port,
            "/5250/session/1",
            "/5250/session/1",
            &sid,
            &[("k", "FldExit")],
        )?;
        want(&fe, 303, &[], "Field Exit")?;
        let pw = post(
            port,
            page1,
            page1,
            &sid,
            &[("t", PASSWORD), ("ok", "Paste")],
        )?;
        want(&pw, 303, &[], "Paste the password")?;
        let enter = post(
            port,
            "/5250/session/1",
            "/5250/session/1",
            &sid,
            &[("k", "Enter")],
        )?;
        want(&enter, 303, &[], "Enter")?;
        let menu = screen_until(port, &sid, 1, 45, |t| {
            t.contains("Main Menu") || t.contains("Sign-on Information")
        })?;
        if menu.contains(PASSWORD) {
            return Err("the password shows on the screen's text".into());
        }
        let pdf = afw(port, "GET", "/5250/session/1/print", Some(&sid), None)?;
        if pdf.status != 200 || !pdf.body.starts_with("%PDF-1.4") {
            return Err(format!("Print Screen:\n{}", pdf.head));
        }
        let off = post(
            port,
            "/5250/session/1",
            "/5250/session/1",
            &sid,
            &[("tb", "Disconnect")],
        )?;
        want(&off, 303, &[], "Disconnect")?;
        let gone = afw(port, "GET", "/5250/session/1", Some(&sid), None)?;
        want(
            &gone,
            200,
            &[&format!("{system}: Not connected"), "QAD1006"],
            "the display after Disconnect",
        )?;
        let del = post(
            port,
            "/5250/1/delete/confirm",
            "/5250/1/delete/confirm",
            &sid,
            &[("ok", "OK")],
        )?;
        want(&del, 303, &[], "Delete")?;
        let after = afw(port, "GET", "/5250", Some(&sid), None)?;
        want(
            &after,
            200,
            &["XWA0011", "You have no saved sessions."],
            "the list after Delete",
        )?;
        let ended = s.command("ENDTCPSVR SERVER(*HTTP) HTTPSVR(ACCESSWEB)", CMD)?;
        if !ended.contains("TCP1A0A") {
            return Err(format!("ENDTCPSVR ACCESSWEB:\n{ended}"));
        }
        s.command(&format!("DLTUSRPRF USRPRF({USER})"), CMD)?;
        let shown = first
            .lines()
            .find(|l| l.contains("Sign On"))
            .unwrap_or("")
            .trim()
            .to_string();
        Ok(format!(
            "{USER} signed on to Access for Web over 2003 (host {port}); session Step 154 saved, downloaded as .qsd and started: the ACCESSWEB job's TN5250E client drew {system}'s \"{shown}\" (screen.png, the WebSocket's first screen); {USER} signed on through the page's Paste, Field Exit and Enter and reached the Main Menu; Print Screen a PDF; Disconnect (QAD1006), Delete (XWA0011); the instance ended"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_fields() {
        assert_eq!(
            csrf_in("<input type=\"hidden\" name=\"csrf\" value=\"ab12\">").as_deref(),
            Some("ab12")
        );
        assert_eq!(system_in("System: <b>QS400</b>").as_deref(), Some("QS400"));
    }
}
