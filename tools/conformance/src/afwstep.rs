// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 153: QS/400 Access for Web from live state. `QS00AFW` is a
//! shipped licensed program: `SAVLICPGM` saves it, `DLTLICPGM` deletes
//! it (then `STRTCPSVR *HTTP HTTPSVR(ACCESSWEB)` answers `TCP1A06`),
//! `RSTLICPGM` brings it back. Then `HTTPSVR(ACCESSWEB)` starts job
//! `ACCESSWEB`; from the host over HTTP on 2003: `QSECOFR` signs on, My
//! home page names the system, a preference is changed and used (the
//! store `QUSRSYS/QZXWPRF`), a policy denies a new profile's sign-on
//! (`XWA0001`, the store `QUSRSYS/QZXWPOL`) and is cleared again, a POST
//! without the form field is refused, an area not built says so
//! (`XWA0099`, Messages), sign-off; then sign-on and My home page over TLS on
//! 2006; `ENDTCPSVR ... HTTPSVR(ACCESSWEB)` ends the job.

use std::time::{Duration, Instant};

use crate::httpadm::{Reply, enc, http_at, secofr_password, session_cookie, tls_at, want};
use crate::phase5::SECOFR_PASSWORD;
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};
use crate::tcp::{GUEST, forward_port};

/// The product's URLs.
const PREFIX: &str = "/qs400/accessweb";

/// The profile the policy is tried on, and its password.
const USER: &str = "AFW153";
const PASSWORD: &str = "AFW153PW";

fn afw(port: u16, method: &str, path: &str, cookie: Option<&str>, form: Option<&str>) -> Result<Reply, String> {
    http_at(PREFIX, port, method, path, cookie, form)
}

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

/// A page's message rows, as text.
fn message_row(body: &str) -> String {
    let Some(at) = body.find("<table class=\"msg\">") else {
        return "no message row".into();
    };
    let end = body[at..].find("</table>").map_or(body.len(), |e| at + e);
    body[at..end].to_string()
}

/// Sign on as `user`: the session cookie.
fn sign_on(port: u16, user: &str, password: &str) -> Result<String, String> {
    let form = format!("user={user}&password={}", enc(password));
    let ok = afw(port, "POST", "/signon", None, Some(&form))?;
    if ok.status != 303 || !ok.header("Location").is_some_and(|l| l.ends_with("/home")) {
        return Err(format!("signing on as {user}:\n{}", ok.brief()));
    }
    session_cookie(&ok).ok_or_else(|| format!("no session cookie:\n{}", ok.brief()))
}

/// The page's CSRF field, read from a GET of `path`.
fn token(port: u16, path: &str, sid: &str) -> Result<String, String> {
    let p = afw(port, "GET", path, Some(sid), None)?;
    csrf_in(&p.body).ok_or_else(|| format!("no form field on {path}:\n{}", p.brief()))
}

/// The product saved, deleted and restored; the instance unknown in
/// between.
fn product(s: &mut Session) -> Result<(), String> {
    s.command("crtsavf FILE(QGPL/S153AFW)", CMD)?;
    let saved = s.command("savlicpgm LICPGM(QS00AFW) DEV(*SAVF) OPTION(*ALL) SAVF(QGPL/S153AFW)", CMD)?;
    if saved.contains("CPF") {
        return Err(format!("SAVLICPGM QS00AFW: {saved}"));
    }
    let del = s.command("dltlicpgm LICPGM(QS00AFW)", CMD)?;
    if !del.contains("CPC3D82") {
        return Err(format!("DLTLICPGM QS00AFW: {del}"));
    }
    let none = s.command("STRTCPSVR SERVER(*HTTP) HTTPSVR(ACCESSWEB)", CMD)?;
    if !none.contains("TCP1A06") {
        return Err(format!("STRTCPSVR ACCESSWEB without QS00AFW:\n{none}"));
    }
    let rst = s.command("rstlicpgm LICPGM(QS00AFW) DEV(*SAVF) SAVF(QGPL/S153AFW)", CMD)?;
    if !rst.contains("CPC3D81") {
        return Err(format!("RSTLICPGM QS00AFW: {rst}"));
    }
    for (o, t) in [("QZXWMSG", "*MSGF"), ("QZXWPOL", "*USRIDX"), ("QZXWPRF", "*USRIDX")] {
        let chk = s.command(&format!("chkobj OBJ(QUSRSYS/{o}) OBJTYPE({t})"), CMD)?;
        if chk.contains("CPF") {
            return Err(format!("{o} not restored: {chk}"));
        }
    }
    Ok(())
}

/// A preference changed through the store, then used and set back.
fn preference(s: &mut Session, port: u16, sid: &str) -> Result<(), String> {
    let t = token(port, "/preferences?tab=home", sid)?;
    let form = format!("csrf={}&hl=compact&apply=Apply", enc(&t));
    let r = afw(port, "POST", "/preferences?tab=home", Some(sid), Some(&form))?;
    if r.status != 303 {
        let aut = s.command("dspobjaut OBJ(QUSRSYS/QZXWPRF) OBJTYPE(*USRIDX)", CMD)?;
        return Err(format!("Preferences' Apply: {}\nthe store's authority:\n{aut}", message_row(&r.body)));
    }
    let back = afw(port, "GET", "/preferences?tab=home", Some(sid), None)?;
    want(&back, 200, &["XWA0003", "value=\"compact\" checked"], "the preference kept")?;
    let home = afw(port, "GET", "/home", Some(sid), None)?;
    if home.body.contains("class=\"home\"") {
        return Err(format!("the compact layout was not used:\n{}", home.brief()));
    }
    let t = token(port, "/preferences?tab=home", sid)?;
    let form = format!("csrf={}&restore_defaults=Restore+defaults", enc(&t));
    let r = afw(port, "POST", "/preferences?tab=home", Some(sid), Some(&form))?;
    want(&r, 303, &[], "Restore defaults")
}

/// A policy that denies `USER` signing on, then cleared.
fn policy(port: u16, sid: &str) -> Result<(), String> {
    let path = format!("/policies?p={USER}");
    let page = afw(port, "GET", &path, Some(sid), None)?;
    want(&page, 200, &["<h1>Policies</h1>", &format!("Effective policies for {USER}")], "Policies")?;
    let set = |v: &str| -> Result<(), String> {
        let t = token(port, &path, sid)?;
        let form = format!("csrf={}&p_signon_0={v}&apply=Apply", enc(&t));
        let r = afw(port, "POST", &path, Some(sid), Some(&form))?;
        want(&r, 303, &[], "Policies' Apply")?;
        let back = afw(port, "GET", &path, Some(sid), None)?;
        want(&back, 200, &["XWA0002"], "the policy kept")
    };
    set("D")?;
    let form = format!("user={USER}&password={}", enc(PASSWORD));
    let refused = afw(port, "POST", "/signon", None, Some(&form))?;
    want(&refused, 200, &["XWA0001"], "a sign-on the policy denies")?;
    set("A")?;
    sign_on(port, USER, PASSWORD).map(|_| ())
}

impl Suite {
    /// Step 153: Access for Web over HTTP and HTTPS.
    pub(crate) fn access_web(&self, s: &mut Session) -> Result<String, String> {
        secofr_password(s)?;
        product(s)?;
        s.command(
            &format!("CRTUSRPRF USRPRF({USER}) PASSWORD({PASSWORD}) USRCLS(*USER) PWDEXP(*NO) TEXT('Step 153')"),
            CMD,
        )?;
        let end = Instant::now() + Duration::from_secs(60);
        loop {
            let started = s.command("STRTCPSVR SERVER(*HTTP) HTTPSVR(ACCESSWEB)", CMD)?;
            if started.contains("TCP1A07") || (started.contains("TCP1A08") && Instant::now() > end) {
                break;
            }
            if !started.contains("TCP1A08") {
                return Err(format!("STRTCPSVR ACCESSWEB:\n{started}"));
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        let plain = forward_port(self, GUEST, 2003)?;
        let page = afw(plain, "GET", "/signon", None, None)?;
        want(&page, 200, &["QS/400 Access for Web", "name=\"password\""], "the sign-on page")?;
        let sid = sign_on(plain, "QSECOFR", SECOFR_PASSWORD)?;
        let home = afw(plain, "GET", "/home", Some(&sid), None)?;
        want(&home, 200, &["<h1>My home page</h1>", "(<span class=\"mono\">QSECOFR</span>)"], "My home page")?;
        let name = system_in(&home.body).ok_or_else(|| format!("no system name:\n{}", home.brief()))?;
        let neta = s.command("DSPNETA", CMD)?;
        if !neta.contains(&name) {
            return Err(format!("My home page names {name}; DSPNETA:\n{neta}"));
        }
        preference(s, plain, &sid)?;
        policy(plain, &sid)?;
        let forged = afw(plain, "POST", "/preferences", Some(&sid), Some("apply=Apply"))?;
        want(&forged, 403, &["out of date"], "a POST without the form field")?;
        let later = afw(plain, "GET", "/messages", Some(&sid), None)?;
        want(&later, 501, &["XWA0099"], "an area not built")?;
        let off = afw(plain, "GET", "/signoff", Some(&sid), None)?;
        want(&off, 303, &["Max-Age=0"], "sign-off")?;
        let after = afw(plain, "GET", "/home", Some(&sid), None)?;
        if after.status != 303 || !after.header("Location").is_some_and(|l| l.ends_with("/signon")) {
            return Err(format!("the session outlived sign-off:\n{}", after.brief()));
        }
        // TLS on 2006: the certificate of QS400_HTTP_SERVER_ACCESSWEB,
        // else the system's own.
        let secure = forward_port(self, GUEST, 2006)?;
        let t = tls_at(secure, "GET", &format!("{PREFIX}/signon"), None, None)?;
        want(&t, 200, &["name=\"password\""], "the sign-on page over TLS")?;
        let form = format!("user=QSECOFR&password={}", enc(SECOFR_PASSWORD));
        let ok = tls_at(secure, "POST", &format!("{PREFIX}/signon"), None, Some(&form))?;
        if ok.status != 303 {
            return Err(format!("signing on over TLS:\n{}", ok.brief()));
        }
        let tsid = session_cookie(&ok).ok_or_else(|| format!("no session cookie over TLS:\n{}", ok.brief()))?;
        if !ok.head.contains("Secure") {
            return Err(format!("the TLS session cookie is not Secure:\n{}", ok.brief()));
        }
        let th = tls_at(secure, "GET", &format!("{PREFIX}/home"), Some(&tsid), None)?;
        want(&th, 200, &["<h1>My home page</h1>"], "My home page over TLS")?;
        let ended = s.command("ENDTCPSVR SERVER(*HTTP) HTTPSVR(ACCESSWEB)", CMD)?;
        if !ended.contains("TCP1A0A") {
            return Err(format!("ENDTCPSVR ACCESSWEB:\n{ended}"));
        }
        s.command(&format!("DLTUSRPRF USRPRF({USER})"), CMD)?;
        Ok(format!(
            "QS00AFW saved, deleted (TCP1A06), restored with QZXWMSG, QZXWPOL, QZXWPRF; signed on as QSECOFR over 2003 (host {plain}), My home page of {name}, a preference kept and used, a policy denying {USER}'s sign-on (XWA0001) and cleared, a forged POST refused, Messages not available (XWA0099), signed off; signed on over TLS on 2006 (host {secure})"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_fields() {
        assert_eq!(csrf_in("<input type=\"hidden\" name=\"csrf\" value=\"ab12\">").as_deref(), Some("ab12"));
        assert_eq!(system_in("System: <b>QSHQ400</b> &nbsp;").as_deref(), Some("QSHQ400"));
        assert_eq!(system_in("System: <b> </b>"), None);
    }
}
