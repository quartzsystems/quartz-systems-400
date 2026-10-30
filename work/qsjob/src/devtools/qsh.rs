// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QSH terminal session (`STRQSH`, `QSH` with no command): the
//! `qshterm` display over a `qsh_job::run::Session`. The shell runs on
//! its own thread; this thread serves it while it runs (the IFS, CL, the
//! job's objects) and shows its output as it comes. F12 leaves the
//! session waiting in the job, and the next `STRQSH` resumes it, as
//! IBM's does; F3 (or `exit`) ends it.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use mi::Job;
use qsh_job::run::Session;
use qshterm::{Action, CMD_COL, CMD_ROW, Key, Term};
use tn5250::ds::aid;
use tn5250::screen::Read;

use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::{Surface, Waited};
use crate::display::uim::apply;
use crate::qsh::side::JobSide;

thread_local! {
    /// The job's session, between displays.
    static SESSION: RefCell<Option<(Session, Term)>> = const { RefCell::new(None) };
}

/// How often the display is redrawn while a command writes.
const REDRAW: Duration = Duration::from_millis(500);

fn key_of(a: u8) -> Key {
    match a {
        aid::ENTER | aid::AUTO_ENTER => Key::Enter,
        aid::ROLL_UP => Key::PageDown,
        aid::ROLL_DOWN => Key::PageUp,
        other => aid::function_of(other).map_or(Key::Other, Key::F),
    }
}

/// Draw the display; with `read`, wait for a key: the key and the
/// command line as typed. `poll` runs each second meanwhile and may end
/// the wait (`None` key) to redraw.
fn show(
    surface: &mut Surface,
    term: &Term,
    read: bool,
    poll: &mut dyn FnMut() -> bool,
) -> Option<(Option<Key>, String)> {
    let screen = term.screen();
    surface.with_model(|m| apply(&screen, m));
    if !read {
        surface.flush(Read::None);
        return Some((None, String::new()));
    }
    surface.flush(Read::MdtFields);
    let key = match surface.wait_or(poll)? {
        Waited::Key(a, _) => Some(key_of(a)),
        Waited::Polled => None,
    };
    let typed = surface.with_model(|m| {
        m.field_index_at(CMD_ROW, CMD_COL)
            .map(|i| m.fields()[i].text())
            .unwrap_or_default()
    });
    Some((key, typed))
}

/// Take what the shell wrote into the display.
fn drain(sess: &Session, term: &mut Term) -> bool {
    let out = sess.take_output();
    if out.is_empty() {
        return false;
    }
    term.output(&out);
    true
}

/// The session: run it at the display until F3, F12 or `exit`.
pub fn session(job: &mut Job, host: &Host) -> Result<String, Fail> {
    let (sess, mut term) = SESSION
        .with(|s| s.borrow_mut().take())
        .unwrap_or_else(|| (Session::start(crate::qsh::setup(host, "5250")), Term::new()));
    let mut surface = Surface::new(host.clone());
    let mut side = JobSide::new(job, host);
    let mut last = Instant::now();
    loop {
        // While a command runs and reads nothing, serve it and redraw.
        loop {
            let st = sess.state();
            if !st.running || st.wants_input || st.exited.is_some() {
                break;
            }
            sess.serve(&mut side, Duration::from_millis(200));
            if drain(&sess, &mut term) && last.elapsed() >= REDRAW {
                show(&mut surface, &term, false, &mut || false);
                last = Instant::now();
            }
        }
        drain(&sess, &mut term);
        if sess.ended() {
            sess.end(&mut side);
            return Ok(String::new());
        }
        let mut poll = || {
            sess.serve(&mut side, Duration::from_millis(50));
            sess.state().exited.is_some() || sess.has_output()
        };
        let Some((key, typed)) = show(&mut surface, &term, true, &mut poll) else {
            // The display is gone: keep the session for the next STRQSH.
            SESSION.with(|s| *s.borrow_mut() = Some((sess, term)));
            return Ok(String::new());
        };
        let Some(key) = key else {
            continue;
        };
        match term.key(key, &typed) {
            Action::Run(line) => sess.send(&line),
            Action::Exit => {
                sess.end(&mut side);
                return Ok(String::new());
            }
            Action::Disconnect => {
                SESSION.with(|s| *s.borrow_mut() = Some((sess, term)));
                return Ok(String::new());
            }
            Action::Print => {
                let lines = term.lines();
                let m = crate::qsh::cmd::print(host, "QPRINT", &lines)
                    .unwrap_or_else(|e| format!("Nothing was printed: {e}"));
                term.set_message(&m);
            }
            Action::ClEntry => {
                let line = typed.trim().to_string();
                if line.is_empty() {
                    term.set_message("Type a CL command on the command line, then press F21.");
                } else {
                    term.set_input("");
                    let o = crate::command::run(side.job, host, &line);
                    term.output(format!("> {line}\n").as_bytes());
                    term.output(o.text.as_bytes());
                    if !o.text.ends_with('\n') && !o.text.is_empty() {
                        term.output(b"\n");
                    }
                }
            }
            Action::Show => {}
        }
    }
}

/// `QSH CMD()` at a display: the command's output on the terminal
/// display until Enter, F3 or F12.
pub fn show_output(host: &Host, text: &str) {
    let mut term = Term::new();
    term.output(text.as_bytes());
    term.set_message("Press Enter to end the terminal session.");
    let mut surface = Surface::new(host.clone());
    loop {
        match show(&mut surface, &term, true, &mut || false) {
            Some((Some(Key::Enter | Key::F(3 | 12)), _)) | None => return,
            Some((Some(k), typed)) => {
                let _ = term.key(k, &typed);
                term.set_message("Press Enter to end the terminal session.");
            }
            Some((None, _)) => {}
        }
    }
}
