// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The disk suite's steps. They run against a working copy of the image so
//! state persists across guest restarts and the built image stays pristine.
//! Phase 2 steps live in `phase2`, the database steps in `db::qemu`.

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
    "SRC C1001070",
    "SRC C1001080",
    "SRC C1001040",
    "SRC C1001060",
    "SRC C6001000",
    "SRC C6001005",
    "SRC C6001010",
    "SRC C6001012",
    "SRC C6001030",
    "SRC C6001040",
    "SRC C6001050",
    SRC_IPL_COMPLETE,
];

/// The sequence of a D IPL from media with no LIC partition, up to the
/// IPL menu. Without an installed disk it goes no further.
pub const SRC_SEQUENCE_MEDIA: &[&str] = &[
    SRC_IPL_STARTED,
    SRC_MEDIA_IPL,
    "SRC C1001020",
    "SRC C1001030",
    "SRC C1001040",
];

/// The code the media IPL stops at with no operating system installed.
pub const SRC_MEDIA_LAST: &str = "C1001040";

pub(crate) const CMD: Duration = Duration::from_secs(10);

/// What the suite boots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Media {
    /// The disk image: the six Phase 1 steps.
    Disk,
    /// The installation ISO: media IPL alone, then with the disk attached
    /// under both boot orders.
    Iso,
    /// The ISO written raw onto a virtio disk.
    IsoRaw,
    /// The ISO's tree copied onto a FAT32 disk (the Rufus case).
    IsoFat,
}

impl std::str::FromStr for Media {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "disk" => Ok(Media::Disk),
            "iso" => Ok(Media::Iso),
            "iso-raw" => Ok(Media::IsoRaw),
            "iso-fat" => Ok(Media::IsoFat),
            other => Err(format!(
                "unknown media `{other}` (disk, iso, iso-raw or iso-fat)"
            )),
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
    pub(crate) launch: Launch,
    media: Media,
    /// With `Media::Iso`: run the installer onto a blank disk instead.
    install: bool,
    /// Objects for the bench step.
    pub(crate) bench: u64,
    /// Power pulls to run (0 skips the step).
    pub(crate) pulls: u32,
    pub(crate) iso: Option<PathBuf>,
    /// Directory for the working image, the QMP socket and screenshots.
    pub out_dir: PathBuf,
    /// Time allowed for an IPL to reach completion.
    pub(crate) boot_timeout: Duration,
    /// Run only the steps whose names start with one of these (after
    /// the boot).
    pub(crate) only: Option<Vec<String>>,
    /// The telnet port forwarded to the address TCP/IP configuration
    /// set up (step 61); the TN5250 steps use it once it is there.
    pub(crate) tn_port: std::cell::Cell<Option<u16>>,
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
            (Media::Iso | Media::IsoRaw | Media::IsoFat, None) => return Err(HarnessError::ImageMissing(out_dir.join("iso.path"))),
            (Media::Iso | Media::IsoRaw | Media::IsoFat, Some(iso)) if !iso.is_file() => {
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
        let telnet_port = free_port();
        Ok(Self {
            launch: Launch {
                qemu: qemu::locate()?,
                firmware: qemu::firmware::locate()?,
                kvm: qemu::kvm_available(),
                image: work,
                cdrom: None,
                cdrom_first: false,
                extra_disks: Vec::new(),
                qmp: out_dir.join("qmp.sock"),
                telnet_port: Some(telnet_port),
                host_ports: crate::hostsvr::forwards(Some(telnet_port)),
                memory_mb: 1024,
                cpus: 2,
                display: None,
            },
            media,
            install: false,
            bench: 20_000,
            pulls: 3,
            iso: iso.map(Path::to_path_buf),
            out_dir,
            boot_timeout,
            only: None,
            tn_port: std::cell::Cell::new(None),
        })
    }

    /// Run only the steps whose names start with `prefix`; the boot
    /// step always runs.
    pub fn set_only(&mut self, prefixes: Option<String>) {
        self.only = prefixes.map(|p| p.split(',').map(|s| format!("{} ", s.trim())).collect());
    }

    /// With the ISO: install onto a blank 4 GiB disk and boot it, instead
    /// of the three media steps.
    pub fn set_install(&mut self, install: bool) {
        self.install = install;
    }

    /// Size of the object bench and number of power pulls in the disk
    /// suite.
    pub fn set_phase2(&mut self, bench: u64, pulls: u32) {
        self.bench = bench;
        self.pulls = pulls;
    }

    /// Launch settings.
    pub fn launch(&self) -> &Launch {
        &self.launch
    }

    /// The host port the TN5250 steps connect to: the one forwarded to
    /// the configured interface once step 61 set it up, else the
    /// launch's.
    pub(crate) fn telnet_port(&self) -> Option<u16> {
        self.tn_port.get().or(self.launch.telnet_port)
    }

    /// Run every step in order. Stops at the first failure.
    pub fn run_all(&self) -> Result<Vec<StepReport>, HarnessError> {
        match self.media {
            Media::Disk => self.run_disk(),
            Media::Iso if self.install => self.run_install(),
            Media::Iso => self.run_iso(),
            Media::IsoRaw => self.run_media_disk(false),
            Media::IsoFat => self.run_media_disk(true),
        }
    }

    /// Install from the ISO: automatically onto a blank disk, boot the
    /// disk alone, then interactively over it through every screen.
    fn run_install(&self) -> Result<Vec<StepReport>, HarnessError> {
        let iso = self.iso.clone().expect("checked in new");
        let blank = self.out_dir.join("install.img");
        let f = fs::File::create(&blank)?;
        f.set_len(4 * 1024 * 1024 * 1024)?;
        drop(f);
        let mut reports = Vec::new();
        let media = Launch {
            image: Some(blank.clone()),
            cdrom: Some(iso),
            cdrom_first: true,
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&media)?;
        reports.push(self.step(&mut s, "1 automatic install", Self::automatic_install)?);
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(20));
        drop(s);
        let disk = Launch {
            image: Some(blank),
            cdrom: None,
            cdrom_first: false,
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&disk)?;
        reports.push(self.step(&mut s, "2 boot installed disk", Self::installed_boot)?);
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(15));
        drop(s);
        let mut s = Session::spawn(&media)?;
        reports.push(self.step(&mut s, "3 interactive install", Self::interactive_install)?);
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(20));
        drop(s);
        reports.extend(self.dst_scenarios(&media)?);
        Ok(reports)
    }

    /// Install step 1: option 4 writes the LIC onto the first disk and
    /// re-IPLs; the second boot from the media installs the operating
    /// system without prompts and reaches sign-on.
    pub(crate) fn automatic_install(&self, s: &mut Session) -> Result<String, String> {
        s.expect_in_order(SRC_SEQUENCE_MEDIA, self.boot_timeout)?;
        s.expect("Selection", self.boot_timeout)?;
        s.send("4")?;
        s.expect("SRC C6004090", Duration::from_secs(120))?;
        s.expect("SRC D1001090", CMD)?;
        s.expect("SRC C6004100", self.boot_timeout)?;
        s.expect("SRC C6004190", Duration::from_secs(120))?;
        s.expect(SRC_IPL_COMPLETE, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        let dump = s.command("screen", CMD)?;
        if !dump.contains("Sign On") {
            return Err("sign-on display is not showing".into());
        }
        let asp = s.command("svc.asp", CMD)?;
        if !asp.contains("created by V1R0M0") {
            return Err(format!("svc.asp did not report the store: {asp}"));
        }
        Ok("LIC written by option 4, re-IPL, operating system installed, sign-on".into())
    }

    /// Install step 3: option 2 through every screen, over the disk
    /// installed in step 1: the LIC menus, the load source, F10, the
    /// re-IPL into DST, sign-on, the operating system screens, the IPL
    /// options, sign-on.
    fn interactive_install(&self, s: &mut Session) -> Result<String, String> {
        s.expect("Selection", self.boot_timeout)?;
        s.send("2")?;
        s.expect("Install Licensed Internal Code", CMD)?;
        s.send("1")?;
        s.expect("Install Licensed Internal Code (LIC)", CMD)?;
        s.send("2")?;
        s.expect("(has data)", CMD)?;
        self.snap("install-load-source");
        s.send("1")?;
        s.expect("Install LIC and Initialize System - Confirmation", CMD)?;
        self.snap("install-lic-confirm");
        s.send("key f10")?;
        s.expect("SRC C6004090", Duration::from_secs(120))?;
        s.expect("SRC D1001090", CMD)?;
        // The re-IPL from the media lands in DST.
        s.expect("SRC C6004100", self.boot_timeout)?;
        s.expect("Dedicated Service Tools (DST) Sign On", self.boot_timeout)?;
        s.send("type QSECOFR")?;
        s.send("key tab")?;
        s.send("type QSECOFR")?;
        s.send("key enter")?;
        s.expect("Change Service Tools User Password", CMD)?;
        s.send("key tab")?;
        s.send("type QSECOFR")?;
        s.send("key tab")?;
        s.send("type Quartz1")?;
        s.send("key tab")?;
        s.send("type Quartz1")?;
        s.send("key enter")?;
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
        s.expect("SRC C6004120", Duration::from_secs(120))?;
        s.expect("IPL Options", self.boot_timeout)?;
        self.snap("install-ipl-options");
        s.send("key enter")?;
        s.expect("Set Major System Options", CMD)?;
        s.send("key enter")?;
        s.expect("SRC C6004190", CMD)?;
        s.expect(SRC_IPL_COMPLETE, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        let dump = s.command("screen", CMD)?;
        if !dump.contains("Sign On") {
            return Err("sign-on display is not showing".into());
        }
        let vals = s.command("svc.sysval QAUTOCFG", CMD)?;
        if !vals.contains("QAUTOCFG: 1") {
            return Err(format!("major options not applied: {vals}"));
        }
        Ok("every install screen walked over serial; options applied; sign-on".into())
    }

    /// Install step 2: the disk IPLs on its own and reaches sign-on.
    pub(crate) fn installed_boot(&self, s: &mut Session) -> Result<String, String> {
        let seen = s.expect_in_order(SRC_SEQUENCE, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        let dump = s.command("screen", CMD)?;
        if !dump.contains("Sign On") {
            return Err("sign-on display is not showing".into());
        }
        let asp = s.command("svc.asp", CMD)?;
        if !asp.contains("created by V1R0M0") {
            return Err(format!("svc.asp did not report the store: {asp}"));
        }
        let last = seen.last().map_or(0.0, |(_, t)| t.as_secs_f64());
        Ok(format!("C1001080 and C6001005 seen, sign-on at {last:.1}s"))
    }

    /// The installation media steps, each in its own guest.
    fn run_iso(&self) -> Result<Vec<StepReport>, HarnessError> {
        let iso = self.iso.clone().expect("checked in new");
        let mut reports = Vec::new();

        // 1. The ISO alone: a D IPL with nothing installed stops at a panel.
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

        // 4. ISO first again, option 1 this time: the disk's system IPLs.
        let mut s = Session::spawn(&cd_first)?;
        reports.push(self.step(&mut s, "4 media IPL of the disk", Self::media_installed)?);
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(20));
        drop(s);
        Ok(reports)
    }

    /// Media step 1: the D IPL sequence with C1001015, the attended IPL
    /// menu answered with option 1 over serial; with no disk there is no
    /// operating system, so the panel that says so, never the OS phase.
    pub(crate) fn media_ipl(&self, s: &mut Session) -> Result<String, String> {
        s.expect_in_order(SRC_SEQUENCE_MEDIA, self.boot_timeout)?;
        s.expect("Selection", self.boot_timeout)?;
        s.send("1")?;
        let out = s.expect("No Operating System Installed", CMD)?;
        if out.contains("SRC C6001000") {
            return Err("the OS phase started without an installed system".into());
        }
        let dump = crate::phase5::screen_wait(s, "No Operating System Installed")?;
        for want in [
            SRC_MEDIA_LAST,
            "(No disk units detected)",
            "2. Install the operating system",
            "3. Use Dedicated Service Tools (DST)",
            "F3=Return to IPL menu",
        ] {
            if !dump.contains(want) {
                return Err(format!("the panel lacks {want:?}:\n{dump}"));
            }
        }
        self.snap("media-no-os");
        s.send("key f3")?;
        crate::phase5::screen_wait(s, "IPL or Install the System")?;
        if s.command("svc.srcs", CMD)?.contains("C6001090") {
            return Err("C6001090 was reached".into());
        }
        Ok(format!(
            "C1001015 seen, option 1 shows No Operating System Installed at {SRC_MEDIA_LAST}, F3 returns"
        ))
    }

    /// Media step 4: an installed disk and the media, the CD-ROM first:
    /// option 1 IPLs from the disk to sign-on.
    fn media_installed(&self, s: &mut Session) -> Result<String, String> {
        s.expect(MEDIA_LINE, self.boot_timeout)?;
        s.expect("Selection", self.boot_timeout)?;
        s.send("1")?;
        let seen = s.expect_in_order(
            &["SRC C1001070", "SRC C6001000", SRC_IPL_COMPLETE],
            self.boot_timeout,
        )?;
        s.expect(crate::PROMPT, CMD)?;
        crate::phase5::screen_wait(s, "Sign On")?;
        let last = seen.last().map_or(0.0, |(_, t)| t.as_secs_f64());
        Ok(format!(
            "option 1 on the media IPLed the disk's ASP 1, sign-on at {last:.1}s"
        ))
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
        reports.push(self.step(&mut s, "7 libraries", Self::libraries)?);
        reports.push(self.step(&mut s, "8 sysval in store", Self::sysval_in_store)?);
        reports.push(self.step(&mut s, "9 object bench", Self::object_bench)?);
        reports.push(self.step(&mut s, "13 database kit", Self::db_kit)?);
        reports.push(self.step(&mut s, "15 IR kit", Self::qir_kit)?);
        reports.push(self.step(&mut s, "16 active jobs", Self::active_jobs)?);
        reports.push(self.step(&mut s, "17 job descriptions", Self::job_descriptions)?);
        reports.push(self.step(&mut s, "18 library list", Self::library_list)?);
        reports.push(self.step(&mut s, "19 job log", Self::job_log)?);
        reports.push(self.step(&mut s, "20 message queues", Self::message_queues)?);
        reports.push(self.step(&mut s, "21 history log", Self::history_log)?);
        reports.push(self.step(&mut s, "22 reply list", Self::reply_list)?);
        reports.push(self.step(&mut s, "23 subsystems", Self::subsystems)?);
        reports.push(self.step(&mut s, "24 console sign-on", Self::console_sign_on)?);
        reports.push(self.step(&mut s, "25 job queues", Self::job_queues)?);
        reports.push(self.step(&mut s, "26 job scheduler", Self::job_scheduler)?);
        reports.push(self.step(&mut s, "27 spooled files", Self::spooled_files)?);
        reports.push(self.step(&mut s, "28 printer files", Self::printer_files)?);
        reports.push(self.step(&mut s, "29 writers", Self::writers)?);
        reports.push(self.step(&mut s, "30 power down", Self::power_down)?);
        reports.push(self.step(&mut s, "31 statistics", Self::statistics)?);
        reports.push(self.step(&mut s, "32 report job", Self::report_job)?);
        reports.push(self.step(&mut s, "33 batch inquiry", Self::batch_inquiry)?);
        reports.push(self.step(&mut s, "34 end busy subsystem", Self::end_busy_subsystem)?);
        reports.push(self.step(&mut s, "35 killed job", Self::killed_job)?);
        reports.push(self.step(&mut s, "36 work management bench", Self::wm_bench)?);
        reports.push(self.step(&mut s, "37 user profiles", Self::user_profiles)?);
        reports.push(self.step(&mut s, "38 sign-on as a user", Self::user_sign_on)?);
        reports.push(self.step(&mut s, "39 adopted authority", Self::adopted_authority)?);
        reports.push(self.step(&mut s, "40 domain and state", Self::domain_state)?);
        reports.push(self.step(&mut s, "41 auditing", Self::auditing)?);
        reports.push(self.step(&mut s, "42 audit end action", Self::audit_end_action)?);
        reports.push(self.step(&mut s, "43 delete profile", Self::delete_profile)?);
        reports.push(self.step(&mut s, "44 security levels", Self::security_levels)?);
        reports.push(self.step(&mut s, "45 security bench", Self::security_bench)?);
        reports.push(self.step(&mut s, "46 command entry", Self::command_entry)?);
        reports.push(self.step(&mut s, "47 CL program", Self::cl_program)?);
        reports.push(self.step(&mut s, "48 limited user", Self::limited_user)?);
        reports.push(self.step(&mut s, "49 startup program", Self::startup_program)?);
        reports.push(self.step(&mut s, "50 analyzer console", Self::analyzer_console)?);
        reports.push(self.step(&mut s, "61 TCP/IP configuration", Self::tcp_config)?);
        reports.push(self.step(&mut s, "51 telnet sign-on", Self::telnet_sign_on)?);
        reports.push(self.step(
            &mut s,
            "52 telnet command entry",
            Self::telnet_command_entry,
        )?);
        reports.push(self.step(&mut s, "53 telnet device recovery", Self::telnet_recovery)?);
        reports.push(self.step(&mut s, "54 display file", Self::telnet_display_file)?);
        reports.push(self.step(&mut s, "55 developer tools", Self::telnet_tools)?);
        reports.push(self.step(&mut s, "56 RPG program", Self::rpg_program)?);
        reports.push(self.step(&mut s, "57 RPG embedded SQL", Self::rpg_sql)?);
        reports.push(self.step(&mut s, "58 RPG service program", Self::rpg_service_program)?);
        reports.push(self.step(&mut s, "59 IFS mount", Self::ifs_mount)?);
        reports.push(self.step(&mut s, "60 communications lines", Self::cmn_lines)?);
        reports.push(self.step(&mut s, "62 ENDTCP and STRTCP", Self::tcp_restart)?);
        reports.push(self.step(&mut s, "63 IFS commands", Self::ifs_commands)?);
        reports.push(self.step(&mut s, "64 TCP servers", Self::tcp_servers)?);
        reports.push(self.step(&mut s, "68 host servers", Self::host_servers)?);
        reports.push(self.step(&mut s, "66 network tools", Self::net_tools)?);
        reports.push(self.step(&mut s, "69 small servers", Self::small_servers)?);
        reports.push(self.step(&mut s, "67 FTP server", Self::ftp_server)?);
        reports.push(self.step(&mut s, "70 certificates", Self::certificates)?);
        reports.push(self.step(&mut s, "65 QSH and PASE", Self::qsh_pase)?);
        reports.push(self.step(&mut s, "71 save and restore", Self::save_restore)?);
        reports.push(self.step(&mut s, "72 problems, dumps and traces", Self::service)?);
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(15));
        drop(s);
        // Each in its own guest; with `--only` when named.
        #[cfg(unix)]
        reports.extend(self.display_steps()?);
        if self.only.is_some() {
            return Ok(reports);
        }
        reports.push(self.db_crash()?);
        reports.push(self.abnormal()?);
        reports.push(self.power_pulls()?);
        reports.push(self.mirrored_pair()?);
        reports.push(self.dst_signon()?);
        Ok(reports)
    }

    pub(crate) fn step(
        &self,
        s: &mut Session,
        name: &'static str,
        f: fn(&Self, &mut Session) -> Result<String, String>,
    ) -> Result<StepReport, HarnessError> {
        let start = Instant::now();
        if let Some(prefixes) = &self.only
            && !prefixes.iter().any(|p| name.starts_with(p.as_str()))
            && !name.starts_with("1 ")
        {
            return Ok(StepReport {
                name,
                elapsed: Duration::ZERO,
                detail: "skipped".into(),
            });
        }
        match f(self, s) {
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
        // The console's sign-on comes over its TN5250 session to the
        // telnet server, a moment after the IPL completes.
        let mut dump = s.command("screen", CMD)?;
        for _ in 0..40 {
            if dump.contains("Sign On") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
            dump = s.command("screen", CMD)?;
        }
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
        Ok("rows 1-12 match the layout".into())
    }

    /// Step 3: read QMODEL, change and read back QSECURITY.
    fn sysvals(&self, s: &mut Session) -> Result<String, String> {
        let model = s.command("svc.sysval QMODEL", CMD)?;
        if !model.contains("QMODEL: QS40") {
            return Err(format!("svc.sysval QMODEL: {model:?}"));
        }
        let set = s.command("svc.sysval QSECURITY 40", CMD)?;
        if !set.contains("QSECURITY set to 40") {
            return Err(format!("svc.sysval QSECURITY 40: {set:?}"));
        }
        let back = s.command("svc.sysval QSECURITY", CMD)?;
        if !back.contains("QSECURITY: 40") {
            return Err(format!("svc.sysval QSECURITY: {back:?}"));
        }
        Ok("QMODEL is QS40; QSECURITY set and read back as 40".into())
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
        let security = s.command("svc.sysval QSECURITY", CMD)?;
        if !security.contains("QSECURITY: 40") {
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
        // After step 4's restart the console's session may still be connecting.
        crate::phase5::screen_wait(s, "Sign On")?;
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

    /// A screenshot of the display as `<name>.png` in the output
    /// directory, for the documentation. Failures are reported, never
    /// fatal: the screens are checked as text.
    #[cfg(unix)]
    pub(crate) fn snap(&self, name: &str) {
        std::thread::sleep(Duration::from_millis(400));
        let result = (|| -> Result<(), String> {
            let mut q = crate::qemu::qmp::Qmp::connect(&self.launch.qmp)?;
            let png = self.out_dir.join(format!("{name}.png"));
            let raw = self.out_dir.join(format!("{name}.ppm"));
            match q.screendump(&raw)? {
                "png" => fs::rename(&raw, &png).map_err(|e| e.to_string())?,
                _ => png::ppm_to_png(&raw, &png)?,
            }
            let _ = fs::remove_file(&raw);
            Ok(())
        })();
        if let Err(e) = result {
            eprintln!("screenshot {name}: {e}");
        }
    }

    /// Screenshots need a Unix socket for QMP.
    #[cfg(not(unix))]
    pub(crate) fn snap(&self, _name: &str) {}

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
        let sw = match s.command("svc.sysval QABNORMSW", CMD) {
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

/// A free TCP port on the loopback for the telnet forward.
pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map_or(2323, |a| a.port())
}

/// Rows 1 to 12 of the sign-on display, built from the layout in the Phase 1
/// specification (independently of the console crate). The shipped
/// `QPWDLVL` is 3, so it is `QDSIGNON2`: the long password takes rows 7-8.
pub fn expected_sign_on_rows() -> Vec<String> {
    let mut rows = vec![" ".repeat(80); 12];
    let mut put = |row: usize, col: usize, text: &str| {
        let r = &mut rows[row - 1];
        r.replace_range(col - 1..col - 1 + text.len(), text);
    };
    put(1, 36, "Sign On");
    put(2, 48, "System  . . . . . :");
    put(2, 68, "QS400");
    put(3, 48, "Subsystem . . . . :");
    put(3, 68, "QBASE");
    put(4, 48, "Display . . . . . :");
    put(4, 68, "QSCONSOLE");
    put(6, 17, "User  . . . . . . . . . . . . . .");
    put(7, 17, "Password  . . . . . . . . . . . .");
    put(10, 17, "Program/procedure . . . . . . . .");
    put(11, 17, "Menu  . . . . . . . . . . . . . .");
    put(12, 17, "Current library . . . . . . . . .");
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_rows_are_80_wide() {
        let rows = expected_sign_on_rows();
        assert_eq!(rows.len(), 12);
        assert!(rows.iter().all(|r| r.chars().count() == 80));
        assert_eq!(&rows[0][35..42], "Sign On");
        assert_eq!(&rows[1][67..72], "QS400");
    }
}
