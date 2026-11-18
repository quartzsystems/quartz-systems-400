// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 11 QEMU step: save files and the library and object saves and
//! restores at the service console.

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

fn want(out: &str, what: &str, needles: &[&str]) -> Result<(), String> {
    if needles.iter().all(|n| out.contains(n)) {
        Ok(())
    } else {
        Err(format!("{what}: expected {needles:?}:\n{out}"))
    }
}

// The steps share one signature so `step` can take them as fn pointers.
#[allow(clippy::unused_self)]
impl Suite {
    /// Step 71: `CRTSAVF`, `SAVLIB` to it, `RSTLIB` to a new name with the
    /// data intact, a non-empty save file refused without `CLEAR`, and
    /// `SAVOBJ`/`RSTOBJ` of one object.
    pub(crate) fn save_restore(&self, s: &mut Session) -> Result<String, String> {
        s.command("crtlib LIB(QS71)", CMD)?;
        s.command(
            "crtdtaara DTAARA(QS71/DA1) TYPE(*CHAR) LEN(20) VALUE('kept value')",
            CMD,
        )?;
        s.command("crtpf FILE(QS71/CUST) RCDLEN(40)", CMD)?;
        let made = s.command("crtsavf FILE(QGPL/S71)", CMD)?;
        if made.contains("CPF") {
            return Err(format!("CRTSAVF: {made}"));
        }
        let saved = s.command("savlib LIB(QS71) DEV(*SAVF) SAVF(QGPL/S71)", CMD)?;
        want(&saved, "SAVLIB", &["from library QS71 were saved to save file S71"])?;
        let again = s.command("savlib LIB(QS71) DEV(*SAVF) SAVF(QGPL/S71)", CMD)?;
        want(&again, "SAVLIB without CLEAR", &["CPF3794"])?;
        let rst = s.command(
            "rstlib SAVLIB(QS71) DEV(*SAVF) SAVF(QGPL/S71) RSTLIB(QS71B)",
            CMD,
        )?;
        want(&rst, "RSTLIB", &["restored to library QS71B"])?;
        let v = s.command("svc.dtaara get QS71B/DA1", CMD)?;
        want(&v, "restored data area", &["kept value"])?;
        let out = s.command(
            "savlib LIB(QS71) DEV(*SAVF) SAVF(QGPL/S71) CLEAR(*ALL) OUTPUT(*OUTFILE) OUTFILE(QGPL/S71OUT)",
            CMD,
        )?;
        want(
            &out,
            "SAVLIB OUTPUT(*OUTFILE)",
            &["from library QS71 were saved"],
        )?;
        let ffd = s.command("dspffd FILE(QGPL/S71OUT)", CMD)?;
        want(&ffd, "the QSRSAV outfile", &["SROCMD", "SROPSN"])?;
        let one = s.command(
            "savobj OBJ(DA1) LIB(QS71) DEV(*SAVF) SAVF(QGPL/S71) CLEAR(*ALL)",
            CMD,
        )?;
        want(&one, "SAVOBJ", &["from library QS71 were saved"])?;
        let none = s.command(
            "rstobj OBJ(DA1) SAVLIB(QS71) DEV(*SAVF) SAVF(QGPL/S71) RSTLIB(QS71C)",
            CMD,
        )?;
        want(&none, "RSTOBJ to a missing library", &["CPF3773", "QS71C"])?;
        s.command("crtlib LIB(QS71C)", CMD)?;
        let obj = s.command(
            "rstobj OBJ(DA1) SAVLIB(QS71) DEV(*SAVF) SAVF(QGPL/S71) RSTLIB(QS71C)",
            CMD,
        )?;
        want(&obj, "RSTOBJ", &["restored to library QS71C"])?;
        let v = s.command("svc.dtaara get QS71C/DA1", CMD)?;
        want(&v, "object restored alone", &["kept value"])?;
        for l in ["QS71", "QS71B", "QS71C"] {
            s.command(&format!("dltlib LIB({l})"), CMD).ok();
        }
        s.command("svc.obj dlt *SAVF QGPL/S71", CMD).ok();
        s.command("dltf FILE(QGPL/S71OUT)", CMD).ok();
        Ok("SAVLIB, RSTLIB to a new name, SAVOBJ and RSTOBJ through a save file".into())
    }
}
