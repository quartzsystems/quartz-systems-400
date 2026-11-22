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
use crate::steps::{SRC_IPL_COMPLETE, Suite};
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

/// A PTF save file, its header read.
struct Ptf {
    id: String,
    product: String,
    lic: bool,
    prereqs: Vec<String>,
    bytes: Vec<u8>,
}

/// The lines the release notes must give, as the gate runs them.
fn instructed(p: &Ptf) -> Vec<String> {
    let (id, product) = (&p.id, &p.product);
    let mut lines = vec![
        format!("CRTSAVF FILE(QGPL/{id})"),
        format!("put {id}.savf QGPL/{id}"),
        format!("LODPTF LICPGM({product}) DEV(*SAVF) SELECT({id})"),
        format!("APYPTF LICPGM({product}) SELECT({id})"),
    ];
    lines.extend(p.prereqs.iter().map(|q| format!("needs {q}")));
    lines
}

fn sibling(path: &Path, ext: &str) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(ext);
    PathBuf::from(p)
}

/// The PTF at `path`, checked against the `.sha256` beside it.
fn load(path: &Path) -> Result<Ptf, String> {
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
    Ok(Ptf {
        id,
        product: release::product_id(&h.product).to_string(),
        lic: h.lic,
        prereqs: h.prereqs.iter().map(|r| r.ptf.clone()).collect(),
        bytes,
    })
}

/// The PTF at `path` and its notes, which must give the gate's steps.
fn ptf_at(path: &Path) -> Result<Ptf, String> {
    let p = load(path)?;
    let notes_path = path.with_extension("txt");
    let notes = fs::read_to_string(&notes_path)
        .map_err(|e| format!("no release notes {}: {e}", notes_path.display()))?;
    let missing: Vec<String> = instructed(&p)
        .into_iter()
        .filter(|l| !notes.contains(l.as_str()))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "the release notes do not say how to install it: no {}",
            missing.join(", ")
        ));
    }
    Ok(p)
}

/// `p`'s prerequisites from the save files beside `path`, each after its
/// own, the first to install first.
fn prerequisites(path: &Path, p: &Ptf) -> Result<Vec<Ptf>, String> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut out: Vec<Ptf> = Vec::new();
    let mut todo: Vec<(String, bool)> =
        p.prereqs.iter().rev().map(|q| (q.clone(), false)).collect();
    while let Some((id, expanded)) = todo.pop() {
        if out.iter().any(|q| q.id == id) {
            continue;
        }
        let q = load(&dir.join(format!("{id}.savf")))
            .map_err(|e| format!("prerequisite {id} (beside the PTF): {e}"))?;
        if expanded || q.prereqs.iter().all(|r| out.iter().any(|o| &o.id == r)) {
            out.push(q);
        } else {
            todo.push((id, true));
            todo.extend(q.prereqs.iter().rev().map(|r| (r.clone(), false)));
        }
    }
    Ok(out)
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
        let prereqs = match prerequisites(path, &ptf) {
            Ok(q) => q,
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
        // The release media as a stick with a response file: an address
        // for telnet and FTP, the security officer's password.
        let media = match fs::read(&iso)
            .map_err(|e| e.to_string())
            .and_then(|b| crate::install::rsp_stick(&b, &rsp()))
        {
            Ok(m) => m,
            Err(e) => return Ok(report(false, format!("the media: {e}"), String::new())),
        };
        let stick = self.out_dir.join(format!("gate-{}-media.img", ptf.id));
        fs::write(&stick, media)?;
        let launch = Launch {
            vars: Some(vars),
            image: Some(stick),
            cdrom: None,
            cdrom_first: false,
            extra_disks: vec![disk.clone()],
            ..self.launch.clone()
        };
        // After the install the media comes out: the disk alone from then on.
        let installed = Launch {
            image: Some(disk),
            extra_disks: Vec::new(),
            ..launch.clone()
        };
        let mut s = Session::spawn(&launch)?;
        let mut log = Log::default();
        let r = self.gate_steps(&mut s, &installed, &prereqs, &ptf, &mut log);
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

    fn gate_steps(
        &self,
        s: &mut Session,
        installed: &Launch,
        prereqs: &[Ptf],
        p: &Ptf,
        log: &mut Log,
    ) -> Result<(), String> {
        let install_wait = self.boot_timeout * 10;
        // The response file's install, then the IPL from the disk.
        s.expect(
            "slic: install: answers from the response file",
            install_wait,
        )?;
        s.expect("SRC C6004100", install_wait)?;
        s.expect("slic: install: response file applied", install_wait)?;
        s.expect(SRC_IPL_COMPLETE, install_wait)?;
        log.ok("release installed from its media");
        // The media out, as after any install: down, then up from the disk.
        let port = self.telnet_port().ok_or("the launch has no telnet port")?;
        self.ftp_up(port)?;
        let mut f = self.ftp_signed_on()?;
        let down = "PWRDWNSYS OPTION(*IMMED) RESTART(*NO) CONFIRM(*NO)";
        let _ = f.cmd(&format!("RCMD {down}"), 250);
        drop(f);
        if s.expect("SRC D1001000", self.boot_timeout).is_err() {
            // The session ends as the system goes down.
            let _ = Tn::open(port)?.command(down);
        }
        s.expect("SRC D1001090", self.boot_timeout * 4)?;
        s.wait_exit(Duration::from_secs(60));
        s.kill();
        *s = Session::spawn(installed).map_err(|e| e.to_string())?;
        s.expect(SRC_IPL_COMPLETE, install_wait)?;
        log.ok("media removed, IPL from the disk");

        for q in prereqs {
            self.send_and_load(port, q)?;
            self.make_permanent(s, port, q)?;
            log.ok(format!(
                "prerequisite {} loaded and permanently applied",
                q.id
            ));
        }

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
        log.ok(format!(
            "IPL{}, DSPPTF: temporarily applied",
            if p.lic { " on the other slot" } else { "" }
        ));

        self.ftp_up(port)?;
        let mut f = self.ftp_signed_on()?;
        f.cmd(&format!("RCMD RMVPTF {lp}"), 250)?;
        drop(f);
        self.gate_ipl(s, port, false)?;
        let shown = Tn::open(port)?.command(&format!("DSPPTF {lp}"))?;
        // Off the system: kept as removed, or (a LIC PTF whose image the
        // IPL left behind) no longer listed; then it is loaded again.
        if shown.contains("Temporarily removed") {
            log.ok("RMVPTF, IPL, DSPPTF: temporarily removed");
        } else if !shown.contains(&p.id) {
            log.ok("RMVPTF, IPL, DSPPTF: no longer listed");
            self.ftp_up(port)?;
            let mut f = self.ftp_signed_on()?;
            f.cmd(
                &format!(
                    "RCMD LODPTF LICPGM({}) DEV(*SAVF) SELECT({})",
                    p.product, p.id
                ),
                250,
            )?;
            log.ok("LODPTF again");
        } else {
            return Err(format!("DSPPTF after RMVPTF: still applied:\n{shown}"));
        }

        self.make_permanent(s, port, p)?;
        log.ok("APYPTF APY(*PERM), IPL, DSPPTF: permanently applied");
        Ok(())
    }

    /// `q` by FTP into its save file, then `LODPTF`.
    fn send_and_load(&self, port: u16, q: &Ptf) -> Result<(), String> {
        self.ftp_up(port)?;
        let mut f = self.ftp_signed_on()?;
        f.cmd(&format!("RCMD CRTSAVF FILE(QGPL/{})", q.id), 250)?;
        f.cmd("TYPE I", 200)?;
        f.stor(&format!("STOR QGPL/{}", q.id), &q.bytes)?;
        f.cmd(
            &format!(
                "RCMD LODPTF LICPGM({}) DEV(*SAVF) SELECT({})",
                q.product, q.id
            ),
            250,
        )?;
        Ok(())
    }

    /// `APYPTF APY(*PERM)` until `DSPPTF` says permanently applied, an IPL
    /// after each that leaves it for the next one.
    fn make_permanent(&self, s: &mut Session, port: u16, q: &Ptf) -> Result<(), String> {
        let lp = format!("LICPGM({}) SELECT({})", q.product, q.id);
        let mut shown = String::new();
        for _ in 0..3 {
            self.ftp_up(port)?;
            let mut f = self.ftp_signed_on()?;
            f.cmd(&format!("RCMD APYPTF {lp} APY(*PERM)"), 250)?;
            drop(f);
            shown = Tn::open(port)?.command(&format!("DSPPTF {lp}"))?;
            if shown.contains("Permanently applied") {
                return Ok(());
            }
            self.gate_ipl(s, port, false)?;
            shown = Tn::open(port)?.command(&format!("DSPPTF {lp}"))?;
            if shown.contains("Permanently applied") {
                return Ok(());
            }
        }
        Err(format!("{} not permanently applied:\n{shown}", q.id))
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
        const CMD: &str = "PWRDWNSYS OPTION(*IMMED) RESTART(*YES) CONFIRM(*NO)";
        let mut f = self.ftp_signed_on()?;
        // The connection may end before the reply: the system is going down.
        let rcmd = f.cmd(&format!("RCMD {CMD}"), 250).unwrap_or_else(|e| e);
        drop(f);
        if s.expect("SRC D1001000", self.boot_timeout).is_err() {
            // Not over FTP: at a display, as an operator would.
            let shown = Tn::open(port)?.command(CMD).unwrap_or_else(|e| e);
            s.expect("SRC D1001000", self.boot_timeout * 4)
                .map_err(|e| {
                    format!("{e}; RCMD answered:\n{rcmd}\nthe display showed:\n{shown}")
                })?;
        }
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

/// The gate's response file: one blank disk, the network the harness
/// forwards to, the harness's security officer password.
fn rsp() -> String {
    format!(
        "QS400-INSTALL-RSP 1
# the PTF gate's install
lic-option = 2
load-source = vdb
language = 2924
system-name = QSGATE
ip = 10.0.2.15/24
gateway = 10.0.2.2
date = *CURRENT
time = *CURRENT
qsecofr-password = {SECOFR_PASSWORD}
licpgm = *ALL
start-print-writers = N
auto-configure = Y
device-naming = *NORMAL
special-environment = *NONE
qstrup = QSYS/QSTRUP
"
    )
}
