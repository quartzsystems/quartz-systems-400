// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console as a TN5250E client of the system's own telnet server:
//! once the IPL is complete and `QTVTELNET` listens, the framebuffer
//! console connects as device `QSCONSOLE`, the session's records go to
//! the console's terminal and the operator's keys come back as
//! records. The console then shows the same sign-on, menus and panels
//! as any display; its own screens (SRCs, DST, the interim sign-on)
//! remain for the IPL and for when the server is not there.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpStream};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use console::Command;
use stg::Client;
use tn5250::telnet;
use tn5250::telnet::client::{Client as TelnetClient, Config};

use crate::ipl::Event;
use crate::ipl::system::System;

/// The device the console's session uses.
pub const DEVICE: &str = "QSCONSOLE";

/// The default telnet port.
pub const PORT: u16 = 23;

/// The open session's stream, for `disconnect`.
fn current() -> &'static Mutex<Option<TcpStream>> {
    static LINK: OnceLock<Mutex<Option<TcpStream>>> = OnceLock::new();
    LINK.get_or_init(|| Mutex::new(None))
}

/// The IPL loop's event sender, kept for `svc.tn5250 connect`.
fn events() -> &'static Mutex<Option<Sender<Event>>> {
    static EVENTS: OnceLock<Mutex<Option<Sender<Event>>>> = OnceLock::new();
    EVENTS.get_or_init(|| Mutex::new(None))
}

/// Whether the console is attached to a session.
pub fn attached() -> bool {
    current().lock().is_ok_and(|l| l.is_some())
}

/// The telnet server's port: `PORT(n)` in `QUSRSYS/QTVTELNA`, else 23.
pub fn port(system: &mut System) -> u16 {
    let Some(stg) = system.procs.stg.as_mut() else {
        return PORT;
    };
    let text = stg
        .da_read("QTVTELNA".into(), "QUSRSYS".into())
        .ok()
        .and_then(Result::ok)
        .map(|v| ccsid::from_ebcdic(&v.value).to_ascii_uppercase())
        .unwrap_or_default();
    text.split_once("PORT(")
        .and_then(|(_, r)| r.split(')').next())
        .and_then(|p| p.trim().parse().ok())
        .unwrap_or(PORT)
}

/// Connect the console to the telnet server on `port`: the terminal
/// takes over the display until the session ends, when `events` gets
/// `ConsoleLinkGone`.
pub fn connect(system: &mut System, port: u16, events: &Sender<Event>) -> Result<(), String> {
    if let Ok(mut e) = self::events().lock() {
        *e = Some(events.clone());
    }
    let console = system
        .console
        .clone()
        .ok_or("the console is not attached")?;
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))
        .map_err(|e| format!("telnet port {port}: {e}"))?;
    let _ = stream.set_nodelay(true);
    let mut reader = stream.try_clone().map_err(|e| e.to_string())?;
    let mut replies = stream.try_clone().map_err(|e| e.to_string())?;
    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    *current().lock().map_err(|_| "link lock")? = Some(stream);
    let (rec_tx, rec_rx) = mpsc::channel::<Vec<u8>>();
    let _ = console.send(Command::Attach(rec_tx));
    let to_console = console.clone();
    let gone = events.clone();
    thread::spawn(move || {
        let mut client = TelnetClient::new(Config {
            terminal_type: "IBM-3477-FC".into(),
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
                if to_console.send(Command::Record(r)).is_err() {
                    break;
                }
            }
        }
        let _ = to_console.send(Command::Detach);
        if let Ok(mut l) = current().lock() {
            *l = None;
        }
        let _ = gone.send(Event::ConsoleLinkGone);
    });
    thread::spawn(move || {
        while let Ok(bytes) = rec_rx.recv() {
            if writer.write_all(&telnet::record(&bytes)).is_err() {
                break;
            }
        }
    });
    system
        .out
        .serial(&format!("slic: console: TN5250 session to port {port} as {DEVICE}\n"));
    Ok(())
}

/// End the console's session; the console shows its own screens.
pub fn disconnect() -> bool {
    let Ok(mut l) = current().lock() else {
        return false;
    };
    match l.take() {
        Some(s) => {
            let _ = s.shutdown(Shutdown::Both);
            true
        }
        None => false,
    }
}

/// `svc.tn5250 status|connect|disconnect`.
pub fn command(system: &mut System, rest: &[&str]) -> String {
    let events = self::events().lock().ok().and_then(|e| e.clone());
    match rest {
        [] | ["status"] => {
            if attached() {
                format!("Console attached to the telnet server as {DEVICE}.\n")
            } else {
                "Console not attached; its own screens show.\n".to_string()
            }
        }
        ["connect"] => {
            let Some(events) = events else {
                return "Console link not available before the IPL completes.
".to_string();
            };
            let p = port(system);
            match connect(system, p, &events) {
                Ok(()) => format!("Console attached to port {p} as {DEVICE}.\n"),
                Err(e) => format!("Console not attached: {e}\n"),
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
