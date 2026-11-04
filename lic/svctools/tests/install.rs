// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The installer's screens against a fixed environment: golden files
//! for every menu, list, confirmation, status display and option form.

use std::fs;
use std::path::{Path, PathBuf};

use svctools::env::{MemEnv, NonConfigured};
use svctools::install::os::{DeviceKind, InstallDevice};
use svctools::install::{LIC_STEPS, OS_STEPS, Progress, RESTORE_STEPS, disk_status};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace")
        .join("tools")
        .join("conformance")
        .join("golden")
        .join("install")
}

fn check(name: &str, dump: &str) {
    let path = golden_dir().join(format!("{name}.txt"));
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() {
        fs::create_dir_all(golden_dir()).unwrap();
        fs::write(&path, dump).unwrap();
        return;
    }
    let want = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run with QS400_UPDATE_GOLDEN=1", path.display()));
    let want = want.replace("\r\n", "\n");
    assert_eq!(dump, want, "screen {name} differs from {}", path.display());
}

fn f(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

fn env() -> MemEnv {
    let mut e = MemEnv::new("QS400");
    e.spare = vec![
        NonConfigured {
            device: "/dev/vda".into(),
            resource: "DD001".into(),
            serial: "QM00001".into(),
            type_model: "QEMU HARDDISK".into(),
            size_mib: 4096,
            has_data: true,
            controller: "DC01".into(),
        },
        NonConfigured {
            device: "/dev/vdb".into(),
            resource: "DD002".into(),
            serial: "QM00002".into(),
            type_model: "QEMU HARDDISK".into(),
            size_mib: 4096,
            has_data: false,
            controller: "DC02".into(),
        },
    ];
    e.devices = vec![
        InstallDevice {
            name: "OPT01".into(),
            kind: DeviceKind::Optical,
            location: "/dev/sr0".into(),
            media: "Install media V1R0M0 (QS00SS1 QS00WDS QS00TC1 QS00ST1)".into(),
            action: "Fresh install of V1R0M0".into(),
            refused: None,
        },
        InstallDevice {
            name: "TAPVRT01".into(),
            kind: DeviceKind::Virtual,
            location: "/dev/vdc".into(),
            media: "SAVSYS media V1R0M0 (QS00SS1)".into(),
            action: "Slip install of V1R0M0 over V1R0M0".into(),
            refused: None,
        },
        InstallDevice {
            name: "OPTVRT01".into(),
            kind: DeviceKind::Virtual,
            location: "/dev/vdd".into(),
            media: "Install media V0R9M0 (QS00SS1)".into(),
            action: "Downgrade from V1R0M0".into(),
            refused: Some(svctools::dst::upgrade_refused("V0R9M0", "V1R0M0")),
        },
    ];
    e
}

#[test]
fn install_screens_match_their_golden_files() {
    let mut e = env();
    let mut r = svctools::install::lic::start();
    check("lic-menu", &r.screen(&mut e).dump());
    r.enter(&f(&["1"]), &mut e);
    check("lic-options", &r.screen(&mut e).dump());
    r.enter(&f(&["2"]), &mut e);
    check("load-source", &r.screen(&mut e).dump());
    r.enter(&f(&["", "1"]), &mut e);
    check("lic-confirm", &r.screen(&mut e).dump());
    check("disk-status", &disk_status("QS400", "/dev/vdb", 40).dump());
    let mut p = Progress::new(&LIC_STEPS);
    p.done(0);
    p.done(1);
    check("lic-status", &svctools::install::lic_status(&e, &p).dump());
    let mut p = Progress::new(&RESTORE_STEPS);
    p.done(0);
    p.done(1);
    p.done(2);
    check("restore-status", &svctools::install::lic_status(&e, &p).dump());
    let mut none = MemEnv::new("QS400");
    let mut r = svctools::install::lic::start();
    r.enter(&f(&["1"]), &mut none);
    r.enter(&f(&["1"]), &mut none);
    check("lic-options-no-source", &r.screen(&mut none).dump());
    let mut r = svctools::install::lic::start();
    r.enter(&f(&["1"]), &mut e);
    r.enter(&f(&["3"]), &mut e);
    check("recover-source", &r.screen(&mut e).dump());
    r.enter(&f(&["1"]), &mut e);
    check("recover-confirm", &r.screen(&mut e).dump());
    let mut r = svctools::install::lic::start();
    r.enter(&f(&["1"]), &mut e);
    r.enter(&f(&["4"]), &mut e);
    check("restore-source", &r.screen(&mut e).dump());
    r.enter(&f(&["1"]), &mut e);
    check("restore-confirm", &r.screen(&mut e).dump());
    let mut r = svctools::install::os::start();
    check("device-type", &r.screen(&mut e).dump());
    r.enter(&f(&["3"]), &mut e);
    check("install-device", &r.screen(&mut e).dump());
    r.enter(&f(&["", "1"]), &mut e);
    check("os-confirm-refused", &r.screen(&mut e).dump());
    let mut r = svctools::install::os::start();
    r.enter(&f(&["2"]), &mut e);
    check("os-confirm", &r.screen(&mut e).dump());
    r.function(10, &mut e);
    check("language", &r.screen(&mut e).dump());
    r.enter(&f(&["1"]), &mut e);
    check("language-confirm", &r.screen(&mut e).dump());
    r.function(10, &mut e);
    check("install-os", &r.screen(&mut e).dump());
    r.enter(&f(&["2", "2026-01-01", "00:00:00"]), &mut e);
    check("install-options", &r.screen(&mut e).dump());
    let mut p = Progress::new(&OS_STEPS);
    p.done(0);
    p.done(1);
    p.note = "Install device OPT01, language feature 2924".into();
    check("os-status", &svctools::install::os_status(&e, &p).dump());
    let mut r = svctools::install::os::start();
    r.enter(&f(&["3"]), &mut e);
    r.enter(&f(&["1", ""]), &mut e);
    check("os-confirm-upgrade", &r.screen(&mut e).dump());
    let mut r = svctools::install::options::start();
    check("ipl-options", &r.screen(&mut e).dump());
    r.enter(
        &f(&["2026-01-01", "00:00:00", "Y", "N", "N", "N", "N", "Y", "N"]),
        &mut e,
    );
    check("major-options", &r.screen(&mut e).dump());
    let src = ("C1001040", "Clock set from RTC");
    check(
        "no-os",
        &svctools::install::noos::screen("QS400", src, &e.spare).dump(),
    );
    check(
        "no-os-no-disks",
        &svctools::install::noos::screen("QS400", src, &[]).dump(),
    );
}

#[test]
fn define_or_change_the_system_at_ipl() {
    use svctools::Outcome;
    use svctools::install::define::{AbnormalEnd, DefineSystem, reclaim_screen};
    use svctools::panel::Runner;
    let mut e = env();
    e.sysvals = vec![
        ("QDATE".into(), "103026".into(), "System date".into()),
        ("QIPLTYPE".into(), "0".into(), "IPL type".into()),
        ("QSTRUPPGM".into(), "QSTRUP     QSYS".into(), "Startup program".into()),
    ];
    // IPL Options with "define or change" Y and no major options.
    let mut r = svctools::install::options::start();
    r.enter(&f(&["2026-10-30", "12:00:00", "N", "Y", "N", "N", "N", "Y", "N"]), &mut e);
    assert_eq!(r.top(), "ipl-define");
    check("ipl-define", &r.screen(&mut e).dump());
    for (sel, name) in [
        ("1", "ipl-define-config"),
        ("2", "ipl-define-datetime"),
        ("3", "ipl-define-sysvals"),
        ("4", "ipl-define-neta"),
        ("5", "ipl-define-edtrbdap"),
        ("6", "ipl-define-edtcpcst"),
        ("7", "ipl-define-usrprf"),
        ("8", "ipl-define-strup"),
    ] {
        let mut r = Runner::new(Box::new(DefineSystem::new()));
        r.enter(&f(&[sel]), &mut e);
        check(name, &r.screen(&mut e).dump());
    }
    // 2 beside QIPLTYPE, a new value.
    let mut r = Runner::new(Box::new(DefineSystem::new()));
    r.enter(&f(&["3"]), &mut e);
    r.enter(&f(&["", "2", ""]), &mut e);
    check("ipl-define-chgsysval", &r.screen(&mut e).dump());
    r.enter(&f(&["1"]), &mut e);
    assert!(e.actions.contains(&"sysval QIPLTYPE 1".to_string()));
    assert!(r.screen(&mut e).dump().contains("System value QIPLTYPE changed"));
    assert_eq!(r.function(3, &mut e), Some(Outcome::Continue));
    // The abnormal end offer and the reclaim screens.
    let mut r = Runner::new(Box::new(AbnormalEnd));
    check("ipl-abnormal", &r.screen(&mut e).dump());
    assert_eq!(r.enter(&f(&["2"]), &mut e), Some(Outcome::Dst));
    check("rclstg-running", &reclaim_screen("QS400", None).dump());
    check("rclstg-done", &reclaim_screen("QS400", Some("12 objects checked, 0 lost")).dump());
}

/// A report as the QEMU guest of the conformance suite gives it.
fn qual_report() -> Vec<String> {
    use hwqual::report::{Check, Grade, Report};
    let pass = |r: &str, t: &str, s: &str| Check::new(Grade::Pass, r, t, s);
    let warn = |r: &str, t: &str, s: &str| Check::new(Grade::Warn, r, t, s);
    Report {
        machine: "QEMU Standard PC (Q35 + ICH9, 2009)".into(),
        hypervisor: "KVM/QEMU".into(),
        when: "2026-11-04 09:30:00".into(),
        checks: vec![
            (
                "Firmware and security".into(),
                vec![
                    pass("UEFI", "", "UEFI firmware: QS/400 boots its UKI from the EFI system partition."),
                    warn("SECBOOT", "", "Secure Boot is disabled: the firmware starts any loader."),
                    warn("TPM", "", "No TPM was found: QS/400 runs without one, but cannot seal keys to this machine."),
                ],
            ),
            (
                "Processors and main storage".into(),
                vec![
                    pass("PN01", "QP01-001", "processor 0: QEMU Virtual CPU version 2.5+"),
                    pass("PN02", "QP01-001", "processor 1: QEMU Virtual CPU version 2.5+"),
                    warn("MS01", "QM01-001", "Main storage 987 MiB: enough to IPL; 2048 MiB or more for work."),
                ],
            ),
            (
                "Storage controllers and disk units".into(),
                vec![
                    pass("DC01", "QD03-001", "ICH9 AHCI, driver ahci, 1 disk units"),
                    pass("DC02", "QD01-001", "virtio block, driver virtio-pci, 1 disk units"),
                    pass("DD001", "VIRT", "sda 4096 MiB at 0:0:0:0 on DC01"),
                    pass("DD002", "VIRT", "vda 4096 MiB on DC02"),
                ],
            ),
            (
                "Communications".into(),
                vec![
                    pass("CMN01", "QN01-001", "Virtio Ethernet port eth0 52:54:00:12:34:56 virtio_net, link up"),
                    pass("CMN03", "QV01-001", "virtual channel org.qemu.guest_agent.0 (vport1p1): the agent's."),
                ],
            ),
            (
                "Console".into(),
                vec![
                    pass("CMN02", "QS01-001", "serial port ttyS0: the service console runs here."),
                    pass("FB0", "", "Framebuffer simpledrmdrmfb 1280x800 at 32 bits."),
                ],
            ),
        ],
    }
    .lines()
}

#[test]
fn hardware_qualification_from_the_media() {
    let mut e = env();
    e.qual = qual_report();
    let mut r = svctools::install::lic::start();
    r.enter(&f(&["4"]), &mut e);
    check("hwqual", &r.screen(&mut e).dump());
    r.enter(&f(&["1"]), &mut e);
    check("hwqual-report", &r.screen(&mut e).dump());
    r.function(8, &mut e);
    check("hwqual-report-2", &r.screen(&mut e).dump());
    r.function(12, &mut e);
    // No stick: the error says so; with one, where it went.
    r.enter(&f(&["2"]), &mut e);
    assert!(r.screen(&mut e).dump().contains("No USB device"));
    e.usb = Some("The report was written to QSHWQUAL.TXT on sdb1 (Cruzer Blade).".into());
    r.enter(&f(&["2"]), &mut e);
    check("hwqual-usb", &r.screen(&mut e).dump());
    assert_eq!(e.actions.iter().filter(|a| *a == "write report to USB").count(), 2);
    // An environment without the run says so.
    let mut none = MemEnv::new("QS400");
    let mut r = svctools::install::lic::start();
    r.enter(&f(&["4"]), &mut none);
    r.enter(&f(&["1"]), &mut none);
    assert!(r.screen(&mut none).dump().contains("not available here"));
}
