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
use licpart::ConsoleMode;
use svctools::dst::console_mode::ModeText;

use crate::ipl::Event;
use crate::ipl::system::System;
use crate::src;

/// Open the display, start the console and keyboard threads, replay the
/// codes emitted so far. Keyboard input comes back as `Event::Console`.
pub fn attach(system: &mut System, events: mpsc::Sender<Event>) {
    let mode = console_mode(system);
    let identity = console::Identity {
        banner: release::banner(),
        ready_code: src::IPL_COMPLETE.code.to_string(),
    };
    let (mut con, status) = console::Console::open_scaled(
        identity,
        mode == ConsoleMode::Local,
        system.boot.console_scale,
    );
    let (tx, rx) = mpsc::channel();
    // A partition's console is its console port: the grid as cells, the
    // keys back as the keyboard gives them.
    let mut status = status.map_err(|e| e.to_string());
    let mut shown = con.describe();
    if mode == ConsoleMode::Partition {
        let sys = std::path::Path::new("/sys");
        let dev = std::path::Path::new("/dev");
        match console::qsi::open(sys, dev, qsipart::CONSOLE_PORT) {
            Some((out, input, path)) => {
                let link = console::qsi::Link::new();
                let d = console::qsi::QsiDisplay::new(out, std::sync::Arc::clone(&link), &path);
                shown = console::Display::describe(&d);
                con.add_display(Box::new(d));
                console::qsi::spawn_reader(input, link, tx.clone());
            }
            None => {
                status = Err(format!("no console port {}", qsipart::CONSOLE_PORT));
            }
        }
    } else {
        viewers(system, &mut con, &tx);
    }
    system
        .out
        .serial(&format!("slic: display: {shown} ({})\n", mode.text()));
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

/// Beside the local display, the console for one viewer at
/// `wm::dev::QSI_CONSOLE` (the QSI server's console family): the same
/// frames, its keys as the keyboard's.
fn viewers(system: &mut System, con: &mut console::Console, tx: &mpsc::Sender<Command>) {
    use console::qsi::tee::{self, Ask, Poke};
    let hub = tee::Hub::new();
    let path = std::path::Path::new(wm::dev::QSI_CONSOLE);
    let tx = tx.clone();
    let poke: Poke = std::sync::Arc::new(move |a| {
        let _ = tx.send(match a {
            Ask::Key(k) => Command::Virtual(k),
            Ask::Redraw => Command::Redraw,
        });
    });
    match tee::listen(&hub, path, poke) {
        Ok(()) => con.add_display(Box::new(hub.display(wm::dev::QSI_CONSOLE))),
        Err(e) => system
            .out
            .serial(&format!("slic: {}: {e}\n", path.display())),
    }
}

/// The console mode the LIC record keeps. When it says nothing (no
/// record yet: a D IPL onto a blank disk in a partition), the console
/// port when the hypervisor gave one, else the local console.
pub fn console_mode(system: &System) -> ConsoleMode {
    mode_of(
        system.lic.as_ref().and_then(|(_, r)| ConsoleMode::of(r)),
        std::path::Path::new("/sys"),
    )
}

/// The recorded mode, else the console port if `sys` has it.
fn mode_of(recorded: Option<ConsoleMode>, sys: &std::path::Path) -> ConsoleMode {
    recorded.unwrap_or_else(|| {
        if console::qsi::find_port(sys, qsipart::CONSOLE_PORT).is_some() {
            ConsoleMode::Partition
        } else {
            ConsoleMode::default()
        }
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sys_with(port: Option<&str>) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "slic-mode-{}-{}",
            std::process::id(),
            port.unwrap_or("none")
        ));
        let _ = std::fs::remove_dir_all(&d);
        let ports = d.join("class/virtio-ports");
        std::fs::create_dir_all(ports.join("vport0p1")).unwrap();
        std::fs::write(
            ports.join("vport0p1/name"),
            "org.qemu.guest_agent.0
",
        )
        .unwrap();
        if let Some(name) = port {
            std::fs::create_dir_all(ports.join("vport0p2")).unwrap();
            std::fs::write(
                ports.join("vport0p2/name"),
                format!(
                    "{name}
"
                ),
            )
            .unwrap();
        }
        d
    }

    #[test]
    fn no_record_in_a_partition_is_the_console_port() {
        let sys = sys_with(Some(qsipart::CONSOLE_PORT));
        assert_eq!(mode_of(None, &sys), ConsoleMode::Partition);
        // What the record says wins.
        assert_eq!(
            mode_of(Some(ConsoleMode::Serial), &sys),
            ConsoleMode::Serial
        );
        let _ = std::fs::remove_dir_all(sys);
    }

    #[test]
    fn no_record_and_no_port_is_the_local_console() {
        let sys = sys_with(None);
        assert_eq!(mode_of(None, &sys), ConsoleMode::Local);
        assert_eq!(mode_of(None, &sys.join("absent")), ConsoleMode::Local);
        let _ = std::fs::remove_dir_all(sys);
    }
}
