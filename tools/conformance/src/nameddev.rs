// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 172: display sessions that name their device (`DEVNAME`). A
//! session names `S172DSP`: the device is made and shows the sign-on
//! display; after it ends a second session gets the same device.
//! Under `QMAXSGNACN(1)`, `QMAXSIGN` wrong passwords at it vary the
//! device off: `WRKCFGSTS` says so, `QSYSOPR` has `CPF1397`, the
//! profile stays enabled, and a session naming the device is closed.
//! `VRYCFG STATUS(*ON)` brings it back.

use std::time::{Duration, Instant};

use tn5250_driver::script::Context;
use tn5250_driver::session::Session as Display;
use tn5250_driver::{Connect, ScriptOptions};

use crate::phase5::SECOFR_PASSWORD;
use crate::phase8::{run, secofr};
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The device the step names.
const DEVICE: &str = "S172DSP";
/// The user whose sign-ons fail.
const USER: &str = "S172USR";

/// A session naming [`DEVICE`], at its sign-on display; `Err` when the
/// system closes it or shows none in time.
fn named(port: u16) -> Result<Display, String> {
    let mut c = Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
    c.device = DEVICE.into();
    let mut d = Display::connect(&c).map_err(|e| format!("{e:#}"))?;
    let until = Instant::now() + Duration::from_secs(15);
    while Instant::now() < until {
        d.pump().map_err(|e| format!("{e:#}"))?;
        if d.term.contains("Sign On") {
            return Ok(d);
        }
    }
    Err("no sign-on display".into())
}

/// Wait until `WRKCFGSTS` shows [`DEVICE`] with `status`.
fn status(s: &mut Session, status: &str) -> Result<(), String> {
    let mut last = String::new();
    for _ in 0..60 {
        last = s.command("WRKCFGSTS CFGTYPE(*DEV)", CMD)?;
        if last
            .lines()
            .any(|l| l.starts_with(DEVICE) && l.contains(status))
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(format!("{DEVICE} is not {status}:\n{last}"))
}

impl Suite {
    /// Step 172: named display sessions.
    pub(crate) fn named_displays(&self, s: &mut Session) -> Result<String, String> {
        let port = self.telnet_port().ok_or("the launch has no telnet port")?;
        let connect = Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
        let options = ScriptOptions::default();
        let mut ctx = Context::new(&connect, &options);
        // The first session makes the device.
        let mut d = named(port).map_err(|e| format!("the first session naming {DEVICE}: {e}"))?;
        if !d.term.contains(DEVICE) {
            return Err(format!(
                "the sign-on display does not name {DEVICE}:\n{}",
                d.term.text()
            ));
        }
        status(s, "SIGNON DISPLAY")?;
        d.disconnect();
        drop(d);
        status(s, "VARIED ON")?;
        // The device exists now: a second session gets it, signs on and off.
        let mut d = named(port).map_err(|e| format!("a second session naming {DEVICE}: {e}"))?;
        secofr(&mut d, &mut ctx)?;
        run(
            &mut d,
            &mut ctx,
            "wait \"Main Menu\"\ntype signoff\nkey enter\nwait \"Sign On\"\n",
        )?;
        d.disconnect();
        drop(d);
        status(s, "VARIED ON")?;
        // Wrong passwords up to QMAXSIGN vary the device off (action 1).
        s.command(
            &format!("CRTUSRPRF USRPRF({USER}) PASSWORD(S172Right1)"),
            CMD,
        )?;
        s.command("CHGSYSVAL SYSVAL(QMAXSIGN) VALUE(3)", CMD)?;
        s.command("CHGSYSVAL SYSVAL(QMAXSGNACN) VALUE(1)", CMD)?;
        let outcome = varied_off(s, port, &mut ctx);
        s.command("CHGSYSVAL SYSVAL(QMAXSGNACN) VALUE(3)", CMD)?;
        let _ = s.command(
            &format!("VRYCFG CFGOBJ({DEVICE}) CFGTYPE(*DEV) STATUS(*ON)"),
            CMD,
        );
        let _ = s.command(&format!("DLTUSRPRF USRPRF({USER})"), CMD);
        outcome?;
        Ok(format!(
            "{DEVICE} made by its first session and given to the next; 3 wrong passwords varied it off (WRKCFGSTS, CPF1397), {USER} stayed enabled, a session naming it was closed; VRYCFG *ON brought it back"
        ))
    }
}

/// The vary-off, its signs, the refusal and the recovery.
fn varied_off(s: &mut Session, port: u16, ctx: &mut Context) -> Result<(), String> {
    let mut d = named(port).map_err(|e| format!("the session for the wrong passwords: {e}"))?;
    for _ in 0..3 {
        // The last one may close the session.
        let _ = run(
            &mut d,
            ctx,
            &format!("signon {USER} S172Wrong9\nsettle 1500\n"),
        );
    }
    d.disconnect();
    drop(d);
    status(s, "VARIED OFF")?;
    let opr = s.command("dspmsg MSGQ(QSYSOPR)", CMD)?;
    if !opr
        .lines()
        .any(|l| l.contains(DEVICE) && l.contains("varied off"))
    {
        return Err(format!("QSYSOPR has no CPF1397 for {DEVICE}:\n{opr}"));
    }
    let prf = s.command(&format!("dspusrprf USRPRF({USER})"), CMD)?;
    if !prf.contains("*ENABLED") {
        return Err(format!(
            "QMAXSGNACN(1) disabled {USER} as well as the device:\n{prf}"
        ));
    }
    if let Ok(d) = named(port) {
        return Err(format!(
            "a session got {DEVICE} while it is varied off:\n{}",
            d.term.text()
        ));
    }
    // The refusal leaves it off; varying it on brings it back.
    status(s, "VARIED OFF")?;
    let on = s.command(
        &format!("VRYCFG CFGOBJ({DEVICE}) CFGTYPE(*DEV) STATUS(*ON)"),
        CMD,
    )?;
    status(s, "VARIED ON").map_err(|e| format!("{e}\nVRYCFG said:\n{on}"))?;
    let mut d =
        named(port).map_err(|e| format!("a session naming {DEVICE} after VRYCFG *ON: {e}"))?;
    d.disconnect();
    drop(d);
    status(s, "VARIED ON")
}
