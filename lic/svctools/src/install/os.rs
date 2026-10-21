// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Installing the operating system: device type, confirmation, the
//! language group, and the status display while ASP 1 is initialized.
//! The IPL options follow once the system values exist.

use crate::dst::common::{Confirm, Select, Submenu};
use crate::env::DstEnv;
use crate::install::{OS_STEPS, Progress, os_status};
use crate::panel::{Flow, Outcome, Runner};

/// The language groups the install offers. 2924 is English; the table
/// is here so others can be added.
pub const LANGUAGES: [(&str, &str); 1] = [("2924", "English")];

/// The operating system installer, starting at the device type.
#[must_use]
pub fn start() -> Runner {
    Runner::new(Box::new(device_type_menu()))
}

/// `Install Device Type Selection`: only optical works.
#[must_use]
pub fn device_type_menu() -> Submenu {
    Submenu::new(
        "install-device-type",
        "Install Device Type Selection",
        vec![
            (1, "Tape", None),
            (
                2,
                "Optical",
                Some(Box::new(|_env| Flow::Push(Box::new(confirm_os())))),
            ),
            (3, "Virtual device", None),
        ],
    )
}

/// `Confirm Install of the Operating System`.
#[must_use]
pub fn confirm_os() -> Confirm {
    Confirm::new(
        "install-os-confirm",
        "Confirm Install of the Operating System",
        vec![
            "The operating system will be installed from the optical device onto".into(),
            "the load source. ASP 1 is initialized: any data on it is lost.".into(),
        ],
        Box::new(|_env: &mut dyn DstEnv| Ok(String::new())),
    )
    .then(|_env| Flow::Push(Box::new(select_language())))
}

/// `Select a Language Group`.
#[must_use]
pub fn select_language() -> Select {
    let rows: Vec<String> = LANGUAGES
        .iter()
        .map(|(code, name)| format!("{code}   {name}"))
        .collect();
    Select::new(
        "install-language",
        "Select a Language Group",
        "Type 1 beside the language feature to install, then press Enter.",
        "Opt  Feature  Language",
        rows,
        Box::new(|chosen: &[usize], _env: &mut dyn DstEnv| {
            let (code, name) = LANGUAGES[chosen[0]];
            Flow::Push(Box::new(confirm_language(code, name)))
        }),
    )
}

/// `Confirm Language Feature Selection`: F10 installs.
#[must_use]
pub fn confirm_language(code: &'static str, name: &'static str) -> Confirm {
    Confirm::new(
        "install-language-confirm",
        "Confirm Language Feature Selection",
        vec![
            format!("Language feature . . . :  {code}  {name}"),
            String::new(),
            "Press F10 to install the operating system with this language.".into(),
        ],
        Box::new(move |env: &mut dyn DstEnv| {
            run_install(env, code)?;
            Ok("Operating system installed".into())
        }),
    )
    .exiting(Outcome::Continue)
}

/// Initialize ASP 1 with the status display.
pub fn run_install(env: &mut dyn DstEnv, language: &str) -> Result<(), String> {
    let mut progress = Progress::new(&OS_STEPS);
    progress.note = format!("Language feature {language}");
    env.install_os(&mut |step, env| {
        progress.done(step);
        env.show(os_status(env, &progress));
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MemEnv;

    fn f(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn the_sequence_installs_and_continues() {
        let mut e = MemEnv::new("QS400");
        let mut r = start();
        assert_eq!(r.enter(&f(&["1"]), &mut e), None);
        assert!(r.screen(&mut e).dump().contains("not available"));
        assert_eq!(r.enter(&f(&["2"]), &mut e), None);
        assert_eq!(r.top(), "install-os-confirm");
        assert_eq!(r.function(10, &mut e), None);
        assert_eq!(r.top(), "install-language");
        assert_eq!(r.enter(&f(&["1"]), &mut e), None);
        assert_eq!(r.top(), "install-language-confirm");
        assert_eq!(r.function(10, &mut e), Some(Outcome::Continue));
        assert_eq!(e.actions, vec!["install-os"]);
        assert!(
            e.shown
                .last()
                .unwrap()
                .contains("X  Create the system values")
        );
    }
}
