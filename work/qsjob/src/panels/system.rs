// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! System and security displays: `DSPSYSVAL`/`WRKSYSVAL`,
//! `DSPSFWRSC`, `DSPNETA`, `DSPAUTUSR`, `DSPUSRPRF`/`WRKUSRPRF`,
//! `WRKCFGSTS`, `DSPDEVD`, `DSPAUTL`, `WRKAUTL`, `DSPSECAUD`,
//! `DSPSGNINF`, the Information Assistant. `TODO(phase8)`: the drivers.

use cmd::Resolved;
use mi::Job;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// The driver of a system or security command, if it has one.
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
