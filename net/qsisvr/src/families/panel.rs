// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The control panel of a standalone system, at ID 0: the displays (01,
//! 11 to 19, 20) at any time; 03 restart, 08 power off, 21 service tools
//! on the console and 22 a system dump only with the keylock in manual
//! (`QSI0452`) and once the IPL is complete (`QSI0453`). A function the
//! system does not carry out is `QSI0451`.

use qsi::base::{Keylock, Power, PowerAction, PowerState, State};
use qsi::host::panel::{FunctionInfo, function, msgid};

use crate::host::{Host, Refusal, refusal};

/// One function: its number, what it does, whether it needs manual,
/// whether a console asks first.
const FUNCTIONS: &[(u8, &str, bool, bool)] = &[
    (
        function::DISPLAY_IPL,
        "Display the IPL type and the keylock",
        false,
        false,
    ),
    (
        function::RESTART,
        "End the system at once and IPL it again",
        true,
        true,
    ),
    (
        function::POWER_OFF,
        "Power the system off at once",
        true,
        true,
    ),
    (
        11,
        "Display word 1 of the current reference code",
        false,
        false,
    ),
    (
        12,
        "Display word 2 of the current reference code",
        false,
        false,
    ),
    (
        13,
        "Display word 3 of the current reference code",
        false,
        false,
    ),
    (
        14,
        "Display word 4 of the current reference code",
        false,
        false,
    ),
    (
        15,
        "Display word 5 of the current reference code",
        false,
        false,
    ),
    (
        16,
        "Display word 6 of the current reference code",
        false,
        false,
    ),
    (
        17,
        "Display word 7 of the current reference code",
        false,
        false,
    ),
    (
        18,
        "Display word 8 of the current reference code",
        false,
        false,
    ),
    (
        19,
        "Display word 9 of the current reference code",
        false,
        false,
    ),
    (
        function::IDENTITY,
        "Display the machine type, model, serial number and release",
        false,
        false,
    ),
    (
        function::DST,
        "Start dedicated service tools on the system's console",
        true,
        true,
    ),
    (function::DUMP, "Take a system dump", true, true),
];

/// The functions this system has, by number.
#[must_use]
pub fn list() -> Vec<FunctionInfo> {
    FUNCTIONS
        .iter()
        .map(|&(f, text, manual, confirm)| FunctionInfo {
            function: f,
            text: text.into(),
            manual,
            confirm,
        })
        .collect()
}

fn keylock_text(k: Option<Keylock>) -> &'static str {
    match k {
        Some(Keylock::Manual) => "manual",
        Some(Keylock::Auto) => "auto",
        Some(Keylock::Secure) => "secure",
        _ => "normal",
    }
}

fn no_function(f: u8) -> Refusal {
    refusal(
        msgid::NO_FUNCTION,
        &format!("This system has no control panel function {f:02}."),
    )
}

/// Carry out `f`; what the panel shows.
///
/// # Errors
/// `QSI0451` a function this system does not have, `QSI0452` the
/// keylock not in manual, `QSI0453` before the IPL completes; the
/// system's own refusal of the action.
pub fn run(host: &mut dyn Host, f: u8, user: &str) -> Result<String, Refusal> {
    let Some(&(_, _, manual, _)) = FUNCTIONS.iter().find(|e| e.0 == f) else {
        return Err(no_function(f));
    };
    let state = host.state()?;
    if manual && state.keylock != Some(Keylock::Manual) {
        return Err(refusal(
            msgid::NEEDS_MANUAL,
            &format!(
                "Function {f:02} needs the keylock in manual; it is {}.",
                keylock_text(state.keylock)
            ),
        ));
    }
    if manual && state.power != PowerState::Running {
        return Err(refusal(
            msgid::NOT_NOW,
            &format!("Function {f:02} waits until the system's IPL is complete."),
        ));
    }
    match f {
        function::DISPLAY_IPL => Ok(display_ipl(&state)),
        11..=19 => Ok(word(&state, f - 10)),
        function::IDENTITY => {
            let id = host.identity()?;
            let or_n = |s: &str| {
                if s.is_empty() {
                    "*N".to_string()
                } else {
                    s.to_string()
                }
            };
            Ok(format!(
                "20  {}  {}  {}",
                or_n(&id.model),
                or_n(&id.serial),
                id.release
            ))
        }
        function::RESTART | function::POWER_OFF => {
            let restart = f == function::RESTART;
            let action = if restart {
                PowerAction::Restart
            } else {
                PowerAction::Off
            };
            host.power(
                &Power {
                    action,
                    immediate: true,
                    delay: 0,
                },
                user,
            )?;
            Ok(if restart {
                "The system is ending at once and will IPL again.".into()
            } else {
                "The system is powering off at once.".into()
            })
        }
        function::DST => {
            host.service_tools(user)?;
            Ok("Dedicated service tools are on the system's console.".into())
        }
        function::DUMP => {
            let name = host.take_dump("Control panel function 22", user)?;
            Ok(format!("System dump {name} was taken."))
        }
        _ => Err(no_function(f)),
    }
}

/// Function 01: the IPL type (the slot, `D` from media) and the
/// keylock, as the panel shows them.
fn display_ipl(state: &State) -> String {
    let side = if state.slot.is_empty() {
        "*N"
    } else {
        &state.slot
    };
    let mode = match state.keylock {
        Some(Keylock::Manual) => "M",
        Some(Keylock::Auto) => "A",
        Some(Keylock::Secure) => "S",
        _ => "N",
    };
    format!("01  {side}  {mode}")
}

/// Functions 11 to 19: word `n` of the current code. A QS/400 code is
/// its word 1 alone; the others are zeros.
fn word(state: &State, n: u8) -> String {
    let zeros = || "00000000".to_string();
    let w = match (n, state.refcode.as_ref()) {
        (_, None) => zeros(),
        (1, Some(c)) => c.code.clone(),
        (n, Some(c)) => c
            .words
            .get(usize::from(n) - 2)
            .cloned()
            .unwrap_or_else(zeros),
    };
    format!("{}  {w}", 10 + n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::MemHost;

    #[test]
    fn displays_answer_at_any_keylock_and_actions_need_manual() {
        let mut h = MemHost::new();
        assert_eq!(run(&mut h, 1, "U").unwrap(), "01  A  N");
        assert_eq!(run(&mut h, 11, "U").unwrap(), "11  C6001090");
        assert_eq!(run(&mut h, 12, "U").unwrap(), "12  00000000");
        assert!(run(&mut h, 20, "U").unwrap().starts_with("20  "));
        for f in [3, 8, 21, 22] {
            assert_eq!(run(&mut h, f, "U").unwrap_err()[0].msgid, "QSI0452");
        }
        assert_eq!(run(&mut h, 2, "U").unwrap_err()[0].msgid, "QSI0451");
        assert_eq!(run(&mut h, 34, "U").unwrap_err()[0].msgid, "QSI0451");
        h.keylock = Keylock::Manual;
        h.complete = false;
        assert_eq!(run(&mut h, 21, "U").unwrap_err()[0].msgid, "QSI0453");
        h.complete = true;
        assert!(run(&mut h, 21, "U").is_ok());
        assert_eq!(h.service_tools, 1);
        assert!(run(&mut h, 22, "U").unwrap().contains("QSD0000001"));
        assert!(run(&mut h, 3, "U").is_ok());
        assert_eq!(
            h.powered.last().map(|p| p.action),
            Some(PowerAction::Restart)
        );
        assert_eq!(list().len(), FUNCTIONS.len());
    }
}
