// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Steps 114 to 117, in a guest of their own the way Proxmox and libvirt
//! dress one: a virtio-serial port `org.qemu.guest_agent.0` on a Unix
//! socket, a SATA disk on the q35 AHCI controller beside the virtio one,
//! a FAT USB stick, and a TPM when `swtpm` is on the host.
//!
//! - 114: `QSHWQUAL` at the console, printed, and written to the stick.
//! - 115: the guest agent as a management stack talks to it.
//! - 116: the agent's channel among the `*CMN` resources.
//! - 117: `guest-shutdown`, a controlled power down.
//!
//! The media suite's step 5 (`media_qshwqual`) runs `QSHWQUAL` from the
//! install media's menu.

use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::qemu::Launch;
use crate::qemu::session::Session;
use crate::steps::{CMD, StepReport, Suite};

/// The steps' names, for `--only`.
const NAMES: [&str; 4] = ["114 QSHWQUAL", "115 guest agent", "116 agent channel resource", "117 agent power down"];

/// Where the stick's FAT partition starts (`media::fat_stick_of`).
const FAT_AT: usize = 2048 * 512;

/// A blank FAT stick image.
fn stick(path: &Path) -> Result<(), String> {
    let readme = ("README.TXT".to_string(), b"QS/400 conformance stick\r\n".to_vec());
    let img = crate::media::fat_stick_of(&[readme])?;
    // Room for the report.
    let mut img = img;
    if img.len() < 16 << 20 {
        img.resize(16 << 20, 0);
    }
    fs::write(path, img).map_err(|e| e.to_string())
}

/// The report on the stick, read with the same FAT code that wrote it.
pub(crate) fn report_on_stick(path: &Path) -> Result<String, String> {
    let img = fs::read(path).map_err(|e| e.to_string())?;
    let part = img.get(FAT_AT..).ok_or("the stick has no partition")?.to_vec();
    hwqual::usb::read_from(Cursor::new(part), hwqual::USB_FILE)
}

/// swtpm for the guest's TPM, when the host has it.
fn swtpm(dir: &Path) -> Option<(Child, PathBuf)> {
    let state = dir.join("tpm");
    fs::create_dir_all(&state).ok()?;
    let sock = dir.join("swtpm.sock");
    let _ = fs::remove_file(&sock);
    let child = Command::new("swtpm")
        .args(["socket", "--tpm2", "--tpmstate"])
        .arg(format!("dir={}", state.display()))
        .arg("--ctrl")
        .arg(format!("type=unixio,path={}", sock.display()))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while !sock.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    sock.exists().then_some((child, sock))
}

/// A client of the guest agent over QEMU's socket.
struct Agent {
    s: std::os::unix::net::UnixStream,
    buf: Vec<u8>,
}

impl Agent {
    fn connect(path: &Path) -> Result<Self, String> {
        let s = std::os::unix::net::UnixStream::connect(path).map_err(|e| format!("{}: {e}", path.display()))?;
        s.set_read_timeout(Some(Duration::from_millis(500))).map_err(|e| e.to_string())?;
        Ok(Self { s, buf: Vec::new() })
    }

    fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.s.write_all(bytes).map_err(|e| e.to_string())
    }

    /// The next reply line within `wait`, after a `0xFF` when `delimited`.
    fn line(&mut self, wait: Duration, delimited: bool) -> Result<Value, String> {
        let deadline = Instant::now() + wait;
        loop {
            if delimited
                && let Some(i) = self.buf.iter().position(|b| *b == 0xFF)
            {
                self.buf.drain(..=i);
            }
            if (!delimited || !self.buf.is_empty())
                && let Some(i) = self.buf.iter().position(|b| *b == b'\n')
            {
                let l: Vec<u8> = self.buf.drain(..=i).collect();
                let text = String::from_utf8_lossy(&l);
                return serde_json::from_str(text.trim()).map_err(|e| format!("{e}: {text}"));
            }
            if Instant::now() >= deadline {
                return Err(format!("no reply from the guest agent within {wait:?}"));
            }
            let mut b = [0u8; 4096];
            match self.s.read(&mut b) {
                Ok(0) => return Err("the guest agent's socket closed".into()),
                Ok(n) => self.buf.extend_from_slice(&b[..n]),
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
    }

    /// `guest-sync-delimited` until the agent answers, as libvirt does.
    fn sync(&mut self, wait: Duration) -> Result<(), String> {
        let deadline = Instant::now() + wait;
        let mut id = 7_000_u64;
        loop {
            id += 1;
            self.buf.clear();
            // A raw 0xFF first: the agent drops anything half read.
            let mut req = vec![0xFF];
            req.extend(format!("{{\"execute\":\"guest-sync-delimited\",\"arguments\":{{\"id\":{id}}}}}\n").into_bytes());
            self.send(&req)?;
            if let Ok(v) = self.line(Duration::from_secs(2), true)
                && v["return"] == id
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!("the guest agent did not sync within {wait:?}"));
            }
        }
    }

    fn call(&mut self, cmd: &str, args: Option<&str>) -> Result<Value, String> {
        let req = match args {
            Some(a) => format!("{{\"execute\":\"{cmd}\",\"arguments\":{a}}}\n"),
            None => format!("{{\"execute\":\"{cmd}\"}}\n"),
        };
        self.send(req.as_bytes())?;
        let v = self.line(Duration::from_secs(20), false)?;
        if v.get("error").is_some() {
            return Err(format!("{cmd}: {v}"));
        }
        Ok(v["return"].clone())
    }
}

impl Suite {
    /// Steps 114 to 117 in their own guest; with `--only` when named.
    pub(crate) fn guest_steps(&self) -> Result<Vec<StepReport>, crate::HarnessError> {
        if let Some(only) = &self.only
            && !NAMES.iter().any(|n| only.iter().any(|p| n.starts_with(p.as_str())))
        {
            return Ok(Vec::new());
        }
        let sata = self.out_dir.join("guest-sata.img");
        fs::File::create(&sata)?.set_len(2 << 30)?;
        let usb = self.out_dir.join("guest-stick.img");
        stick(&usb).map_err(|reason| crate::HarnessError::Failed {
            step: "114 QSHWQUAL",
            reason,
            log: String::new(),
        })?;
        let tpm = swtpm(&self.out_dir);
        let launch = Launch {
            guest_agent: Some(self.out_dir.join("qga.sock")),
            sata_disks: vec![sata],
            usb_stick: Some(usb),
            tpm: tpm.as_ref().map(|(_, s)| s.clone()),
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&launch)?;
        let reports = vec![
            self.step(&mut s, NAMES[0], Self::qshwqual)?,
            self.step(&mut s, NAMES[1], Self::guest_agent)?,
            self.step(&mut s, NAMES[2], Self::agent_channel)?,
            self.step(&mut s, NAMES[3], Self::agent_power_down)?,
        ];
        if !s.wait_exit(Duration::from_secs(5)) {
            s.command("pwrdwn", CMD).ok();
            s.wait_exit(Duration::from_secs(20));
        }
        if let Some((mut c, _)) = tpm {
            let _ = c.kill();
            let _ = c.wait();
        }
        Ok(reports)
    }

    /// 114: the report at the console, printed, and on the stick.
    fn qshwqual(&self, s: &mut Session) -> Result<String, String> {
        s.expect(crate::steps::SRC_IPL_COMPLETE, 4 * self.boot_timeout)?;
        s.expect("Service console ready", 4 * self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        let out = s.command("QSHWQUAL", CMD)?;
        fs::write(self.out_dir.join("qshwqual.txt"), &out).map_err(|e| e.to_string())?;
        for want in [
            "QS/400 Hardware Qualification (QSHWQUAL)",
            "Hypervisor . . . . . :   KVM/QEMU",
            "PASS   UEFI",
            "TPM",
            "Storage controllers and disk units",
            "QD03-001",
            "QD01-001",
            "org.qemu.guest_agent.0",
            "End of report",
        ] {
            if !out.contains(want) {
                return Err(format!("QSHWQUAL shows no `{want}`:\n{out}"));
            }
        }
        let tpm = if out.contains("PASS   TPM") { "TPM 2.0 passed (swtpm)" } else { "TPM absent: WARN" };
        if !out.contains("PASS   TPM") && !out.contains("WARN   TPM") {
            return Err(format!("QSHWQUAL has no TPM line:\n{out}"));
        }
        let result = out
            .lines()
            .find(|l| l.contains("Result . . . . . . . :"))
            .map(|l| l.split(":   ").nth(1).unwrap_or("").trim().to_string())
            .ok_or("QSHWQUAL has no result")?;
        let printed = s.command("QSHWQUAL OUTPUT(*PRINT)", CMD)?;
        if !printed.contains("report printed") {
            return Err(format!("QSHWQUAL OUTPUT(*PRINT): {printed}"));
        }
        let usb = s.command("QSHWQUAL TODEV(*USB)", CMD)?;
        if !usb.contains("written to QSHWQUAL.TXT") {
            return Err(format!("QSHWQUAL TODEV(*USB): {usb}"));
        }
        let word = s.command("svc.qshwqual", CMD)?;
        if !word.contains("End of report") {
            return Err(format!("svc.qshwqual: {word}"));
        }
        Ok(format!("result {result}; {tpm}; AHCI and virtio controllers; printed; written to the stick"))
    }

    /// 115: sync, ping, info and the facts, as libvirt and Proxmox ask.
    fn guest_agent(&self, s: &mut Session) -> Result<String, String> {
        let log = s.log.clone();
        if !log.contains("slic: guest agent ") {
            s.expect("slic: guest agent ", CMD)?;
        }
        let mut a = Agent::connect(&self.out_dir.join("qga.sock"))?;
        a.sync(Duration::from_secs(60))?;
        let ping = a.call("guest-ping", None)?;
        if ping != serde_json::json!({}) {
            return Err(format!("guest-ping: {ping}"));
        }
        let info = a.call("guest-info", None)?;
        let cmds: Vec<String> = info["supported_commands"]
            .as_array()
            .map(|v| v.iter().filter_map(|c| c["name"].as_str().map(String::from)).collect())
            .unwrap_or_default();
        for want in ["guest-shutdown", "guest-network-get-interfaces", "guest-get-fsinfo", "guest-get-osinfo"] {
            if !cmds.iter().any(|c| c == want) {
                return Err(format!("guest-info lacks {want}: {info}"));
            }
        }
        let host = a.call("guest-get-host-name", None)?;
        let name = host["host-name"].as_str().unwrap_or_default().to_string();
        let sysname = s.command("svc.sysval QSYSNAME", CMD)?;
        if name.is_empty() || !sysname.contains(&name) {
            return Err(format!("guest-get-host-name {host}; QSYSNAME {sysname}"));
        }
        let os = a.call("guest-get-osinfo", None)?;
        if os["id"] != "qs400" || !os["version"].as_str().unwrap_or("").starts_with('V') {
            return Err(format!("guest-get-osinfo: {os}"));
        }
        let nics = a.call("guest-network-get-interfaces", None)?;
        let addrs: Vec<String> = nics
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|n| n["ip-addresses"].as_array().cloned().unwrap_or_default())
            .filter_map(|a| a["ip-address"].as_str().map(String::from))
            .collect();
        let count = nics.as_array().map_or(0, Vec::len);
        if count == 0 {
            return Err(format!("guest-network-get-interfaces: {nics}"));
        }
        let fsinfo = a.call("guest-get-fsinfo", None)?;
        let total = fsinfo[0]["total-bytes"].as_u64().unwrap_or(0);
        if total == 0 {
            return Err(format!("guest-get-fsinfo: {fsinfo}"));
        }
        let st = a.call("qs400-get-status", None)?;
        if st["ipl-state"] != "complete" || st["last-src"].as_str().unwrap_or("").len() != 8 {
            return Err(format!("qs400-get-status: {st}"));
        }
        Ok(format!(
            "synced; {} commands; host {name}; {} {}; {count} interfaces, {} addresses; IFS {} MiB; last SRC {}",
            cmds.len(),
            os["pretty-name"].as_str().unwrap_or(""),
            os["kernel-release"].as_str().unwrap_or(""),
            addrs.len(),
            total >> 20,
            st["last-src"].as_str().unwrap_or("")
        ))
    }

    /// 116: `WRKHDWRSC *CMN` lists the channel as the other ports are.
    fn agent_channel(&self, s: &mut Session) -> Result<String, String> {
        use crate::phase5::SECOFR_PASSWORD;
        use crate::phase8::{connect, run, secofr};
        use tn5250_driver::script::Context;
        use tn5250_driver::{Connect, ScriptOptions};
        // The list as text, where no panel shows it.
        let out = s.command("DSPHDWRSC TYPE(*CMN) OUTPUT(*)", CMD)?;
        fs::write(self.out_dir.join("dsphdwrsc-cmn.txt"), &out).map_err(|e| e.to_string())?;
        let row = out
            .lines()
            .find(|l| l.contains("Virtual Channel"))
            .ok_or_else(|| format!("DSPHDWRSC *CMN lists no channel:\n{out}"))?
            .trim()
            .to_string();
        if !row.contains("CMN") || !row.contains("QV01") || !row.contains("Operational") {
            return Err(format!("the channel's row: {row}"));
        }
        let name = row.split_whitespace().next().unwrap_or("").to_string();
        let hw = s.command("svc.hw", CMD)?;
        if !hw.lines().any(|l| l.starts_with(&name) && l.contains("org.qemu.guest_agent.0")) {
            return Err(format!("svc.hw does not name {name} the agent's port:\n{hw}"));
        }
        // The panel at a display: its row laid out as the port's.
        let set = s.command(
            &format!("CHGUSRPRF USRPRF(QSECOFR) PASSWORD('{SECOFR_PASSWORD}') PWDEXP(*NO)"),
            self.boot_timeout,
        )?;
        if set.trim_start().starts_with("CPF") {
            return Err(format!("CHGUSRPRF QSECOFR: {set}"));
        }
        crate::tcp::guest_address(s)?;
        let port = self.launch.telnet_port.ok_or("the launch has no telnet port")?;
        let c = Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
        let options = ScriptOptions {
            golden: None,
            update: false,
            release: String::new(),
            timeout_ms: 20000,
        };
        let mut ctx = Context::new(&c, &options);
        let mut d = connect(port, "QSECOFR", SECOFR_PASSWORD)?;
        secofr(&mut d, &mut ctx)?;
        run(&mut d, &mut ctx, "wait \"Main Menu\"\n")?;
        run(&mut d, &mut ctx, "type \"WRKHDWRSC TYPE(*CMN)\"\nkey enter\nsettle 2500\n")?;
        let screen = crate::hwrsrc::screen(&d);
        fs::write(self.out_dir.join("wrkhdwrsc-cmn.txt"), &screen).map_err(|e| e.to_string())?;
        let mine = crate::hwrsrc::row_of(&d, &name).ok_or_else(|| format!("WRKHDWRSC *CMN has no {name}:\n{screen}"))?;
        let port_row = crate::hwrsrc::row_of(&d, "CMN01").ok_or_else(|| format!("no CMN01:\n{screen}"))?;
        let col = |r: &str, w: &str| r.find(w);
        if !mine.contains("QV01") || !mine.contains("Virtual Channel") || col(&mine, "Operational") != col(&port_row, "Operational") {
            return Err(format!("{name}'s row is not laid out as CMN01's:\n{port_row}\n{mine}"));
        }
        run(&mut d, &mut ctx, "key f3\nsettle 1000\n")?;
        Ok(format!("`{}`; on the panel `{}`", row, mine.trim()))
    }

    /// 117: `guest-shutdown` powers the system down, controlled.
    fn agent_power_down(&self, s: &mut Session) -> Result<String, String> {
        let mut a = Agent::connect(&self.out_dir.join("qga.sock"))?;
        a.sync(Duration::from_secs(30))?;
        a.send(b"{\"execute\":\"guest-shutdown\",\"arguments\":{\"mode\":\"powerdown\"}}\n")?;
        // Success has no reply; the power down shows on the console.
        s.expect("SRC D1001000", Duration::from_secs(60))?;
        s.expect("SRC D1001090", Duration::from_mins(11))?;
        if !s.wait_exit(Duration::from_secs(30)) {
            return Err("QEMU did not exit after the power down".into());
        }
        Ok("D1001000 to D1001090, QEMU exited".into())
    }

    /// Media step 5: `QSHWQUAL` from the install media's menu, on a
    /// machine with nothing installed, and the report on a stick.
    pub(crate) fn media_qshwqual(&self, s: &mut Session) -> Result<String, String> {
        let stick = self.out_dir.join("media-stick.img");
        s.expect_in_order(crate::steps::SRC_SEQUENCE_MEDIA, self.boot_timeout)?;
        s.expect("Selection", self.boot_timeout)?;
        s.send("2")?;
        s.expect("Install Licensed Internal Code", CMD)?;
        s.expect("Selection", CMD)?;
        s.send("4")?;
        s.expect("Hardware Qualification", CMD)?;
        s.expect("Selection", CMD)?;
        s.send("1")?;
        s.expect("Hardware Qualification Report", Duration::from_secs(60))?;
        std::thread::sleep(Duration::from_secs(1));
        let screen = s.command("screen", CMD).unwrap_or_default();
        self.snap("media-qshwqual");
        if !screen.contains("Result . . . . . . . :") || !screen.contains("UEFI") {
            // The panel as the serial line drew it.
            s.expect("UEFI", CMD).map_err(|e| format!("{e}; the report display:\n{screen}"))?;
        }
        s.send("key f12")?;
        s.expect("Selection", CMD)?;
        s.send("2")?;
        // The message line may not reach the serial line; the stick is read below.
        let told = s.expect("written to QSHWQUAL.TXT", Duration::from_secs(30)).is_ok();
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(20));
        let text = report_on_stick(&stick)?;
        fs::write(self.out_dir.join("media-qshwqual.txt"), &text).map_err(|e| e.to_string())?;
        if !text.contains("End of report") || !text.contains("Storage controllers and disk units") {
            return Err(format!("the stick's report:\n{text}"));
        }
        let result = text
            .lines()
            .find(|l| l.contains("Result . . . . . . . :"))
            .map_or("", |l| l.split(":   ").nth(1).unwrap_or(""))
            .trim()
            .to_string();
        let said = if told { "; the message said so" } else { "" };
        Ok(format!("from the media's menu: {result}; the report on the stick ({} lines){said}", text.lines().count()))
    }

    /// Media step 5's guest: the ISO alone with a SATA disk and a stick.
    pub(crate) fn run_media_qshwqual(&self, iso: &Path) -> Result<StepReport, crate::HarnessError> {
        let name = "5 QSHWQUAL from media";
        if let Some(only) = &self.only
            && !only.iter().any(|p| name.starts_with(p.as_str()))
        {
            return Ok(StepReport { name, elapsed: Duration::ZERO, detail: "skipped".into() });
        }
        let usb = self.out_dir.join("media-stick.img");
        stick(&usb).map_err(|reason| crate::HarnessError::Failed { step: name, reason, log: String::new() })?;
        let sata = self.out_dir.join("media-sata.img");
        fs::File::create(&sata)?.set_len(2 << 30)?;
        let launch = Launch {
            image: None,
            cdrom: Some(iso.to_path_buf()),
            cdrom_first: true,
            sata_disks: vec![sata],
            usb_stick: Some(usb),
            guest_agent: Some(self.out_dir.join("qga-media.sock")),
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&launch)?;
        let r = self.step(&mut s, name, Self::media_qshwqual);
        s.kill();
        r
    }
}
