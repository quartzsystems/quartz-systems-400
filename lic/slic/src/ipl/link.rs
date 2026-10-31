// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console as a 5250 display: once the IPL is complete, the
//! framebuffer console's terminal connects to the console job
//! `QCONSOLE` (`procs::console`) over a local socket, as a TN5250E
//! client of the session that holds device `QSCONSOLE`. The session's
//! records go to the console's terminal and the operator's keys come
//! back as records; no TCP/IP is involved. The console then shows the
//! same sign-on, menus and panels as any display; its own screens
//! (SRCs, DST, the installer) remain for the IPL and for when the link
//! is down. A link that drops is made again every second.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use console::Command;
use tn5250::telnet;
use tn5250::telnet::client::{Client as TelnetClient, Config};
use wm::dev::CONSOLE_TERMINAL;

use crate::ipl::Event;
use crate::ipl::system::System;

/// The device the console's session uses.
pub const DEVICE: &str = "QSCONSOLE";

/// The link's state: the open session's stream and whether a link is
/// wanted (`tn5250 disconnect` and power down say no).
#[derive(Default)]
struct State {
    stream: Option<UnixStream>,
    wanted: bool,
    started: bool,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State::default()))
}

/// Whether the console is attached to its session.
pub fn attached() -> bool {
    state().lock().is_ok_and(|s| s.stream.is_some())
}

fn wanted() -> bool {
    state().lock().is_ok_and(|s| s.wanted)
}

/// Start linking the console to its session; wait up to `wait` for
/// the first screen (the sign-on), so whatever reads the console after
/// the IPL sees it. The link is kept from then on.
pub fn start(system: &mut System, events: &Sender<Event>, wait: Duration) {
    let Some(console) = system.console.clone() else {
        return;
    };
    let first = {
        let Ok(mut s) = state().lock() else {
            return;
        };
        s.wanted = true;
        if s.started {
            return;
        }
        s.started = true;
        let (tx, rx) = mpsc::channel::<()>();
        let events = events.clone();
        thread::spawn(move || supervise(&console, &events, Some(tx)));
        rx
    };
    let shown = first.recv_timeout(wait).is_ok();
    system.out.serial(&format!(
        "slic: console: {}\n",
        if shown {
            format!("display {DEVICE} attached")
        } else {
            format!("display {DEVICE} not ready yet; its own screens show")
        }
    ));
}

/// Connect while a link is wanted; each session runs to its end.
fn supervise(console: &Sender<Command>, events: &Sender<Event>, mut first: Option<Sender<()>>) {
    let mut failing = false;
    loop {
        if !wanted() {
            thread::sleep(Duration::from_millis(500));
            continue;
        }
        match UnixStream::connect(CONSOLE_TERMINAL) {
            Ok(s) => {
                failing = false;
                if session(s, console, &mut first) {
                    let _ = events.send(Event::ConsoleLinkGone);
                }
            }
            Err(e) => {
                if !failing {
                    let _ = events.send(Event::ConsoleLinkDown(e.to_string()));
                }
                failing = true;
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
}

/// One session: records to the terminal, the terminal's records back.
/// Whether it got as far as attaching.
fn session(stream: UnixStream, console: &Sender<Command>, first: &mut Option<Sender<()>>) -> bool {
    let (Ok(mut reader), Ok(mut replies), Ok(mut writer)) =
        (stream.try_clone(), stream.try_clone(), stream.try_clone())
    else {
        return false;
    };
    if let Ok(mut s) = state().lock() {
        s.stream = Some(stream);
    }
    let (rec_tx, rec_rx) = mpsc::channel::<Vec<u8>>();
    let _ = console.send(Command::Attach(rec_tx));
    thread::spawn(move || {
        while let Ok(bytes) = rec_rx.recv() {
            if writer.write_all(&telnet::record(&bytes)).is_err() {
                break;
            }
        }
    });
    let mut client = TelnetClient::new(Config {
        terminal_type: "IBM-3179-2".into(),
        device: DEVICE.into(),
        ..Config::default()
    });
    let mut buf = [0u8; 8192];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let (reply, records) = client.feed(&buf[..n]);
        if !reply.is_empty() && replies.write_all(&reply).is_err() {
            break;
        }
        for r in records {
            if console.send(Command::Record(r)).is_err() {
                break;
            }
            if let Some(f) = first.take() {
                // The console thread has the record queued before this.
                let (tx, rx) = mpsc::channel();
                let _ = console.send(Command::Dump(tx));
                let _ = rx.recv_timeout(Duration::from_secs(2));
                let _ = f.send(());
            }
        }
    }
    let _ = console.send(Command::Detach);
    if let Ok(mut s) = state().lock() {
        s.stream = None;
    }
    true
}

/// End the console's session and keep it ended; the console shows its
/// own screens.
pub fn disconnect() -> bool {
    let Ok(mut s) = state().lock() else {
        return false;
    };
    s.wanted = false;
    match s.stream.take() {
        Some(st) => {
            let _ = st.shutdown(std::net::Shutdown::Both);
            true
        }
        None => false,
    }
}

/// `tn5250 status|connect|disconnect` at the service console.
pub fn command(system: &mut System, rest: &[&str]) -> String {
    match rest {
        [] | ["status"] => {
            if attached() {
                format!("Console attached to its session as {DEVICE}.\n")
            } else {
                "Console not attached; its own screens show.\n".to_string()
            }
        }
        ["connect"] => {
            if !system.complete {
                return "Console link not available before the IPL completes.\n".to_string();
            }
            if attached() {
                return format!("Console already attached as {DEVICE}.\n");
            }
            match state().lock() {
                Ok(mut s) if s.started => {
                    s.wanted = true;
                    "Console link wanted; it attaches within a second.\n".to_string()
                }
                _ => "The console job was never started.\n".to_string(),
            }
        }
        ["disconnect"] => {
            if disconnect() {
                "Console detached.\n".to_string()
            } else {
                "Console was not attached.\n".to_string()
            }
        }
        _ => "tn5250 status|connect|disconnect\n".to_string(),
    }
}
