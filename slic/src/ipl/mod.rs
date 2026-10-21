// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IPL state machine: LIC phase, OS phase, then the run loop that
//! serves signals and the service console until power down.

pub mod clock;
pub mod cmdline;
pub mod display;
pub mod dst;
pub mod install;
pub mod lic;
pub mod log;
pub mod menu;
pub mod power;
pub mod storage;
pub mod system;
pub mod sysvals;

use std::panic;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use licpart::IplMode;

use crate::hwrsrc;
use crate::net;
use crate::platform::{console, mounts, serial, sys};
use crate::procs;
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
    /// A kernel hotplug event.
    Uevent(hwrsrc::uevent::Uevent),
    /// A message from the service console job.
    Job(wm::ctl::FromJob),
    /// The service console job's channel closed.
    JobGone,
    /// Something from the local console's keyboard.
    Console(::console::Input),
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
    system.out.both(&format!("{}\n", release::banner()));
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

    if system.boot.ipl_d {
        system.out.serial(&format!(
            "slic: IPL type D from installation media ({})\n",
            system.boot.media.as_deref().unwrap_or("unknown")
        ));
    }

    let (tx, rx) = mpsc::channel();
    let rx = std::rc::Rc::new(rx);
    system.events = Some(std::rc::Rc::clone(&rx));
    spawn_signal_thread(tx.clone());
    if !serial::spawn_reader(tx.clone()) {
        system
            .out
            .serial("slic: serial port not readable; no service console\n");
    }

    if let Err(fatal) = lic_phase(&mut system, &rx, &tx) {
        system.fatal(fatal);
    }
    os_phase(&mut system, &rx, &tx);
    run_loop(&mut system, &rx, &tx)
}

/// C1001000 through C1001060, plus the manual-mode menu.
fn lic_phase(
    system: &mut System,
    rx: &mpsc::Receiver<Event>,
    tx: &mpsc::Sender<Event>,
) -> Result<(), &'static src::Src> {
    lic::attach(system)?;

    system.hw = hwrsrc::Table::coldplug();
    system.out.serial(&format!(
        "slic: {} hardware resources\n",
        system.hw.resources.len()
    ));
    if !hwrsrc::uevent::spawn_listener(tx.clone()) {
        system
            .out
            .serial("slic: uevent socket unavailable; no hotplug\n");
    }
    system.emit(&src::HW_ENUMERATED);

    // The display first, so a missing disk unit can be worked on in DST.
    display::attach(system, tx.clone());

    // A load source with the LIC but no operating system yet: install
    // it now, from the media or the disk alike.
    let installing = install::pending(system).is_some();
    if installing {
        pending_install(system, rx)?;
    }

    // A D IPL may be about to install onto the disk, so its store opens
    // only after the menu.
    if !system.boot.ipl_d && !installing {
        open_or_service(system, rx)?;
    }

    match clock::set_from_rtc(system) {
        Ok(now) => {
            system.clock = Some(now);
            system
                .out
                .serial(&format!("slic: clock set to {now} UTC from RTC\n"));
        }
        Err(e) => system.out.serial(&format!("slic: clock not set: {e}\n")),
    }
    system.emit(&src::CLOCK_SET);

    if system.mode == IplMode::Manual && !installing {
        menu::run(system, rx);
    }
    if system.boot.ipl_d && system.storage.is_none() {
        storage::locate_and_open(system)?;
    }

    match net::links() {
        Ok(links) => {
            if let Some(lo) = links.iter().find(|l| l.name == "lo")
                && !lo.is_up()
                && let Err(e) = net::set_up(lo.index)
            {
                system.out.serial(&format!("slic: lo up: {e}\n"));
            }
            system.links = net::links().unwrap_or(links);
            system
                .out
                .serial(&format!("slic: {} network links\n", system.links.len()));
        }
        Err(e) => system.out.serial(&format!("slic: rtnetlink: {e}\n")),
    }
    system.emit(&src::NET_ENUMERATED);
    Ok(())
}

/// C6004100: the operating system install the LIC install left
/// pending. Without prompts when the LIC install was automatic;
/// otherwise through DST, where option 2 installs and option 1 IPLs.
fn pending_install(
    system: &mut System,
    rx: &mpsc::Receiver<Event>,
) -> Result<(), &'static src::Src> {
    let mut auto = install::pending(system).unwrap_or(false);
    system.emit(&src::INSTALL_OS_PENDING);
    // The ASP as it is: a pair formed in DST synchronizes here, before
    // the service tools or the installer use it. A blank one is
    // formatted. Whatever fails is reported again by DST on demand.
    if system.storage.is_none()
        && let Err(code) = storage::locate_and_open(system)
    {
        system.out.serial(&format!(
            "slic: install: ASP 1 not opened yet: {}
",
            code.text
        ));
    }
    if !auto {
        let spare = diskimg::locate::whole_disks()
            .unwrap_or_default()
            .iter()
            .filter(|d| !d.has_qs400)
            .count();
        if spare > 0 {
            system.emit(&src::DISK_NOT_CONFIGURED);
        }
    }
    loop {
        if auto {
            return match install::run_os_auto(system) {
                Ok(()) => Ok(()),
                Err(e) => {
                    system.out.serial(&format!("slic: install: {e}\n"));
                    Err(&src::ASP_UNUSABLE)
                }
            };
        }
        match dst::run(system, rx) {
            dst::Outcome::InstallOs => {
                if install::run_os(system, rx) {
                    return Ok(());
                }
            }
            dst::Outcome::AutoInstall => auto = true,
            dst::Outcome::PerformIpl => power::down(system, true),
            dst::Outcome::Exit => {
                system
                    .out
                    .both("slic: IPL halted; the operating system is not installed\n");
                park();
            }
            _ => {}
        }
    }
}

/// Open the store; on a missing disk unit, B1001030 and the service
/// tools, where option 1 re-IPLs once the operator has dealt with it.
fn open_or_service(
    system: &mut System,
    rx: &mpsc::Receiver<Event>,
) -> Result<(), &'static src::Src> {
    match storage::locate_and_open(system) {
        Ok(()) => Ok(()),
        Err(code) if std::ptr::eq(code, &src::ASP_NOT_FOUND) => {
            system.emit(&src::ASP_NOT_FOUND);
            lic::fail(system, &src::ASP_NOT_FOUND);
            system.mode = IplMode::Manual;
            loop {
                match dst::run(system, rx) {
                    dst::Outcome::PerformIpl => power::down(system, true),
                    dst::Outcome::Exit => {
                        system.out.both("slic: IPL halted; disk unit missing\n");
                        park();
                    }
                    _ => {}
                }
            }
        }
        Err(code) => Err(code),
    }
}

/// C6001000 through C6001090. A fresh install asks for the IPL options
/// once the system values exist.
fn os_phase(system: &mut System, rx: &mpsc::Receiver<Event>, tx: &mpsc::Sender<Event>) {
    system.emit(&src::OS_STARTED);
    if let Err(fatal) = storage::open_objects(system) {
        system.fatal(fatal);
    }
    storage::open_temp(system);
    sysvals::load(system);
    system.emit(&src::SYSVALS_LOADED);
    if let Some(auto) = install::pending(system) {
        if !auto {
            install::run_options(system, rx);
        }
        install::finish(system);
    }
    // The storage service takes the store over; from here on the jobs
    // and the console reach it through the service.
    if system.storage.is_some()
        && let Err(e) = procs::stg::start(system)
    {
        system.out.serial(&format!("slic: {e}\n"));
        system.fatal(&src::STG_ENDED);
    }
    system.emit(&src::STRUP_SKIPPED);
    lic::complete(system);
    system.emit(&src::IPL_COMPLETE);
    system.show(display::Command::ShowSignOn {
        system: system.system_name(),
        subsystem: "QINTER".to_string(),
        display: "QSCONSOLE".to_string(),
    });
    if system.boot.svc {
        if system.procs.stg.is_some()
            && let Err(e) = procs::svcjob::start(system, tx.clone())
        {
            system.out.serial(&format!("slic: {e}\n"));
        }
        svc::banner(system);
    }
}

/// A line typed on the serial port: to the service console job, or
/// answered here when it is one of `slic`'s own commands or no job runs.
fn console_line(system: &mut System, line: &str) {
    match system.procs.svcjob.as_mut() {
        Some(j) if !j.is_local(line) => {
            if !j.line(line) {
                system.out.serial("slic: QSVCJOB is not answering\n");
            }
        }
        None if system.procs.stg.is_some() && !procs::svcjob::is_own(line) => {
            // The store is the service's now; only QSVCJOB can reach it.
            system
                .out
                .serial("slic: QSVCJOB is not running; only slic's own commands work\n");
            system.out.serial(svc::PROMPT);
        }
        _ => svc::handle(system, line),
    }
}

/// What the service console job sent.
fn job_message(system: &mut System, m: wm::ctl::FromJob) {
    use wm::ctl::FromJob;
    match m {
        FromJob::Output { text } => system.out.serial(&text),
        FromJob::Done {
            prompt,
            srcs,
            sysvals,
        } => {
            for code in srcs {
                if let Some(s) = src::lookup(&code) {
                    system.emit(s);
                }
            }
            if let Some(blob) = sysvals {
                sysvals::adopt(system, &blob);
            }
            if let Some(j) = system.procs.svcjob.as_mut() {
                j.busy = false;
                j.prompt.clone_from(&prompt);
            }
            system.out.serial(&prompt);
        }
        FromJob::Ending { reason } => {
            system.out.serial(&format!("slic: QSVCJOB ending: {reason}\n"));
        }
        FromJob::Ready { .. } => {}
    }
}

/// Children that exited: the storage service ending is fatal, the
/// service console job is started again.
fn reaped(system: &mut System, pids: &[u32], tx: &mpsc::Sender<Event>) {
    for &pid in pids {
        if system
            .procs
            .stg
            .as_ref()
            .is_some_and(|p| p.pid == pid && !p.closing)
        {
            system.procs.stg = None;
            system.fatal(&src::STG_ENDED);
        }
        if system.procs.svcjob.as_ref().is_some_and(|j| j.pid == pid) {
            system.procs.svcjob = None;
            system
                .out
                .serial(&format!("slic: QSVCJOB ({pid}) ended; starting it again\n"));
            if let Err(e) = procs::svcjob::start(system, tx.clone()) {
                system.out.serial(&format!("slic: {e}\n"));
            }
            system.out.serial(svc::PROMPT);
        }
    }
}

/// Serve events forever. Power down leaves through `power::down`.
fn run_loop(system: &mut System, rx: &mpsc::Receiver<Event>, tx: &mpsc::Sender<Event>) -> ! {
    loop {
        let wait = if storage::busy(system) {
            Duration::from_millis(20)
        } else {
            Duration::from_secs(5)
        };
        match rx.recv_timeout(wait) {
            Err(mpsc::RecvTimeoutError::Timeout) => storage::tick(system),
            Ok(Event::Signal(s)) if s == libc_sigchld() => {
                let pids = sys::reap();
                reaped(system, &pids, tx);
            }
            Ok(Event::Signal(s)) if s == libc_sigusr1() => power::down(system, false),
            // SIGTERM and SIGINT are ignored by PID 1; the reaper restarts
            // a job whose channel closed.
            Ok(Event::Signal(_) | Event::JobGone) => {}
            Ok(Event::Line(line)) => {
                if system.boot.svc {
                    console_line(system, &line);
                }
            }
            Ok(Event::Job(m)) => job_message(system, m),
            Ok(Event::Uevent(ev)) => {
                if let Some(msg) = system.hw.apply_uevent(&ev) {
                    system.out.serial(&format!("{msg}\n"));
                }
            }
            Ok(Event::Console(input)) => {
                system.out.serial(&format!("slic: console {input:?}\n"));
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
