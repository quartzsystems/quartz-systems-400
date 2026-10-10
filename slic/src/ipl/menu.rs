// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The attended "IPL or Install the System" menu, shown on the display
//! and echoed on serial. Keys arrive from the console thread; a line
//! typed on serial is typed into the current field and entered, unless
//! it is a service console command.

use std::sync::mpsc::Receiver;

use console::{Input, Key, Screen};
use svctools::ui;

use crate::ipl::display::Command;
use crate::ipl::system::System;
use crate::ipl::{Event, dst, install};
use crate::platform::sys;
use crate::svc;

/// The options.
pub const OPTIONS: [(u8, &str); 5] = [
    (1, "Perform an IPL"),
    (2, "Install the operating system"),
    (3, "Use Dedicated Service Tools (DST)"),
    (4, "Perform automatic installation of the operating system"),
    (5, "Save Licensed Internal Code"),
];

/// Message for an option that does nothing yet.
pub const NOT_AVAILABLE: &str = "Option not available in this release";

/// What the operator did on an application screen.
pub enum Answer {
    /// Enter with the fields.
    Enter(Vec<String>),
    /// A function key.
    Function(u8),
}

/// Show a screen on the display and echo its text on serial.
pub fn show(system: &mut System, screen: Screen) {
    let dump = screen.dump();
    system.out.serial("\n");
    for line in dump.lines() {
        let line = line.trim_end();
        if !line.is_empty() {
            system.out.serial(&format!("{line}\n"));
        }
    }
    system.show(Command::ShowScreen(screen));
}

/// Wait for Enter or a function key on the current screen, serving
/// signals and hotplug meanwhile. A serial line that is not a service
/// console command is typed into the current field and entered.
/// `None` when the event source is gone.
pub fn read(system: &mut System, rx: &Receiver<Event>) -> Option<Answer> {
    loop {
        match rx.recv() {
            Ok(Event::Console(Input::Enter(fields))) => return Some(Answer::Enter(fields)),
            Ok(Event::Console(Input::Function(n))) => return Some(Answer::Function(n)),
            Ok(Event::Line(line)) => {
                if svc::during_menu(system, &line) {
                    continue;
                }
                for c in line.trim_end().chars() {
                    system.show(Command::Virtual(Key::Char(c)));
                }
                system.show(Command::Virtual(Key::Enter));
            }
            Ok(Event::Signal(s)) if s == sys::HANDLED[0] => {
                sys::reap();
            }
            Ok(Event::Uevent(ev)) => {
                if let Some(msg) = system.hw.apply_uevent(&ev) {
                    system.out.serial(&format!("{msg}\n"));
                }
            }
            Ok(Event::Signal(_)) => {}
            Err(_) => return None,
        }
    }
}

/// The first field of an answer, trimmed; a function key reads as empty.
#[must_use]
pub fn first_field(answer: &Answer) -> String {
    match answer {
        Answer::Enter(fields) => fields
            .first()
            .map(|f| f.trim().to_string())
            .unwrap_or_default(),
        Answer::Function(_) => String::new(),
    }
}

/// The menu screen.
pub fn screen(system: &System) -> Screen {
    ui::menu(
        "IPL or Install the System",
        &system.system_name(),
        &OPTIONS,
        "F3=Exit   F12=Cancel",
    )
}

/// Show the menu and block until option 1 is chosen. Option 2 runs the
/// installer on installation media, option 3 the service tools.
pub fn run(system: &mut System, rx: &Receiver<Event>) {
    let mut message: Option<(String, bool)> = None;
    loop {
        let mut s = screen(system);
        if let Some((text, error)) = message.take() {
            ui::message(&mut s, &text, error);
        }
        show(system, s);
        let Some(answer) = read(system, rx) else {
            return;
        };
        let choice = first_field(&answer);
        match choice.as_str() {
            "1" => return,
            "2" if system.boot.ipl_d => install::run_lic(system, rx),
            "4" if system.boot.ipl_d => install::run_auto(system),
            "3" => match dst::run(system, rx) {
                dst::Outcome::PerformIpl => return,
                dst::Outcome::InstallOs if system.boot.ipl_d => install::run_lic(system, rx),
                dst::Outcome::AutoInstall if system.boot.ipl_d => install::run_auto(system),
                dst::Outcome::InstallOs | dst::Outcome::AutoInstall => {
                    message = Some((NOT_AVAILABLE.to_string(), false));
                }
                _ => {}
            },
            "2" | "4" | "5" => message = Some((NOT_AVAILABLE.to_string(), false)),
            "" => {}
            other => message = Some((format!("Selection '{other}' is not valid"), true)),
        }
    }
}
