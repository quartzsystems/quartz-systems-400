// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Steps 207 to 210: the system's console over QSI while the local
//! display keeps it (docs/QSID.md section 12), in the guest of 201.
//!
//! - 207: the sign-on display as the viewer's frames, equal to its
//!   golden and to the local display at the same time.
//! - 208: a sign-on typed from the viewer as keys, to a menu, which the
//!   local display shows too.
//! - 209: a second viewer refused `QSI0601`; with take over the first is
//!   ended and the second's keys run a command.
//! - 210: the screen alone, without attaching.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use qsi::any::{AnyReply, AnyRequest};
use qsi::host::console;
use qsi::host::model::SYSTEM_ID;
use qsipart::console::Key;

use super::view::View;
use super::{ask, enter, strqsipair, with};
use crate::partition::golden;
use crate::phase5::{SECOFR_PASSWORD, screen_any, screen_wait};
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The steps, in order.
pub(crate) const NAMES: [&str; 4] = [
    "207 QSI console sign-on frames",
    "208 QSI console sign-on by keys",
    "209 QSI console take over",
    "210 QSI console screen",
];

/// The viewer the steps hand on.
static VIEW: Mutex<Option<View>> = Mutex::new(None);

fn view() -> MutexGuard<'static, Option<View>> {
    VIEW.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The local display's 24 rows (the last 24 lines `screen` printed),
/// trailing blanks dropped.
fn local(s: &mut Session) -> Result<Vec<String>, String> {
    let got = s.command("screen", CMD)?;
    let raw: Vec<&str> = got.lines().collect();
    Ok(raw[raw.len().saturating_sub(24)..]
        .iter()
        .map(|l| l.trim_end().to_string())
        .collect())
}

fn rows(text: &str) -> Vec<String> {
    text.lines().map(|l| l.trim_end().to_string()).collect()
}

/// The viewer's grid and the local display row for row.
fn same(grid: &str, local: &[String]) -> Result<(), String> {
    let g = rows(grid);
    if g != local {
        let at = g
            .iter()
            .zip(local)
            .position(|(a, b)| a != b)
            .unwrap_or(g.len().min(local.len()));
        return Err(format!(
            "the viewer and the local display differ at row {}:\n  viewer: {:?}\n  local:  {:?}",
            at + 1,
            g.get(at),
            local.get(at)
        ));
    }
    Ok(())
}

/// The local display, once it holds `want`, against the viewer's grid
/// once that holds it.
fn both_show(s: &mut Session, v: &mut View, want: &str) -> Result<String, String> {
    let text = v.wait_text(want, CMD)?;
    screen_wait(s, want)?;
    let mut last = Err(String::new());
    // The two are looked at a moment apart: a few tries.
    for _ in 0..10 {
        v.pump(Duration::from_millis(300))?;
        last = same(&v.grid.text(), &local(s)?);
        if last.is_ok() {
            return Ok(v.grid.text());
        }
    }
    last.map(|()| text)
}

/// Sign on at the viewer by its keys, whatever state QSECOFR's password
/// is in; leaves the first menu showing.
fn sign_on(v: &mut View) -> Result<(), String> {
    for pw in [SECOFR_PASSWORD, "QSECOFR"] {
        v.key(Key::Home)?;
        v.typed("QSECOFR")?;
        v.key(Key::FieldExit)?;
        v.typed(pw)?;
        v.key(Key::FieldExit)?;
        v.key(Key::Enter)?;
        let (i, _) = v.wait_any(
            &[
                "===>",
                "Change Password",
                "not correct",
                "Sign-on Information",
            ],
            CMD,
        )?;
        match i {
            0 => return Ok(()),
            1 => {
                v.typed(pw)?;
                v.key(Key::FieldAdvance)?;
                v.typed(SECOFR_PASSWORD)?;
                v.key(Key::FieldAdvance)?;
                v.typed(SECOFR_PASSWORD)?;
                v.key(Key::Enter)?;
                let (i, _) = v.wait_any(&["===>", "Sign-on Information"], CMD)?;
                if i == 1 {
                    v.key(Key::Enter)?;
                }
                v.wait_text("===>", CMD)?;
                return Ok(());
            }
            3 => {
                v.key(Key::Enter)?;
                v.wait_text("===>", CMD)?;
                return Ok(());
            }
            _ => {}
        }
    }
    Err(format!("QSECOFR could not sign on:\n{}", v.grid.text()))
}

impl Suite {
    /// 207: the sign-on display as frames, as its golden and as the
    /// local display shows it.
    #[allow(clippy::unused_self)] // a step's signature
    pub(super) fn qsi_console_frames(&self, s: &mut Session) -> Result<String, String> {
        let (i, _) = screen_any(s, &["Sign On", "===>"])?;
        if i == 1 {
            enter(s, "SIGNOFF")?;
        }
        screen_wait(s, "Sign On")?;
        let mut v = with(|st| View::attach(&st.mc01, st.port, false))?
            .map_err(|id| format!("the attach was refused {id}"))?;
        let text = both_show(s, &mut v, "Sign On")?;
        if v.frames == 0 {
            return Err("no whole frame came first".into());
        }
        let long = text
            .lines()
            .nth(9)
            .is_some_and(|l| l.contains("Program/procedure"));
        let name = if long { "sign-on-pwdlvl2" } else { "sign-on" };
        golden("signon", name, &text, &[], 12).map_err(|e| format!("{e}\n{text}"))?;
        let shown = local(s)?.join("\n");
        golden("signon", name, &shown, &[], 12)
            .map_err(|e| format!("the local display: {e}\n{shown}"))?;
        let frames = v.frames;
        *view() = Some(v);
        Ok(format!(
            "MC01 attached to ID 0: a whole frame first ({frames} in all), the sign-on display as signon/{name}; the local display the same, row for row"
        ))
    }

    /// 208: a sign-on typed at the viewer.
    #[allow(clippy::unused_self)] // a step's signature
    pub(super) fn qsi_console_keys(&self, s: &mut Session) -> Result<String, String> {
        let mut g = view();
        let v = g.as_mut().ok_or("step 207 did not run")?;
        let before = v.updates;
        sign_on(v)?;
        let menu = both_show(s, v, "===>")?;
        let title = menu
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim()
            .to_string();
        Ok(format!(
            "QSECOFR signed on by the viewer's keys ({} updates) to {title:?}, on the local display too",
            v.updates - before
        ))
    }

    /// 209: one viewer at a time, take over.
    #[allow(clippy::unused_self)] // a step's signature
    pub(super) fn qsi_console_take_over(&self, s: &mut Session) -> Result<String, String> {
        let mut first = view().take().ok_or("step 207 did not run")?;
        // A second console pinned: MC02 from 204, or paired here.
        let listed = s.command("WRKQSICSL", CMD)?;
        if !listed.contains("MC02") {
            screen_wait(s, "===>")?;
            let (password, _) = strqsipair(s)?;
            with(|st| st.mc02.pair(st.port, &password).map(|_| ()))?;
        }
        let refused = with(|st| View::attach(&st.mc02, st.port, false))?;
        match refused {
            Err(id) if id == console::msgid::VIEWED => {}
            Err(id) => return Err(format!("the second attach: {id}, not QSI0601")),
            Ok(v) => {
                v.close();
                return Err("a second viewer was let in without take over".into());
            }
        }
        let mut second = with(|st| View::attach(&st.mc02, st.port, true))?
            .map_err(|id| format!("the take over was refused {id}"))?;
        let until = std::time::Instant::now() + CMD;
        while first.ended.is_none() || !first.closed {
            if std::time::Instant::now() > until {
                return Err(format!(
                    "the first viewer was not ended (ended {:?}, closed {})",
                    first.ended, first.closed
                ));
            }
            first.pump(Duration::from_millis(200))?;
        }
        let why = first.ended.clone().unwrap_or_default();
        first.close();
        both_show(s, &mut second, "===>")?;
        second.typed("DSPLIBL")?;
        second.key(Key::Enter)?;
        both_show(s, &mut second, "Library List")?;
        second.key(Key::Function(12))?;
        both_show(s, &mut second, "===>")?;
        *view() = Some(second);
        Ok(format!(
            "MC02 refused QSI0601 without take over; with it MC01 was ended ({why:?}) and its stream closed; MC02's keys ran DSPLIBL, shown on the local display too"
        ))
    }

    /// 210: the screen without attaching.
    #[allow(clippy::unused_self)] // a step's signature
    pub(super) fn qsi_console_screen(&self, s: &mut Session) -> Result<String, String> {
        let shown = local(s)?;
        let capture = with(|st| {
            let mut c = st.mc01.connect(st.port)?;
            let r = ask(
                &mut c,
                &AnyRequest::Console(console::Request::Screen(console::Screen { id: SYSTEM_ID })),
            )?;
            c.close();
            match r {
                Ok(AnyReply::Console(console::Reply::Screen(cap))) => Ok(cap),
                other => Err(format!("screen: {other:?}")),
            }
        })?;
        if capture.serial || capture.frame.is_none() {
            return Err(format!("the capture: {capture:?}"));
        }
        same(&capture.text, &shown)?;
        // The viewer attached throughout is still the console's.
        let still = view().take().map(|mut v| {
            let ok = v.pump(Duration::from_millis(500)).is_ok() && v.ended.is_none() && !v.closed;
            v.close();
            ok
        });
        if still == Some(false) {
            return Err("the screen request ended the viewer".into());
        }
        Ok("the console's screen alone, its frame and text equal to the local display; the viewer left attached".into())
    }
}
