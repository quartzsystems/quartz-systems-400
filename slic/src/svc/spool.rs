// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Output queue and spooled file commands of the service console.

use crate::ipl::system::System;
use crate::stgsvc::splcmd;

/// Help for these commands.
pub const HELP: &str = "\
crtoutq LIB/Q [SEQ() MAXPAGES() DSPDTA() TEXT()] | chgoutq Q ... | dltoutq Q
hldoutq Q | rlsoutq Q | clroutq Q | wrkoutq [LIB/Q] | wrkoutqd Q
wrksplf [SELECT(user|*ALL)] [JOB()] [VIEW(1|2|3)] | wrksplfa FILE [JOB() SPLNBR()]
dspsplf FILE [JOB(n/u/name) SPLNBR(n|*LAST)] [VIEW(*TEXT|*FCFC|*HEX)] [PAGE(n)]
cpysplf FILE TOFILE(LIB/F) [JOB() SPLNBR() TOMBR() MBROPT(*REPLACE|*ADD) CTLCHAR(*NONE|*FCFC|*PRTCTL)]
chgsplfa FILE [JOB() SPLNBR()] [OUTQ() COPIES() OUTPTY() FORMTYPE() USRDTA() SAVE() ...]
hldsplf FILE [JOB() SPLNBR()] | rlssplf FILE ... | dltsplf FILE ...
";

/// Dispatch; `None` when `cmd` is not one of these.
pub fn command(system: &mut System, cmd: &str, line: &str) -> Option<String> {
    let me = system.current_job;
    splcmd::command(system, me, cmd, line).map(|r| match r {
        Ok(s) => s,
        Err(e) => format!("{e}\n"),
    })
}
