// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSHWQUAL` from the install media (`docs/QUALIFIED_HARDWARE.md`):
//! the report on a paged display, and written to a USB stick for the
//! qualification record. The checks are the environment's
//! ([`DstEnv::hardware_report`]).

use crate::dst::common::{Notice, Submenu};
use crate::env::DstEnv;
use crate::panel::Flow;

/// The `Hardware Qualification` menu.
#[must_use]
pub fn menu() -> Submenu {
    Submenu::new(
        "hwqual",
        "Hardware Qualification",
        vec![
            (
                1,
                "Display hardware qualification report",
                Some(Box::new(display)),
            ),
            (2, "Write report to USB device", Some(Box::new(usb))),
        ],
    )
    .with_lead(vec![
        "QSHWQUAL checks the processors, main storage, controllers and".to_string(),
        "disks, network ports, console, TPM and firmware of this machine.".to_string(),
        String::new(),
    ])
}

fn display(env: &mut dyn DstEnv) -> Flow {
    match env.hardware_report() {
        Ok(lines) => Flow::Push(Box::new(Notice::new(
            "hwqual-report",
            "Hardware Qualification Report",
            lines,
        ))),
        Err(e) => Flow::Error(e),
    }
}

fn usb(env: &mut dyn DstEnv) -> Flow {
    match env.write_report_usb() {
        Ok(done) => Flow::Message(done),
        Err(e) => Flow::Error(e),
    }
}
