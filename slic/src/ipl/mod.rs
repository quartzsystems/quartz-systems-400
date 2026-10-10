// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IPL state machine: LIC phase, OS phase, then the run loop that
//! serves signals and the service console until power down.

pub mod cmdline;
pub mod lic;
pub mod log;
pub mod menu;
pub mod power;
pub mod system;

use std::panic;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use licpart::IplMode;

use crate::platform::{console, mounts, serial, sys};
use crate::src;
use crate::svc;
use system::System;

/// What the run loop reacts to.
#[derive(Debug)]
pub enum Event {
    /// A handled signal arrived.
    Signal(i32),
    /// A line was typed on the serial port.
    Line(String),
}

/// Run as PID 1. Never returns.
pub fn run() -> ! {
    install_panic_hook();
    if !sys::is_init() {
        eprintln!("slic: not PID 1, refusing to run");
        std::process::exit(1);
    }
    sys::block_signals();
    let mounted = mounts::mount_all();

    let mut system = System::new(console::Outputs::open(), cmdline::Boot::read());
    system.emit(&src::IPL_STARTED);
    system.out.both("QS/400 IPL - phase 1\n");
    for (target, result) in mounted {
        match result {
            Ok(mounts::Outcome::Mounted) => system.out.serial(&format!("slic: mounted {target}\n")),
            Ok(mounts::Outcome::AlreadyMounted) => {
                system
                    .out
                    .serial(&format!("slic: {target} already mounted\n"));
            }
            Err(e) => system
                .out
                .both(&format!("slic: mount {target} failed: {e}\n")),
        }
    }
    system.out.serial(&format!(
        "slic: slot {:?}{}, mode {:?}, svc {}\n",
        system.boot.slot,
        if system.boot.slot_from_cmdline {
            ""
        } else {
            " (default)"
        },
        system.boot.mode,
        if system.boot.svc { "on" } else { "off" }
    ));

    let (tx, rx) = mpsc::channel();
    spawn_signal_thread(tx.clone());
    if !serial::spawn_reader(tx) {
        system
            .out
            .serial("slic: serial port not readable; no service console\n");
    }

    if let Err(fatal) = lic_phase(&mut system, &rx) {
        system.fatal(fatal);
    }
    os_phase(&mut system);
    run_loop(&mut system, &rx)
}

/// C1001000 through C1001060, plus the manual-mode menu.
fn lic_phase(system: &mut System, rx: &mpsc::Receiver<Event>) -> Result<(), &'static src::Src> {
    lic::attach(system)?;

    // Hardware, clock and network come in later commits; the phases
    // already exist so the SRC sequence is complete.
    system.emit(&src::HW_ENUMERATED);
    system.emit(&src::CLOCK_SET);

    // No local console yet: say so and carry on with serial.
    system.emit(&src::CONSOLE_MISSING);

    if system.mode == IplMode::Manual {
        menu::run(system, rx);
    }
    system.emit(&src::NET_ENUMERATED);
    Ok(())
}

/// C6001000 through C6001090.
fn os_phase(system: &mut System) {
    system.emit(&src::OS_STARTED);
    system.emit(&src::SYSVALS_LOADED);
    system.emit(&src::STRUP_SKIPPED);
    lic::complete(system);
    system.emit(&src::IPL_COMPLETE);
    if system.boot.svc {
        svc::banner(system);
    }
}

/// Serve events forever. Power down leaves through `power::down`.
fn run_loop(system: &mut System, rx: &mpsc::Receiver<Event>) -> ! {
    loop {
        match rx.recv() {
            Ok(Event::Signal(s)) if s == libc_sigchld() => {
                sys::reap();
            }
            Ok(Event::Signal(s)) if s == libc_sigusr1() => power::down(system, false),
            Ok(Event::Signal(_)) => {} // SIGTERM/SIGINT are ignored by PID 1
            Ok(Event::Line(line)) => {
                if system.boot.svc {
                    svc::handle(system, &line);
                }
            }
            Err(_) => park(),
        }
    }
}

fn spawn_signal_thread(tx: mpsc::Sender<Event>) {
    thread::spawn(move || {
        loop {
            let s = sys::wait_signal();
            if tx.send(Event::Signal(s)).is_err() {
                break;
            }
        }
    });
}

// Thin wrappers keep the libc constants inside platform::sys's scope of use.
fn libc_sigchld() -> i32 {
    sys::HANDLED[0]
}
fn libc_sigusr1() -> i32 {
    sys::HANDLED[1]
}

/// On panic: SRC and message to serial and console, then park forever.
fn install_panic_hook() {
    panic::set_hook(Box::new(|info| {
        let text = format!("{}slic panic: {info}\n", src::PANIC.line());
        console::emergency(&text);
        park()
    }));
}

/// Sleep forever. PID 1 must not exit and a fatal SRC does not reboot.
pub fn park() -> ! {
    loop {
        thread::sleep(Duration::from_secs(3600));
    }
}
