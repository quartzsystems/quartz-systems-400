// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! C1001050: start the console thread. Without a framebuffer the thread
//! still runs so `screen` works over serial, and A6005008 is raised.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub use console::Command;

use crate::ipl::system::System;
use crate::src;

/// Open the display, start the thread, replay the codes emitted so far.
pub fn attach(system: &mut System) {
    let (con, status) = console::Console::open();
    system
        .out
        .serial(&format!("slic: display: {}\n", con.describe()));
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || con.run(&rx));
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
