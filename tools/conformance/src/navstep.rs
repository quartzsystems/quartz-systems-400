// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 88: Navigator from live state. Navigator is an instance of its
//! own, off until started: `STRTCPSVR *HTTP HTTPSVR(NAVIGATOR)` starts
//! `NAVIGATOR` (QS00NAV is installed); from the host, over HTTP on
//! 2002: `QSECOFR` signs on, System status names the system, Active
//! jobs lists the `NAVIGATOR` job itself (which `DSPJOB` knows), the
//! `QSYSOPR` queue loads, Work management's lists and each printer's
//! page load with the `QSCAPTURE` writer started (the job once died on
//! Printers), a POST without the form field is refused,
//! Run SQL returns a result table from `QSYS2.SYSTABLES`, Licensed
//! programs lists `QS00SS1` and `QS00NAV`, PTFs and the audit journal
//! load, sign-off; then the sign-on page over TLS on 2005;
//! `ENDTCPSVR ... HTTPSVR(NAVIGATOR)` ends the job.

use std::time::{Duration, Instant};

use crate::httpadm::{Reply, enc, http_at, secofr_password, session_cookie, tls_get, want};
use crate::phase5::SECOFR_PASSWORD;
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};
use crate::tcp::{GUEST, forward_port};

/// The product's URLs.
const PREFIX: &str = "/qs400/navigator";

fn nav(port: u16, method: &str, path: &str, cookie: Option<&str>, form: Option<&str>) -> Result<Reply, String> {
    http_at(PREFIX, port, method, path, cookie, form)
}

/// What step 88 runs on Run SQL.
const QUERY: &str = "SELECT * FROM QSYS2.SYSTABLES FETCH FIRST 5 ROWS ONLY";

/// The form field every POST carries (`name="csrf" value="..."`).
fn csrf_in(body: &str) -> Option<String> {
    let at = body.find("name=\"csrf\" value=\"")? + "name=\"csrf\" value=\"".len();
    let end = body[at..].find('"')?;
    Some(body[at..at + end].to_string())
}

/// The system name in the header (`System: <b>NAME</b>`).
fn system_in(body: &str) -> Option<String> {
    let at = body.find("System: <b>")? + "System: <b>".len();
    let end = body[at..].find("</b>")?;
    let n = body[at..at + end].trim();
    (!n.is_empty()).then(|| n.to_string())
}

/// `nnnnnn/QTMHHTTP/NAVIGATOR` from a link `/job/nnnnnn/QTMHHTTP/NAVIGATOR`.
fn nav_job(body: &str) -> Option<String> {
    let at = body.find("/QTMHHTTP/NAVIGATOR")?;
    let number = body.get(at.checked_sub(6)?..at)?;
    number
        .bytes()
        .all(|b| b.is_ascii_digit())
        .then(|| format!("{number}/QTMHHTTP/NAVIGATOR"))
}

impl Suite {
    /// Step 88: Navigator over HTTP and HTTPS.
    pub(crate) fn navigator(&self, s: &mut Session) -> Result<String, String> {
        secofr_password(s)?;
        // Step 87's ENDTCPSVR may still be ending the jobs: "already
        // started" until they are gone.
        let end = Instant::now() + Duration::from_secs(60);
        loop {
            let started = s.command("STRTCPSVR SERVER(*HTTP) HTTPSVR(NAVIGATOR)", CMD)?;
            if started.contains("TCP1A07") || (started.contains("TCP1A08") && Instant::now() > end) {
                break;
            }
            if !started.contains("TCP1A08") {
                return Err(format!("STRTCPSVR NAVIGATOR:\n{started}"));
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        let plain = forward_port(self, GUEST, 2002)?;
        let page = nav(plain, "GET", "/signon", None, None)?;
        want(&page, 200, &["Navigator for QS/400", "name=\"password\""], "the sign-on page")?;
        let form = format!("user=QSECOFR&password={}", enc(SECOFR_PASSWORD));
        let ok = nav(plain, "POST", "/signon", None, Some(&form))?;
        if ok.status != 303 || !ok.header("Location").is_some_and(|l| l.ends_with("/home")) {
            return Err(format!("signing on as QSECOFR:\n{}", ok.brief()));
        }
        let sid = session_cookie(&ok).ok_or_else(|| format!("no session cookie:\n{}", ok.brief()))?;
        // System status, with the system's name.
        let home = nav(plain, "GET", "/home", Some(&sid), None)?;
        want(&home, 200, &["<h1>System status</h1>"], "System status")?;
        let name = system_in(&home.body).ok_or_else(|| format!("no system name:\n{}", home.brief()))?;
        let neta = s.command("DSPNETA", CMD)?;
        if !neta.contains(&name) {
            return Err(format!("System status names {name}; DSPNETA:\n{neta}"));
        }
        // Active jobs: this job, in a job the system knows.
        let jobs = nav(plain, "GET", "/jobs?user=QTMHHTTP", Some(&sid), None)?;
        want(&jobs, 200, &["NAVIGATOR"], "Active jobs")?;
        let job = nav_job(&jobs.body).ok_or_else(|| format!("no NAVIGATOR job on Active jobs:\n{}", jobs.brief()))?;
        let shown = s.command(&format!("dspjob JOB({job})"), CMD)?;
        if !shown.contains("NAVIGATOR") || shown.contains("CPF1070") {
            return Err(format!("dspjob of the page's job {job}:\n{shown}"));
        }
        // The operator's queue.
        let opr = nav(plain, "GET", "/msgq/QSYS/QSYSOPR", Some(&sid), None)?;
        want(&opr, 200, &["Message queue: QSYSOPR"], "QSYSOPR")?;
        let live = live_config(s, plain, &sid)?;
        let work = work_pages(s, plain, &sid)?;
        // A POST without the form's field is refused.
        let forged = nav(plain, "POST", "/mymsgs", Some(&sid), Some("send=Send"))?;
        want(&forged, 403, &["out of date"], "a POST without the form field")?;
        // Run SQL over the catalog, then the software and audit pages.
        let sql = nav(plain, "GET", "/sql", Some(&sid), None)?;
        let token = csrf_in(&sql.body).ok_or_else(|| format!("no form field on Run SQL:\n{}", sql.brief()))?;
        let form = format!("csrf={}&sql={}&run=Run", enc(&token), enc(QUERY));
        let ran = nav(plain, "POST", "/sql", Some(&sid), Some(&form))?;
        want(&ran, 303, &[], "Run SQL's POST")?;
        let result = nav(plain, "GET", "/sql", Some(&sid), None)?;
        want(&result, 200, &["class=\"list\"", "TABLE_NAME", "SQL7958"], "Run SQL's result table")?;
        let lp = nav(plain, "GET", "/licpgm", Some(&sid), None)?;
        want(&lp, 200, &["Licensed programs", "QS00SS1", "QS00NAV"], "Licensed programs")?;
        let ptf = nav(plain, "GET", "/ptf", Some(&sid), None)?;
        want(&ptf, 200, &["PTFs"], "PTFs")?;
        let audit = nav(plain, "GET", "/audit", Some(&sid), None)?;
        want(&audit, 200, &["Audit journal"], "the audit journal")?;
        let off = nav(plain, "GET", "/signoff", Some(&sid), None)?;
        want(&off, 303, &["Max-Age=0"], "sign-off")?;
        let after = nav(plain, "GET", "/home", Some(&sid), None)?;
        if after.status != 303 || !after.header("Location").is_some_and(|l| l.ends_with("/signon")) {
            return Err(format!("the session outlived sign-off:\n{}", after.brief()));
        }
        // TLS on 2005: the certificate of QS400_HTTP_SERVER_NAVIGATOR,
        // else the system's own, so it always answers.
        let secure = forward_port(self, GUEST, 2005)?;
        let t = tls_get(secure, &format!("{PREFIX}/signon"))?;
        want(&t, 200, &["name=\"password\""], "the sign-on page over TLS")?;
        let ended = s.command("ENDTCPSVR SERVER(*HTTP) HTTPSVR(NAVIGATOR)", CMD)?;
        if !ended.contains("TCP1A0A") {
            return Err(format!("ENDTCPSVR NAVIGATOR:\n{ended}"));
        }
        Ok(format!(
            "signed on as QSECOFR over 2002 (host {plain}), System status of {name}, NAVIGATOR in {job} on Active jobs, QSYSOPR, {live}, {work}, a forged POST refused, Run SQL, QS00SS1 and QS00NAV installed, PTFs, the audit journal, signed off; the sign-on page over TLS on 2005 (host {secure})"
        ))
    }
}

/// The table row from `needle` to its end.
fn row_of<'a>(body: &'a str, needle: &str) -> Option<&'a str> {
    let at = body.find(needle)?;
    let end = body[at..].find("</tr>").map_or(body.len(), |e| at + e);
    Some(&body[at..end])
}

/// The first `<td class="mono">` value after `needle`.
fn mono_after(body: &str, needle: &str) -> Option<String> {
    let row = row_of(body, needle)?;
    let cell = "<td class=\"mono\">";
    let at = row[needle.len()..].find(cell)? + needle.len() + cell.len();
    let end = row[at..].find("</td>")?;
    Some(row[at..at + end].trim().to_string())
}

/// The devices a page links to as `/writers/<dev>`.
fn writer_links(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let key = format!("{PREFIX}/writers/");
    let mut rest = body;
    while let Some(at) = rest.find(&key) {
        rest = &rest[at + key.len()..];
        let dev: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || "#@$_".contains(*c)).collect();
        if !dev.is_empty() && !out.contains(&dev) {
            out.push(dev);
        }
    }
    out
}

/// Work management's pages, with a spooled file on hand and the
/// capture device's writer started: every list and each printer's own
/// page answer and the job stays up. A page that fails shows the jobs
/// (and, read with them, what the job wrote to the serial line).
fn work_pages(s: &mut Session, port: u16, sid: &str) -> Result<String, String> {
    work_pages_of(s, port, sid).map_err(|e| {
        let jobs = s.command("WRKACTJOB", CMD).unwrap_or_default();
        format!("{e}
WRKACTJOB:
{jobs}")
    })
}

fn work_pages_of(s: &mut Session, port: u16, sid: &str) -> Result<String, String> {
    s.command("DSPLIBL OUTPUT(*PRINT)", CMD)?;
    s.command("STRSBS QSYS/QSPL", CMD)?;
    let started = s.command("STRPRTWTR DEV(QSCAPTURE)", CMD)?;
    let end = Instant::now() + Duration::from_secs(10);
    while !s.command("WRKWTR", CMD)?.lines().any(|l| l.contains("QSCAPTURE") && l.contains("STR")) {
        if Instant::now() > end {
            return Err(format!("the QSCAPTURE writer did not start:
{started}"));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    for (path, title) in [
        ("/jobs", "Active jobs"),
        ("/jobq", "Job queues"),
        ("/sbs", "Subsystems"),
        ("/outq", "Output queues"),
        ("/outq/QGPL/QPRINT", "QPRINT"),
        ("/jobscde", "Scheduled jobs"),
        ("/writers", "Printers"),
    ] {
        let p = nav(port, "GET", path, Some(sid), None)?;
        want(&p, 200, &[title], path)?;
    }
    let list = nav(port, "GET", "/writers", Some(sid), None)?;
    want(&list, 200, &["QSCAPTURE", "Started"], "Printers with a writer started")?;
    let devs = writer_links(&list.body);
    for d in &devs {
        let p = nav(port, "GET", &format!("/writers/{d}"), Some(sid), None)?;
        want(&p, 200, &[&format!("Printer: {d}")], &format!("printer {d}"))?;
    }
    let start = nav(port, "GET", "/writers?start=1", Some(sid), None)?;
    want(&start, 200, &["Start writer"], "Start writer")?;
    s.command("ENDWTR WTR(QSCAPTURE) OPTION(*IMMED)", CMD)?;
    Ok(format!("work management's pages and {} printer(s)", devs.len()))
}

/// System values, interfaces, servers and System activity from live
/// state: `QSECURITY` as `DSPSYSVAL` shows it, `10.0.2.15`, `*HTTP`
/// started.
fn live_config(s: &mut Session, port: u16, sid: &str) -> Result<String, String> {
    let sv = nav(port, "GET", "/sysval?cat=security", Some(sid), None)?;
    want(&sv, 200, &["QSECURITY"], "Security values")?;
    let level = mono_after(&sv.body, "QSECURITY</td>").ok_or_else(|| format!("no QSECURITY value:\n{}", sv.brief()))?;
    let shown = s.command("DSPSYSVAL SYSVAL(QSECURITY)", CMD)?;
    if level.is_empty() || !shown.contains(&level) {
        return Err(format!("Security values show QSECURITY {level}; DSPSYSVAL:\n{shown}"));
    }
    let ifc = nav(port, "GET", "/tcp/ifc", Some(sid), None)?;
    want(&ifc, 200, &["10.0.2.15"], "TCP/IP interfaces")?;
    let svr = nav(port, "GET", "/tcp/servers", Some(sid), None)?;
    want(&svr, 200, &["*HTTP"], "TCP servers")?;
    if !row_of(&svr.body, "*HTTP</a>").is_some_and(|r| r.contains("Started")) {
        return Err(format!("*HTTP is not started on TCP servers:\n{}", svr.brief()));
    }
    let act = nav(port, "GET", "/perf/sys", Some(sid), None)?;
    want(&act, 200, &["<h1>System activity</h1>"], "System activity")?;
    Ok(format!("QSECURITY {level} as DSPSYSVAL, 10.0.2.15, *HTTP started, System activity"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_jobs() {
        assert_eq!(system_in("System: <b>QS400</b> &nbsp;").as_deref(), Some("QS400"));
        assert_eq!(
            nav_job("<a href=\"/qs400/navigator/job/012345/QTMHHTTP/NAVIGATOR/general\">").as_deref(),
            Some("012345/QTMHHTTP/NAVIGATOR")
        );
        assert_eq!(nav_job("x/QTMHHTTP/NAVIGATOR"), None);
        let body = "<tr><td class=\"mono\">QSECURITY</td>\n<td class=\"mono\">40</td></tr><tr><td>x</td></tr>";
        assert_eq!(mono_after(body, "QSECURITY</td>").as_deref(), Some("40"));
        assert!(row_of(body, "QSECURITY").is_some_and(|r| !r.contains('x')));
        assert_eq!(csrf_in("<input type=\"hidden\" name=\"csrf\" value=\"ab12\">").as_deref(), Some("ab12"));
    }
}
