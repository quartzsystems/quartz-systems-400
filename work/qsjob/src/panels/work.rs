// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work-management displays: `WRKACTJOB` and its views, `WRKUSRJOB`,
//! `WRKSBSJOB`, `WRKJOB` and its options, `DSPJOB`, `WRKSBS`,
//! `DSPSBSD`, `WRKJOBD`/`DSPJOBD`, `WRKCLS`/`DSPCLS`, `WRKSPLF`,
//! `WRKOUTQ`, `WRKWTR`, `WRKJOBQ`, `WRKJOBSCDE`, `WRKSYSSTS`,
//! `WRKDSKSTS`, `WRKSYSACT`, `WRKSHRPOOL`, `DSPJOBLOG`. `TODO(phase8)`:
//! the drivers.

use cmd::Resolved;
use mi::Job;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// The driver of a work-management command, if it has one.
#[allow(clippy::match_single_binding)] // the arms come with the drivers
pub fn run(
    _job: &mut Job,
    _host: &Host,
    _surface: &mut Surface,
    r: &Resolved,
    _out: &mut Outcome,
) -> Option<Result<String, Fail>> {
    match r.name.as_str() {
        _ => None,
    }
}
