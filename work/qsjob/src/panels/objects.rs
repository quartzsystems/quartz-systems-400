// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Message and object displays: `DSPMSG`/`WRKMSG`, `DSPMSGD`,
//! `WRKMSGD`, Additional Message Information, `WRKRPYLE`, `DSPLOG`,
//! `WRKOBJ`, `DSPLIB`/`WRKLIB`, `DSPOBJD`, `DSPOBJAUT`/`EDTOBJAUT`,
//! `WRKBNDDIR`, `DSPLIBL`/`EDTLIBL`, `DSPDTAARA`, `WRKOBJOWN`,
//! `DSPFD`/`DSPFFD`/`DSPDBR`. `TODO(phase8)`: the drivers.

use cmd::Resolved;
use mi::Job;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// The driver of a message or object command, if it has one.
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
