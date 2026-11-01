// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Four disks: the installed system with three blank disks added to
//! ASP 1 in DST and data put on every unit, then
//!
//! | Step | What |
//! |---|---|
//! | 12 | mirrored protection started over the four units (1 with 4, 2 with 3, data moved off 3 and 4 first), the copies at the re-IPL, then the IPL with disk 4 pulled |
//! | 13 | device parity (RAID 5) over the four units, synchronized with the system up (written and read meanwhile, a power down part way), an IPL with disk 4 pulled (exposed, A6001257), the disk back and rebuilt with the system up |
//!
//! Every disk action goes through the DST screens over serial.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::dst::{PASSWORD, sign_on};
use crate::error::HarnessError;
use crate::qemu::Launch;
use crate::qemu::session::Session;
use crate::steps::{CMD, SRC_IPL_COMPLETE, StepReport, Suite};

const GIB: u64 = 1024 * 1024 * 1024;
/// User spaces `obj fill` makes before protection starts, and their size.
const FILL: (&str, u32, u32) = ("QFILL", 3000, 32);

/// One `unit` line of `svc.asp`.
#[derive(Debug)]
struct UnitLine {
    unit: u16,
    state: String,
    used: u64,
    serial: String,
    partner: Option<u16>,
    attached: bool,
}

/// The units `svc.asp` lists.
fn units(asp: &str) -> Vec<UnitLine> {
    asp.lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix("unit ")?;
            let mut w = rest.split_whitespace();
            let unit = w.next()?.parse().ok()?;
            let state = w.next()?.to_string();
            let used = l
                .split(", ")
                .find_map(|p| p.strip_suffix(" used"))
                .and_then(|n| n.trim().parse().ok())
                .unwrap_or(0);
            let serial = l
                .split("serial ")
                .nth(1)
                .and_then(|t| t.split_whitespace().next())
                .unwrap_or_default()
                .to_string();
            let partner = l
                .split(" partner ")
                .nth(1)
                .and_then(|t| t.split_whitespace().next())
                .and_then(|n| n.parse().ok());
            Some(UnitLine {
                unit,
                state,
                used,
                serial,
                partner,
                attached: !l.contains("(not attached)"),
            })
        })
        .collect()
}

/// The disk a unit's serial names: `QS400HDn` is disk `n + 1`.
fn disk_of(serial: &str) -> Option<u8> {
    serial
        .rsplit("HD")
        .next()
        .and_then(|n| n.parse::<u8>().ok())
        .map(|n| n + 1)
}

/// Sign on to DST as QSECOFR: the shipped password and its change on
/// a system that never signed on, else the harness's password.
fn dst_sign_on(s: &mut Session) -> Result<(), String> {
    sign_on(s, "QSECOFR")?;
    if s.expect("Change Service Tools User Password", CMD).is_ok() {
        for l in [
            "key tab",
            "type QSECOFR",
            "key tab",
            &format!("type {PASSWORD}"),
            "key tab",
            &format!("type {PASSWORD}"),
            "key enter",
        ] {
            s.send(l)?;
        }
    } else {
        // Field Exit clears the user and lands in the password field.
        for l in [
            "key home",
            "key fieldexit",
            &format!("type {PASSWORD}"),
            "key tab",
            "type QSECOFR",
            "key enter",
        ] {
            s.send(l)?;
        }
    }
    s.expect("Use Dedicated Service Tools (DST)", CMD)?;
    Ok(())
}

/// From the running system into DST: a manual IPL, option 3.
fn enter_dst(s: &mut Session, boot: Duration) -> Result<(), String> {
    s.command("mode manual", CMD)?;
    s.send("pwrdwn restart")?;
    s.expect("SRC D1001090", CMD)?;
    s.expect("IPL or Install the System", boot)?;
    s.send("3")?;
    dst_sign_on(s)
}

/// From DST's main menu: Work with disk units > configuration.
fn disk_configuration(s: &mut Session) -> Result<(), String> {
    s.send("4")?;
    s.expect("Work with Disk Units", CMD)?;
    s.send("1")?;
    s.expect("Work with Disk Configuration", CMD)?;
    Ok(())
}

/// From DST's main menu: every non-configured disk (`n`) added to ASP 1.
fn add_units(s: &mut Session, n: usize) -> Result<(), String> {
    disk_configuration(s)?;
    s.send("3")?;
    s.expect("Work with ASP Configuration", CMD)?;
    s.send("2")?;
    s.expect("Add Units to ASP 1", CMD)?;
    // One-position option fields skip to the next as they fill.
    for _ in 0..n {
        s.send("type 1")?;
    }
    s.send("key enter")?;
    s.expect("Confirm Add Units", CMD)?;
    s.send("key f10")?;
    s.expect("added to ASP 1", Duration::from_mins(2))?;
    s.send("key f3")?;
    s.expect("Use Dedicated Service Tools (DST)", CMD)?;
    Ok(())
}

/// From DST's main menu: option 1, the IPL options as shown, the IPL to
/// its end.
fn ipl_from_dst(s: &mut Session, boot: Duration) -> Result<String, String> {
    s.send("1")?;
    let before = s.expect("IPL Options", boot)?;
    s.send("key enter")?;
    s.expect("Set Major System Options", CMD)?;
    s.send("key enter")?;
    let after = s.expect(SRC_IPL_COMPLETE, boot)?;
    s.expect(crate::PROMPT, CMD)?;
    Ok(format!("{before}{after}"))
}

/// The installed system, booted with three blank disks: they are added
/// to ASP 1 and `FILL` puts data on every unit. Ends at the prompt.
fn configure(s: &mut Session, boot: Duration) -> Result<String, String> {
    s.expect(SRC_IPL_COMPLETE, boot * 2)?;
    s.expect(crate::PROMPT, CMD)?;
    enter_dst(s, boot)?;
    add_units(s, 3)?;
    ipl_from_dst(s, boot)?;
    let (lib, count, kib) = FILL;
    let out = s.command(&format!("svc.obj fill {lib} {count} {kib}"), Duration::from_mins(20))?;
    if !out.contains("filled") {
        return Err(format!("obj fill:\n{out}"));
    }
    let asp = s.command("svc.asp", CMD)?;
    let u = units(&asp);
    if u.len() != 4 || u.iter().any(|u| u.used == 0) {
        return Err(format!("not four units, each with data:\n{asp}"));
    }
    let used: Vec<String> = u.iter().map(|u| u.used.to_string()).collect();
    Ok(format!("pages used per unit {}", used.join("/")))
}

/// `obj verify` of what `fill` made.
fn verify(s: &mut Session, lib: &str, n: u32, kib: u32) -> Result<(), String> {
    let out = s.command(&format!("svc.obj verify {lib} {n} {kib}"), Duration::from_mins(10))?;
    if out.contains("intact") {
        Ok(())
    } else {
        Err(format!("obj verify {lib}:\n{out}"))
    }
}

fn chkstg(s: &mut Session) -> Result<(), String> {
    let chk = s.command("svc.chkstg", Duration::from_mins(10))?;
    if chk.contains(": 0 bad") {
        Ok(())
    } else {
        Err(format!("chkstg:\n{chk}"))
    }
}

fn power_down(s: &mut Session) {
    s.command("mode normal", CMD).ok();
    s.command("pwrdwn", CMD).ok();
    s.wait_exit(Duration::from_secs(60));
}

fn blank(path: &Path) -> Result<(), HarnessError> {
    let f = fs::File::create(path)?;
    f.set_len(4 * GIB)?;
    Ok(())
}

/// What a step's boots return: the detail, or why and the log.
type Boot = Result<String, (String, String)>;

impl Suite {
    /// Steps 12 and 13, each over a copy of step 1's disk.
    pub(crate) fn four_disks(&self, installed: &Path) -> Result<Vec<StepReport>, HarnessError> {
        Ok(vec![self.four_mirrored(installed)?, self.four_parity(installed)?])
    }

    fn wanted(&self, name: &str) -> bool {
        self.only
            .as_ref()
            .is_none_or(|p| p.iter().any(|p| name.starts_with(p.as_str())))
    }

    /// The installed disk copied and three blank disks, named for `tag`.
    fn four(&self, installed: &Path, tag: &str) -> Result<[PathBuf; 4], HarnessError> {
        let d = |n: u8| self.out_dir.join(format!("{tag}-{n}.img"));
        let disks = [d(1), d(2), d(3), d(4)];
        fs::copy(installed, &disks[0])?;
        for p in &disks[1..] {
            blank(p)?;
        }
        Ok(disks)
    }

    fn launch_of(&self, disks: &[&PathBuf]) -> Launch {
        Launch {
            image: Some(disks[0].clone()),
            cdrom: None,
            cdrom_first: false,
            extra_disks: disks[1..].iter().map(|p| (*p).clone()).collect(),
            ..self.launch.clone()
        }
    }

    fn finish(name: &'static str, start: Instant, r: Boot) -> Result<StepReport, HarnessError> {
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

    /// Step 12.
    fn four_mirrored(&self, installed: &Path) -> Result<StepReport, HarnessError> {
        let name = "12 four-unit mirror";
        let start = Instant::now();
        if !self.wanted(name) {
            return Ok(StepReport { name, elapsed: Duration::ZERO, detail: "skipped".into() });
        }
        let [d1, d2, d3, d4] = self.four(installed, "mirror4")?;
        let boot = self.boot_timeout;
        let mut s = Session::spawn(&self.launch_of(&[&d1, &d2, &d3, &d4]))?;
        let r = (|| -> Result<String, String> {
            let filled = configure(&mut s, boot)?;
            enter_dst(&mut s, boot)?;
            disk_configuration(&mut s)?;
            s.send("4")?;
            s.expect("Work with Mirrored Protection", CMD)?;
            s.send("2")?;
            s.expect("Select ASP to Start Mirrored Protection", CMD)?;
            s.send("1")?;
            s.expect("Confirm Start Mirrored Protection", CMD)?;
            self.snap("mirror4-confirm");
            s.send("key f10")?;
            s.expect("pair(s) formed", Duration::from_mins(20))?;
            s.send("key f3")?;
            s.expect("Use Dedicated Service Tools (DST)", CMD)?;
            // The copies run at the next IPL, one half after another.
            let ipl = ipl_from_dst(&mut s, Duration::from_mins(30))?;
            if !ipl.contains("SRC C1001075") {
                return Err("no synchronization at the re-IPL".into());
            }
            let synced = ipl.matches(": 100%").count();
            let asp = s.command("svc.asp", CMD)?;
            let u = units(&asp);
            let pairs = |x: u8, y: u8| {
                u.iter().any(|p| {
                    disk_of(&p.serial) == Some(x)
                        && p.partner.and_then(|q| u.iter().find(|o| o.unit == q))
                            .is_some_and(|o| disk_of(&o.serial) == Some(y))
                })
            };
            if !asp.contains("protection: mirrored")
                || u.len() != 4
                || u.iter().any(|p| p.state != "Active")
                || !pairs(1, 4)
                || !pairs(2, 3)
            {
                return Err(format!("not paired 1-4 and 2-3, all active:\n{asp}"));
            }
            verify(&mut s, FILL.0, FILL.1, FILL.2)?;
            chkstg(&mut s)?;
            Ok(format!(
                "{filled}; pairs 1-4 and 2-3 formed in DST; {synced} half(s) copied at the re-IPL; data intact; chkstg clean"
            ))
        })();
        let first = match r {
            Ok(d) => d,
            Err(e) => {
                let log = s.log.clone();
                s.kill();
                return Self::finish(name, start, Err((e, log)));
            }
        };
        power_down(&mut s);
        drop(s);
        // Disk 4 pulled: unit 1's partner is gone.
        let mut s = Session::spawn(&self.launch_of(&[&d1, &d2, &d3]))?;
        let r = (|| -> Result<String, String> {
            s.expect("SRC A6001255", boot)?;
            s.expect(SRC_IPL_COMPLETE, boot)?;
            s.expect(crate::PROMPT, CMD)?;
            let asp = s.command("svc.asp", CMD)?;
            let listed = units(&asp);
            let gone: Vec<&UnitLine> = listed.iter().filter(|u| !u.attached).collect();
            if gone.len() != 1 || gone[0].state != "Missing" {
                return Err(format!("disk 4 not the one unit missing:\n{asp}"));
            }
            verify(&mut s, FILL.0, FILL.1, FILL.2)?;
            Ok(format!("{first}; with disk 4 pulled: A6001255, unit {} Missing, IPL complete, data intact", gone[0].unit + 1))
        })()
        .map_err(|e| (e, s.log.clone()));
        power_down(&mut s);
        Self::finish(name, start, r)
    }

    /// Step 13.
    #[allow(clippy::too_many_lines)] // one scenario, four boots
    fn four_parity(&self, installed: &Path) -> Result<StepReport, HarnessError> {
        let name = "13 device parity RAID 5";
        let start = Instant::now();
        if !self.wanted(name) {
            return Ok(StepReport { name, elapsed: Duration::ZERO, detail: "skipped".into() });
        }
        let [d1, d2, d3, d4] = self.four(installed, "parity4")?;
        let boot = self.boot_timeout;
        let all = self.launch_of(&[&d1, &d2, &d3, &d4]);
        let mut s = Session::spawn(&all)?;
        let r = (|| -> Result<String, String> {
            let filled = configure(&mut s, boot)?;
            enter_dst(&mut s, boot)?;
            disk_configuration(&mut s)?;
            s.send("5")?;
            s.expect("Work with Device Parity Protection", CMD)?;
            s.send("2")?;
            s.expect("Start Device Parity Protection - RAID 5", CMD)?;
            for _ in 0..4 {
                s.send("type 1")?;
            }
            s.send("key enter")?;
            s.expect("Confirm Start Device Parity Protection", CMD)?;
            self.snap("parity4-confirm");
            s.send("key f10")?;
            s.expect("parity is computed in the background", Duration::from_mins(2))?;
            s.send("1")?;
            s.expect("Display Device Parity Status", CMD)?;
            let shown = s.expect("F3=Exit", CMD)?;
            if shown.matches("% Resyncing").count() != 4 {
                return Err(format!("the status display:\n{shown}"));
            }
            s.send("key f3")?;
            s.expect("Use Dedicated Service Tools (DST)", CMD)?;
            // The IPL does not wait for the synchronization.
            let ipl = ipl_from_dst(&mut s, boot)?;
            if !ipl.contains("device parity work continues in the background") {
                return Err("the IPL did not leave the synchronization to the system".into());
            }
            let first = s.command("svc.asp", CMD)?;
            let Some(p0) = sync_percent(&first) else {
                return Err(format!("not synchronizing after the IPL:\n{first}"));
            };
            // Writes while it runs, on both sides of the mark.
            let out = s.command("svc.obj fill QPAR2 300 32", Duration::from_mins(10))?;
            if !out.contains("filled") {
                return Err(format!("obj fill QPAR2:\n{out}"));
            }
            verify(&mut s, FILL.0, FILL.1, FILL.2)?;
            let after = s.command("svc.asp", CMD)?;
            let p1 = sync_percent(&after);
            Ok(format!(
                "{filled}; RAID 5 started over units 1-4 in DST; the IPL completed at {p0}% synchronized; QPAR2 written and QFILL read back during the synchronization (then {})",
                p1.map_or_else(|| "done".to_string(), |p| format!("{p}%"))
            ))
        })();
        let first = match r {
            Ok(d) => d,
            Err(e) => {
                let log = s.log.clone();
                s.kill();
                return Self::finish(name, start, Err((e, log)));
            }
        };
        // Power down in the middle: the next IPL carries on from the mark.
        power_down(&mut s);
        drop(s);
        let mut s = Session::spawn(&all)?;
        let r = (|| -> Result<String, String> {
            s.expect(SRC_IPL_COMPLETE, boot)?;
            s.expect(crate::PROMPT, CMD)?;
            let asp = s.command("svc.asp", CMD)?;
            let resumed = sync_percent(&asp);
            let waited = wait_active(&mut s, 4)?;
            verify(&mut s, FILL.0, FILL.1, FILL.2)?;
            verify(&mut s, "QPAR2", 300, 32)?;
            chkstg(&mut s)?;
            Ok(format!(
                "{first}; after a power down the next IPL {}; DPY/Active {waited}s later; data intact; chkstg clean",
                resumed.map_or_else(
                    || "found it synchronized".to_string(),
                    |p| format!("went on from {p}%")
                )
            ))
        })();
        let first = match r {
            Ok(d) => d,
            Err(e) => {
                let log = s.log.clone();
                s.kill();
                return Self::finish(name, start, Err((e, log)));
            }
        };
        power_down(&mut s);
        drop(s);
        // Disk 4 pulled: the set runs exposed.
        let mut s = Session::spawn(&self.launch_of(&[&d1, &d2, &d3]))?;
        let r = (|| -> Result<String, String> {
            s.expect("SRC A6001257", boot)?;
            s.expect(SRC_IPL_COMPLETE, boot)?;
            s.expect(crate::PROMPT, CMD)?;
            let asp = s.command("svc.asp", CMD)?;
            if !asp.contains("DPY/Failed") {
                return Err(format!("no DPY/Failed unit:\n{asp}"));
            }
            verify(&mut s, FILL.0, FILL.1, FILL.2)?;
            verify(&mut s, "QPAR2", 300, 32)?;
            let out = s.command("svc.obj fill QPAR3 100 32", Duration::from_mins(10))?;
            if !out.contains("filled") {
                return Err(format!("obj fill QPAR3 exposed:\n{out}"));
            }
            verify(&mut s, "QPAR3", 100, 32)?;
            Ok(format!("{first}; disk 4 pulled: A6001257, DPY/Failed, every page read through parity, written exposed"))
        })();
        let second = match r {
            Ok(d) => d,
            Err(e) => {
                let log = s.log.clone();
                s.kill();
                return Self::finish(name, start, Err((e, log)));
            }
        };
        power_down(&mut s);
        drop(s);
        // Disk 4 back: stale, rebuilt with the system up.
        let mut s = Session::spawn(&all)?;
        let r = (|| -> Result<String, String> {
            s.expect(SRC_IPL_COMPLETE, boot)?;
            s.expect(crate::PROMPT, CMD)?;
            let asp = s.command("svc.asp", CMD)?;
            let Some(r0) = rebuild_percent(&asp) else {
                return Err(format!("no rebuild under way after the IPL:\n{asp}"));
            };
            // Reads and writes while it runs: unit 4's pages from the
            // unit below the mark, from parity above it.
            verify(&mut s, FILL.0, FILL.1, FILL.2)?;
            verify(&mut s, "QPAR3", 100, 32)?;
            let out = s.command("svc.obj fill QPAR4 300 32", Duration::from_mins(10))?;
            if !out.contains("filled") {
                return Err(format!("obj fill QPAR4 during the rebuild:\n{out}"));
            }
            let mid = s.command("svc.asp", CMD)?;
            let r1 = rebuild_percent(&mid);
            let waited = wait_active(&mut s, 4)?;
            verify(&mut s, FILL.0, FILL.1, FILL.2)?;
            verify(&mut s, "QPAR2", 300, 32)?;
            verify(&mut s, "QPAR3", 100, 32)?;
            verify(&mut s, "QPAR4", 300, 32)?;
            chkstg(&mut s)?;
            Ok(format!(
                "{second}; disk 4 back: the IPL completed at {r0}% rebuilt, data read and QPAR4 written during the rebuild (then {}), DPY/Active {waited}s later, data intact, chkstg clean",
                r1.map_or_else(|| "done".to_string(), |p| format!("{p}%"))
            ))
        })()
        .map_err(|e| (e, s.log.clone()));
        power_down(&mut s);
        Self::finish(name, start, r)
    }
}

/// The percent `svc.asp` shows for a set being synchronized.
fn sync_percent(asp: &str) -> Option<u8> {
    asp.split("synchronizing ")
        .nth(1)
        .and_then(|t| t.split('%').next())
        .and_then(|n| n.trim().parse().ok())
}

/// The percent `svc.asp` shows for a unit being rebuilt.
fn rebuild_percent(asp: &str) -> Option<u8> {
    asp.split("DPY/Rebuild ")
        .nth(1)
        .and_then(|t| t.split('%').next())
        .and_then(|n| n.trim().parse().ok())
}

/// Poll `svc.asp` until `n` units are `DPY/Active`; the seconds waited.
fn wait_active(s: &mut Session, n: usize) -> Result<u64, String> {
    let start = Instant::now();
    let deadline = start + Duration::from_mins(60);
    loop {
        let asp = s.command("svc.asp", CMD)?;
        if asp.matches("DPY/Active").count() == n {
            return Ok(start.elapsed().as_secs());
        }
        if Instant::now() > deadline {
            return Err(format!("not {n} DPY/Active units after an hour:\n{asp}"));
        }
        std::thread::sleep(Duration::from_secs(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asp_lines_parse() {
        let asp = "ASP 1: 4 unit(s)\n  unit 0 Active          100 pages (90 usable), 12 used, serial QS400HD0 partner 1\n  unit 1 Missing         100 pages (90 usable), 0 used, serial QS400HD3 partner 0 (not attached)\n";
        let u = units(asp);
        assert_eq!(u.len(), 2);
        assert_eq!(u[0].used, 12);
        assert_eq!(u[0].partner, Some(1));
        assert_eq!(disk_of(&u[1].serial), Some(4));
        assert_eq!(u[1].state, "Missing");
        assert!(!u[1].attached);
    }

    #[test]
    fn parity_percents_parse() {
        let sync = "  parity set 1 (RAID 5): unit 0 DPY/Unprotected, unit 1 DPY/Unprotected; synchronizing 37%\n";
        assert_eq!(sync_percent(sync), Some(37));
        assert_eq!(rebuild_percent(sync), None);
        let rebuild = "  parity set 1 (RAID 5): unit 0 DPY/Unprotected, unit 3 DPY/Rebuild 5%\n";
        assert_eq!(rebuild_percent(rebuild), Some(5));
        assert_eq!(sync_percent(rebuild), None);
    }
}
