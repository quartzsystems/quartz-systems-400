// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The six Phase 1 steps. They run against a working copy of the image so
//! state persists across guest restarts and the built image stays pristine.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::error::HarnessError;
#[cfg(unix)]
use crate::png;
use crate::qemu::session::Session;
use crate::qemu::{self, Launch};

/// First line of every IPL.
pub const SRC_IPL_STARTED: &str = "SRC C1001000";
/// Last line of a successful IPL.
pub const SRC_IPL_COMPLETE: &str = "SRC C6001090";

/// Emitted instead of C1001010 when booting from installation media
/// without a LIC partition.
pub const SRC_MEDIA_IPL: &str = "SRC C1001015";
/// The line slic prints on every D IPL, with or without a LIC partition.
pub const MEDIA_LINE: &str = "slic: IPL type D from installation media";

/// Codes that must appear in this order on every normal IPL. Codes not
/// listed (attention codes, the abnormal notice) may appear in between.
pub const SRC_SEQUENCE: &[&str] = &[
    SRC_IPL_STARTED,
    "SRC C1001010",
    "SRC C1001020",
    "SRC C1001030",
    "SRC C1001040",
    "SRC C1001060",
    "SRC C6001000",
    "SRC C6001010",
    "SRC C6001020",
    SRC_IPL_COMPLETE,
];

/// The sequence of a D IPL from media with no LIC partition.
pub const SRC_SEQUENCE_MEDIA: &[&str] = &[
    SRC_IPL_STARTED,
    SRC_MEDIA_IPL,
    "SRC C1001020",
    "SRC C1001030",
    "SRC C1001040",
    "SRC C1001060",
    "SRC C6001000",
    "SRC C6001010",
    "SRC C6001020",
    SRC_IPL_COMPLETE,
];

const CMD: Duration = Duration::from_secs(10);

/// What the suite boots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Media {
    /// The disk image: the six Phase 1 steps.
    Disk,
    /// The installation ISO: media IPL alone, then with the disk attached
    /// under both boot orders.
    Iso,
}

impl std::str::FromStr for Media {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "disk" => Ok(Media::Disk),
            "iso" => Ok(Media::Iso),
            other => Err(format!("unknown media `{other}` (disk or iso)")),
        }
    }
}

/// What one step observed.
#[derive(Debug)]
pub struct StepReport {
    /// Step number and name.
    pub name: &'static str,
    /// Wall time for the step.
    pub elapsed: Duration,
    /// One line of detail.
    pub detail: String,
}

/// The suite: launch settings and output locations.
pub struct Suite {
    launch: Launch,
    media: Media,
    iso: Option<PathBuf>,
    /// Directory for the working image, the QMP socket and screenshots.
    pub out_dir: PathBuf,
    /// Time allowed for an IPL to reach completion.
    pub boot_timeout: Duration,
}

// The steps share one signature so `step` can take them as fn pointers.
#[allow(clippy::unused_self)]
impl Suite {
    /// Resolve QEMU and firmware; copy `image` into `out_dir` as the
    /// working disk. The ISO suite needs `iso` and uses `image` for the
    /// boot-order steps.
    pub fn new(
        image: Option<&Path>,
        iso: Option<&Path>,
        media: Media,
        out_dir: PathBuf,
    ) -> Result<Self, HarnessError> {
        fs::create_dir_all(&out_dir)?;
        let work = match image {
            Some(image) if image.is_file() => {
                let work = out_dir.join("work.img");
                fs::copy(image, &work)?;
                Some(work)
            }
            Some(image) => return Err(HarnessError::ImageMissing(image.to_path_buf())),
            None => None,
        };
        match (media, iso) {
            (Media::Iso, None) => return Err(HarnessError::ImageMissing(out_dir.join("iso.path"))),
            (Media::Iso, Some(iso)) if !iso.is_file() => {
                return Err(HarnessError::ImageMissing(iso.to_path_buf()));
            }
            (Media::Disk, _) if work.is_none() => {
                return Err(HarnessError::ImageMissing(out_dir.join("disk.path")));
            }
            _ => {}
        }
        let boot_timeout = std::env::var("QS400_BOOT_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .map_or(Duration::from_secs(60), Duration::from_secs);
        Ok(Self {
            launch: Launch {
                qemu: qemu::locate()?,
                firmware: qemu::firmware::locate()?,
                kvm: qemu::kvm_available(),
                image: work,
                cdrom: None,
                cdrom_first: false,
                qmp: out_dir.join("qmp.sock"),
            },
            media,
            iso: iso.map(Path::to_path_buf),
            out_dir,
            boot_timeout,
        })
    }

    /// Launch settings.
    pub fn launch(&self) -> &Launch {
        &self.launch
    }

    /// Run every step in order. Stops at the first failure.
    pub fn run_all(&self) -> Result<Vec<StepReport>, HarnessError> {
        match self.media {
            Media::Disk => self.run_disk(),
            Media::Iso => self.run_iso(),
        }
    }

    /// The installation media steps, each in its own guest.
    fn run_iso(&self) -> Result<Vec<StepReport>, HarnessError> {
        let iso = self.iso.clone().expect("checked in new");
        let mut reports = Vec::new();

        // 1. The ISO alone: a D IPL without a LIC partition reaches sign-on.
        let alone = Launch {
            image: None,
            cdrom: Some(iso.clone()),
            cdrom_first: true,
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&alone)?;
        reports.push(self.step(&mut s, "1 media IPL", Self::media_ipl)?);
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(15));
        drop(s);

        if self.launch.image.is_none() {
            return Ok(reports);
        }

        // 2. ISO and disk, firmware told to boot the CD-ROM first.
        let cd_first = Launch {
            cdrom: Some(iso.clone()),
            cdrom_first: true,
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&cd_first)?;
        reports.push(self.step(&mut s, "2 ISO first", Self::media_chosen)?);
        s.kill();

        // 3. ISO and disk, disk first: the disk must win.
        let disk_first = Launch {
            cdrom: Some(iso),
            cdrom_first: false,
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&disk_first)?;
        reports.push(self.step(&mut s, "3 disk first", Self::disk_chosen)?);
        s.kill();
        Ok(reports)
    }

    /// Media step 1: the D IPL sequence with C1001015, then the sign-on screen.
    fn media_ipl(&self, s: &mut Session) -> Result<String, String> {
        let seen = s.expect_in_order(SRC_SEQUENCE_MEDIA, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        let dump = s.command("screen", CMD)?;
        if !dump.contains("Sign On") {
            return Err("sign-on display is not showing".into());
        }
        let last = seen.last().map_or(0.0, |(_, t)| t.as_secs_f64());
        Ok(format!("C1001015 seen, sign-on display at {last:.1}s"))
    }

    /// Media step 2: with the disk attached and the CD-ROM first, the ISO
    /// boots. Its D IPL then finds the disk's LIC partition, so the signal
    /// is the media line slic prints, not C1001015.
    fn media_chosen(&self, s: &mut Session) -> Result<String, String> {
        s.expect(MEDIA_LINE, self.boot_timeout)?;
        s.expect("SRC C1001010", self.boot_timeout)?;
        Ok("ISO booted (IPL type D) and used the disk's LIC partition".into())
    }

    /// Media step 3: with the disk first, the disk boots: C1001010 arrives
    /// without the media line before it.
    fn disk_chosen(&self, s: &mut Session) -> Result<String, String> {
        let before = s.expect("SRC C1001010", self.boot_timeout)?;
        if before.contains(MEDIA_LINE) {
            return Err("ISO was booted despite the disk being first".into());
        }
        Ok("disk booted: C1001010 without a media IPL".into())
    }

    /// The six Phase 1 steps against the disk image.
    fn run_disk(&self) -> Result<Vec<StepReport>, HarnessError> {
        let mut reports = Vec::new();
        let mut s = Session::spawn(&self.launch)?;
        reports.push(self.step(&mut s, "1 SRC sequence", Self::src_sequence)?);
        reports.push(self.step(&mut s, "2 sign-on screen", Self::sign_on_screen)?);
        reports.push(self.step(&mut s, "3 system values", Self::sysvals)?);
        reports.push(self.step(&mut s, "4 restart and history", Self::restart)?);
        reports.push(self.step(&mut s, "6 screenshot", Self::screenshot)?);
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(15));
        drop(s);
        reports.push(self.abnormal()?);
        Ok(reports)
    }

    fn step(
        &self,
        s: &mut Session,
        name: &'static str,
        f: fn(&Self, &mut Session) -> Result<String, String>,
    ) -> Result<StepReport, HarnessError> {
        let start = Instant::now();
        match f(self, s) {
            Ok(detail) => Ok(StepReport {
                name,
                elapsed: start.elapsed(),
                detail,
            }),
            Err(reason) => Err(HarnessError::Failed {
                step: name,
                reason,
                log: s.log.clone(),
            }),
        }
    }

    /// Step 1: the SRC sequence in order, then the service prompt.
    fn src_sequence(&self, s: &mut Session) -> Result<String, String> {
        let seen = s.expect_in_order(SRC_SEQUENCE, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        let last = seen.last().map_or(0.0, |(_, t)| t.as_secs_f64());
        Ok(format!(
            "{} codes in order, IPL complete at {last:.1}s",
            seen.len()
        ))
    }

    /// Step 2: rows 1 to 10 of the sign-on display, exactly.
    fn sign_on_screen(&self, s: &mut Session) -> Result<String, String> {
        let dump = s.command("screen", CMD)?;
        let rows: Vec<&str> = dump.lines().filter(|l| l.chars().count() == 80).collect();
        if rows.len() < 24 {
            return Err(format!(
                "screen dump has {} rows of 80, want 24",
                rows.len()
            ));
        }
        let want = expected_sign_on_rows();
        for (i, (got, want)) in rows.iter().zip(want.iter()).enumerate() {
            if got != want {
                return Err(format!(
                    "row {} differs\n  got:  {got:?}\n  want: {want:?}",
                    i + 1
                ));
            }
        }
        Ok("rows 1-10 match the layout".into())
    }

    /// Step 3: read QMODEL, change and read back QSECURITY.
    fn sysvals(&self, s: &mut Session) -> Result<String, String> {
        let model = s.command("sysval QMODEL", CMD)?;
        if !model.contains("QMODEL: QS40") {
            return Err(format!("sysval QMODEL: {model:?}"));
        }
        let set = s.command("sysval QSECURITY 50", CMD)?;
        if !set.contains("QSECURITY set to 50") {
            return Err(format!("sysval QSECURITY 50: {set:?}"));
        }
        let back = s.command("sysval QSECURITY", CMD)?;
        if !back.contains("QSECURITY: 50") {
            return Err(format!("sysval QSECURITY: {back:?}"));
        }
        Ok("QMODEL is QS40; QSECURITY set and read back as 50".into())
    }

    /// Step 4: `pwrdwn restart`, the power-down codes, a second IPL, two
    /// history entries with the first normal.
    fn restart(&self, s: &mut Session) -> Result<String, String> {
        s.send("pwrdwn restart")?;
        s.expect("SRC D1001000", CMD)?;
        s.expect("SRC D1001090", CMD)?;
        let t0 = s.elapsed();
        s.expect_in_order(SRC_SEQUENCE, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        let boot = s.elapsed().saturating_sub(t0);
        let hist = s.command("hist", CMD)?;
        let entries: Vec<&str> = hist
            .lines()
            .filter(|l| {
                l.split_whitespace()
                    .next()
                    .is_some_and(|w| w.chars().all(|c| c.is_ascii_digit()))
            })
            .collect();
        if entries.len() != 2 {
            return Err(format!(
                "hist shows {} entries, want 2:\n{hist}",
                entries.len()
            ));
        }
        let first: Vec<&str> = entries[0].split_whitespace().collect();
        if first.get(7) != Some(&"Normal") {
            return Err(format!("first history entry is not Normal: {}", entries[0]));
        }
        let security = s.command("sysval QSECURITY", CMD)?;
        if !security.contains("QSECURITY: 50") {
            return Err(format!(
                "QSECURITY did not persist across restart: {security:?}"
            ));
        }
        Ok(format!(
            "restarted in {:.1}s; 2 history entries, first Normal; QSECURITY persisted",
            boot.as_secs_f64()
        ))
    }

    /// Step 6: screenshot of the sign-on display through QMP.
    #[cfg(unix)]
    fn screenshot(&self, s: &mut Session) -> Result<String, String> {
        let dump = s.command("screen", CMD)?;
        if !dump.contains("Sign On") {
            return Err("sign-on display is not showing".into());
        }
        let mut q = crate::qemu::qmp::Qmp::connect(&self.launch.qmp)?;
        let png = self.out_dir.join("signon.png");
        let raw = self.out_dir.join("signon.ppm");
        match q.screendump(&raw)? {
            "png" => fs::rename(&raw, &png).map_err(|e| e.to_string())?,
            _ => png::ppm_to_png(&raw, &png)?,
        }
        let _ = fs::remove_file(&raw);
        Ok(png.display().to_string())
    }

    /// Step 6 needs a Unix socket for QMP.
    #[cfg(not(unix))]
    fn screenshot(&self, _s: &mut Session) -> Result<String, String> {
        Err("QMP screendump needs a Unix host".into())
    }

    /// Step 5: kill the guest during the LIC phase, boot again, expect the
    /// abnormal notice and QABNORMSW 1.
    fn abnormal(&self) -> Result<StepReport, HarnessError> {
        let name = "5 abnormal IPL";
        let start = Instant::now();
        let mut s = Session::spawn(&self.launch)?;
        let fail = |s: &Session, reason: String| HarnessError::Failed {
            step: name,
            reason,
            log: s.log.clone(),
        };
        if let Err(e) = s.expect("SRC C1001030", self.boot_timeout) {
            return Err(fail(&s, e));
        }
        s.kill();
        let mut s = Session::spawn(&self.launch)?;
        if let Err(e) = s.expect("SRC C1002000", self.boot_timeout) {
            return Err(fail(&s, e));
        }
        if let Err(e) = s.expect(SRC_IPL_COMPLETE, self.boot_timeout) {
            return Err(fail(&s, e));
        }
        if let Err(e) = s.expect(crate::PROMPT, CMD) {
            return Err(fail(&s, e));
        }
        let sw = match s.command("sysval QABNORMSW", CMD) {
            Ok(t) => t,
            Err(e) => return Err(fail(&s, e)),
        };
        if !sw.contains("QABNORMSW: 1") {
            return Err(fail(&s, format!("QABNORMSW: {sw:?}")));
        }
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(15));
        Ok(StepReport {
            name,
            elapsed: start.elapsed(),
            detail: "C1002000 after a killed IPL; QABNORMSW is 1".into(),
        })
    }
}

/// Rows 1 to 10 of the sign-on display, built from the layout in the Phase 1
/// specification (independently of the console crate).
pub fn expected_sign_on_rows() -> Vec<String> {
    let mut rows = vec![" ".repeat(80); 10];
    let mut put = |row: usize, col: usize, text: &str| {
        let r = &mut rows[row - 1];
        r.replace_range(col - 1..col - 1 + text.len(), text);
    };
    put(1, 36, "Sign On");
    put(2, 48, "System  . . . . . :");
    put(2, 68, "QS400");
    put(3, 48, "Subsystem . . . . :");
    put(3, 68, "QINTER");
    put(4, 48, "Display . . . . . :");
    put(4, 68, "QSCONSOLE");
    put(6, 17, "User  . . . . . . . . . . . . . .");
    put(7, 17, "Password  . . . . . . . . . . . .");
    put(8, 17, "Program/procedure . . . . . . . .");
    put(9, 17, "Menu  . . . . . . . . . . . . . .");
    put(10, 17, "Current library . . . . . . . . .");
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_rows_are_80_wide() {
        let rows = expected_sign_on_rows();
        assert_eq!(rows.len(), 10);
        assert!(rows.iter().all(|r| r.chars().count() == 80));
        assert_eq!(&rows[0][35..42], "Sign On");
        assert_eq!(&rows[1][67..72], "QS400");
    }
}
