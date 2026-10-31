// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! C1001050: start the console thread. The LIC record's console mode
//! says whether the display and keyboards are used at all; without a
//! framebuffer the thread still runs so `screen` works over serial, and
//! A6005008 is raised.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub use console::Command;
use svctools::dst::console_mode::{ConsoleMode, TAG_CONSOLE_MODE};

use crate::ipl::Event;
use crate::ipl::system::System;
use crate::src;

/// Open the display, start the console and keyboard threads, replay the
/// codes emitted so far. Keyboard input comes back as `Event::Console`.
pub fn attach(system: &mut System, events: mpsc::Sender<Event>) {
    let mode = console_mode(system);
    let (con, status) =
        console::Console::open_scaled(mode == ConsoleMode::Local, system.boot.console_scale);
    system.out.serial(&format!(
        "slic: display: {} ({})\n",
        con.describe(),
        mode.text()
    ));
    let (tx, rx) = mpsc::channel();
    let (input_tx, input_rx) = mpsc::channel::<console::Input>();
    thread::spawn(move || con.run(&rx, input_tx));
    thread::spawn(move || {
        while let Ok(input) = input_rx.recv() {
            if events.send(Event::Console(input)).is_err() {
                break;
            }
        }
    });
    let keyboards = if mode == ConsoleMode::Local {
        console::keyboard::spawn_readers(&tx)
    } else {
        0
    };
    system
        .out
        .serial(&format!("slic: {keyboards} keyboard device(s)\n"));
    system.console = Some(tx);

    let name = system.system_name();
    let backlog: Vec<(String, String)> = system
        .log
        .entries()
        .map(|e| (e.src.code.to_string(), e.src.text.to_string()))
        .collect();
    for (code, text) in backlog {
        system.show(Command::ShowSrc {
            system: name.clone(),
            code,
            text,
        });
    }

    match status {
        Ok(()) => system.emit(&src::CONSOLE_ATTACHED),
        Err(e) => {
            system.out.serial(&format!("slic: {e}\n"));
            system.emit(&src::CONSOLE_MISSING);
        }
    }
}

/// The console mode the LIC record keeps; the local console when it
/// says nothing.
pub fn console_mode(system: &System) -> ConsoleMode {
    system
        .lic
        .as_ref()
        .and_then(|(_, r)| r.tlv(TAG_CONSOLE_MODE))
        .and_then(ConsoleMode::decode)
        .unwrap_or_default()
}

/// The display's mode and the scale chosen, asked of the console thread.
pub fn describe(system: &System) -> String {
    let Some(tx) = &system.console else {
        return "Console not attached".to_string();
    };
    let (reply_tx, reply_rx) = mpsc::channel();
    if tx.send(Command::Describe(reply_tx)).is_err() {
        return "Console thread gone".to_string();
    }
    reply_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_else(|_| "Console did not answer".to_string())
}

/// The screen as 24 lines of text, for the service console and the harness.
pub fn dump(system: &System) -> String {
    let Some(tx) = &system.console else {
        return "Console not attached\n".to_string();
    };
    let (reply_tx, reply_rx) = mpsc::channel();
    if tx.send(Command::Dump(reply_tx)).is_err() {
        return "Console thread gone\n".to_string();
    }
    reply_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_else(|_| "Console did not answer\n".to_string())
}
