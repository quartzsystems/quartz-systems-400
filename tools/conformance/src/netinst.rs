// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Install step 15: a network install (docs/INSTALL.md, "Network
//! install").

use std::fmt::Write as _;
use std::fs;
use std::time::{Duration, Instant};

use crate::error::HarnessError;
use crate::install::{GIB, blank, report};
use crate::qemu::Launch;
use crate::qemu::session::Session;
use crate::steps::{CMD, SRC_IPL_COMPLETE, StepReport, Suite};

/// The response file step 15 serves beside the boot file.
pub const RSP_NETWORK: &str = "QS400-INSTALL-RSP 1
# the harness's network install: answers over TFTP, the address from DHCP
lic-option = 2
system-name = QSNET
ip = *IP4DHCP
qsecofr-password = Quartz1
licpgm = *NONE
";

impl Suite {
    /// Install step 15: a network boot. QEMU's DHCP server names the
    /// media UKI on its TFTP server, the firmware boots it, the IPL asks
    /// DHCP again and fetches `INSTALL.RSP` beside the boot file; the
    /// install takes its payload from the CD and its answers from the
    /// network, and the installed system gets its address from DHCP.
    pub(crate) fn network_install(&self) -> Result<StepReport, HarnessError> {
        let name = "15 network install";
        let start = Instant::now();
        if self.skipped(name) {
            return Ok(StepReport { name, elapsed: Duration::ZERO, detail: "skipped".into() });
        }
        let Some(iso_path) = self.iso.clone() else {
            return Ok(StepReport { name, elapsed: Duration::ZERO, detail: "skipped: no media".into() });
        };
        let iso = fs::read(&iso_path)?;
        let files = crate::media::iso_files(&iso).map_err(|reason| HarnessError::Failed {
            step: name,
            reason,
            log: String::new(),
        })?;
        let uki = files
            .iter()
            .find(|(p, _)| p.eq_ignore_ascii_case("EFI/BOOT/BOOTX64.EFI"))
            .map(|(_, b)| b.clone())
            .ok_or_else(|| HarnessError::Failed {
                step: name,
                reason: "the media has no EFI/BOOT/BOOTX64.EFI".into(),
                log: String::new(),
            })?;
        let tftp = self.out_dir.join("pxe");
        fs::create_dir_all(tftp.join("qs400"))?;
        fs::write(tftp.join("qs400/BOOTX64.EFI"), uki)?;
        fs::write(tftp.join("qs400/INSTALL.RSP"), RSP_NETWORK)?;
        let disk = self.out_dir.join("pxe-disk.img");
        blank(&disk, 4 * GIB)?;
        let launch = Launch {
            image: Some(disk.clone()),
            cdrom: Some(iso_path),
            cdrom_first: false,
            tftp: Some((tftp, "qs400/BOOTX64.EFI".into())),
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&launch)?;
        let r = (|| -> Result<String, String> {
            s.expect("slic: eth0 dhcp", self.boot_timeout * 3)?;
            s.expect("slic: install: response file from tftp://10.0.2.2/qs400/INSTALL.RSP", self.boot_timeout)?;
            s.expect("slic: install: answers from the response file", self.boot_timeout)?;
            s.expect("SRC C6004090", Duration::from_mins(5))?;
            s.expect("SRC C6004100", self.boot_timeout * 2)?;
            s.expect(
                "slic: install: ADDTCPIFC INTNETADR(*IP4DHCP) LIND(ETH0) AUTOSTART(*YES)",
                Duration::from_mins(20),
            )?;
            s.expect("slic: install: response file applied", Duration::from_mins(5))?;
            s.expect(SRC_IPL_COMPLETE, Duration::from_mins(5))?;
            s.expect(crate::PROMPT, CMD)?;
            let name = s.command("svc.sysval QSYSNAME", CMD)?;
            if !name.contains("QSNET") {
                return Err(format!("the network answers were not applied: {name}"));
            }
            // QSTRUP's STRTCP starts the autostarted *IP4DHCP interface;
            // QEMU's DHCP server leases it an address.
            leased(&mut s)
        })()
        .map_err(|e| (e, s.log.clone()));
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(30));
        drop(s);
        let first = match r {
            Ok(l) => l,
            Err(e) => return report(name, start, Err(e)),
        };
        // The disk alone: no network boot, so no address from the IPL;
        // the kept configuration brings the interface up again.
        let alone = Launch {
            image: Some(disk),
            cdrom: None,
            cdrom_first: false,
            tftp: None,
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&alone)?;
        let r = (|| -> Result<String, String> {
            s.expect(SRC_IPL_COMPLETE, self.boot_timeout * 2)?;
            s.expect(crate::PROMPT, CMD)?;
            if s.log.contains("slic: eth0 dhcp") {
                return Err("the IPL from the disk asked DHCP itself".into());
            }
            let again = leased(&mut s)?;
            Ok(format!(
                "PXE boot over TFTP; INSTALL.RSP beside the boot file; QSYSNAME QSNET; \
                 *IP4DHCP AUTOSTART(*YES) started by QSTRUP: {first}; after an IPL from the disk: {again}"
            ))
        })()
        .map_err(|e| (e, s.log.clone()));
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(30));
        report(name, start, r)
    }
}

/// TCP/IP active with the `*IP4DHCP` interface holding a lease, within
/// two minutes of sign-on; the lease line.
fn leased(s: &mut Session) -> Result<String, String> {
    let end = Instant::now() + Duration::from_secs(120);
    let ifc = loop {
        let ifc = s.command("svc.tcp ifc", CMD)?;
        if ifc.contains("TCP/IP Active") && ifc.contains("lease *IP4DHCP") {
            break ifc;
        }
        if Instant::now() > end {
            let mut diag = String::new();
            for c in ["svc.sysval QSTRUPPGM", "wrksbs", "dsplog"] {
                let _ = write!(diag, "=== {c}\n{}\n", s.command(c, CMD).unwrap_or_default());
            }
            return Err(format!("the installed system's *IP4DHCP interface has no lease:\n{ifc}\n{diag}"));
        }
        std::thread::sleep(Duration::from_secs(1));
    };
    if !ifc.lines().any(|l| l.starts_with("ifc *IP4DHCP ") && l.contains(" Active ")) {
        return Err(format!("the *IP4DHCP interface is not active:\n{ifc}"));
    }
    Ok(ifc.lines().find(|l| l.starts_with("lease ")).unwrap_or_default().to_string())
}
