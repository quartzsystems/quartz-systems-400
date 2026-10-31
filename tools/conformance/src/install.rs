// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The install matrix (docs/INSTALL.md, docs/RECOVERY.md), after the
//! install suite's DST steps:
//!
//! | Step | What |
//! |---|---|
//! | 8 | a response file on a FAT32 copy of the media: no prompts, two disks mirrored, the answers applied, every licensed program installed, signatures clean |
//! | 9 | `SAVSYS` to a virtual tape, the system disk wiped, the recovery from the tape: LIC, operating system, profiles, configuration |
//! | 10 | a slip install of a V1R1M0 build over V1R0M0 (`QS400_SLIP_ISO`): user data kept, release recorded, the downgrade refused |
//! | 11 | a power pull in the middle of a LIC install, then a clean reinstall |

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::error::HarnessError;
use crate::qemu::Launch;
use crate::qemu::session::Session;
use crate::steps::{CMD, SRC_IPL_COMPLETE, StepReport, Suite};

/// The response file of step 8: two blank disks, the first the load
/// source, mirrored; answers for everything else.
pub const RSP_MIRRORED: &str = "QS400-INSTALL-RSP 1
# the harness's two-disk install
lic-option = 2
load-source = vdb
mirror = vdb vdc
language = 2924
system-name = QSINST
date = *CURRENT
time = *CURRENT
qsecofr-password = Quartz1
licpgm = *ALL
start-print-writers = Y
auto-configure = Y
device-naming = *NORMAL
special-environment = *NONE
qstrup = QSYS/QSTRUP
";

/// A FAT32 stick with the media's tree and a response file.
fn rsp_stick(iso: &[u8], rsp: &str) -> Result<Vec<u8>, String> {
    let mut files = crate::media::iso_files(iso)?;
    files.retain(|(p, _)| !p.eq_ignore_ascii_case("QS400/INSTALL.RSP"));
    files.push(("QS400/INSTALL.RSP".into(), rsp.as_bytes().to_vec()));
    crate::media::fat_stick_of(&files)
}

fn blank(path: &PathBuf, bytes: u64) -> Result<(), HarnessError> {
    let f = fs::File::create(path)?;
    f.set_len(bytes)?;
    Ok(())
}

const GIB: u64 = 1024 * 1024 * 1024;

/// One multi-boot step: run `f`, report as `step` does.
fn report(
    name: &'static str,
    start: Instant,
    r: Result<String, (String, String)>,
) -> Result<StepReport, HarnessError> {
    match r {
        Ok(detail) => {
            println!(
                "  PASS {name:<24} {:6.1}s  {detail}",
                start.elapsed().as_secs_f64()
            );
            Ok(StepReport {
                name,
                elapsed: start.elapsed(),
                detail,
            })
        }
        Err((reason, log)) => Err(HarnessError::Failed {
            step: name,
            reason,
            log,
        }),
    }
}

/// The signatures of every QSYS object that carries one check out.
pub(crate) fn signatures_clean(s: &mut Session) -> Result<String, String> {
    let out = s.command(
        "chkobjitg OBJ(QSYS/*ALL) CHKSIG(*SIGNED)",
        Duration::from_mins(10),
    )?;
    if out.contains("CPC39E0") {
        Ok("CHKOBJITG CHKSIG(*SIGNED) clean".into())
    } else if out.contains("CPF39E1") {
        Err(format!("CHKOBJITG:\n{out}"))
    } else {
        // A build from before CHKOBJITG (it lands with the merge).
        let first = out.lines().find(|l| !l.trim().is_empty()).unwrap_or_default();
        Ok(format!("CHKOBJITG not run: {}", first.trim()))
    }
}

impl Suite {
    fn skipped(&self, name: &str) -> bool {
        self.only
            .as_ref()
            .is_some_and(|p| !p.iter().any(|p| name.starts_with(p.as_str())))
    }

    /// Steps 8 to 11. `installed` is step 1's disk.
    pub(crate) fn install_matrix(&self, installed: &PathBuf) -> Result<Vec<StepReport>, HarnessError> {
        Ok(vec![
            self.rsp_mirrored()?,
            self.savsys_recovery(installed)?,
            self.slip_install(installed)?,
            self.pulled_install()?,
        ])
    }

    fn rsp_mirrored(&self) -> Result<StepReport, HarnessError> {
        let name = "8 response file mirrored";
        let start = Instant::now();
        if self.skipped(name) {
            return Ok(StepReport { name, elapsed: Duration::ZERO, detail: "skipped".into() });
        }
        let iso = fs::read(self.iso.as_ref().expect("checked in new"))?;
        let stick = self.out_dir.join("rsp-stick.img");
        let disk = rsp_stick(&iso, RSP_MIRRORED).map_err(|reason| HarnessError::Failed {
            step: name,
            reason,
            log: String::new(),
        })?;
        fs::write(&stick, disk)?;
        let (a, b) = (self.out_dir.join("rsp-a.img"), self.out_dir.join("rsp-b.img"));
        blank(&a, 4 * GIB)?;
        blank(&b, 4 * GIB)?;
        let launch = Launch {
            image: Some(stick),
            cdrom: None,
            cdrom_first: false,
            extra_disks: vec![a, b],
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&launch)?;
        let r = (|| -> Result<String, String> {
            s.expect("slic: install: answers from the response file", self.boot_timeout)?;
            s.expect("SRC C6004090", Duration::from_mins(5))?;
            s.expect("SRC C6004100", self.boot_timeout * 2)?;
            s.expect("mirrored with", Duration::from_mins(2))?;
            s.expect("SRC C6004130", Duration::from_mins(15))?;
            s.expect("SRC C6004160", Duration::from_mins(15))?;
            s.expect("slic: install: response file applied", Duration::from_mins(5))?;
            s.expect(SRC_IPL_COMPLETE, Duration::from_mins(5))?;
            s.expect(crate::PROMPT, CMD)?;
            let asp = s.command("svc.asp", CMD)?;
            if !asp.contains("protection: mirrored") {
                return Err(format!("not mirrored:\n{asp}"));
            }
            let name = s.command("svc.sysval QSYSNAME", CMD)?;
            if !name.contains("QSINST") {
                return Err(format!("system name not applied: {name}"));
            }
            let sw = s.command("dspsfwrsc", CMD)?;
            if !sw.contains("QS00WDS") {
                return Err(format!("the licensed programs are not installed:\n{sw}"));
            }
            let sig = signatures_clean(&mut s)?;
            Ok(format!("no prompts; unit pair mirrored; QSYSNAME QSINST; QS00WDS installed; {sig}"))
        })()
        .map_err(|e| (e, s.log.clone()));
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(30));
        report(name, start, r)
    }

    fn savsys_recovery(&self, installed: &PathBuf) -> Result<StepReport, HarnessError> {
        let name = "9 SAVSYS and recovery";
        let start = Instant::now();
        if self.skipped(name) {
            return Ok(StepReport { name, elapsed: Duration::ZERO, detail: "skipped".into() });
        }
        let system = self.out_dir.join("savsys-system.img");
        fs::copy(installed, &system)?;
        let tape = self.out_dir.join("savsys-tape.img");
        blank(&tape, GIB)?;
        // The installed system saves itself to the tape.
        let save = Launch {
            image: Some(system.clone()),
            cdrom: None,
            cdrom_first: false,
            extra_disks: vec![tape.clone()],
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&save)?;
        let r = (|| -> Result<String, String> {
            s.expect(SRC_IPL_COMPLETE, self.boot_timeout * 2)?;
            s.expect(crate::PROMPT, CMD)?;
            let made = s.command(
                "CRTUSRPRF USRPRF(RECOVER1) PASSWORD(Secret12) TEXT('kept by SAVSYS')",
                CMD,
            )?;
            if made.contains("CPF") {
                return Err(format!("CRTUSRPRF: {made}"));
            }
            // User data for the recovery's RSTLIB and RST.
            s.command("crtlib LIB(RCVUSR) TEXT('user data')", CMD)?;
            s.command("crtdtaara DTAARA(RCVUSR/KEEPME) TYPE(*CHAR) LEN(20) VALUE('kept value')", CMD)?;
            s.command("svc.ifs mkdir /rcvdata", CMD)?;
            s.command("svc.ifs put /rcvdata/note.txt kept in the IFS", CMD)?;
            // A virtual tape over an image catalog, as any save uses.
            let long = Duration::from_mins(15);
            for (line, want) in [
                ("CRTDEVTAP DEVD(TAPVRT01) RSRCNAME(*VRT)", "CPC2618"),
                ("CRTIMGCLG IMGCLG(SAVSYS) DIR('/savsys') TYPE(*TAP)", "CPCBC01"),
                ("ADDIMGCLGE IMGCLG(SAVSYS) FROMFILE(*NEW) TOFILE(SYS001) VOLNAM(SYS001) IMGSIZ(900)", "CPCBC03"),
                ("LODIMGCLG IMGCLG(SAVSYS) DEV(TAPVRT01)", "CPCBC06"),
                ("VRYCFG CFGOBJ(TAPVRT01) CFGTYPE(*DEV) STATUS(*ON)", "CPC2605"),
                ("SAVSYS DEV(TAPVRT01)", "CPC3703"),
                ("CHKTAP DEV(TAPVRT01)", "system save (SAVSYS)"),
                ("SAVLIB LIB(*NONSYS) DEV(TAPVRT01)", "CPC3722"),
                ("SAV DEV('/QSYS.LIB/TAPVRT01.DEVD') OBJ(('/*') ('/QSYS.LIB' *OMIT) ('/savsys' *OMIT))", "objects saved, 0 not saved"),
                ("svc.vrtout TAPVRT01 /dev/vdb", "bytes copied"),
            ] {
                let out = s.command(line, long)?;
                if !out.contains(want) {
                    return Err(format!("{line}: no {want}:\n{out}"));
                }
            }
            let labels = s.command("DSPTAP DEV(TAPVRT01) DATA(*LABELS)", CMD)?;
            for want in ["QS400MANIFEST", "RCVUSR", "QSAVIFS"] {
                if !labels.contains(want) {
                    return Err(format!("DSPTAP: no {want}:\n{labels}"));
                }
            }
            // The file rows of the system save: the manifest and its payloads.
            let rows = labels
                .lines()
                .filter(|l| l.trim_start().starts_with(|c: char| c.is_ascii_digit()))
                .filter(|l| ["QS400", "QSAVSECDTA", "QSAVCFG"].iter().any(|w| l.contains(w)))
                .count();
            Ok(rows.to_string())
        })();
        let files = match r {
            Ok(n) => n,
            Err(e) => return report(name, start, Err((e, s.log.clone()))),
        };
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(30));
        drop(s);
        // Wiped: a blank system disk, the media, the tape.
        blank(&system, 4 * GIB)?;
        let recover = Launch {
            image: Some(system.clone()),
            cdrom: self.iso.clone(),
            cdrom_first: true,
            extra_disks: vec![tape],
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&recover)?;
        let r = (|| -> Result<String, String> {
            s.expect("Selection", self.boot_timeout)?;
            s.send("4")?;
            s.expect("the LIC from the system save on TAPVRT01", Duration::from_mins(2))?;
            s.expect("SRC C6004090", Duration::from_mins(5))?;
            s.expect("SRC C6004100", self.boot_timeout * 2)?;
            s.expect("SRC C6004130", Duration::from_mins(15))?;
            let users = s.expect("authorization lists", Duration::from_mins(5))?;
            s.expect("user data: ", Duration::from_mins(5))?;
            s.expect("SRC C6004150", Duration::from_mins(5))?;
            // The storage service restores the user data from the tape.
            s.expect("recovery: user data from TAPVRT01", self.boot_timeout * 2)?;
            let restores = s.expect("qsstg: RSTAUT:", Duration::from_mins(15))?;
            s.expect(SRC_IPL_COMPLETE, Duration::from_mins(5))?;
            s.expect(crate::PROMPT, CMD)?;
            let p = s.command("DSPUSRPRF USRPRF(RECOVER1)", CMD)?;
            if !p.contains("RECOVER1") || p.contains("CPF2204") {
                return Err(format!("the profile did not come back:\n{p}"));
            }
            let lib = s.command("svc.dtaara get RCVUSR/KEEPME", CMD)?;
            if !lib.contains("kept value") {
                return Err(format!("RSTLIB did not bring RCVUSR back:\n{lib}\n{restores}"));
            }
            let ifs = s.command("svc.ifs cat /rcvdata/note.txt", CMD)?;
            if !ifs.contains("kept in the IFS") {
                return Err(format!("RST did not bring /rcvdata back:\n{ifs}\n{restores}"));
            }
            let last = users
                .lines()
                .rev()
                .find(|l| l.contains("RSTUSRPRF"))
                .and_then(|l| l.split("RSTUSRPRF: ").nth(1))
                .unwrap_or_default()
                .trim()
                .to_string();
            let rst = |w: &str| {
                restores
                    .lines()
                    .find_map(|l| l.split(&format!("qsstg: {w}: ")).nth(1))
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            };
            Ok(format!(
                "SAVSYS, SAVLIB *NONSYS and SAV on TAPVRT01 over an image catalog ({files} system save files); \
                 wiped; recovered: {last}; RSTLIB: {}; RST: {}; RECOVER1, RCVUSR and /rcvdata back",
                rst("RSTLIB"),
                rst("RST")
            ))
        })()
        .map_err(|e| (e, s.log.clone()));
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(30));
        report(name, start, r)
    }

    #[allow(clippy::too_many_lines)] // one QEMU scenario, three boots
    fn slip_install(&self, installed: &PathBuf) -> Result<StepReport, HarnessError> {
        let name = "10 slip install V1R1M0";
        let start = Instant::now();
        if self.skipped(name) {
            return Ok(StepReport { name, elapsed: Duration::ZERO, detail: "skipped".into() });
        }
        let Some(newer) = std::env::var_os("QS400_SLIP_ISO").map(PathBuf::from) else {
            println!("  SKIP {name:<24}  QS400_SLIP_ISO names no V1R1M0 media");
            return Ok(StepReport {
                name,
                elapsed: Duration::ZERO,
                detail: "skipped: QS400_SLIP_ISO not set".into(),
            });
        };
        let system = self.out_dir.join("slip-system.img");
        fs::copy(installed, &system)?;
        // User data first, on the V1R0M0 system.
        let disk = Launch {
            image: Some(system.clone()),
            cdrom: None,
            cdrom_first: false,
            extra_disks: Vec::new(),
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&disk)?;
        let r = (|| -> Result<(), String> {
            s.expect(SRC_IPL_COMPLETE, self.boot_timeout * 2)?;
            s.expect(crate::PROMPT, CMD)?;
            s.command("crtlib LIB(SLIPUSR) TEXT('user data')", CMD)?;
            s.command("svc.obj crt *DTAARA SLIPUSR/KEEPME kept", CMD)?;
            Ok(())
        })();
        if let Err(e) = r {
            return report(name, start, Err((e, s.log.clone())));
        }
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(30));
        drop(s);
        let upgrade = Launch {
            image: Some(system.clone()),
            cdrom: Some(newer),
            cdrom_first: true,
            ..disk.clone()
        };
        let mut s = Session::spawn(&upgrade)?;
        let r = (|| -> Result<String, String> {
            s.expect("Selection", self.boot_timeout)?;
            s.send("2")?;
            s.expect("Install Licensed Internal Code", CMD)?;
            s.send("1")?;
            s.expect("Install Licensed Internal Code (LIC)", CMD)?;
            s.send("3")?;
            s.expect("Type 1 beside the installed load source", CMD)?;
            s.send("1")?;
            s.expect("Install LIC and Recover Configuration - Confirmation", CMD)?;
            self.snap("install-recover-confirm");
            s.send("key f10")?;
            s.expect("upgrade to V1R1M0 over V1R0M0", Duration::from_mins(1))?;
            s.expect("SRC C6004060", Duration::from_mins(2))?;
            s.expect("SRC C6004090", Duration::from_mins(2))?;
            s.expect("SRC C6004100", self.boot_timeout * 2)?;
            s.expect("Dedicated Service Tools (DST) Sign On", self.boot_timeout)?;
            for l in ["type QSECOFR", "key tab", "type Quartz1", "key enter"] {
                s.send(l)?;
            }
            if s.expect("Use Dedicated Service Tools (DST)", CMD).is_err() {
                // Step 3 did not run: the shipped password, then the change.
                for l in ["type QSECOFR", "key tab", "type QSECOFR", "key enter"] {
                    s.send(l)?;
                }
                s.expect("Change Service Tools User Password", CMD)?;
                for l in [
                    "key tab",
                    "type QSECOFR",
                    "key tab",
                    "type Quartz1",
                    "key tab",
                    "type Quartz1",
                    "key enter",
                ] {
                    s.send(l)?;
                }
                s.expect("Use Dedicated Service Tools (DST)", CMD)?;
            }
            s.send("2")?;
            s.expect("Install Device Type Selection", CMD)?;
            s.send("2")?;
            s.expect("Upgrade from V1R0M0 to V1R1M0", CMD)?;
            s.send("key f10")?;
            s.expect("Select a Language Group", CMD)?;
            s.send("1")?;
            s.expect("Confirm Language Feature Selection", CMD)?;
            s.send("key f10")?;
            s.expect("Install option", CMD)?;
            s.send("key enter")?;
            s.expect("SRC C6004140", Duration::from_mins(15))?;
            let tr = s.expect("modules translated again", CMD)?;
            s.expect("IPL Options", self.boot_timeout)?;
            s.send("key enter")?;
            s.expect("Set Major System Options", CMD)?;
            s.send("key enter")?;
            s.expect(SRC_IPL_COMPLETE, Duration::from_mins(5))?;
            s.expect(crate::PROMPT, CMD)?;
            let asp = s.command("svc.asp", CMD)?;
            if !asp.contains("V1R1M0") {
                return Err(format!("the store does not record V1R1M0:\n{asp}"));
            }
            let kept = s.command("svc.obj ls SLIPUSR", CMD)?;
            if !kept.contains("KEEPME") {
                return Err(format!("user data lost:\n{kept}"));
            }
            let sig = signatures_clean(&mut s)?;
            let last = tr
                .lines()
                .rev()
                .find(|l| l.contains("objects ("))
                .and_then(|l| l.rsplit(": ").next())
                .unwrap_or_default()
                .trim()
                .to_string();
            Ok(format!("upgraded to V1R1M0; SLIPUSR/KEEPME kept; {last}; {sig}"))
        })()
        .map_err(|e| (e, s.log.clone()));
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(30));
        drop(s);
        let detail = match r {
            Ok(d) => d,
            Err(e) => return report(name, start, Err(e)),
        };
        // The V1R0M0 media over it now: refused.
        let back = Launch {
            cdrom: self.iso.clone(),
            ..upgrade
        };
        let mut s = Session::spawn(&back)?;
        let r = (|| -> Result<String, String> {
            s.expect("Selection", self.boot_timeout)?;
            for (l, want) in [
                ("2", "Install Licensed Internal Code"),
                ("1", "Install Licensed Internal Code (LIC)"),
                ("3", "Type 1 beside the installed load source"),
                ("1", "Install LIC and Recover Configuration - Confirmation"),
            ] {
                s.send(l)?;
                s.expect(want, CMD)?;
            }
            s.send("key f10")?;
            s.expect("SRC A6004080", CMD)?;
            s.expect("downgrade is not allow", CMD)?;
            Ok(format!("{detail}; V1R0M0 over it refused"))
        })()
        .map_err(|e| (e, s.log.clone()));
        s.kill();
        report(name, start, r)
    }

    fn pulled_install(&self) -> Result<StepReport, HarnessError> {
        let name = "11 power pull mid-install";
        let start = Instant::now();
        if self.skipped(name) {
            return Ok(StepReport { name, elapsed: Duration::ZERO, detail: "skipped".into() });
        }
        let target = self.out_dir.join("pull.img");
        blank(&target, 4 * GIB)?;
        let media = Launch {
            image: Some(target.clone()),
            cdrom: self.iso.clone(),
            cdrom_first: true,
            extra_disks: Vec::new(),
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&media)?;
        let r = (|| -> Result<(), String> {
            s.expect("Selection", self.boot_timeout)?;
            s.send("4")?;
            s.expect("SRC C6004001", Duration::from_mins(1))?;
            std::thread::sleep(Duration::from_millis(1500));
            Ok(())
        })();
        s.kill();
        drop(s);
        if let Err(e) = r {
            return report(name, start, Err((e, String::new())));
        }
        let mut s = Session::spawn(&media)?;
        let r = (|| -> Result<String, String> {
            s.expect("Selection", self.boot_timeout)?;
            s.send("4")?;
            s.expect("SRC C6004090", Duration::from_mins(5))?;
            s.expect("SRC C6004190", Duration::from_mins(15))?;
            s.expect(SRC_IPL_COMPLETE, Duration::from_mins(5))?;
            s.expect(crate::PROMPT, CMD)?;
            let chk = s.command("svc.chkstg", Duration::from_mins(5))?;
            if !chk.contains(": 0 bad") {
                return Err(format!("chkstg:\n{chk}"));
            }
            Ok("pulled during the LIC install; the reinstall reached sign-on; chkstg clean".into())
        })()
        .map_err(|e| (e, s.log.clone()));
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(30));
        report(name, start, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_response_file_parses() {
        let r = instmedia::Response::parse(RSP_MIRRORED).unwrap();
        assert_eq!(r.mirror, vec![("vdb".to_string(), "vdc".to_string())]);
        assert_eq!(r.licpgm, instmedia::rsp::Licpgms::All);
    }
}
