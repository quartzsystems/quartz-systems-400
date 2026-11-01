// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 88: Navigator from live state. `STRTCPSVR *HTTP HTTPSVR(*ADMIN)`
//! starts `NAVIGATOR` next to `ADMIN`; from the host, over HTTP on
//! 2002: `QSECOFR` signs on, System status names the system, Active
//! jobs lists the `NAVIGATOR` job itself (which `DSPJOB` knows), the
//! `QSYSOPR` queue loads, a POST without the form field is refused,
//! sign-off; then the sign-on page over TLS on 2005; `ENDTCPSVR` ends
//! both jobs.

use crate::httpadm::{Reply, enc, http_at, session_cookie, tls_get, want};
use crate::phase5::SECOFR_PASSWORD;
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};
use crate::tcp::{GUEST, forward_port};

/// The product's URLs.
const PREFIX: &str = "/qs400/navigator";

fn nav(port: u16, method: &str, path: &str, cookie: Option<&str>, form: Option<&str>) -> Result<Reply, String> {
    http_at(PREFIX, port, method, path, cookie, form)
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
        let started = s.command("STRTCPSVR SERVER(*HTTP) HTTPSVR(*ADMIN)", CMD)?;
        if !started.contains("TCP1A07") && !started.contains("TCP1A08") {
            return Err(format!("STRTCPSVR *ADMIN:\n{started}"));
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
        // A POST without the form's field is refused.
        let forged = nav(plain, "POST", "/mymsgs", Some(&sid), Some("send=Send"))?;
        want(&forged, 403, &["out of date"], "a POST without the form field")?;
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
        let ended = s.command("ENDTCPSVR SERVER(*HTTP) HTTPSVR(*ADMIN)", CMD)?;
        if !ended.contains("TCP1A0A") {
            return Err(format!("ENDTCPSVR *ADMIN:\n{ended}"));
        }
        Ok(format!(
            "signed on as QSECOFR over 2002 (host {plain}), System status of {name}, NAVIGATOR in {job} on Active jobs, QSYSOPR, a forged POST refused, signed off; the sign-on page over TLS on 2005 (host {secure})"
        ))
    }
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
    }
}
