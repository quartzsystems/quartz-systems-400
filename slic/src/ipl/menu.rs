// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The manual-mode "IPL or Install the System" menu, text version on
//! serial. The console version comes with the console crate.

use std::sync::mpsc::Receiver;

use crate::ipl::Event;
use crate::ipl::system::System;
use crate::platform::sys;

/// Menu text. Quartz wording.
pub const MENU: &str = "\n\
    IPL or Install the System\n\
    \n\
    Select one of the following:\n\
    \n\
    \x20    1. Perform an IPL\n\
    \x20    2. Install the operating system\n\
    \x20    3. Use Dedicated Service Tools (DST)\n\
    \x20    4. Perform automatic installation\n\
    \x20    5. Save Licensed Internal Code\n\
    \n";

/// Message for everything but option 1.
pub const NOT_AVAILABLE: &str = "Option not available in this release";

/// Show the menu and block until option 1 is chosen.
pub fn run(system: &mut System, rx: &Receiver<Event>) {
    loop {
        system.out.both(MENU);
        system.out.both("Selection: ");
        let choice = loop {
            match rx.recv() {
                Ok(Event::Line(l)) => break l,
                Ok(Event::Signal(s)) if s == sys::HANDLED[0] => {
                    sys::reap();
                }
                Ok(Event::Signal(_) | Event::Uevent(_)) => {}
                Err(_) => return,
            }
        };
        match choice.trim() {
            "1" => return,
            "2" | "3" | "4" | "5" => system.out.both(&format!("\n{NOT_AVAILABLE}\n")),
            other => system
                .out
                .both(&format!("\nSelection '{other}' is not valid\n")),
        }
    }
}
