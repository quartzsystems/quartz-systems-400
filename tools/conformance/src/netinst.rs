// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Install step 15: a network install (docs/INSTALL.md, "Network
//! install").

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
            image: Some(disk),
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
            s.expect("slic: install: ADDTCPIFC INTNETADR(*IP4DHCP) LIND(ETH0)", Duration::from_mins(20))?;
            s.expect("slic: install: response file applied", Duration::from_mins(5))?;
            s.expect(SRC_IPL_COMPLETE, Duration::from_mins(5))?;
            s.expect(crate::PROMPT, CMD)?;
            let name = s.command("svc.sysval QSYSNAME", CMD)?;
            if !name.contains("QSNET") {
                return Err(format!("the network answers were not applied: {name}"));
            }
            // TCP/IP starts the *IP4DHCP interface; QEMU leases it an address.
            // BLOCKED(phase12): under QEMU (2026-11-04) the install and IPL pass but TCP/IP stays Inactive after QSTRUP (eth0 keeps the installer's 10.0.2.50); STRTCP's failure there is not yet seen.
            let end = Instant::now() + Duration::from_secs(120);
            let ifc = loop {
                let ifc = s.command("svc.tcp ifc", CMD)?;
                if ifc.contains("lease *IP4DHCP") {
                    break ifc;
                }
                if Instant::now() > end {
                    return Err(format!("the installed system's *IP4DHCP interface has no lease:\n{ifc}"));
                }
                std::thread::sleep(Duration::from_secs(1));
            };
            let lease = ifc.lines().find(|l| l.starts_with("lease ")).unwrap_or_default().to_string();
            Ok(format!("PXE boot over TFTP; INSTALL.RSP beside the boot file; QSYSNAME QSNET; {lease}"))
        })()
        .map_err(|e| (e, s.log.clone()));
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(30));
        report(name, start, r)
    }
}
