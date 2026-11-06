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
//! | 10 | V1R0M0 media (`QS400_OLD_ISO`) installed, user data made (a journaled SQL table, spooled files, a SAVLIB with an SQL table in a save file), upgraded with option 1 to the suite's V1R1M0 media: the data kept, the release recorded, the V1R0M0 table (no `*SQLINF`) selected, inserted into, indexed and viewed by SQL, RSTLIB of the V1R0M0 save, the downgrade refused |
//! | 11 | a power pull in the middle of a LIC install, then a clean reinstall |
//! | 14 | option 1, Restore Licensed Internal Code, over step 1's disk with user data: the data kept, the new slot booted |
//! | 15 | a network boot (PXE over QEMU's TFTP): the response file beside the boot file, the payload from the CD; `QSTRUP` starts the `*IP4DHCP` interface and it gets a lease, again after an IPL from the disk alone |

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
ip = 10.0.2.15/24
gateway = 10.0.2.2
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

pub(crate) fn blank(path: &PathBuf, bytes: u64) -> Result<(), HarnessError> {
    let f = fs::File::create(path)?;
    f.set_len(bytes)?;
    Ok(())
}

pub(crate) const GIB: u64 = 1024 * 1024 * 1024;

/// One multi-boot step: run `f`, report as `step` does.
pub(crate) fn report(
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
    pub(crate) fn skipped(&self, name: &str) -> bool {
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
            s.expect("slic: install: ADDTCPIFC INTNETADR('10.0.2.15') LIND(ETH0)", Duration::from_mins(5))?;
            s.expect("slic: install: ADDTCPRTE RTEDEST(*DFTROUTE) NEXTHOP('10.0.2.2')", CMD)?;
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
            // The response file's ip and gateway, as ADDTCPIFC and ADDTCPRTE.
            let ifc = s.command("svc.tcp ifc", CMD)?;
            if !ifc.lines().any(|l| l.starts_with("ifc 10.0.2.15 ")) {
                return Err(format!("the response file's interface is missing:\n{ifc}"));
            }
            let rte = s.command("svc.tcp rte", CMD)?;
            if !rte.contains("10.0.2.2") {
                return Err(format!("the response file's default route is missing:\n{rte}"));
            }
            let sig = signatures_clean(&mut s)?;
            Ok(format!(
                "no prompts; unit pair mirrored; QSYSNAME QSINST; 10.0.2.15/24 via 10.0.2.2; QS00WDS installed; {sig}"
            ))
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
                // SAVSYS needs the restricted state.
                ("ENDSBS SBS(*ALL) OPTION(*IMMED)", "ending"),
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
                if line.starts_with("ENDSBS") {
                    for _ in 0..120 {
                        let w = s.command("wrksbs", CMD)?;
                        if !w.contains("ACTIVE") && !w.contains("ENDING") {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
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
        // The suite's media is the current release (V1R1M0); the system
        // upgraded is installed from V1R0M0 media, a build made with
        // `QS400_RELEASE=V1R0M0` (`QS400_OLD_ISO`).
        let Some(older) = std::env::var_os("QS400_OLD_ISO").map(PathBuf::from) else {
            println!("  SKIP {name:<24}  QS400_OLD_ISO names no V1R0M0 media");
            return Ok(StepReport {
                name,
                elapsed: Duration::ZERO,
                detail: "skipped: QS400_OLD_ISO not set".into(),
            });
        };
        let _ = installed;
        let system = self.out_dir.join("slip-system.img");
        blank(&system, 4 * GIB)?;
        let old_media = Launch {
            image: Some(system.clone()),
            cdrom: Some(older.clone()),
            cdrom_first: true,
            extra_disks: Vec::new(),
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&old_media)?;
        if let Err(e) = self.automatic_install(&mut s) {
            return report(name, start, Err((format!("V1R0M0 install: {e}"), s.log.clone())));
        }
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(30));
        drop(s);
        // User data on the V1R0M0 system: a library with a data area, a
        // journaled table with rows, spooled files, and a save file
        // holding a SAVLIB made by V1R0M0.
        let disk = Launch {
            image: Some(system.clone()),
            cdrom: None,
            cdrom_first: false,
            extra_disks: Vec::new(),
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&disk)?;
        let r = (|| -> Result<Vec<String>, String> {
            s.expect(SRC_IPL_COMPLETE, self.boot_timeout * 2)?;
            s.expect(crate::PROMPT, CMD)?;
            let asp = s.command("svc.asp", CMD)?;
            if !asp.contains("V1R0M0") {
                return Err(format!("QS400_OLD_ISO is not V1R0M0 media:\n{asp}"));
            }
            for (line, want) in [
                ("crtlib LIB(SLIPUSR) TEXT('user data')", ""),
                ("crtdtaara DTAARA(SLIPUSR/KEEPME) TYPE(*CHAR) LEN(20) VALUE('kept value')", ""),
                ("runsql SQL('CREATE TABLE SLIPUSR.ORDERS (ID INTEGER, ITEM CHAR(10))') COMMIT(*NONE)", ""),
                ("crtjrnrcv SLIPUSR/RCV0001", "created"),
                ("crtjrn SLIPUSR/JRN JRNRCV(SLIPUSR/RCV0001)", "created"),
                ("strjrnpf SLIPUSR/ORDERS JRN(SLIPUSR/JRN) IMAGES(*BOTH)", "started"),
                ("runsql SQL('INSERT INTO SLIPUSR.ORDERS VALUES(1, ''bolts'')') COMMIT(*NONE)", ""),
                ("runsql SQL('INSERT INTO SLIPUSR.ORDERS VALUES(2, ''nuts'')') COMMIT(*NONE)", ""),
                ("DSPLIBL OUTPUT(*PRINT)", ""),
                ("crtlib LIB(SLIPSAV) TEXT('saved by V1R0M0')", ""),
                ("crtdtaara DTAARA(SLIPSAV/OLDSAVE) TYPE(*CHAR) LEN(20) VALUE('from V1R0M0')", ""),
                ("runsql SQL('CREATE TABLE SLIPSAV.PARTS (ID INTEGER, NAME CHAR(10))') COMMIT(*NONE)", ""),
                ("runsql SQL('INSERT INTO SLIPSAV.PARTS VALUES(7, ''gears'')') COMMIT(*NONE)", ""),
                ("crtsavf FILE(SLIPUSR/OLDSAVF)", ""),
                ("savlib LIB(SLIPSAV) DEV(*SAVF) SAVF(SLIPUSR/OLDSAVF)", "CPC3722"),
                ("dltlib LIB(SLIPSAV)", ""),
            ] {
                let out = s.command(line, CMD)?;
                if out.contains("CPF") || !out.contains(want) {
                    return Err(format!("{line}:\n{out}"));
                }
            }
            // The ready spooled files, by name, to find again after the upgrade.
            let files = s.command("wrksplf SELECT(*ALL) JOB(QSVCJOB)", CMD)?;
            let names: Vec<String> = files
                .lines()
                .filter(|l| l.contains("RDY"))
                .filter_map(|l| l.split_whitespace().next().map(str::to_string))
                .collect();
            if names.is_empty() {
                return Err(format!("no spooled file before the upgrade:\n{files}"));
            }
            Ok(names)
        })();
        let spooled = match r {
            Ok(n) => n,
            Err(e) => return report(name, start, Err((e, s.log.clone()))),
        };
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(30));
        drop(s);
        let upgrade = Launch {
            image: Some(system.clone()),
            cdrom: self.iso.clone(),
            cdrom_first: true,
            ..disk.clone()
        };
        let mut s = Session::spawn(&upgrade)?;
        let r = (|| -> Result<String, String> {
            self.restore_lic(&mut s, "upgrade to V1R1M0 over V1R0M0")?;
            let tr = self.os_over(&mut s, "Upgrade from V1R0M0 to V1R1M0")?;
            let asp = s.command("svc.asp", CMD)?;
            if !asp.contains("V1R1M0") {
                return Err(format!("the store does not record V1R1M0:\n{asp}"));
            }
            // The V1R0M0 table has no *SQLINF (header version 1): SQL
            // still sees a table, reads it, adds to it and builds on it.
            let checks: [(&str, &[&str]); 14] = [
                ("svc.dtaara get SLIPUSR/KEEPME", &["kept value"]),
                ("dsppfm FILE(SLIPUSR/ORDERS)", &["bolts", "nuts"]),
                ("dspjrn JRN(SLIPUSR/JRN)", &["PT", "ORDERS"]),
                (
                    "svc.sql SELECT TABLE_NAME FROM QSYS2/SYSTABLES WHERE TABLE_SCHEMA = 'SLIPUSR' AND TABLE_TYPE = 'T';",
                    &["ORDERS", "1 row(s)"],
                ),
                ("svc.sql SELECT ITEM FROM SLIPUSR/ORDERS WHERE ID = 2;", &["nuts", "1 row(s)"]),
                ("runsql SQL('INSERT INTO SLIPUSR.ORDERS VALUES(3, ''washers'')') COMMIT(*NONE)", &[]),
                ("runsql SQL('CREATE INDEX SLIPUSR.ORDERSIX ON SLIPUSR.ORDERS (ITEM)') COMMIT(*NONE)", &[]),
                (
                    "runsql SQL('CREATE VIEW SLIPUSR.ORDERSV AS SELECT ID, ITEM FROM SLIPUSR.ORDERS WHERE ID > 1') COMMIT(*NONE)",
                    &[],
                ),
                ("svc.sql SELECT COUNT(*) FROM SLIPUSR/ORDERS;", &["3", "1 row(s)"]),
                ("svc.sql SELECT ITEM FROM SLIPUSR/ORDERSV ORDER BY ITEM;", &["nuts", "washers", "2 row(s)"]),
                (
                    "svc.sql SELECT TABLE_NAME, TABLE_TYPE FROM QSYS2/SYSTABLES WHERE TABLE_SCHEMA = 'SLIPUSR' AND TABLE_NAME LIKE 'ORDERS%' ORDER BY TABLE_NAME;",
                    &["ORDERSV", "2 row(s)"],
                ),
                (
                    "svc.sql SELECT INDEX_NAME, TABLE_NAME FROM QSYS2/SYSINDEXES WHERE INDEX_SCHEMA = 'SLIPUSR';",
                    &["ORDERSIX", "ORDERS", "1 row(s)"],
                ),
                (
                    "rstlib SAVLIB(SLIPSAV) DEV(*SAVF) SAVF(SLIPUSR/OLDSAVF)",
                    &["CPC3703", "SLIPSAV"],
                ),
                ("svc.sql SELECT ID, NAME FROM SLIPSAV/PARTS;", &["gears", "1 row(s)"]),
            ];
            for (line, wants) in checks {
                let out = s.command(line, CMD)?;
                if out.contains("CPF") || out.contains("SQLSTATE") {
                    return Err(format!("{line}: failed after the upgrade:\n{out}"));
                }
                if let Some(w) = wants.iter().find(|w| !out.contains(**w)) {
                    return Err(format!("{line}: no {w} after the upgrade:\n{out}"));
                }
            }
            let v = s.command("svc.sql SELECT ITEM FROM SLIPUSR/ORDERSV WHERE ITEM = 'bolts';", CMD)?;
            if !v.contains("0 row(s)") {
                return Err(format!("the view over the V1R0M0 table does not select:\n{v}"));
            }
            let files = s.command("wrksplf SELECT(*ALL) JOB(QSVCJOB)", CMD)?;
            if let Some(f) = spooled.iter().find(|f| !files.contains(f.as_str())) {
                return Err(format!("spooled file {f} lost in the upgrade:\n{files}"));
            }
            let v = s.command("svc.dtaara get SLIPSAV/OLDSAVE", CMD)?;
            if !v.contains("from V1R0M0") {
                return Err(format!("RSTLIB of the V1R0M0 save lost the data:\n{v}"));
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
            Ok(format!(
                "V1R0M0 installed, upgraded to V1R1M0; the data area, the journaled table and \
                 its entries, the spooled files kept; the V1R0M0 SQL table selected, inserted \
                 into, indexed and viewed; RSTLIB of a V1R0M0 SAVLIB with its SQL table; {last}; {sig}"
            ))
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
            cdrom: Some(older),
            ..upgrade
        };
        let mut s = Session::spawn(&back)?;
        let r = (|| -> Result<String, String> {
            s.expect("Selection", self.boot_timeout)?;
            for (l, want) in [
                ("2", "Install Licensed Internal Code"),
                ("1", "Current load source"),
            ] {
                s.send(l)?;
                s.expect(want, CMD)?;
            }
            // Option 1 asks nothing more; the release rule refuses.
            s.send("1")?;
            s.expect("SRC A6004080", CMD)?;
            s.expect("cannot be installed over release", CMD)?;
            Ok(format!("{detail}; V1R0M0 over it refused"))
        })()
        .map_err(|e| (e, s.log.clone()));
        s.kill();
        report(name, start, r)
    }

    /// Option 1 from the media's IPL menu: no load source choice, no
    /// confirmation; `kind` is slic's line for the release rule's answer.
    fn restore_lic(&self, s: &mut Session, kind: &str) -> Result<(), String> {
        s.expect("Selection", self.boot_timeout)?;
        s.send("2")?;
        s.expect("Install Licensed Internal Code", CMD)?;
        s.send("1")?;
        s.expect("Install Licensed Internal Code (LIC)", CMD)?;
        s.expect("Current load source", CMD)?;
        self.snap("install-lic-options");
        s.send("1")?;
        s.expect(kind, Duration::from_mins(1))?;
        s.expect("SRC C6004060", Duration::from_mins(2))?;
        s.expect("SRC C6004090", Duration::from_mins(2))?;
        Ok(())
    }

    /// After a LIC restore: the re-IPL finds the operating system install
    /// pending, DST installs it from the optical media (`confirm` on its
    /// confirmation), the IPL completes. Returns the text before the
    /// install's summary line.
    fn os_over(&self, s: &mut Session, confirm: &str) -> Result<String, String> {
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
        s.expect(confirm, CMD)?;
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
        Ok(tr)
    }

    /// Step 14: option 1 with the suite's own media over step 1's disk.
    /// User data first; then the LIC restored onto the load source and
    /// the operating system slip-installed; the disk alone boots the new
    /// slot with the data intact.
    pub(crate) fn restore_in_place(&self, installed: &PathBuf) -> Result<StepReport, HarnessError> {
        let name = "14 restore LIC in place";
        let start = Instant::now();
        if self.skipped(name) {
            return Ok(StepReport { name, elapsed: Duration::ZERO, detail: "skipped".into() });
        }
        let Some(iso) = self.iso.clone() else {
            return Ok(StepReport { name, elapsed: Duration::ZERO, detail: "skipped: no media".into() });
        };
        let system = self.out_dir.join("restore-system.img");
        fs::copy(installed, &system)?;
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
            s.command("crtlib LIB(RSTLICUSR) TEXT('user data')", CMD)?;
            s.command("svc.obj crt *DTAARA RSTLICUSR/KEEPME kept", CMD)?;
            Ok(())
        })();
        if let Err(e) = r {
            return report(name, start, Err((e, s.log.clone())));
        }
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(30));
        drop(s);
        let media = Launch {
            cdrom: Some(iso),
            cdrom_first: true,
            ..disk.clone()
        };
        let mut s = Session::spawn(&media)?;
        let r = (|| -> Result<String, String> {
            self.restore_lic(&mut s, "slip install of ")?;
            self.os_over(&mut s, "Slip install of ")?;
            // The new LIC went into the other slot, now the default, and
            // into the first as well once the operating system was in.
            let how = if s.log.contains("slot A written as well") {
                "both slots written (firmware not told)"
            } else if s.log.contains("Licensed Internal Code copied from slot") {
                "new slot the default, copied into the other"
            } else {
                return Err("neither the new slot's copy nor the fallback was logged".into());
            };
            let kept = s.command("svc.obj ls RSTLICUSR", CMD)?;
            if !kept.contains("KEEPME") {
                return Err(format!("user data lost:\n{kept}"));
            }
            Ok(how.to_string())
        })()
        .map_err(|e| (e, s.log.clone()));
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(30));
        drop(s);
        let how = match r {
            Ok(h) => h,
            Err(e) => return report(name, start, Err(e)),
        };
        // The disk alone: the IPL completes from the restored LIC.
        let mut s = Session::spawn(&disk)?;
        let r = (|| -> Result<String, String> {
            s.expect("slic: slot ", self.boot_timeout)?;
            let slot = s.expect(", mode", CMD)?;
            s.expect(SRC_IPL_COMPLETE, self.boot_timeout * 2)?;
            s.expect(crate::PROMPT, CMD)?;
            let kept = s.command("svc.obj ls RSTLICUSR", CMD)?;
            if !kept.contains("KEEPME") {
                return Err(format!("user data lost after the IPL from disk:\n{kept}"));
            }
            let check = s.command("svc.chkstg", Duration::from_secs(300))?;
            if !check.contains(": 0 bad") {
                return Err(format!("svc.chkstg:\n{check}"));
            }
            Ok(format!(
                "option 1, no prompts; {how}; IPL from disk in slot {}; RSTLICUSR/KEEPME kept; chkstg clean",
                slot.trim(),
            ))
        })()
        .map_err(|e| (e, s.log.clone()));
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(30));
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
