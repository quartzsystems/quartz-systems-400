// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The panels an interactive job shows before and around its work:
//! Change Password (`QDCHGPWD`'s geometry), Sign-on Information, and
//! the interim MAIN menu that stands in until `*MENU` objects exist
//! (`MIGRATE(phase8)`). Golden files under `tools/conformance/golden/`
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

/// Change Password: the current password, the new one twice.
#[must_use]
pub fn change_password(system: &str, changed: u64, message: &str) -> Panel {
    let mut p = Panel::default();
    p.text(1, 32, 1, "Change Password");
    p.text(2, 63, 0, "System:");
    p.text(2, 72, 0, system);
    if changed != 0 {
        let (d, _) = date_time(changed);
        p.text(4, 2, 0, "Password last changed . . . . . . . :");
        p.text(4, 41, 0, d);
    }
    p.text(6, 2, 0, "Type choices, press Enter.");
    p.text(8, 2, 0, "Current password . . . . . . . . . .");
    p.hidden(8, 41, 128);
    p.text(10, 2, 0, "New password . . . . . . . . . . . .");
    p.hidden(10, 41, 128);
    p.text(12, 2, 0, "New password (to verify) . . . . . .");
    p.hidden(12, 41, 128);
    p.text(23, 2, 2, "F3=Exit   F12=Cancel");
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

/// The interim MAIN menu with the command line holding `field`.
#[must_use]
pub fn main_menu(system: &str, field: &str, message: &str) -> Panel {
    let mut p = Panel::default();
    p.text(1, 2, 1, "MAIN");
    p.text(1, 31, 1, "QS/400 Main Menu");
    p.text(2, 62, 0, "System:");
    p.text(2, 72, 0, system);
    p.text(4, 2, 0, "Select one of the following:");
    for (i, o) in MAIN_OPTIONS.iter().enumerate() {
        let row = u8::try_from(6 + i).expect("few options");
        p.text(row, 6, 0, format!("{:>3}. {o}", i + 1));
    }
    p.text(18, 6, 0, " 90. Sign off");
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
