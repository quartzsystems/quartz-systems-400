// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 199: the directory server's administration pages against the
//! running `QUSRDIR`. Navigator (`NAVIGATOR` started, `QSECOFR` signed on
//! over 2002): the instances (`QUSRDIR` and its job), its properties,
//! administrator, suffixes, indexes, schema, the suffix's access
//! control, the monitor, publishing and the users agent, the end's
//! confirmation (not confirmed); an index on `givenName` added and
//! removed through the forms. Then QAP's `Dirsrv` (`qap/dirsrv.rs`).

use std::time::{Duration, Instant};

use crate::httpadm::{enc, secofr_password, session_cookie, want};
use crate::navstep::{csrf_in, nav};
use crate::phase5::SECOFR_PASSWORD;
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};
use crate::tcp::{GUEST, forward_port};

/// The instance's pages.
const Q: &str = "/dirsrv/inst/QUSRDIR";

/// `STRTCPSVR` of Navigator, waiting out an earlier end.
fn start_navigator(s: &mut Session) -> Result<(), String> {
    let end = Instant::now() + Duration::from_secs(60);
    loop {
        let out = s.command("STRTCPSVR SERVER(*HTTP) HTTPSVR(NAVIGATOR)", CMD)?;
        if out.contains("TCP1A07") || (out.contains("TCP1A08") && Instant::now() > end) {
            return Ok(());
        }
        if !out.contains("TCP1A08") {
            return Err(format!("STRTCPSVR NAVIGATOR:\n{out}"));
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// A page, 200 with `needles`.
fn page(port: u16, sid: &str, path: &str, needles: &[&str]) -> Result<String, String> {
    let r = nav(port, "GET", path, Some(sid), None)?;
    want(&r, 200, needles, path)?;
    Ok(r.body)
}

/// A form's POST from the page at `path`, which must redirect.
fn post(port: u16, sid: &str, path: &str, form: &str) -> Result<(), String> {
    let r = nav(port, "GET", path, Some(sid), None)?;
    let token =
        csrf_in(&r.body).ok_or_else(|| format!("no form field on {path}:\n{}", r.brief()))?;
    let r = nav(
        port,
        "POST",
        path,
        Some(sid),
        Some(&format!("csrf={}&{form}", enc(&token))),
    )?;
    want(&r, 303, &[], &format!("POST {path}"))
}

impl Suite {
    /// Step 199: Navigator's and QAP's directory server pages.
    pub(crate) fn dir_admin_pages(&self, s: &mut Session) -> Result<String, String> {
        let r = self.dir_admin_body(s);
        self.dir_checked(s, "199", r)
    }

    fn dir_admin_body(&self, s: &mut Session) -> Result<String, String> {
        self.dir_ensure(s)?;
        secofr_password(s)?;
        start_navigator(s)?;
        let port = forward_port(self, GUEST, 2002)?;
        let form = format!("user=QSECOFR&password={}", enc(SECOFR_PASSWORD));
        let ok = nav(port, "POST", "/signon", None, Some(&form))?;
        let sid = session_cookie(&ok).ok_or_else(|| format!("signing on:\n{}", ok.brief()))?;
        let pages = [
            (
                "/dirsrv".to_string(),
                vec!["Directory server instances", "QDIRSRV/QUSRDIR"],
            ),
            (format!("{Q}/props"), vec!["Properties of QUSRDIR", "389"]),
            (
                format!("{Q}/admin"),
                vec!["Administrator of QUSRDIR", "cn=Administrator"],
            ),
            (
                format!("{Q}/suffixes"),
                vec!["Suffixes of QUSRDIR", "cn=localhost"],
            ),
            (
                format!("{Q}/indexes"),
                vec!["Indexes of QUSRDIR", "objectclass"],
            ),
            (format!("{Q}/schema"), vec!["Schema of QUSRDIR"]),
            (
                format!("{Q}/schema?kind=Object+classes&q=person"),
                vec!["Schema of QUSRDIR", "inetOrgPerson"],
            ),
            (
                format!("{Q}/access?dn=cn%3Dlocalhost"),
                vec!["Access control of QUSRDIR"],
            ),
            (format!("{Q}/monitor"), vec!["Monitor QUSRDIR"]),
            ("/dirsrv/publish".into(), vec!["Directory publishing"]),
            ("/dirsrv/publish/Users".into(), vec!["Publishing of users"]),
            (
                format!("{Q}/confirm?op=end"),
                vec!["Confirm End Directory Server"],
            ),
        ];
        for (path, needles) in &pages {
            let body = page(port, &sid, path, needles)?;
            if path.ends_with("/monitor") && body.contains("is not active") {
                return Err(format!(
                    "the monitor says the instance is not active:\n{body}"
                ));
            }
        }
        let indexes = format!("{Q}/indexes");
        post(port, &sid, &indexes, "add=Add&attr=givenName&e=on")?;
        page(port, &sid, &indexes, &["givenName"])?;
        post(
            port,
            &sid,
            &format!("{Q}/indexes/remove?attr=givenName"),
            "ok=OK",
        )?;
        let after = page(port, &sid, &indexes, &["Indexes of QUSRDIR"])?;
        if after.contains("givenName") {
            return Err(format!("givenName still indexed:\n{after}"));
        }
        let _ = nav(port, "GET", "/signoff", Some(&sid), None);
        let ended = s.command("ENDTCPSVR SERVER(*HTTP) HTTPSVR(NAVIGATOR)", CMD)?;
        if !ended.contains("TCP1A0A") {
            return Err(format!("ENDTCPSVR NAVIGATOR:\n{ended}"));
        }
        let qap = self.qap_dirsrv(s)?;
        Ok(format!(
            "Navigator: {} pages of the directory server, an index on givenName added and removed; {qap}",
            pages.len()
        ))
    }
}
