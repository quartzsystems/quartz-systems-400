// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The oracle's display: one 5250 session with pub400 through the
//! driver, signed on automatically (straight to the menu), where a
//! member with `keys:` is called. Each screen the program reads is
//! captured as text rows, answered with the member's next step, and
//! the session goes back to the command line when the program ends.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use tn5250::term::Key;
use tn5250_driver::env::Env;
use tn5250_driver::session::Session;
use tn5250_driver::{Connect, Encoding, ModelArg, Target};

use rpg_oracle::keys;
use rpg_oracle::outcome::{Run, Screen, screen_rows};

/// How long the host may take to answer a key.
const WAIT: Duration = Duration::from_secs(30);

/// The command line's prompt on a menu.
const PROMPT: &str = "===>";

/// A signed-on session.
pub struct Display {
    s: Session,
}

impl Display {
    /// Sign on and prepare the job: the work library in the list,
    /// inquiries answered with their defaults.
    pub fn open(lib: &str) -> Result<Self> {
        let env = Env::load()?;
        let c = env.credentials(Target::Pub400, None)?;
        let connect = Connect {
            target: Target::Pub400,
            host: c.host,
            port: c.port,
            user: c.user,
            password: c.password,
            auto_signon: true,
            encoding: Encoding::Sha,
            device: String::new(),
            model: ModelArg::M3179,
            gui: false,
            trace: false,
            hex: false,
        };
        let s = Session::connect(&connect).context("the 5250 session")?;
        let mut d = Self { s };
        d.back_to_menu()?;
        d.command(&format!("ADDLIBLE LIB({lib}) POSITION(*FIRST)"))?;
        // The kit's job: inquiries take their defaults, numbers edit
        // with a decimal point.
        d.command("CHGJOB INQMSGRPY(*DFT) DECFMT(*BLANK)")?;
        Ok(d)
    }

    /// Wait for the keyboard and a quiet line.
    fn settle(&mut self) -> Result<()> {
        if !self.s.wait_unlocked(WAIT)? {
            bail!("the keyboard stayed locked");
        }
        self.s.settle(Duration::from_millis(400))?;
        Ok(())
    }

    /// Get to a command line: past any screen the sign-on shows.
    fn back_to_menu(&mut self) -> Result<()> {
        for _ in 0..5 {
            self.settle()?;
            if self.s.term.contains(PROMPT) {
                return Ok(());
            }
            self.s.key(Key::Enter)?;
        }
        bail!("no command line after sign-on")
    }

    /// The command line's field start, row and column.
    fn command_line(&self) -> Option<(usize, usize)> {
        let rows = self.s.term.text_rows();
        let row = rows.iter().rposition(|r| r.contains(PROMPT))?;
        let cols = self.s.term.cols();
        let f = self.s.term.fields().iter().find(|f| f.start / cols == row)?;
        Some((row + 1, f.start % cols + 1))
    }

    /// Type a command on the command line and press Enter.
    fn enter(&mut self, cl: &str) -> Result<()> {
        self.settle()?;
        let (r, c) = self.command_line().context("no command line on the screen")?;
        self.s.term.set_cursor(r, c);
        self.s.key(Key::EraseEof)?;
        self.s.type_str(cl)?;
        self.s.key(Key::Enter)?;
        Ok(())
    }

    /// Run a command and wait for the command line again.
    fn command(&mut self, cl: &str) -> Result<()> {
        self.enter(cl)?;
        self.back_to_menu()
    }

    /// Call a program and answer its screens: how it ended and every
    /// screen it read.
    pub fn call(&mut self, cl: &str, steps: &[String]) -> Result<(Run, Vec<Screen>)> {
        self.enter(cl)?;
        let mut screens = Vec::new();
        for step in steps {
            self.settle()?;
            if !self.program_screen() {
                // The program ended before its last screen.
                break;
            }
            screens.push(screen_rows(&self.s.term.text_rows()));
            let actions = keys::parse(step).map_err(anyhow::Error::msg)?;
            if let Err(e) = keys::play(&mut self.s.term, &actions) {
                // The step does not fit the screen: keep what was seen,
                // end the request.
                eprintln!("step {step:?}: {e}");
                self.s.key(Key::Reset)?;
                self.end_request()?;
                return Ok((Run::Escape("DISPLAY".into()), screens));
            }
            self.s.flush()?;
        }
        // The program ends: back at the command line, with its last
        // message, if any, on the message line.
        let back = self.s.wait_for(WAIT, |t| t.contains(PROMPT) && !t.is_locked())?;
        if !back {
            // Still in the program: end the request.
            screens.push(screen_rows(&self.s.term.text_rows()));
            self.end_request()?;
            return Ok((Run::Escape("CPF1907".into()), screens));
        }
        self.s.settle(Duration::from_millis(400))?;
        let rows = self.s.term.text_rows();
        let msg = rows.last().map(|r| r.trim().to_string()).unwrap_or_default();
        let run = match msg.split_whitespace().next() {
            Some(w) if w.len() == 7 && w.chars().skip(3).all(|c| c.is_ascii_digit()) => Run::Escape(w.to_string()),
            _ => Run::Ok,
        };
        Ok((run, screens))
    }

    /// System Request option 2: end the program, back to the menu.
    fn end_request(&mut self) -> Result<()> {
        self.s.key(Key::SysReq)?;
        self.s.settle(Duration::from_millis(500))?;
        self.s.type_str("2")?;
        self.s.key(Key::Enter)?;
        self.back_to_menu()
    }

    /// Whether the screen is the program's rather than the menu the
    /// call was typed on.
    fn program_screen(&self) -> bool {
        !self.s.term.contains(PROMPT)
    }

    /// Sign off.
    pub fn close(mut self) {
        let _ = self.enter("SIGNOFF");
        let _ = self.s.settle(Duration::from_millis(500));
        self.s.disconnect();
    }
}
