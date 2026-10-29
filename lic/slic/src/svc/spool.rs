// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Output queue and spooled file commands of the service console.

use crate::ipl::system::System;
use crate::stgsvc::{prtdev, splcmd, wtr};

/// Help for these commands.
pub const HELP: &str = "\
crtoutq LIB/Q [SEQ() MAXPAGES() DSPDTA() TEXT()] | chgoutq Q ... | dltoutq Q
hldoutq Q | rlsoutq Q | clroutq Q | wrkoutq [LIB/Q] | wrkoutqd Q
wrksplf [SELECT(user|*ALL)] [JOB()] [VIEW(1|2|3)] | wrksplfa FILE [JOB() SPLNBR()]
dspsplf FILE [JOB(n/u/name) SPLNBR(n|*LAST)] [VIEW(*TEXT|*FCFC|*HEX)] [PAGE(n)]
cpysplf FILE TOFILE(LIB/F) [JOB() SPLNBR() TOMBR() MBROPT(*REPLACE|*ADD) CTLCHAR(*NONE|*FCFC|*PRTCTL)]
chgsplfa FILE [JOB() SPLNBR()] [OUTQ() COPIES() OUTPTY() FORMTYPE() USRDTA() SAVE() ...]
hldsplf FILE [JOB() SPLNBR()] | rlssplf FILE ... | dltsplf FILE ...
strprtwtr DEV(QSCAPTURE) [OUTQ(*DEV|LIB/Q) WTR() FORMTYPE(*ALL) FILESEP(n) AUTOEND(*NO|*YES)]
strrmtwtr OUTQ(LIB/Q|*ALL) [WTR(*OUTQ|name) FORMTYPE(*ALL) AUTOEND(*NO|*YES)]
crtdevprt DEVD() DEVCLS(*LAN) TYPE(3812) MODEL(1) LANATTACH(*IP) PORT(9100) RMTLOCNAME(host) [SYSDRVPGM() TRANSFORM() MFRTYPMDL()] | chgdevprt DEVD() ...
crtwscst WSCST(LIB/N) [SRCFILE(LIB/QTXTSRC) SRCMBR()] | rtvwscst DEVTYPE(*TRANSFORM) MFRTYPMDL(*HP4) SRCMBR() [SRCFILE()]
endwtr WTR [OPTION(*CNTRLD|*IMMED)] | hldwtr WTR | rlswtr WTR | chgwtr WTR [OUTQ() FORMTYPE()] | wrkwtr
prtcapture list | prtcapture get FILE [JOB() SPLNBR() FORMAT(*TEXT|*PDF)] | prtcapture clear
";

/// Dispatch; `None` when `cmd` is not one of these.
pub fn command(system: &mut System, cmd: &str, line: &str) -> Option<String> {
    let me = system.current_job;
    let p = wm::cmd::Params::parse(line);
    let r = match cmd {
        "strprtwtr" => Some(wtr::strprtwtr(system, me, &p)),
        "endwtr" | "hldwtr" | "rlswtr" | "chgwtr" => Some(wtr::control(system, me, cmd, &p)),
        "wrkwtr" => Some(Ok(wtr::wrkwtr(system))),
        "prtcapture" => Some(wtr::prtcapture(system, line)),
        "strrmtwtr" => Some(wtr::strrmtwtr(system, me, &p)),
        "crtdevprt" | "chgdevprt" => Some(prtdev::crt_chg(system, me, cmd, &p)),
        "crtwscst" => Some(prtdev::crtwscst(system, me, &p)),
        "rtvwscst" => Some(prtdev::rtvwscst(system, &p)),
        _ => splcmd::command(system, me, cmd, line),
    };
    r.map(|r| match r {
        Ok(s) => s,
        Err(e) => format!("{e}\n"),
    })
}
