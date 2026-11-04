// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 12 QEMU steps for the network items Phases 10 and 11 left:
//!
//! | Step | What |
//! |---|---|
//! | 96 | `*IP4DHCP`: QEMU's DHCP server leases an address, `NETSTAT *IFC` shows it beside the `*IP4DHCP` entry, the lease is kept in the IFS, `ENDTCPIFC` releases it |
//! | 97 | Telnet automatic sign-on: RFC 4777 SHA-1 substitutes as `QRMTSIGN` says, then an `INIT0100` exit program signing on a profile of its choosing |
//! | 98 | `ADDSMTPLE TYPE(*HOSTAUTH)`: `SNDSMTPEMM` signs on to a fake mail router with `AUTH PLAIN` |

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tn5250_driver::session::Session as Display;
use tn5250_driver::{Connect, Encoding};

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The first address QEMU's DHCP server hands out (`dhcpstart` in the
/// launch), apart from the addresses the other steps configure.
pub(crate) const DHCP_FIRST: &str = "10.0.2.50";
/// The NIC's line.
const LINE: &str = "ETH0";
/// The profile step 97 signs on.
const AUTO_USER: &str = "AUTOUSR";
/// Its password.
const AUTO_PWD: &str = "Auto2026";
/// The exit program of step 97.
const EXIT_SRC: &str = "             PGM        PARM(&USR &DEV &CONN &ENV &ENVLEN &ALLOW &AUTO)
             DCL        VAR(&USR) TYPE(*CHAR) LEN(44)
             DCL        VAR(&DEV) TYPE(*CHAR) LEN(32)
             DCL        VAR(&CONN) TYPE(*CHAR) LEN(132)
             DCL        VAR(&ENV) TYPE(*CHAR) LEN(1)
             DCL        VAR(&ENVLEN) TYPE(*CHAR) LEN(4)
             DCL        VAR(&ALLOW) TYPE(*CHAR) LEN(1)
             DCL        VAR(&AUTO) TYPE(*CHAR) LEN(1)
             CHGVAR     VAR(%SST(&USR 5 10)) VALUE('AUTOUSR')
             CHGVAR     VAR(&ALLOW) VALUE('1')
             CHGVAR     VAR(&AUTO) VALUE('1')
             ENDPGM";

/// A service console command whose output holds every word of `want`.
fn expect(s: &mut Session, line: &str, want: &[&str]) -> Result<String, String> {
    let out = s.command(line, CMD)?;
    if want.iter().all(|w| out.contains(w)) {
        Ok(out)
    } else {
        Err(format!("{line}: no {want:?} in:\n{out}"))
    }
}

/// Repeat `line` until its output holds every word of `want` (90 s).
fn until(s: &mut Session, line: &str, want: &[&str]) -> Result<String, String> {
    let end = Instant::now() + Duration::from_secs(90);
    loop {
        let out = s.command(line, CMD)?;
        if want.iter().all(|w| out.contains(w)) {
            return Ok(out);
        }
        if Instant::now() > end {
            return Err(format!("{line}: never showed {want:?}:\n{out}"));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// A TN5250 session on the forwarded port, with or without the RFC 4777
/// variables, pumped until one of `want` shows; which one, and the screen.
fn telnet(port: u16, user: &str, password: &str, auto: bool, want: &[&str]) -> Result<(usize, String), String> {
    let mut c = Connect::qs400(port, user, password);
    c.auto_signon = auto;
    c.encoding = Encoding::Sha;
    let end = Instant::now() + Duration::from_secs(60);
    let mut d = loop {
        match Display::connect(&c) {
            Ok(d) => break d,
            Err(e) if Instant::now() < end => {
                let _ = e;
                std::thread::sleep(Duration::from_millis(500));
            }
            Err(e) => return Err(format!("telnet {port}: {e:#}")),
        }
    };
    let until = Instant::now() + Duration::from_secs(40);
    while Instant::now() < until {
        d.pump().map_err(|e| format!("{e:#}"))?;
        if let Some(i) = want.iter().position(|w| d.term.contains(w)) {
            let text = d.term.text();
            d.disconnect();
            return Ok((i, text));
        }
    }
    let text = d.term.text();
    d.disconnect();
    Err(format!("the session showed none of {want:?}:\n{text}"))
}

/// A fake mail router on the harness host: `AUTH PLAIN` and the
/// envelope; what it saw comes back on the channel.
fn fake_router() -> Result<(u16, mpsc::Receiver<Vec<String>>), String> {
    let l = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = l.local_addr().map_err(|e| e.to_string())?.port();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok((s, _)) = l.accept() else { return };
        let _ = s.set_read_timeout(Some(Duration::from_secs(60)));
        let mut w = s.try_clone().expect("clone");
        let mut r = BufReader::new(s);
        let mut seen = Vec::new();
        let _ = w.write_all(b"220 router ESMTP\r\n");
        let mut line = String::new();
        while r.read_line(&mut line).is_ok_and(|n| n > 0) {
            let cmd = line.trim_end().to_string();
            line.clear();
            let upper = cmd.to_ascii_uppercase();
            let answer: &[u8] = if upper.starts_with("EHLO") {
                b"250-router\r\n250 AUTH PLAIN LOGIN\r\n"
            } else if upper.starts_with("AUTH PLAIN") {
                seen.push(cmd.clone());
                b"235 2.7.0 accepted\r\n"
            } else if upper.starts_with("MAIL") || upper.starts_with("RCPT") {
                seen.push(cmd.clone());
                b"250 ok\r\n"
            } else if upper == "DATA" {
                let _ = w.write_all(b"354 go on\r\n");
                let mut body = String::new();
                while r.read_line(&mut body).is_ok_and(|n| n > 0) {
                    if body.ends_with("\r\n.\r\n") || body == ".\r\n" {
                        break;
                    }
                }
                b"250 queued\r\n"
            } else if upper == "QUIT" {
                let _ = w.write_all(b"221 bye\r\n");
                break;
            } else {
                b"250 ok\r\n"
            };
            let _ = w.write_all(answer);
        }
        let _ = tx.send(seen);
    });
    Ok((port, rx))
}

#[allow(clippy::unused_self)] // the steps share one signature
impl Suite {
    /// Step 96: an interface from DHCP on the NIC's line.
    pub(crate) fn dhcp_client(&self, s: &mut Session) -> Result<String, String> {
        expect(s, &format!("ADDTCPIFC INTNETADR(*IP4DHCP) LIND({LINE}) AUTOSTART(*NO)"), &["TCP1D03"])?;
        expect(s, "STRTCPIFC INTNETADR(*IP4DHCP)", &["TCP1A0B"])?;
        until(s, "svc.tcp ifc", &[&format!("kernel addr eth0 {DHCP_FIRST}/24")])?;
        let ifc = until(s, "svc.tcp ifc", &["ifc *IP4DHCP", "Active", &format!("lease *IP4DHCP {DHCP_FIRST}/24 from 10.0.2.2")])?;
        let netstat = expect(s, "NETSTAT OPTION(*IFC)", &["*IP4DHCP", DHCP_FIRST])?;
        let kept = until(s, "svc.ifs cat /QIBM/UserData/OS400/TCPIP/DHCP/LEASES", &[&format!("[{LINE}]"), &format!("addr={DHCP_FIRST}")])?;
        expect(s, "ENDTCPIFC INTNETADR(*IP4DHCP)", &["TCP1A0C"])?;
        let gone = until(s, "svc.tcp ifc", &["ifc *IP4DHCP"])?;
        if gone.contains(&format!("kernel addr eth0 {DHCP_FIRST}/")) {
            return Err(format!("ENDTCPIFC left the leased address:\n{gone}"));
        }
        let after = s.command("svc.ifs cat /QIBM/UserData/OS400/TCPIP/DHCP/LEASES", CMD)?;
        if after.contains(&format!("addr={DHCP_FIRST}")) {
            return Err(format!("the released lease is still kept:\n{after}"));
        }
        expect(s, "RMVTCPIFC INTNETADR(*IP4DHCP)", &["TCP1D05"])?;
        let _ = (ifc, netstat, kept);
        Ok(format!("{DHCP_FIRST} leased on {LINE}, shown by NETSTAT *IFC, kept in the IFS, released by ENDTCPIFC"))
    }

    /// Step 97: automatic sign-on over telnet.
    pub(crate) fn telnet_auto_signon(&self, s: &mut Session) -> Result<String, String> {
        let port = self.telnet_port().ok_or("the launch has no telnet port")?;
        let _ = s.command(&format!("DLTUSRPRF USRPRF({AUTO_USER})"), CMD);
        expect(
            s,
            &format!("CRTUSRPRF USRPRF({AUTO_USER}) PASSWORD('{AUTO_PWD}') PWDEXP(*NO) INLMNU(MAIN) TEXT('Step 97')"),
            &[AUTO_USER],
        )?;
        // Signed on: the main menu or the sign-on information first.
        let want = ["Main Menu", "Sign-on Information", "Sign On"];
        // The profile signs on at the display (the control).
        {
            let c = Connect::qs400(port, AUTO_USER, AUTO_PWD);
            let mut d = Display::connect(&c).map_err(|e| format!("{e:#}"))?;
            let opts = tn5250_driver::ScriptOptions::default();
            let mut ctx = tn5250_driver::script::Context::new(&c, &opts);
            let script = format!("wait \"Sign On\"\nsignon {AUTO_USER} {AUTO_PWD}\nsettle 2500\n");
            crate::phase8::run(&mut d, &mut ctx, &script)?;
            let text = d.term.text();
            d.disconnect();
            if !text.contains("Main Menu") && !text.contains("Sign-on Information") {
                let log = s.command("dsplog", CMD).unwrap_or_default();
                return Err(format!("{AUTO_USER} does not sign on at the display:\n{text}\n{log}"));
            }
        }
        // QRMTSIGN *VERIFY (shipped): the substitute signs on.
        let (i, screen) = telnet(port, AUTO_USER, AUTO_PWD, true, &want)?;
        if i > 1 {
            let log = s.command("dsplog", CMD).unwrap_or_default();
            return Err(format!("a valid SHA-1 substitute did not sign on:\n{screen}\n{log}"));
        }
        // A wrong password: the sign-on display.
        let (i, _) = telnet(port, AUTO_USER, "Wrong2026", true, &want)?;
        if i != 2 {
            return Err("a wrong substitute signed on".into());
        }
        // *FRCSIGNON and *REJECT: the sign-on display whatever came.
        for v in ["*FRCSIGNON", "*REJECT"] {
            expect(s, &format!("CHGSYSVAL SYSVAL(QRMTSIGN) VALUE('{v}')"), &["QRMTSIGN"])?;
            let (i, _) = telnet(port, AUTO_USER, AUTO_PWD, true, &want)?;
            if i != 2 {
                let _ = s.command("CHGSYSVAL SYSVAL(QRMTSIGN) VALUE('*VERIFY')", CMD);
                return Err(format!("QRMTSIGN {v} signed on automatically"));
            }
        }
        expect(s, "CHGSYSVAL SYSVAL(QRMTSIGN) VALUE('*VERIFY')", &["QRMTSIGN"])?;
        // An INIT0100 exit program signs on AUTOUSR for a client that sent
        // no user at all.
        let _ = s.command("CRTSRCPF FILE(QGPL/QCLSRC)", CMD);
        crate::qir::qemu::push(s, "TNEXITSRC", EXIT_SRC)?;
        expect(s, "svc.srcput QGPL/QCLSRC TNEXIT SRCTYPE(CLLE) FROMSTMF(TNEXITSRC)", &["written"])?;
        let _ = s.command("DLTPGM PGM(QGPL/TNEXIT)", CMD);
        expect(s, "CRTCLPGM PGM(QGPL/TNEXIT) SRCFILE(QGPL/QCLSRC) SRCMBR(TNEXIT)", &["TNEXIT"])?;
        expect(
            s,
            "ADDEXITPGM EXITPNT(QIBM_QTG_DEVINIT) FORMAT(INIT0100) PGMNBR(1) PGM(QGPL/TNEXIT)",
            &["added to exit point"],
        )?;
        let r = telnet(port, "", "", false, &want);
        let _ = s.command("RMVEXITPGM EXITPNT(QIBM_QTG_DEVINIT) FORMAT(INIT0100) PGMNBR(1)", CMD);
        let (i, screen) = r?;
        if i > 1 {
            return Err(format!("the exit program's automatic sign-on showed:\n{screen}"));
        }
        let jobs = s.command("wrkactjob", CMD)?;
        let _ = jobs;
        Ok(format!("SHA-1 substitute signs {AUTO_USER} on; a wrong one, *FRCSIGNON and *REJECT show the sign-on display; an INIT0100 program signs {AUTO_USER} on"))
    }

    /// Step 98: the mail router's credentials.
    pub(crate) fn smtp_relay_auth(&self, s: &mut Session) -> Result<String, String> {
        let (port, rx) = fake_router()?;
        expect(s, &format!("CHGSMTPA MAILROUTER('10.0.2.2:{port}') FWDHUBSVR(*NONE)"), &["TCP1A10"])?;
        let _ = s.command("RMVSMTPLE TYPE(*HOSTAUTH) HOSTNAME('10.0.2.2')", CMD);
        expect(
            s,
            "ADDSMTPLE TYPE(*HOSTAUTH) HOSTNAME('10.0.2.2') USERNAME('mailer') PASSWORD('S3cret98')",
            &["TCP1D03"],
        )?;
        expect(
            s,
            "SNDSMTPEMM RCP(('ops@example.com')) SUBJECT('Step 98') NOTE('relay credentials')",
            &["TCP1A4B"],
        )?;
        let seen = rx.recv_timeout(Duration::from_secs(60)).map_err(|_| "the fake router saw no session".to_string())?;
        let want = format!("AUTH PLAIN {}", mail::encode::base64(b"\0mailer\0S3cret98"));
        if !seen.contains(&want) {
            return Err(format!("the router saw no AUTH PLAIN for mailer: {seen:?}"));
        }
        expect(s, "RMVSMTPLE TYPE(*HOSTAUTH) HOSTNAME('10.0.2.2')", &["TCP1D05"])?;
        Ok("ADDSMTPLE *HOSTAUTH kept sealed; SNDSMTPEMM signed on to the router with AUTH PLAIN".into())
    }
}
