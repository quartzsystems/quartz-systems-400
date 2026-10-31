// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Displays whose data is a report: the command's own processing
//! program runs in the storage service, and its lines show in the
//! report viewer (`QUIRPTVW`), never on a menu's message line.

use cmd::Resolved;
use mi::Job;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// The title of a command's report.
fn title(name: &str) -> &'static str {
    match name {
        "DSPUDFS" => "Display User-Defined FS",
        _ => "Display Report",
    }
}

/// Run the command's processing program in the service and show its
/// report.
pub fn show(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let mut out = Outcome::default();
    let text = crate::command::service(job, host, r, &mut out)?;
    crate::panels::system2::view_report(host, surface, title(&r.name), "QSYSPRT", &text)
}
