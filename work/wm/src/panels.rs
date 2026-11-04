// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The panels an interactive job shows before and around its work:
//! Change Password (`QDCHGPWD`'s geometry), Sign-on Information, and
//! the interim MAIN menu that stands in when the `MAIN` menu object is
//! missing (the object itself shows through UIM). Golden files under
//! `tools/conformance/golden/`
//! hold them (`tests/golden.rs`).

use crate::ctl::Panel;

/// `MM/DD/YY` and `HH:MM:SS` of a Unix time.
#[must_use]
#[allow(clippy::many_single_char_names)] // the civil-date algorithm's names
pub fn date_time(t: u64) -> (String, String) {
    let days = i64::try_from(t / 86_400).unwrap_or(0);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let s = t % 86_400;
    (
        format!("{m:02}/{d:02}/{:02}", y % 100),
        format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60),
    )
}

/// Change Password: the current password, the new one twice, at IBM's
/// places (measured on 7.1 and 7.5). A field holds 10 characters at
/// password level 0 and 1; from level 2 (`long_password`) it holds 128,
/// runs on over the next two rows, and the labels are three rows apart
/// so that no field reaches the next label.
#[must_use]
pub fn change_password(user: &str, changed: u64, long_password: bool, message: &str) -> Panel {
    let mut p = Panel::default();
    p.text(1, 32, 1, "Change Password");
    p.text(3, 2, 0, "User profile  . . . . . . . . . . . . . . :");
    p.text(3, 47, 0, user);
    if changed != 0 {
        let (d, _) = date_time(changed);
        p.text(5, 2, 0, "Password last changed . . . . . . . . . . :");
        p.text(5, 47, 0, d);
    }
    p.text(7, 2, 2, "Type choices, press Enter.");
    let (len, step) = if long_password { (128, 3) } else { (10, 2) };
    let labels = [
        "Current password  . . . . . . . . . . . .",
        "New password  . . . . . . . . . . . . . .",
        "New password (to verify)  . . . . . . . .",
    ];
    for (i, label) in (0u8..).zip(labels) {
        let row = 9 + i * step;
        p.text(row, 4, 0, label);
        p.hidden(row, 47, len);
    }
    p.text(23, 2, 2, "F3=Exit");
    p.text(23, 20, 2, "F12=Cancel");
    p.message = message.into();
    p.error = !message.is_empty();
    p
}

/// Sign-on Information: the previous sign-on, the attempts that were
/// not valid since, and the days the password has left when it warns.
#[must_use]
pub fn signon_info(system: &str, previous: u64, invalid: u32, expires_in: Option<i64>) -> Panel {
    let mut p = Panel::default();
    p.text(1, 30, 1, "Sign-on Information");
    p.text(2, 63, 0, "System:");
    p.text(2, 72, 0, system);
    p.text(5, 2, 0, "Previous sign-on . . . . . . . . . . . . :");
    if previous == 0 {
        p.text(5, 47, 0, "*NONE");
    } else {
        let (d, t) = date_time(previous);
        p.text(5, 47, 0, format!("{d}  {t}"));
    }
    p.text(7, 2, 0, "Sign-on attempts not valid . . . . . . . :");
    p.text(7, 47, 0, invalid.to_string());
    if let Some(days) = expires_in {
        p.text(9, 2, 0, "Days until password expires  . . . . . . :");
        p.text(9, 47, 0, days.to_string());
    }
    p.text(22, 2, 0, "Press Enter to continue.");
    p.text(23, 2, 2, "F3=Exit   F12=Cancel");
    p
}

/// The options of the interim MAIN menu.
pub const MAIN_OPTIONS: &[&str] = &[
    "User tasks",
    "Office tasks",
    "General system tasks",
    "Files, libraries, and folders",
    "Programming",
    "Communications",
    "Define or change the system",
    "Problem handling",
    "Display a menu",
    "Information Assistant options",
    "QS/400 Access tasks",
];

/// The sign-on display as the display files `QDSIGNON` and
/// `QDSIGNON2` lay it out:
/// the system, subsystem and device at the top right, the five entry
/// fields, the copyright line. `long_password` is password level 2
/// and 3 (`QDSIGNON2`: 128 characters, nondisplay, the rows below
/// moved down two).
#[must_use]
pub fn sign_on(
    system: &str,
    subsystem: &str,
    device: &str,
    long_password: bool,
    message: &str,
) -> Panel {
    let mut p = Panel::default();
    p.text(1, 36, 1, "Sign On");
    p.text(2, 48, 0, "System  . . . . . :");
    p.text(2, 70, 0, system);
    p.text(3, 48, 0, "Subsystem . . . . :");
    p.text(3, 70, 0, subsystem);
    p.text(4, 48, 0, "Display . . . . . :");
    p.text(4, 70, 0, device);
    p.text(6, 17, 0, "User  . . . . . . . . . . . . . .");
    p.field(6, 53, 10, "");
    p.text(7, 17, 0, "Password  . . . . . . . . . . . .");
    p.hidden(7, 53, if long_password { 128 } else { 10 });
    // A 128-character password runs on through rows 8 and 9.
    let row = if long_password { 10 } else { 8 };
    p.text(row, 17, 0, "Program/procedure . . . . . . . .");
    p.field(row, 53, 10, "");
    p.text(row + 1, 17, 0, "Menu  . . . . . . . . . . . . . .");
    p.field(row + 1, 53, 10, "");
    p.text(row + 2, 17, 0, "Current library . . . . . . . . .");
    p.field(row + 2, 53, 10, "");
    p.text(24, 41, 1, "(C) COPYRIGHT QUARTZ SYSTEMS 2026.");
    p.cursor = Some((6, 53));
    p.message = message.into();
    p.error = !message.is_empty();
    p
}

/// The interim MAIN menu with the command line holding `field`.
#[must_use]
pub fn main_menu(system: &str, field: &str, message: &str) -> Panel {
    let mut p = Panel::default();
    // The geometry of IBM's MAIN, measured on the oracle
    // (tools/conformance/golden/verified/pub400/main-menu.txt).
    p.text(1, 2, 1, "MAIN");
    p.text(1, 33, 1, "QS/400 Main Menu");
    p.text(2, 62, 0, "System:");
    p.text(2, 72, 0, system);
    p.text(3, 2, 0, "Select one of the following:");
    for (i, o) in MAIN_OPTIONS.iter().enumerate() {
        let row = u8::try_from(5 + i).expect("few options");
        p.text(row, 5, 0, format!("{:>3}. {o}", i + 1));
    }
    p.text(17, 5, 0, " 90. Sign off");
    p.text(19, 2, 0, "Selection or command");
    p.text(20, 2, 1, "===>");
    p.field(20, 7, 73, field);
    p.text(
        21,
        2,
        2,
        "F3=Exit   F4=Prompt   F9=Retrieve   F12=Cancel   F13=Information Assistant",
    );
    p.text(22, 2, 2, "F23=Set initial menu");
    p.text(24, 2, 0, "(C) COPYRIGHT QUARTZ SYSTEMS 2026.");
    p.message = message.into();
    p.error = !message.is_empty();
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(
            date_time(1_792_540_800 + 3661),
            ("10/21/26".into(), "01:01:01".into())
        );
    }
}
