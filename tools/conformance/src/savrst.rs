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
        want(&saved, "SAVLIB", &["CPC3722", "QS71"])?;
        let again = s.command("savlib LIB(QS71) DEV(*SAVF) SAVF(QGPL/S71)", CMD)?;
        want(&again, "SAVLIB without CLEAR", &["CPF3794"])?;
        let rst = s.command(
            "rstlib SAVLIB(QS71) DEV(*SAVF) SAVF(QGPL/S71) RSTLIB(QS71B)",
            CMD,
        )?;
        want(&rst, "RSTLIB", &["CPC3703", "QS71B"])?;
        let v = s.command("svc.dtaara get QS71B/DA1", CMD)?;
        want(&v, "restored data area", &["kept value"])?;
        let one = s.command(
            "savobj OBJ(DA1) LIB(QS71) DEV(*SAVF) SAVF(QGPL/S71) CLEAR(*ALL)",
            CMD,
        )?;
        want(&one, "SAVOBJ", &["CPC3722"])?;
        let none = s.command(
            "rstobj OBJ(DA1) SAVLIB(QS71) DEV(*SAVF) SAVF(QGPL/S71) RSTLIB(QS71C)",
            CMD,
        )?;
        want(&none, "RSTOBJ to a missing library", &["CPF3781"])?;
        s.command("crtlib LIB(QS71C)", CMD)?;
        let obj = s.command(
            "rstobj OBJ(DA1) SAVLIB(QS71) DEV(*SAVF) SAVF(QGPL/S71) RSTLIB(QS71C)",
            CMD,
        )?;
        want(&obj, "RSTOBJ", &["CPC3703"])?;
        let v = s.command("svc.dtaara get QS71C/DA1", CMD)?;
        want(&v, "object restored alone", &["kept value"])?;
        for l in ["QS71", "QS71B", "QS71C"] {
            s.command(&format!("dltlib LIB({l})"), CMD).ok();
        }
        s.command("svc.obj dlt *SAVF QGPL/S71", CMD).ok();
        Ok("SAVLIB, RSTLIB to a new name, SAVOBJ and RSTOBJ through a save file".into())
    }
}
