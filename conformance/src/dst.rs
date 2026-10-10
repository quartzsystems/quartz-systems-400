// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Dedicated Service Tools scenarios of the install suite: a
//! two-disk install mirrored through DST with a power pull during the
//! synchronization, a boot with the second disk gone and back, and the
//! service tools user lockout. Every screen is driven over serial.

use std::fs;
use std::thread;
use std::time::Duration;

use crate::error::HarnessError;
use crate::qemu::Launch;
use crate::qemu::session::Session;
use crate::steps::{SRC_IPL_COMPLETE, SRC_SEQUENCE, StepReport, Suite};

const CMD: Duration = Duration::from_secs(10);
/// The DST password QSECOFR gets on first use.
const PASSWORD: &str = "Quartz1";

/// Sign on to DST as QSECOFR with `password`.
fn sign_on(s: &mut Session, password: &str) -> Result<(), String> {
    s.expect("Dedicated Service Tools (DST) Sign On", CMD)?;
    s.send("type QSECOFR")?;
    s.send("key tab")?;
    s.send(&format!("type {password}"))?;
    s.send("key enter")?;
    Ok(())
}

/// QSECOFR's first sign-on: the shipped password, then the change.
fn first_sign_on(s: &mut Session) -> Result<(), String> {
    sign_on(s, "QSECOFR")?;
    s.expect("Change Service Tools User Password", CMD)?;
    s.send("key tab")?;
    s.send("type QSECOFR")?;
    s.send("key tab")?;
    s.send(&format!("type {PASSWORD}"))?;
    s.send("key tab")?;
    s.send(&format!("type {PASSWORD}"))?;
    s.send("key enter")?;
    s.expect("Use Dedicated Service Tools (DST)", CMD)?;
    Ok(())
}

/// Poll `mirror status` until no resume runs.
fn wait_resume(s: &mut Session) -> Result<(), String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(600);
    loop {
        let out = s.command("mirror status", CMD)?;
        if out.contains("no resume running") {
            return Ok(());
        }
        if std::time::Instant::now() > deadline {
            return Err(format!("resume did not finish: {out}"));
        }
        thread::sleep(Duration::from_millis(500));
    }
}

impl Suite {
    /// Steps 4 to 6 of the install suite, on a fresh pair of disks.
    pub(crate) fn dst_scenarios(&self, media: &Launch) -> Result<Vec<StepReport>, HarnessError> {
        let first = self.out_dir.join("dst-a.img");
        let second = self.out_dir.join("dst-b.img");
        for p in [&first, &second] {
            let f = fs::File::create(p)?;
            f.set_len(4 * 1024 * 1024 * 1024)?;
        }
        let two = Launch {
            image: Some(first.clone()),
            extra_disks: vec![second.clone()],
            ..media.clone()
        };
        let one_disk = Launch {
            image: Some(first.clone()),
            cdrom: None,
            cdrom_first: false,
            extra_disks: Vec::new(),
            ..media.clone()
        };
        let two_disks = Launch {
            cdrom: None,
            cdrom_first: false,
            ..two.clone()
        };
        let mut reports = Vec::new();
        let mut s = Session::spawn(&two)?;
        reports.push(self.step(&mut s, "4 DST mirrored install", Self::mirrored_install)?);
        s.kill();
        drop(s);
        reports.push(self.pulled_sync(&two)?);
        let mut s = Session::spawn(&one_disk)?;
        reports.push(self.step(&mut s, "6 half missing in DST", Self::missing_half)?);
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(15));
        drop(s);
        let mut s = Session::spawn(&two_disks)?;
        reports.push(self.step(
            &mut s,
            "7 resume and lockout in DST",
            Self::resume_and_lockout,
        )?);
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(15));
        Ok(reports)
    }

    /// Step 4: the LIC onto DD001, the re-IPL into DST with A6001260,
    /// DD002 added and paired through the disk menus, option 1 to
    /// re-IPL. Ends inside the synchronization of the next IPL, which
    /// step 5 interrupts.
    fn mirrored_install(&self, s: &mut Session) -> Result<String, String> {
        s.expect("Selection", self.boot_timeout)?;
        s.send("2")?;
        s.expect("Install Licensed Internal Code", CMD)?;
        s.send("1")?;
        s.expect("Install Licensed Internal Code (LIC)", CMD)?;
        s.send("2")?;
        s.expect("Select Load Source Device", CMD)?;
        s.send("1")?;
        s.expect("Confirmation", CMD)?;
        s.send("key f10")?;
        s.expect("SRC C6004090", Duration::from_secs(120))?;
        s.expect("SRC D1001090", CMD)?;
        s.expect("SRC C6004100", self.boot_timeout)?;
        s.expect("SRC A6001260", CMD)?;
        first_sign_on(s)?;
        // Work with disk units > configuration > ASP configuration > add.
        s.send("4")?;
        s.expect("Work with Disk Units", CMD)?;
        s.send("1")?;
        s.expect("Work with Disk Configuration", CMD)?;
        s.send("3")?;
        s.expect("Work with ASP Configuration", CMD)?;
        s.send("2")?;
        s.expect("Add Units to ASP 1", CMD)?;
        s.send("1")?;
        s.expect("Confirm Add Units", CMD)?;
        s.send("key f10")?;
        s.expect("added to ASP 1", Duration::from_secs(60))?;
        s.send("key f12")?;
        s.expect("Work with Disk Configuration", CMD)?;
        // Mirrored protection > start.
        s.send("4")?;
        s.expect("Work with Mirrored Protection", CMD)?;
        s.send("2")?;
        s.expect("Select ASP to Start Mirrored Protection", CMD)?;
        s.send("1")?;
        s.expect("Confirm Start Mirrored Protection", CMD)?;
        self.snap("dst-mirror-start-confirm");
        s.send("key f10")?;
        s.expect("pair(s) formed", Duration::from_secs(60))?;
        s.send("1")?;
        s.expect("Display Disk Configuration Status", CMD)?;
        s.expect("Resuming", CMD)?;
        self.snap("dst-disk-status");
        s.send("key f3")?;
        s.expect("Use Dedicated Service Tools (DST)", CMD)?;
        s.send("1")?;
        s.expect("SRC D1001090", CMD)?;
        // The next IPL synchronizes before DST comes back.
        s.expect("SRC C1001075", self.boot_timeout)?;
        s.expect("slic: synchronizing unit 1: 20%", Duration::from_secs(120))?;
        Ok("DD002 added and paired in DST; synchronization running at the re-IPL".into())
    }

    /// Step 5: the power pulled at 20% of the synchronization; the next
    /// IPL continues from the journal, the operating system is
    /// installed through DST, and `asp` shows the pair active.
    fn pulled_sync(&self, two: &Launch) -> Result<StepReport, HarnessError> {
        let name = "5 power pull during synchronization";
        let start = std::time::Instant::now();
        let mut s = Session::spawn(two)?;
        let fail = |s: &Session, reason: String| HarnessError::Failed {
            step: name,
            reason,
            log: s.log.clone(),
        };
        let detail = (|| -> Result<String, String> {
            s.expect("SRC C1001075", self.boot_timeout)?;
            let line = s.expect("chunks already copied", CMD)?;
            self.snap("sync-progress");
            let copied: u64 = line
                .rsplit("unit 1: ")
                .next()
                .and_then(|t| t.split_whitespace().next())
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            if copied == 0 {
                return Err(format!("the journal remembered nothing:{line}"));
            }
            s.expect("slic: synchronizing unit 1: 100%", Duration::from_secs(300))?;
            sign_on(&mut s, PASSWORD)?;
            s.expect("Use Dedicated Service Tools (DST)", CMD)?;
            s.send("2")?;
            s.expect("Install Device Type Selection", CMD)?;
            s.send("2")?;
            s.expect("Confirm Install of the Operating System", CMD)?;
            s.send("key f10")?;
            s.expect("Select a Language Group", CMD)?;
            s.send("1")?;
            s.expect("Confirm Language Feature Selection", CMD)?;
            s.send("key f10")?;
            s.expect("IPL Options", self.boot_timeout)?;
            s.send("key enter")?;
            s.expect("Set Major System Options", CMD)?;
            s.send("key enter")?;
            s.expect(SRC_IPL_COMPLETE, self.boot_timeout)?;
            s.expect(crate::PROMPT, CMD)?;
            let asp = s.command("asp", CMD)?;
            if !asp.contains("protection: mirrored") || asp.matches("Active").count() != 2 {
                return Err(format!("asp after the install:\n{asp}"));
            }
            let chk = s.command("chkstg", Duration::from_secs(300))?;
            if !chk.contains(": 0 bad") {
                return Err(format!("chkstg:\n{chk}"));
            }
            Ok(format!(
                "synchronization resumed with {copied} chunks already copied; OS installed; pair active; chkstg clean"
            ))
        })();
        let detail = detail.map_err(|e| fail(&s, e))?;
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(15));
        Ok(StepReport {
            name,
            elapsed: start.elapsed(),
            detail,
        })
    }

    /// Step 6: the disk alone: A6001255, then a manual IPL into DST
    /// shows unit 2 missing.
    fn missing_half(&self, s: &mut Session) -> Result<String, String> {
        s.expect("SRC A6001255", self.boot_timeout)?;
        s.expect(crate::PROMPT, self.boot_timeout)?;
        s.command("mode manual", CMD)?;
        s.send("pwrdwn restart")?;
        s.expect("SRC D1001090", CMD)?;
        s.expect("SRC A6001255", self.boot_timeout)?;
        s.expect("IPL or Install the System", self.boot_timeout)?;
        s.send("3")?;
        sign_on(s, PASSWORD)?;
        s.expect("Use Dedicated Service Tools (DST)", CMD)?;
        s.send("4")?;
        s.expect("Work with Disk Units", CMD)?;
        s.send("1")?;
        s.expect("Work with Disk Configuration", CMD)?;
        s.send("1")?;
        s.expect("Display Disk Configuration", CMD)?;
        s.send("1")?;
        s.expect("Display Disk Configuration Status", CMD)?;
        let shown = s.expect("Press Enter to continue", CMD)?;
        if !shown.contains("Missing") {
            return Err(format!("unit 2 is not shown missing:\n{shown}"));
        }
        s.send("key f3")?;
        s.expect("Use Dedicated Service Tools (DST)", CMD)?;
        s.send("1")?;
        s.expect_in_order(&SRC_SEQUENCE[7..], self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        s.command("mode manual", CMD)?;
        Ok("A6001255; DST shows unit 2 Missing; IPL goes on from DST".into())
    }

    /// Step 7: both disks again: QSRV disabled by three wrong passwords,
    /// QSECOFR enables it, unit 2 resumed from the recovery menu, the
    /// IPL goes on and the halves agree.
    fn resume_and_lockout(&self, s: &mut Session) -> Result<String, String> {
        s.expect("IPL or Install the System", self.boot_timeout)?;
        s.send("3")?;
        for _ in 0..3 {
            s.expect("Dedicated Service Tools (DST) Sign On", CMD)?;
            s.send("type QSRV")?;
            s.send("key tab")?;
            s.send("type wrong")?;
            s.send("key enter")?;
        }
        s.expect("User ID is disabled", CMD)?;
        // Field Exit clears the user and lands in the password field.
        s.send("key home")?;
        s.send("key fieldexit")?;
        s.send(&format!("type {PASSWORD}"))?;
        s.send("key tab")?;
        s.send("type QSECOFR")?;
        s.send("key enter")?;
        s.expect("Use Dedicated Service Tools (DST)", CMD)?;
        // Environment > user IDs: 5 beside QSRV.
        s.send("5")?;
        s.expect("Work with DST Environment", CMD)?;
        s.send("3")?;
        s.expect("Work with Service Tools User IDs", CMD)?;
        s.expect("QSRV", CMD)?;
        s.send("key down")?;
        s.send("5")?;
        s.expect("User ID QSRV enabled", CMD)?;
        s.send("key f3")?;
        s.expect("Use Dedicated Service Tools (DST)", CMD)?;
        // Disk units > recovery > resume.
        s.send("4")?;
        s.expect("Work with Disk Units", CMD)?;
        s.send("2")?;
        s.expect("Work with Disk Unit Recovery", CMD)?;
        s.send("8")?;
        s.expect("Resume Mirrored Protection", CMD)?;
        s.send("1")?;
        s.expect("resuming in the background", CMD)?;
        s.send("key f3")?;
        s.expect("Use Dedicated Service Tools (DST)", CMD)?;
        s.send("1")?;
        s.expect(SRC_IPL_COMPLETE, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        wait_resume(s)?;
        let chk = s.command("chkstg", Duration::from_secs(300))?;
        if !chk.contains(": 0 bad") {
            return Err(format!("chkstg after the resume:\n{chk}"));
        }
        let asp = s.command("asp", CMD)?;
        if asp.matches("Active").count() != 2 {
            return Err(format!("asp after the resume:\n{asp}"));
        }
        Ok("QSRV disabled after three wrong passwords and enabled by QSECOFR; unit 2 resumed from DST; halves agree".into())
    }
}
