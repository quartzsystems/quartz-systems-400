// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Phase 0 IPL sequence and the forever loop.

use std::panic;
use std::thread;
use std::time::{Duration, Instant};

use crate::codes;
use crate::platform::console::{self, Outputs};
use crate::platform::mounts::{self, Outcome};
use crate::platform::sys::{self, Wake};

const HEARTBEAT_PERIOD: Duration = Duration::from_secs(5);

/// Run as PID 1. Never returns.
pub fn run() -> ! {
    install_panic_hook();
    if !sys::is_init() {
        eprintln!("slic: not PID 1, refusing to run");
        std::process::exit(1);
    }
    sys::block_sigchld();

    let mounted = mounts::mount_all();
    let mut out = Outputs::open();
    out.both(&codes::line(codes::IPL_STARTED));
    out.both(&format!("{}\n", codes::BANNER));
    for (target, result) in mounted {
        match result {
            Ok(Outcome::Mounted) => out.serial(&format!("slic: mounted {target}\n")),
            Ok(Outcome::AlreadyMounted) => out.serial(&format!("slic: {target} already mounted\n")),
            Err(e) => out.both(&format!("slic: mount {target} failed: {e}\n")),
        }
    }
    heartbeat_loop(&mut out)
}

/// Heartbeat on serial every period; reap on `SIGCHLD` in between.
fn heartbeat_loop(out: &mut Outputs) -> ! {
    let mut next = Instant::now() + HEARTBEAT_PERIOD;
    loop {
        let now = Instant::now();
        if now >= next {
            out.serial(&codes::line(codes::HEARTBEAT));
            next += HEARTBEAT_PERIOD;
            continue;
        }
        if let Wake::Child = sys::wait(next - now) {
            sys::reap();
        }
    }
}

/// On panic: SRC and message to serial and console, then park forever.
fn install_panic_hook() {
    panic::set_hook(Box::new(|info| {
        let text = format!("{}slic panic: {info}\n", codes::line(codes::FATAL));
        console::emergency(&text);
        park()
    }));
}

/// Sleep forever. PID 1 must not exit and Phase 0 does not reboot.
pub fn park() -> ! {
    loop {
        thread::sleep(Duration::from_secs(3600));
    }
}
