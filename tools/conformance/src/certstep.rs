// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 10 QEMU step 70: certificates. `QSCRTCRTS` makes the `*SYSTEM`
//! store (its password kept with the LIC key), `QSCRTCRT` a certificate
//! in it, `QSASNCRT` gives it to `QIBM_HTTP_SERVER_ADMIN`, and the HTTP
//! administration instance then serves it on 2005: the harness reads the
//! certificate from the handshake. `QSWRKCRT`, `QSWRKAPPID` and
//! `QSCHKCRT` answer in text at the service console.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use crate::qemu::session::Session;
use crate::steps::Suite;
use crate::tcp::forward_port;

/// The guest's own address on QEMU's user network.
const GUEST: &str = "10.0.2.15";

/// The certificate commands derive store keys (Argon2id), slow without
/// acceleration.
const SLOW: Duration = Duration::from_secs(120);

/// The common name the step's certificate carries.
const CN: &str = "qs400.step70.test";

/// A console command whose output must hold one of `want`.
fn expect(s: &mut Session, line: &str, want: &[&str]) -> Result<String, String> {
    let out = s.command(line, SLOW)?;
    if want.iter().any(|w| out.contains(w)) {
        Ok(out)
    } else {
        Err(format!("{line}: none of {want:?} in:\n{out}"))
    }
}

/// The certificate the server on host port `port` presents, once it
/// answers HTTPS.
fn served_certificate(port: u16) -> Result<Vec<u8>, String> {
    let policy = tls::policy::TlsPolicy::opsys();
    let cfg = tls::config::client_config(
        &policy,
        tls::rustls::RootCertStore::empty(),
        None,
        tls::config::ServerAuthMode::Passthru,
    )
    .map_err(|e| e.to_string())?;
    let name = tls::config::server_name(CN).map_err(|e| e.to_string())?;
    let end = Instant::now() + Duration::from_secs(60);
    loop {
        let r = TcpStream::connect(("127.0.0.1", port))
            .map_err(|e| e.to_string())
            .and_then(|c| {
                let _ = c.set_read_timeout(Some(Duration::from_secs(10)));
                tls::stream::TlsStream::connect(cfg.clone(), name.clone(), c).map_err(|e| e.to_string())
            })
            .and_then(|mut t| {
                let peer = t.session().map(|s| s.peer_certs).unwrap_or_default();
                t.write_all(b"GET / HTTP/1.1\r\nHost: qs400\r\n\r\n")
                    .map_err(|e| e.to_string())?;
                let mut page = Vec::new();
                let _ = t.read_to_end(&mut page);
                if !page.starts_with(b"HTTP/1.1 200 OK") {
                    return Err(format!("the page: {}", String::from_utf8_lossy(&page)));
                }
                peer.into_iter().next().ok_or_else(|| "no certificate".to_string())
            });
        match r {
            Ok(der) => return Ok(der),
            Err(e) if Instant::now() > end => return Err(format!("HTTPS on port {port}: {e}")),
            Err(_) => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}

#[allow(clippy::unused_self)] // the steps share one signature
impl Suite {
    /// Step 70: a certificate assigned to the HTTP administration
    /// instance is the one port 2005 serves.
    pub(crate) fn certificates(&self, s: &mut Session) -> Result<String, String> {
        expect(
            s,
            "QSCRTCRTS KEYSTORE(*SYSTEM) PASSWORD('step70pw')",
            &["TCP1C00", "TCP1C11"],
        )?;
        expect(
            s,
            &format!("QSCRTCRT LABEL(STEP70) CN('{CN}') VALIDITY(20) TEXT('Step 70')"),
            &["TCP1C02", "TCP1C14"],
        )?;
        expect(s, "QSASNCRT APPID(QIBM_HTTP_SERVER_ADMIN) LABEL(STEP70)", &["TCP1C08"])?;
        let list = expect(s, "QSWRKCRT", &["STEP70"])?;
        if !list.lines().any(|l| l.starts_with("STEP70") && l.contains("Expiring") && l.contains("QIBM_HTTP_SERVER_ADMIN")) {
            return Err(format!("QSWRKCRT does not show STEP70 expiring and assigned:\n{list}"));
        }
        let apps = expect(s, "QSWRKAPPID APPID(QIBM_HTTP*)", &["QIBM_HTTP_SERVER_ADMIN"])?;
        if !apps.lines().any(|l| l.starts_with("QIBM_HTTP_SERVER_ADMIN") && l.trim_end().ends_with("STEP70")) {
            return Err(format!("QSWRKAPPID does not show the assignment:\n{apps}"));
        }
        // Twenty days left: the 30-day warning goes to QSYSOPR, once.
        let first = expect(s, "QSCHKCRT", &["TCP1C0D"])?;
        if !first.contains("TCP1C20") {
            return Err(format!("QSCHKCRT sent no warning for STEP70:\n{first}"));
        }
        let again = expect(s, "QSCHKCRT", &["TCP1C0D"])?;
        if again.contains("TCP1C20") {
            return Err(format!("QSCHKCRT warned twice:\n{again}"));
        }
        // The server presents the assigned certificate.
        expect(s, "STRTCPSVR SERVER(*HTTP) HTTPSVR(*ADMIN)", &["TCP1A07", "TCP1A08"])?;
        let port = forward_port(self, GUEST, 2005)?;
        let der = served_certificate(port)?;
        let info = certs::x509::CertInfo::from_der(&der).map_err(|e| e.to_string())?;
        if info.common_name.as_deref() != Some(CN) {
            return Err(format!("2005 served {} rather than CN={CN}", info.subject));
        }
        expect(s, "ENDTCPSVR SERVER(*HTTP) HTTPSVR(*ADMIN)", &["TCP1A0A"])?;
        // Unassigned, the server goes back to the store's default, which
        // is still STEP70 (the only certificate): nothing else to see.
        expect(s, "QSASNCRT APPID(QIBM_HTTP_SERVER_ADMIN) LABEL(*NONE)", &["TCP1C09"])?;
        Ok(format!(
            "*SYSTEM made, STEP70 assigned to QIBM_HTTP_SERVER_ADMIN and served on 2005 (host {port}, SHA-256 {}...), one expiry warning",
            &info.sha256[..16]
        ))
    }
}
