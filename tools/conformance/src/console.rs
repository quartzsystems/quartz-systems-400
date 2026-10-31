// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 84: the framebuffer console is a 5250 display without TCP/IP.
//! With TCP/IP ended, the console signs on, `WRKHDWRSC TYPE(*CMN)`
//! from the main menu opens its panel, `MAIN` option 1 then 2 opens
//! Display Messages, `GO USER` typed on the command line shows the user
//! menu, and a dropped console link gives the job back at the next
//! sign-on. The console is driven through the service console's
//! `screen`, `key` and `type`.

use std::time::Duration;

use crate::phase5::{SECOFR_PASSWORD, console_sign_on_as, screen_wait};
use crate::qemu::session::Session;
use crate::steps::Suite;

const CMD: Duration = Duration::from_secs(10);

/// Type a line on the console and press Enter.
fn enter(s: &mut Session, text: &str) -> Result<(), String> {
    s.command(&format!("type {text}"), CMD)?;
    s.command("key enter", CMD).map(|_| ())
}

/// The console's message line (row 24) of a `screen` dump.
fn message_line(screen: &str) -> String {
    screen
        .lines()
        .filter(|l| l.chars().count() == 80)
        .nth(23)
        .unwrap_or("")
        .trim()
        .to_string()
}

/// `F12` until the main menu shows (`F3` there would sign off).
fn back_to_main(s: &mut Session) -> Result<(), String> {
    for _ in 0..4 {
        let screen = s.command("screen", CMD)?;
        if screen.contains("Main Menu") {
            return Ok(());
        }
        s.command("key f12", CMD)?;
        std::thread::sleep(Duration::from_millis(800));
    }
    screen_wait(s, "Main Menu").map(|_| ())
}

/// QSECOFR signed on at the console, at the main menu.
fn sign_on_main(s: &mut Session) -> Result<(), String> {
    let mut first = console_sign_on_as(s, "QSECOFR", SECOFR_PASSWORD, SECOFR_PASSWORD);
    if first.as_ref().is_err_and(|e| e.contains("not correct")) {
        first = console_sign_on_as(s, "QSECOFR", "QSECOFR", SECOFR_PASSWORD);
    }
    if !first?.contains("Main Menu") {
        // Command Entry: GO there reaches the main menu.
        enter(s, "go main")?;
    }
    screen_wait(s, "Main Menu").map(|_| ())
}

impl Suite {
    /// Step 84: the console display without TCP/IP.
    pub(crate) fn console_display(&self, s: &mut Session) -> Result<String, String> {
        // TCP/IP ended (or never started): nothing below may need it.
        let ended = s.command("ENDTCP", CMD)?;
        std::thread::sleep(Duration::from_secs(2));
        let status = s.command("tn5250 status", CMD)?;
        if !status.contains("attached") || status.contains("not attached") {
            return Err(format!("the console is not attached to its session: {status}"));
        }
        let r = self.console_checks(s);
        // TCP/IP again for whatever runs next, the way it was.
        if ended.contains("TCP1A02") {
            let _ = s.command("STRTCP", CMD);
        }
        r
    }

    #[allow(clippy::unused_self)] // every step has the same shape
    fn console_checks(&self, s: &mut Session) -> Result<String, String> {
        sign_on_main(s)?;
        // WRKHDWRSC opens its panel: no text on the menu's message line.
        enter(s, "WRKHDWRSC TYPE(*CMN)")?;
        let rsc = screen_wait(s, "Work with Communication Resources").map_err(|e| {
            format!("WRKHDWRSC TYPE(*CMN) did not open its panel: {e}")
        })?;
        if rsc.contains("is answered by") {
            return Err(format!("WRKHDWRSC answered in text:\n{rsc}"));
        }
        s.command("key f3", CMD)?;
        screen_wait(s, "Main Menu")?;
        // MAIN option 1 (User Tasks), then 2 (DSPMSG).
        enter(s, "1")?;
        screen_wait(s, "User Tasks")?;
        enter(s, "2")?;
        screen_wait(s, "Display Messages").map_err(|e| {
            format!("User Tasks option 2 did not open Display Messages: {e}")
        })?;
        back_to_main(s)?;
        // GO USER typed on MAIN's command line.
        enter(s, "GO USER")?;
        screen_wait(s, "User Tasks").map_err(|e| format!("GO USER: {e}"))?;
        back_to_main(s)?;
        // The link drops: the job waits; signing on again brings it back.
        s.command("tn5250 disconnect", CMD)?;
        std::thread::sleep(Duration::from_secs(1));
        s.command("tn5250 connect", CMD)?;
        screen_wait(s, "Sign On")?;
        console_sign_on_as(s, "QSECOFR", SECOFR_PASSWORD, SECOFR_PASSWORD)
            .map_err(|e| format!("after the link dropped: {e}"))?;
        screen_wait(s, "Main Menu").map_err(|e| format!("the job did not come back: {e}"))?;
        // GO to a menu that is not there: CPF9801 on the menu, the
        // command kept on the line.
        enter(s, "GO QZZZNOMNU")?;
        std::thread::sleep(Duration::from_millis(1500));
        let missing = screen_wait(s, "Main Menu")?;
        let line = message_line(&missing);
        if !line.contains("QZZZNOMNU") {
            return Err(format!("GO QZZZNOMNU said nothing on the menu:\n{missing}"));
        }
        // The kept command cleared, then sign off.
        s.command("key home", CMD)?;
        s.command("key fieldexit", CMD)?;
        enter(s, "signoff")?;
        screen_wait(s, "Sign On")?;
        Ok(format!(
            "without TCP/IP: WRKHDWRSC panel, DSPMSG from USER option 2, GO USER, {line:?}; job back after the link dropped"
        ))
    }
}
