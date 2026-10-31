// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IPL state machine: LIC phase, OS phase, then the run loop that
//! serves signals and the service console until power down.

pub mod clock;
pub mod cmdline;
pub mod display;
pub mod dst;
pub mod ifs;
pub mod install;
pub mod lic;
pub mod media;
pub mod link;
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
    /// A message from the job signed on at the console display.
    ConsoleJob(wm::ctl::FromJob),
    /// That job's channel closed.
    ConsoleGone(u32),
    /// The console's TN5250 session to the telnet server ended.
    ConsoleLinkGone,
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
    let files = sys::raise_file_limit();
    let mounted = mounts::mount_all();

    let mut system = System::new(console::Outputs::open(), cmdline::Boot::read());
    system.emit(&src::IPL_STARTED);
    system.out.both(&format!("{}\n", release::banner()));
    if !files {
        system.out.serial("slic: open-file limit not raised\n");
    }
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

/// `QIPLTYPE` 1 or 2 at a normal keylock: an attended IPL, which shows
/// the IPL menu as a manual one does. 2 (console in debug mode) is 1
/// here; the service console is the build's (`qs400.svc`).
fn attended(system: &System) -> bool {
    let Some((_, record)) = system.lic.as_ref() else {
        return false;
    };
    let host = crate::ipl::sysvals::host_info();
    sysval::SysVals::from_record(record, &host)
        .ok()
        .and_then(|v| v.get("QIPLTYPE").ok())
        .is_some_and(|v| matches!(v.text().trim(), "1" | "2"))
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

    if (system.mode == IplMode::Manual || attended(system)) && !installing {
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
    // PING and TRACEROUTE use ICMP datagram sockets, which the kernel
    // allows only to the groups in this range: every group may.
    if cfg!(target_os = "linux")
        && let Err(e) = std::fs::write("/proc/sys/net/ipv4/ping_group_range", "0 2147483647\n")
    {
        system.out.serial(&format!("slic: ping_group_range: {e}\n"));
    }
    system.emit(&src::NET_ENUMERATED);
    net_address(system);
    procs::device_dir();
    Ok(())
}

/// The address of the first Ethernet link: `qs400.ip=A.B.C.D/N` on the
/// command line, else the address QEMU's user network expects. It is
/// there before TCP/IP starts; `STRTCP` adds the configured interfaces
/// beside it. `BLOCKED(phase10)`: DHCP (no client yet, `*IP4DHCP`).
fn net_address(system: &mut System) {
    // A link with a device behind it: the kernel makes bond0 and dummy0
    // itself (bonding and dummy are built in), and neither reaches a wire.
    let device = |n: &str| std::path::Path::new(&format!("/sys/class/net/{n}/device")).exists();
    let Some(link) = system
        .links
        .iter()
        .find(|l| device(&l.name))
        .or_else(|| {
            system.links.iter().find(|l| {
                l.name != "lo" && !["sit", "bond", "dummy"].iter().any(|p| l.name.starts_with(p))
            })
        })
    else {
        return;
    };
    let (index, name) = (link.index, link.name.clone());
    let spec = system
        .boot
        .ip
        .clone()
        .unwrap_or_else(|| "10.0.2.15/24".into());
    let Some((addr, prefix)) = spec.split_once('/') else {
        return;
    };
    let octets: Vec<u8> = addr.split('.').filter_map(|o| o.parse().ok()).collect();
    let Ok(prefix) = prefix.parse::<u8>() else {
        return;
    };
    if octets.len() != 4 {
        return;
    }
    let ip = [octets[0], octets[1], octets[2], octets[3]];
    if !link.is_up()
        && let Err(e) = net::set_up(index)
    {
        system.out.serial(&format!("slic: {name} up: {e}\n"));
    }
    match net::add_address(index, ip, prefix) {
        Ok(()) => system.out.serial(&format!("slic: {name} {spec}\n")),
        Err(e) => system.out.serial(&format!("slic: {name} {spec}: {e}\n")),
    }
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
        let media = media::disk_on_d_ipl(system.boot.ipl_d);
        let spare = diskimg::locate::whole_disks()
            .unwrap_or_default()
            .iter()
            .filter(|d| !d.has_qs400 && media.as_deref() != Some(d.name.as_str()))
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
    // The IFS root's filesystem, mounted before the storage service
    // starts so the service inherits it.
    ifs::mount(system);
    // The storage service takes the store over; from here on the jobs
    // and the console reach it through the service.
    if system.storage.is_some()
        && let Err(e) = procs::stg::start(system)
    {
        system.out.serial(&format!("slic: {e}\n"));
        system.fatal(&src::STG_ENDED);
    }
    if system.procs.stg.is_some() {
        // The service has made /QOpenSys on a new IFS by now.
        ifs::mount_qopensys(system);
        start_work(system);
    } else {
        system.emit(&src::STRUP_SKIPPED);
    }
    lic::complete(system);
    system.emit(&src::IPL_COMPLETE);
    // The console becomes a terminal of the telnet server; its own
    // sign-on display stands in when the server is not there.
    let port = link::port(system);
    if let Err(e) = link::connect(system, port, tx) {
        system.out.serial(&format!("slic: console: {e}; the interim sign-on shows
"));
        procs::console::sign_on(system, "");
    }
    audit_lic(system);
    if system.procs.stg.is_some()
        && let Err(e) = procs::svcjob::start(system, tx.clone())
    {
        system.out.serial(&format!("slic: {e}\n"));
    }
    if system.boot.svc {
        // Development images sign the serial console on for the harness.
        system.svc_user = Some("*DEV".into());
        svc::banner(system);
    } else {
        system.out.serial(
            "Service console: sign on with a service tools user ID: signon USER PASSWORD\n",
        );
    }
}

/// Hand the service tools events of the IPL to the storage service for
/// the audit journal.
pub fn audit_lic(system: &mut System) {
    if system.audit_lic.is_empty() {
        return;
    }
    let events = std::mem::take(&mut system.audit_lic);
    if let Some(stg) = system.procs.stg.as_mut() {
        let _ = stg::Client::audit_lic(stg, events);
    }
}

/// `signon USER PASSWORD` on the serial console: a service tools user
/// ID, as DST checks it.
fn serial_sign_on(system: &mut System, line: &str) {
    let mut w = line.split_whitespace();
    if w.next() != Some("signon") {
        system.out.serial("Sign on first: signon USER PASSWORD\n");
        return;
    }
    let user = w.next().unwrap_or("").to_ascii_uppercase();
    let password = w.next().unwrap_or("");
    let Some((part, record)) = system.lic.as_mut() else {
        system
            .out
            .serial("The service tools user IDs are not available.\n");
        return;
    };
    // SST may have changed the user IDs since this copy was read.
    part.refresh(record);
    let Some(mut users) = svctools::UserDb::load(record) else {
        system
            .out
            .serial("The service tools user IDs are not available.\n");
        return;
    };
    let r = users.sign_on(&user, password).map(|_| ());
    if users.store(record).is_ok() {
        let _ = part.write(record);
    }
    match r {
        Ok(()) => {
            system
                .audit_lic
                .push(("ST".into(), "SC".into(), user.clone()));
            system
                .out
                .serial(&format!("Service tools user {user} signed on.\n"));
            system.svc_user = Some(user);
            audit_lic(system);
            system.out.serial(svc::PROMPT);
        }
        Err(e) => {
            system.audit_lic.push(("PW".into(), "X".into(), user));
            audit_lic(system);
            system
                .out
                .serial(&format!("{}\n", svctools::dst::signon::message(&e)));
        }
    }
}

/// What `SCPF` starts before the IPL completes: the system jobs,
/// `QSYSWRK`, the controlling subsystem, and the startup program's job
/// run to its end.
fn start_work(system: &mut System) {
    let n = procs::sysjobs::start_system(system);
    system
        .out
        .serial(&format!("slic: {n} system jobs started\n"));
    system.emit(&src::SYSJOBS_STARTED);
    if let Err(e) = procs::sysjobs::start_subsystem(system, "QSYSWRK") {
        system.out.serial(&format!("slic: QSYSWRK: {e}\n"));
    }
    let ctl = procs::console::controlling(system);
    if let Err(e) = procs::sysjobs::start_subsystem(system, &ctl) {
        system.out.serial(&format!("slic: {ctl}: {e}\n"));
        system.emit(&src::STRUP_SKIPPED);
        return;
    }
    system.emit(&src::CTLSBS_STARTED);
    let none = system
        .sysvals
        .as_ref()
        .and_then(|v| v.get("QSTRUPPGM").ok())
        .is_some_and(|v| v.text().trim_start().starts_with("*NONE"));
    if none {
        system.emit(&src::STRUP_SKIPPED);
        return;
    }
    let ran = procs::sysjobs::wait_job(
        system,
        procs::sysjobs::STARTUP_JOB,
        std::time::Duration::from_secs(30),
        std::time::Duration::from_secs(120),
    );
    if !ran {
        system
            .out
            .serial("slic: the startup job did not run to its end\n");
    }
    system.emit(&src::STRUP_ENDED);
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
            system
                .out
                .serial(&format!("slic: QSVCJOB ending: {reason}\n"));
        }
        FromJob::PowerDown {
            immediate,
            delay,
            restart,
            iplsrc,
        } => power::down_ending(system, restart, immediate, delay, iplsrc),
        FromJob::Ready { .. }
        | FromJob::Show { .. }
        | FromJob::Disconnect
        | FromJob::SecondaryJob
        | FromJob::GroupTransfer { .. }
        | FromJob::GroupStart { .. }
        | FromJob::GroupEnd { .. } => {}
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
        procs::console::gone(system, pid);
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
    if system.boot.svc && system.svc_user.is_none() {
        system.svc_user = Some("*DEV".into());
    }
    loop {
        let wait = if storage::busy(system) {
            Duration::from_millis(20)
        } else {
            Duration::from_secs(5)
        };
        match rx.recv_timeout(wait) {
            Err(mpsc::RecvTimeoutError::Timeout) => {
                storage::tick(system);
                procs::console::inactivity(system);
            }
            Ok(Event::Signal(s)) if s == libc_sigchld() => {
                let pids = sys::reap();
                reaped(system, &pids, tx);
            }
            Ok(Event::Signal(s)) if s == libc_sigusr1() => power::requested(system),
            // SIGTERM and SIGINT are ignored by PID 1; the reaper restarts
            // a job whose channel closed.
            Ok(Event::Signal(_) | Event::JobGone) => {}
            Ok(Event::Line(line)) => {
                if system.svc_user.is_some() {
                    console_line(system, &line);
                } else if system.complete {
                    serial_sign_on(system, &line);
                }
            }
            Ok(Event::Job(m)) => job_message(system, m),
            Ok(Event::Uevent(ev)) => {
                if let Some(msg) = system.hw.apply_uevent(&ev) {
                    system.out.serial(&format!("{msg}\n"));
                }
            }
            Ok(Event::Console(input)) => console_input(system, input, tx),
            Ok(Event::ConsoleJob(m)) => procs::console::from_job(system, m),
            Ok(Event::ConsoleGone(pid)) => procs::console::gone(system, pid),
            Ok(Event::ConsoleLinkGone) => {
                system.out.serial("slic: console: the TN5250 session ended
");
                if system.procs.console.is_none() {
                    procs::console::sign_on(system, "");
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

/// Input at the console display: to the signed-on job, or a sign-on.
fn console_input(system: &mut System, input: ::console::Input, tx: &mpsc::Sender<Event>) {
    use ::console::Input;
    match (system.procs.console.as_mut(), input) {
        (Some(j), Input::Enter(fields)) => j.input(0, fields),
        (Some(j), Input::Function(n, fields)) => j.input(n, fields),
        (None, Input::SignOn(fields)) if system.procs.stg.is_some() => {
            procs::console::sign_on_entered(system, &fields, tx.clone());
        }
        (None, Input::SignOn(_)) => {
            procs::console::sign_on(system, "Sign-on is not available until the IPL completes.");
        }
        (_, other) => system.out.serial(&format!("slic: console {other:?}\n")),
    }
}
