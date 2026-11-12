// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One panel of QSDEMO's own, built with the screen kit and shown by
//! the console on the framebuffer and on the serial terminal at once,
//! under QSDEMO's identity; then the bytes a serial terminal would send
//! are decoded into keys and drive it to its end.

use std::io::Read as _;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use console::ansi::{AnsiDisplay, KeyDecoder};
use console::{Command, Console, Identity, Input, Scale, Screen};
use scrkit::panel::PanelFlow;
use scrkit::{End, Flow, Panel, Runner, ui};

/// What the panels ask: the system's name.
struct Env {
    system: &'static str,
}

/// How the application ends.
#[derive(Debug, PartialEq, Eq)]
enum Done {
    /// Option 1.
    Storage,
}

/// The title the build's check looks for on the serial console.
pub const TITLE: &str = "QSDEMO Service Tools";

struct Menu;

impl Panel<Env, Done> for Menu {
    fn name(&self) -> &'static str {
        "qsdemo-menu"
    }

    fn render(&self, env: &mut Env) -> Screen {
        ui::menu(
            TITLE,
            env.system,
            &[(1, "Work with the protected set"), (2, "Work with routes")],
            "F3=Exit   F12=Cancel",
        )
    }

    fn enter(&mut self, fields: &[String], _env: &mut Env) -> PanelFlow<Env, Done> {
        match fields.first().map(|f| f.trim()) {
            Some("1") => Flow::Exit(Done::Storage),
            Some("2") => Flow::Message("The route is added last.".into()),
            _ => Flow::Error("Selection not valid.".into()),
        }
    }
}

/// The screen as the console holds it.
fn dump(tx: &Sender<Command>) -> Result<String, String> {
    let (reply, rx) = mpsc::channel();
    tx.send(Command::Dump(reply)).map_err(|e| e.to_string())?;
    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|e| format!("no dump: {e}"))
}

/// Bytes of the framebuffer that are not black.
fn lit() -> Result<usize, String> {
    let mut frame = Vec::new();
    std::fs::File::open("/dev/fb0")
        .and_then(|mut f| f.read_to_end(&mut frame))
        .map_err(|e| format!("/dev/fb0: {e}"))?;
    Ok(frame.iter().filter(|&&b| b != 0).count())
}

/// The keys of `bytes` typed at the console; the input they end in.
fn type_bytes(
    tx: &Sender<Command>,
    inputs: &Receiver<Input>,
    decoder: &mut KeyDecoder,
    bytes: &[u8],
) -> Result<Input, String> {
    for key in bytes.iter().filter_map(|&b| decoder.feed(b)) {
        tx.send(Command::Virtual(key)).map_err(|e| e.to_string())?;
    }
    inputs
        .recv_timeout(Duration::from_secs(5))
        .map_err(|e| format!("no input after {bytes:?}: {e}"))
}

/// Show the panel and drive it. One line for the console when all of
/// it held.
pub fn prove(banner: &str) -> Result<String, String> {
    let identity = Identity {
        banner: banner.to_string(),
        ready_code: String::new(),
    };
    let (mut con, status) = Console::open_scaled(identity, true, Scale::Auto);
    status.map_err(|e| format!("framebuffer: {e}"))?;
    if !con.has_display() {
        return Err("no framebuffer".into());
    }
    let mode = con.describe();
    let dark = lit()?;
    con.add_display(Box::new(AnsiDisplay::new(std::io::stdout())));
    let (tx, rx) = mpsc::channel();
    let (input_tx, inputs) = mpsc::channel();
    std::thread::spawn(move || con.run(&rx, input_tx));
    let send = |c: Command| tx.send(c).map_err(|e| e.to_string());

    // The SRC display carries the identity.
    let mut env = Env { system: "QSDEMO1" };
    send(Command::ShowSrc {
        system: env.system.to_string(),
        code: "C1001000".into(),
        text: "QSDEMO service tools".into(),
    })?;
    let src = dump(&tx)?;
    if !src.starts_with(banner) || src.contains("QS/400") {
        return Err(format!("the SRC display is not QSDEMO's:\n{src}"));
    }

    let mut runner = Runner::new(Box::new(Menu));
    send(Command::ShowScreen(runner.screen(&mut env)))?;
    let shown = dump(&tx)?;
    if !shown.contains(TITLE) || !shown.contains("System:  QSDEMO1") {
        return Err(format!("the panel is not shown:\n{shown}"));
    }
    let drawn = lit()?;
    if drawn <= dark {
        return Err(format!(
            "the framebuffer shows nothing: {drawn} bytes lit, {dark} before"
        ));
    }

    // A terminal's bytes: a wrong selection, option 2, F5, option 1.
    let mut decoder = KeyDecoder::new();
    let mut end = None;
    for (bytes, want) in [
        (&b"9\r"[..], "Selection not valid."),
        (b"2\r", "The route is added last."),
        (b"\x1b[15~", ""),
        (b"1\r", ""),
    ] {
        end = match type_bytes(&tx, &inputs, &mut decoder, bytes)? {
            Input::Enter(fields) => runner.enter(&fields, &mut env),
            Input::Function(key, _) => runner.function(key, &mut env),
        };
        if end.is_some() {
            break;
        }
        send(Command::ShowScreen(runner.screen(&mut env)))?;
        let now = dump(&tx)?;
        let message = now.lines().nth(23).unwrap_or("").trim();
        if message != want {
            return Err(format!("after {bytes:?} the message line is {message:?}"));
        }
    }
    // The terminal's cursor below the grid, for what is printed next.
    println!("\x1b[0m\x1b[24;1H");
    if end != Some(End::Outcome(Done::Storage)) {
        return Err(format!("the keys ended the panel with {end:?}"));
    }
    Ok(format!(
        "`{TITLE}` on the framebuffer ({mode}; {drawn} bytes lit) and on the serial \
         terminal; a terminal's keys ended it with {:?}",
        Done::Storage
    ))
}
