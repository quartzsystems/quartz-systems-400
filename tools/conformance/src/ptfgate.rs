// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The PTF release gate (docs/releasing.md, "PTFs"): a PTF is published
//! only when it installs on the release it is for, as shipped. The
//! released media is installed on a blank disk in a guest of its own
//! (with a writable variable store, for a LIC PTF's slots); the gate
//! then does what a customer does, with no service console (a release
//! has none): 5250 over telnet and FTP from the host. It checks that the
//! PTF's release notes give the steps it runs, sends the PTF by FTP
//! into a `*SAVF`, and runs `LODPTF`, `APYPTF APY(*TEMP)`, an IPL,
//! `DSPPTF`, `RMVPTF`, an IPL, `APYPTF APY(*PERM)` and an IPL.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use tn5250_driver::ScriptOptions;
use tn5250_driver::script::Context;

use crate::error::HarnessError;
use crate::ftpsrv::Ftp;
use crate::phase5::SECOFR_PASSWORD;
use crate::phase8::{connect, run, secofr};
use crate::qemu::Launch;
use crate::qemu::session::Session;
use crate::steps::{SRC_IPL_COMPLETE, SRC_SEQUENCE_MEDIA, Suite};
use crate::tcp::{GUEST, forward_port};

/// A disk for the installed system.
const DISK: u64 = 4 << 30;
/// What a LIC PTF's first IPL posts as it comes up on the other slot.
const SRC_SLOT_SWITCH: &str = "SRC C6004503";

/// One PTF through the gate.
#[derive(Debug, Clone)]
pub struct GateReport {
    /// The PTF.
    pub ptf: String,
    /// Whether every step passed.
    pub passed: bool,
    /// Each step, in order, and the first failure.
    pub detail: String,
    /// The guest's serial log, for a failure.
    pub log: String,
}

/// The PTF the file holds and what its release notes say to run.
struct Ptf {
    id: String,
    product: String,
    lic: bool,
    bytes: Vec<u8>,
}

/// The lines the release notes must give, as the gate runs them.
fn instructed(id: &str, product: &str) -> Vec<String> {
    vec![
        format!("CRTSAVF FILE(QGPL/{id})"),
        format!("put {id}.savf QGPL/{id}"),
        format!("LODPTF LICPGM({product}) DEV(*SAVF) SELECT({id})"),
        format!("APYPTF LICPGM({product}) SELECT({id})"),
    ]
}

fn sibling(path: &Path, ext: &str) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(ext);
    PathBuf::from(p)
}

/// The PTF at `path`, its checksum file and notes beside it.
fn ptf_at(path: &Path) -> Result<Ptf, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (h, _) = ptf::package::peek(&bytes).map_err(|e| format!("not a PTF: {e}"))?;
    let id = h.ptf.clone();
    // Step 1 of the notes: the download checked.
    if let Ok(sum) = fs::read_to_string(sibling(path, ".sha256")) {
        let want = sum
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let got = hex::encode(Sha256::digest(&bytes));
        if want != got {
            return Err(format!("{id}.savf.sha256 says {want}, the file is {got}"));
        }
    }
    let notes_path = path.with_extension("txt");
    let notes = fs::read_to_string(&notes_path)
        .map_err(|e| format!("no release notes {}: {e}", notes_path.display()))?;
    let product = release::product_id(&h.product).to_string();
    let missing: Vec<String> = instructed(&id, &product)
        .into_iter()
        .filter(|l| !notes.contains(l.as_str()))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "the release notes do not say how to install it: no {}",
            missing.join(", ")
        ));
    }
    Ok(Ptf {
        id,
        product,
        lic: h.lic,
        bytes,
    })
}

/// A 5250 session signed on as the security officer, at the main menu.
struct Tn {
    d: tn5250_driver::session::Session,
    ctx: Context,
}

impl Tn {
    fn open(port: u16) -> Result<Self, String> {
        let c = tn5250_driver::Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
        let options = ScriptOptions {
            timeout_ms: 60_000,
            ..ScriptOptions::default()
        };
        let mut ctx = Context::new(&c, &options);
        let mut d = connect(port, "QSECOFR", SECOFR_PASSWORD)?;
        secofr(&mut d, &mut ctx)?;
        run(&mut d, &mut ctx, "wait \"Main Menu\"\n")?;
        Ok(Self { d, ctx })
    }

    /// Type `cmd` on the menu's command line; the screen it leaves.
    fn command(&mut self, cmd: &str) -> Result<String, String> {
        run(
            &mut self.d,
            &mut self.ctx,
            &format!("type \"{cmd}\"\nkey enter\nsettle 3000\n"),
        )?;
        let screen = self.screen();
        run(&mut self.d, &mut self.ctx, "key f3\nsettle 1500\n")?;
        Ok(screen)
    }

    fn screen(&self) -> String {
        let mut out = String::new();
        for row in self.d.term.text_rows() {
            out.push_str(row.trim_end());
            out.push('\n');
        }
        out
    }
}

impl Drop for Tn {
    fn drop(&mut self) {
        self.d.disconnect();
    }
}

/// What the gate did so far, for the report.
#[derive(Default)]
struct Log {
    lines: Vec<String>,
}

impl Log {
    fn ok(&mut self, what: impl Into<String>) {
        let what = what.into();
        println!("    ok  {what}");
        self.lines.push(what);
    }
}

impl Suite {
    /// The gate for every PTF in `ptfs` against the release media the
    /// suite was given (`--iso`), each on a system installed fresh.
    pub fn run_ptf_gate(&self, ptfs: &[PathBuf]) -> Result<Vec<GateReport>, HarnessError> {
        let mut out = Vec::new();
        for p in ptfs {
            out.push(self.gate_one(p)?);
        }
        Ok(out)
    }

    fn gate_one(&self, path: &Path) -> Result<GateReport, HarnessError> {
        let start = Instant::now();
        let name = path
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        println!("  GATE {name}");
        let report = |passed: bool, detail: String, log: String| GateReport {
            ptf: name.clone(),
            passed,
            detail,
            log,
        };
        let ptf = match ptf_at(path) {
            Ok(p) => p,
            Err(e) => return Ok(report(false, e, String::new())),
        };
        let iso = self.launch.cdrom.clone().or_else(|| self.iso.clone());
        let Some(iso) = iso else {
            return Ok(report(
                false,
                "no release media (--iso)".into(),
                String::new(),
            ));
        };
        let Some(template) = crate::firmware::locate_vars(&self.launch.firmware) else {
            return Ok(report(
                false,
                format!("no OVMF variable store (set {})", crate::firmware::VARS_ENV),
                String::new(),
            ));
        };
        let vars = self.out_dir.join(format!("gate-{}-vars.fd", ptf.id));
        fs::copy(&template, &vars)?;
        let disk = self.out_dir.join(format!("gate-{}.img", ptf.id));
        crate::install::blank(&disk, DISK)?;
        let launch = Launch {
            vars: Some(vars),
            image: Some(disk),
            cdrom: Some(iso),
            cdrom_first: true,
            extra_disks: Vec::new(),
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&launch)?;
        let mut log = Log::default();
        let r = self.gate_steps(&mut s, &ptf, &mut log);
        s.kill();
        let mut detail = log.lines.join("; ");
        let passed = r.is_ok();
        if let Err(e) = r {
            println!("    FAIL {e}");
            if !detail.is_empty() {
                detail.push_str("; ");
            }
            detail.push_str("FAILED: ");
            detail.push_str(&e);
        }
        println!(
            "  {} {name} {:.0}s",
            if passed { "PASS" } else { "FAIL" },
            start.elapsed().as_secs_f64()
        );
        Ok(report(
            passed,
            detail,
            if passed { String::new() } else { s.log.clone() },
        ))
    }

    fn gate_steps(&self, s: &mut Session, p: &Ptf, log: &mut Log) -> Result<(), String> {
        let install_wait = self.boot_timeout * 10;
        // The media's automatic install, then the IPL from the disk.
        s.expect_in_order(SRC_SEQUENCE_MEDIA, install_wait)?;
        s.expect("Selection", install_wait)?;
        s.send("4")?;
        s.expect("SRC C6004100", install_wait)?;
        s.expect(SRC_IPL_COMPLETE, install_wait)?;
        log.ok("release installed from its media");
        let port = self.telnet_port().ok_or("the launch has no telnet port")?;

        self.ftp_up(port)?;
        let mut f = self.ftp_signed_on()?;
        f.cmd(&format!("RCMD CRTSAVF FILE(QGPL/{})", p.id), 250)?;
        f.cmd("TYPE I", 200)?;
        f.stor(&format!("STOR QGPL/{}", p.id), &p.bytes)?;
        log.ok(format!(
            "{} bytes by FTP into save file QGPL/{}",
            p.bytes.len(),
            p.id
        ));
        let lp = format!("LICPGM({}) SELECT({})", p.product, p.id);
        f.cmd(
            &format!(
                "RCMD LODPTF LICPGM({}) DEV(*SAVF) SELECT({})",
                p.product, p.id
            ),
            250,
        )?;
        log.ok("LODPTF");
        f.cmd(&format!("RCMD APYPTF {lp} APY(*TEMP)"), 250)?;
        log.ok("APYPTF APY(*TEMP)");
        drop(f);

        self.gate_ipl(s, port, p.lic)?;
        let shown = Tn::open(port)?.command(&format!("DSPPTF {lp}"))?;
        want(
            &shown,
            "DSPPTF after the IPL",
            &[&p.id, "Temporarily applied"],
        )?;
        if p.lic {
            want(&shown, "DSPPTF after the IPL", &["##MACH#B"])?;
        }
        log.ok(format!(
            "IPL{}, DSPPTF: temporarily applied",
            if p.lic { " on slot B" } else { "" }
        ));

        self.ftp_up(port)?;
        let mut f = self.ftp_signed_on()?;
        f.cmd(&format!("RCMD RMVPTF {lp}"), 250)?;
        drop(f);
        self.gate_ipl(s, port, false)?;
        let shown = Tn::open(port)?.command(&format!("DSPPTF {lp}"))?;
        want(&shown, "DSPPTF after RMVPTF", &["Temporarily removed"])?;
        log.ok("RMVPTF, IPL, DSPPTF: temporarily removed");

        self.ftp_up(port)?;
        let mut f = self.ftp_signed_on()?;
        f.cmd(&format!("RCMD APYPTF {lp} APY(*PERM)"), 250)?;
        drop(f);
        self.gate_ipl(s, port, p.lic)?;
        let shown = Tn::open(port)?.command(&format!("DSPPTF {lp}"))?;
        want(&shown, "DSPPTF after APY(*PERM)", &["Permanently applied"])?;
        log.ok("APYPTF APY(*PERM), IPL, DSPPTF: permanently applied");
        Ok(())
    }

    /// The FTP server started from the main menu, unless it answers.
    fn ftp_up(&self, port: u16) -> Result<(), String> {
        let fwd = forward_port(self, GUEST, 21)?;
        if std::net::TcpStream::connect_timeout(
            &([127, 0, 0, 1], fwd).into(),
            Duration::from_secs(5),
        )
        .is_ok()
            && Ftp::connect(self, fwd).is_ok()
        {
            return Ok(());
        }
        Tn::open(port)?.command("STRTCPSVR SERVER(*FTP)")?;
        Ok(())
    }

    fn ftp_signed_on(&self) -> Result<Ftp<'_>, String> {
        let fwd = forward_port(self, GUEST, 21)?;
        let mut f = Ftp::connect(self, fwd)?;
        f.cmd("USER QSECOFR", 331)?;
        f.cmd(&format!("PASS {SECOFR_PASSWORD}"), 230)?;
        Ok(f)
    }

    /// An IPL by `PWRDWNSYS` over FTP; a LIC PTF's posts its slot switch.
    fn gate_ipl(&self, s: &mut Session, port: u16, switch: bool) -> Result<(), String> {
        let _ = port;
        let mut f = self.ftp_signed_on()?;
        // The connection may end before the reply: the system is going down.
        let _ = f.cmd("RCMD PWRDWNSYS OPTION(*IMMED) RESTART(*YES)", 250);
        drop(f);
        s.expect("SRC D1001000", self.boot_timeout * 4)?;
        if switch {
            s.expect(SRC_SLOT_SWITCH, self.boot_timeout * 10)?;
        }
        s.expect(SRC_IPL_COMPLETE, self.boot_timeout * 10)?;
        Ok(())
    }
}

fn want(shown: &str, what: &str, needles: &[&str]) -> Result<(), String> {
    if needles.iter().all(|n| shown.contains(n)) {
        Ok(())
    } else {
        Err(format!("{what}: expected {needles:?} on:\n{shown}"))
    }
}
