// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 10 QEMU step 67: the FTP server and client. The server started
//! with TCP/IP; `CHGFTPA`, `ENDTCPSVR` and `STRTCPSVR *FTP` restart
//! `QTFTP00001`; a client on the host signs on through a port
//! forward as a user made for it, stores a member into a file the
//! server makes and a stream file into the home directory, reads both
//! back, runs `RCMD`, and leaves a script member; the `FTP` command on
//! the console runs that script against the server with `INPUT` and
//! `OUTPUT` overridden; `ENDTCPSVR *FTP` ends it all.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};
use crate::tcp::forward_port;

/// The guest's own address on QEMU's user network.
const GUEST: &str = "10.0.2.15";
/// The user the step makes.
const USER: &str = "FTPUSR";
/// Its password.
const PASSWORD: &str = "FtpPass26";
/// Text the step sends and expects back.
const TEXT: &str = "Quartz FTP step line 1\r\nsecond line\r\n";

fn expect(s: &mut Session, line: &str, want: &str) -> Result<String, String> {
    let out = s.command(line, CMD)?;
    if out.contains(want) {
        Ok(out)
    } else {
        Err(format!("{line}: no {want} in:\n{out}"))
    }
}

/// A host-side FTP client, enough for the step.
struct Ftp<'a> {
    suite: &'a Suite,
    ctl: BufReader<TcpStream>,
    log: Vec<String>,
}

impl<'a> Ftp<'a> {
    fn connect(suite: &'a Suite, port: u16) -> Result<Self, String> {
        let end = Instant::now() + Duration::from_secs(30);
        loop {
            // Slirp accepts before the guest does: a banner shows the
            // server is there.
            if let Ok(s) = TcpStream::connect(("127.0.0.1", port)) {
                let _ = s.set_read_timeout(Some(Duration::from_secs(30)));
                let mut f = Self {
                    suite,
                    ctl: BufReader::new(s),
                    log: Vec::new(),
                };
                if let Ok((220, _)) = f.reply() {
                    return Ok(f);
                }
            }
            if Instant::now() > end {
                return Err(format!("no FTP banner on port {port}"));
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    fn reply(&mut self) -> Result<(u16, String), String> {
        let mut first: Option<u16> = None;
        let mut text = String::new();
        loop {
            let mut line = String::new();
            let n = self.ctl.read_line(&mut line).map_err(|e| e.to_string())?;
            if n == 0 {
                return Err(format!("the connection ended:\n{}", self.log.join("\n")));
            }
            let line = line.trim_end().to_string();
            self.log.push(format!("S: {line}"));
            text.push_str(&line);
            text.push('\n');
            let code = line.get(..3).and_then(|c| c.parse::<u16>().ok());
            match (first, code) {
                (None, Some(c)) if line.as_bytes().get(3) == Some(&b'-') => first = Some(c),
                (None, Some(c)) => return Ok((c, text)),
                (Some(f), Some(c)) if c == f && line.as_bytes().get(3) != Some(&b'-') => return Ok((c, text)),
                _ => {}
            }
        }
    }

    fn cmd(&mut self, line: &str, want: u16) -> Result<String, String> {
        self.log.push(format!("C: {line}"));
        self.ctl
            .get_mut()
            .write_all(format!("{line}\r\n").as_bytes())
            .map_err(|e| e.to_string())?;
        let (code, text) = self.reply()?;
        if code != want {
            return Err(format!("{line}: {code}, not {want}:\n{}", self.log.join("\n")));
        }
        Ok(text)
    }

    /// A data connection: `PASV`, the guest's port forwarded.
    fn data(&mut self) -> Result<TcpStream, String> {
        let r = self.cmd("PASV", 227)?;
        let inner = r.split('(').nth(1).and_then(|x| x.split(')').next()).ok_or("no PASV address")?;
        let n: Vec<u16> = inner.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        if n.len() != 6 {
            return Err(format!("PASV address {inner}"));
        }
        let port = forward_port(self.suite, GUEST, n[4] * 256 + n[5])?;
        let s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
        let _ = s.set_read_timeout(Some(Duration::from_secs(30)));
        Ok(s)
    }

    fn retr(&mut self, line: &str) -> Result<Vec<u8>, String> {
        let mut d = self.data()?;
        // 150 for a transfer, 125 for a listing.
        let want = if line.starts_with("LIST") || line.starts_with("NLST") { 125 } else { 150 };
        self.cmd(line, want)?;
        let mut out = Vec::new();
        d.read_to_end(&mut out).map_err(|e| e.to_string())?;
        drop(d);
        let (code, _) = self.reply()?;
        if code / 100 != 2 {
            return Err(format!("{line} ended in {code}:\n{}", self.log.join("\n")));
        }
        Ok(out)
    }

    fn stor(&mut self, line: &str, data: &[u8]) -> Result<(), String> {
        let mut d = self.data()?;
        self.cmd(line, 150)?;
        d.write_all(data).map_err(|e| e.to_string())?;
        drop(d);
        let (code, _) = self.reply()?;
        if code != 226 {
            return Err(format!("{line} ended in {code}:\n{}", self.log.join("\n")));
        }
        Ok(())
    }
}

/// Wait until `WRKACTJOB` does (`present`) or does not show a job.
fn job_shown(s: &mut Session, name: &str, present: bool) -> Result<(), String> {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let out = s.command("wrkactjob", CMD)?;
        let shown = out.lines().any(|l| l.split_whitespace().next() == Some(name));
        if shown == present {
            return Ok(());
        }
        if Instant::now() > end {
            return Err(format!("{name} {} in:\n{out}", if present { "never shown" } else { "still shown" }));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[allow(clippy::unused_self)] // the steps share one signature
impl Suite {
    /// Step 67: the FTP server and the `FTP` client.
    pub(crate) fn ftp_server(&self, s: &mut Session) -> Result<String, String> {
        // A user of its own, with a library and a home directory.
        s.command("crtlib LIB(FTPLIB)", CMD)?;
        s.command(
            &format!("crtusrprf USRPRF({USER}) PASSWORD('{PASSWORD}') CURLIB(FTPLIB) HOMEDIR('/home/{USER}')"),
            CMD,
        )?;
        s.command(&format!("MKDIR DIR('/home/{USER}')"), CMD)?;
        s.command(&format!("CHGOWN OBJ('/home/{USER}') NEWOWN({USER})"), CMD)?;
        // The server started with TCP/IP (AUTOSTART(*YES)); it ends and
        // starts again with the attributes changed.
        job_shown(s, "QTFTP00001", true)?;
        expect(s, "CHGFTPA INACTTIMO(600) AUTOSTART(*NO)", "TCP1A10")?;
        expect(s, "ENDTCPSVR SERVER(*FTP)", "TCP1A0A")?;
        job_shown(s, "QTFTP00001", false)?;
        expect(s, "STRTCPSVR SERVER(*FTP)", "TCP1A07")?;
        job_shown(s, "QTFTP00001", true)?;
        expect(s, "STRTCPSVR SERVER(*FTP)", "TCP1A08")?;
        let port = forward_port(self, GUEST, 21)?;
        let mut f = Ftp::connect(self, port)?;
        f.cmd(&format!("USER {USER}"), 331)?;
        f.cmd(&format!("PASS {PASSWORD}"), 230)?;
        let syst = f.cmd("SYST", 215)?;
        if !syst.contains("OS/400") {
            return Err(format!("SYST: {syst}"));
        }
        // Name format 0 in the user's current library.
        let pwd = f.cmd("PWD", 257)?;
        if !pwd.contains("\"FTPLIB\" is current library.") {
            return Err(format!("PWD: {pwd}"));
        }
        f.cmd("TYPE A", 200)?;
        f.stor("STOR QFTPDATA.MBR1", TEXT.as_bytes())?;
        let back = f.retr("RETR QFTPDATA.MBR1")?;
        if back != TEXT.as_bytes() {
            return Err(format!("the member came back as {:?}", String::from_utf8_lossy(&back)));
        }
        let list = String::from_utf8_lossy(&f.retr("LIST QFTPDATA.*")?).to_string();
        if !list.contains("QFTPDATA.MBR1") {
            return Err(format!("LIST: {list}"));
        }
        f.cmd("RCMD CHKOBJ OBJ(FTPLIB/QFTPDATA) OBJTYPE(*FILE)", 250)?;
        f.cmd("RCMD CHKOBJ OBJ(FTPLIB/NOSUCH) OBJTYPE(*FILE)", 550)?;
        // A stream file in the home directory, in binary.
        f.cmd("SITE NAMEFMT 1", 250)?;
        f.cmd(&format!("CWD /home/{USER}"), 250)?;
        f.cmd("TYPE I", 200)?;
        let bin: Vec<u8> = (0..=255u8).collect();
        f.stor("STOR bytes.bin", &bin)?;
        if f.retr("RETR bytes.bin")? != bin {
            return Err("the stream file came back changed".into());
        }
        let size = f.cmd("SIZE bytes.bin", 213)?;
        if !size.contains("213 256") {
            return Err(format!("SIZE: {size}"));
        }
        // The script and log members for the FTP command.
        f.cmd("TYPE A", 200)?;
        f.cmd("CWD /QSYS.LIB/FTPLIB.LIB", 250)?;
        f.cmd("SITE NAMEFMT 0", 250)?;
        f.cmd("RCMD CRTSRCPF FILE(FTPLIB/QFTPSRC) RCDLEN(112)", 250)?;
        let script = format!("{USER} {PASSWORD}\r\nget QFTPDATA.MBR1 QFTPDATA.MBR2\r\nquote rcmd CHKOBJ OBJ(FTPLIB/QFTPDATA) OBJTYPE(*FILE) MBR(MBR2)\r\nquit\r\n");
        f.stor("STOR QFTPSRC.SCRIPT", script.as_bytes())?;
        f.cmd("RCMD ADDPFM FILE(FTPLIB/QFTPSRC) MBR(LOG)", 250)?;
        // A session job of its own shows while the session lasts.
        let jobs = s.command("wrkactjob", CMD)?;
        if !jobs.lines().any(|l| l.split_whitespace().next().is_some_and(|n| n.starts_with("QTFTP00") && n != "QTFTP00001")) {
            return Err(format!("no session job:\n{jobs}"));
        }
        f.cmd("QUIT", 221)?;
        // The FTP command against the server, in batch form.
        s.command("OVRDBF FILE(INPUT) TOFILE(FTPLIB/QFTPSRC) MBR(SCRIPT)", CMD)?;
        s.command("OVRDBF FILE(OUTPUT) TOFILE(FTPLIB/QFTPSRC) MBR(LOG)", CMD)?;
        let ran = s.command("FTP RMTSYS('127.0.0.1')", CMD)?;
        s.command("DLTOVR FILE(*ALL)", CMD)?;
        let log = s.command("dsppfm FILE(FTPLIB/QFTPSRC) MBR(LOG)", CMD)?;
        for want in ["230", "226", "250", "221"] {
            if !log.contains(want) {
                return Err(format!("the FTP command's log has no {want}:\n{ran}\n{log}"));
            }
        }
        // The member the client fetched is there.
        let chk = s.command("CHKOBJ OBJ(FTPLIB/QFTPDATA) OBJTYPE(*FILE) MBR(MBR2)", CMD)?;
        if chk.contains("CPF") {
            return Err(format!("the member the FTP command fetched is not there:
{chk}
{log}"));
        }
        expect(s, "ENDTCPSVR SERVER(*FTP)", "TCP1A0A")?;
        job_shown(s, "QTFTP00001", false)?;
        expect(s, "ENDTCPSVR SERVER(*FTP)", "TCP1A09")?;
        Ok(format!(
            "QTFTP00001 served host port {port}: sign-on as {USER}, a member made and read back, RCMD, a stream file in binary; the FTP command ran a script with INPUT and OUTPUT overridden"
        ))
    }
}
