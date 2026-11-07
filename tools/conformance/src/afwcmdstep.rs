// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 157: Access for Web's messages, jobs and commands from live
//! state. `HTTPSVR(ACCESSWEB)` starts; from the host over HTTP on 2003
//! `QSECOFR` signs on, runs a command that sends a message and one that
//! makes a data area (owned by `QSECOFR`, not `QTMHHTTP`: it ran under
//! the user's profile), sees the message, sends an inquiry that opens
//! Message Arrived on the next page and replies to it, submits a job to
//! `QBATCH` and finds it in My jobs, prompts `WRKACTJOB` and runs
//! `DLTDTAARA` from its prompt; then signs off and the instance ends.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use crate::httpadm::{Reply, enc, http_at, secofr_password, session_cookie, want};
use crate::phase5::SECOFR_PASSWORD;
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};
use crate::tcp::{GUEST, forward_port};

/// The product's URLs.
const PREFIX: &str = "/qs400/accessweb";
/// The data area the commands make and delete.
const DTAARA: &str = "QGPL/S157DTA";

fn afw(
    port: u16,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    form: Option<&str>,
) -> Result<Reply, String> {
    http_at(PREFIX, port, method, path, cookie, form)
}

/// The value of hidden or text field `name` (`name="x" value="..."`).
fn field(body: &str, name: &str) -> Option<String> {
    let pat = format!("name=\"{name}\" value=\"");
    let at = body.find(&pat)? + pat.len();
    let end = body[at..].find('"')?;
    Some(body[at..at + end].to_string())
}

/// The page's CSRF field, read from a GET of `path`.
fn token(port: u16, path: &str, sid: &str) -> Result<String, String> {
    let p = afw(port, "GET", path, Some(sid), None)?;
    field(&p.body, "csrf").ok_or_else(|| format!("no form field on {path}:\n{}", p.brief()))
}

/// POST `fields` to `path` with the form field from a GET of it.
fn post(port: u16, path: &str, sid: &str, fields: &[(&str, &str)]) -> Result<Reply, String> {
    let t = token(port, path, sid)?;
    let mut form = format!("csrf={}", enc(&t));
    for (k, v) in fields {
        let _ = write!(form, "&{k}={}", enc(v));
    }
    afw(port, "POST", path, Some(sid), Some(&form))
}

/// Run `command` on Run command and wait: the page after.
fn run(port: u16, sid: &str, command: &str, how: &str) -> Result<Reply, String> {
    let r = post(
        port,
        "/cmd",
        sid,
        &[
            ("cmd", command),
            ("how", how),
            ("jobq", "QGPL/QBATCH"),
            ("run", "Run"),
        ],
    )?;
    if r.status != 303 {
        return Err(format!("running {command}:\n{}", r.brief()));
    }
    let to = r
        .header("Location")
        .unwrap_or("/qs400/accessweb/cmd")
        .to_string();
    afw(
        port,
        "GET",
        to.strip_prefix(PREFIX).unwrap_or(&to),
        Some(sid),
        None,
    )
}

/// The text from `needle` to the end of its line of HTML.
fn around(body: &str, needle: &str) -> String {
    body.find(needle).map_or_else(
        || format!("no {needle}"),
        |at| body[at..].chars().take(300).collect(),
    )
}

/// Sign on as `QSECOFR`: the session cookie.
fn sign_on(port: u16) -> Result<String, String> {
    let form = format!("user=QSECOFR&password={}", enc(SECOFR_PASSWORD));
    let ok = afw(port, "POST", "/signon", None, Some(&form))?;
    if ok.status != 303 {
        return Err(format!("signing on:\n{}", ok.brief()));
    }
    session_cookie(&ok).ok_or_else(|| format!("no session cookie:\n{}", ok.brief()))
}

/// Commands under the user's profile, a message seen.
fn commands(s: &mut Session, port: u16, sid: &str) -> Result<(), String> {
    let _ = s.command(&format!("DLTDTAARA DTAARA({DTAARA})"), CMD);
    let p = run(
        port,
        sid,
        "SNDMSG MSG('Step 157 hello') TOUSR(QSECOFR)",
        "int",
    )?;
    want(
        &p,
        200,
        &[
            "<h1>Run command</h1>",
            "QAD5010</a> - Command SNDMSG completed.",
        ],
        "SNDMSG on Run command",
    )?;
    let p = run(
        port,
        sid,
        &format!("CRTDTAARA DTAARA({DTAARA}) TYPE(*CHAR) LEN(10) VALUE('S157')"),
        "int",
    )?;
    want(
        &p,
        200,
        &[
            "QAD5010</a> - Command CRTDTAARA completed.",
            "Recent commands",
        ],
        "CRTDTAARA on Run command",
    )?;
    let aut = s.command(&format!("DSPOBJAUT OBJ({DTAARA}) OBJTYPE(*DTAARA)"), CMD)?;
    if !aut.contains("QSECOFR") || aut.contains("QTMHHTTP") {
        return Err(format!(
            "the data area Run command made is not QSECOFR's:\n{aut}"
        ));
    }
    let bad = run(port, sid, "NOSUCH157 X(1)", "int")?;
    want(
        &bad,
        200,
        &["QAD5013</a> - Command NOSUCH157 not found.", "CPD0030"],
        "a command that is not there",
    )?;
    let m = afw(port, "GET", "/messages", Some(sid), None)?;
    want(
        &m,
        200,
        &["<h1>Messages</h1>", "Step 157 hello"],
        "the message on Messages",
    )
}

/// An inquiry sent from Messages opens Message Arrived; the reply.
fn inquiry(port: u16, sid: &str) -> Result<(), String> {
    let r = post(
        port,
        "/messages",
        sid,
        &[
            ("to", "user"),
            ("tou", "QSECOFR"),
            ("toq", ""),
            ("m", "Step 157 question (Y N)"),
            ("i", "yes"),
            ("send", "Send"),
        ],
    )?;
    want(&r, 303, &[], "Send message")?;
    let sent = afw(port, "GET", "/messages", Some(sid), None)?;
    if !sent.body.contains("QAD9004</a> - Message sent to QSECOFR.")
        || !sent.body.contains("Step 157 question (Y N)</b>")
    {
        return Err(format!(
            "the inquiry sent: {}",
            around(&sent.body, "Step 157 question")
        ));
    }
    // Seen on Messages already: another inquiry opens Message Arrived.
    let r = post(
        port,
        "/messages",
        sid,
        &[
            ("to", "user"),
            ("tou", "QSECOFR"),
            ("toq", ""),
            ("m", "Step 157 arrives (Y N)"),
            ("i", "yes"),
            ("send", "Send"),
        ],
    )?;
    want(&r, 303, &[], "Send message")?;
    let home = afw(port, "GET", "/home", Some(sid), None)?;
    let to = home.header("Location").unwrap_or("").to_string();
    if home.status != 303 || !to.contains("/messages/arrived") {
        return Err(format!(
            "no Message Arrived after an inquiry came:\n{}",
            home.brief()
        ));
    }
    let path = to.strip_prefix(PREFIX).unwrap_or(&to).to_string();
    let page = afw(port, "GET", &path, Some(sid), None)?;
    want(
        &page,
        200,
        &[
            "<h1>Message arrived</h1>",
            "Step 157 arrives (Y N)",
            "valid: Y N",
        ],
        "Message Arrived",
    )?;
    let key = field(&page.body, "key")
        .ok_or_else(|| format!("no key on Message Arrived:\n{}", page.brief()))?;
    let queue = field(&page.body, "queue").unwrap_or_default();
    let r = post(
        port,
        &path,
        sid,
        &[
            ("key", &key),
            ("queue", &queue),
            ("r", "Y"),
            ("reply", "Reply"),
        ],
    )?;
    want(&r, 303, &[], "Reply on Message Arrived")?;
    let back = afw(port, "GET", "/home", Some(sid), None)?;
    want(
        &back,
        200,
        &["QAD9002</a> - Reply sent to"],
        "the reply sent",
    )?;
    let m = afw(port, "GET", "/messages", Some(sid), None)?;
    if m.body.contains("Step 157 arrives (Y N)</b>") {
        return Err(format!(
            "the inquiry still waits:\n{}",
            around(&m.body, "Step 157 arrives")
        ));
    }
    Ok(())
}

/// A job submitted to QBATCH, then in My jobs.
fn jobs(port: u16, sid: &str) -> Result<String, String> {
    let p = run(port, sid, "DLYJOB DLY(120)", "batch")?;
    want(
        &p,
        200,
        &["XWA0050</a> - Command submitted to job queue QGPL/QBATCH as job "],
        "SBMJOB from Run command",
    )?;
    let at = p.body.find(" as job ").map_or(0, |a| a + " as job ".len());
    let job: String = p.body[at..].chars().take_while(|c| *c != '.').collect();
    let number = job.split('/').next().unwrap_or("").to_string();
    if number.len() != 6 {
        return Err(format!(
            "no job in the submit's message: {}",
            around(&p.body, "XWA0050")
        ));
    }
    let mine = afw(port, "GET", "/jobs", Some(sid), None)?;
    want(
        &mine,
        200,
        &[
            "<h1>My jobs</h1>",
            &format!("<td class=\"mono\">{number}</td>"),
        ],
        "the job in My jobs",
    )?;
    let server = afw(port, "GET", "/jobs/server", Some(sid), None)?;
    want(
        &server,
        200,
        &[
            "<h1>Server jobs</h1>",
            "<td class=\"mono\">ACCESSWEB</td>",
            "<td class=\"mono\">QTMHHTTP</td>",
        ],
        "Server jobs",
    )?;
    Ok(job)
}

/// The web F4 of WRKACTJOB, then DLTDTAARA run from its prompt.
fn prompt(s: &mut Session, port: u16, sid: &str) -> Result<(), String> {
    let p = afw(
        port,
        "GET",
        "/cmd/prompt?cmd=WRKACTJOB%20SBS(QHTTPSVR)",
        Some(sid),
        None,
    )?;
    want(
        &p,
        200,
        &[
            "Prompt: Work with Active Jobs (WRKACTJOB)",
            "name=\"sbs1\" value=\"QHTTPSVR\"",
            "Command string: <span class=\"mono\">WRKACTJOB OUTPUT(*)",
        ],
        "the prompt of WRKACTJOB",
    )?;
    let path = format!("/cmd/prompt?cmd=DLTDTAARA%20DTAARA({DTAARA})");
    let p = afw(port, "GET", &path, Some(sid), None)?;
    want(
        &p,
        200,
        &[&format!("name=\"dtaara\" value=\"{DTAARA}\"")],
        "the prompt of DLTDTAARA",
    )?;
    let r = post(port, &path, sid, &[("dtaara", DTAARA), ("run", "Run")])?;
    want(&r, 303, &[], "Run from the prompt")?;
    let chk = s.command(&format!("CHKOBJ OBJ({DTAARA}) OBJTYPE(*DTAARA)"), CMD)?;
    if !chk.contains("CPF9801") {
        return Err(format!(
            "DLTDTAARA from the prompt left the data area:\n{chk}"
        ));
    }
    Ok(())
}

impl Suite {
    /// Step 157: Access for Web's messages, jobs and commands.
    pub(crate) fn access_web_work(&self, s: &mut Session) -> Result<String, String> {
        secofr_password(s)?;
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
        let sid = sign_on(port)?;
        commands(s, port, &sid)?;
        inquiry(port, &sid)?;
        let job = jobs(port, &sid)?;
        prompt(s, port, &sid)?;
        let _ = s.command(&format!("ENDJOB JOB({job}) OPTION(*IMMED)"), CMD);
        let off = afw(port, "GET", "/signoff", Some(&sid), None)?;
        want(&off, 303, &[], "sign-off")?;
        let ended = s.command("ENDTCPSVR SERVER(*HTTP) HTTPSVR(ACCESSWEB)", CMD)?;
        if !ended.contains("TCP1A0A") {
            return Err(format!("ENDTCPSVR ACCESSWEB:\n{ended}"));
        }
        Ok(format!(
            "signed on as QSECOFR over 2003 (host {port}); SNDMSG and CRTDTAARA run (the data area QSECOFR's), a missing command QAD5013; the message on Messages; an inquiry sent, a second opened Message Arrived and was answered (QAD9002); job {job} submitted to QBATCH (XWA0050) and listed in My jobs; Server jobs list ACCESSWEB; WRKACTJOB prompted, DLTDTAARA run from its prompt"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields() {
        assert_eq!(
            field(
                "<input type=\"hidden\" name=\"key\" value=\"0000A1F3\">",
                "key"
            )
            .as_deref(),
            Some("0000A1F3")
        );
        assert_eq!(field("nothing", "key"), None);
    }
}
