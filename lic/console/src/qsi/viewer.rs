// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A viewer's side of the hub's stream ([`super::tee`]): attach, or ask
//! for the screen alone.

use std::io;
use std::time::{Duration, Instant};

use qsipart::console::{self as wire, FromSystem, ToSystem};
use qsipart::frame::{self, Reader};

use super::tee::{Conn, kind, read_frame, timed_out, why};

/// How long the hub has to answer.
const ANSWER_WAIT: Duration = Duration::from_secs(10);

/// What an attach or a screen came to instead.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Another viewer has the console.
    #[error("another viewer has the console")]
    Viewed,
    /// The stream failed, or the hub did not answer.
    #[error("{0}")]
    Io(#[from] io::Error),
}

/// What a viewer sees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seen {
    /// A whole frame.
    Frame(wire::Frame),
    /// What changed.
    Update(wire::Update),
    /// The viewer is detached, and why; the stream is closed.
    Ended(String),
}

/// An attached viewer.
pub struct Viewer<C: Conn> {
    conn: C,
    frames: Reader,
}

fn ask<C: Conn>(conn: &mut C, kind: u16, body: &[u8]) -> io::Result<()> {
    let b = frame::encode(kind, body).map_err(|e| io::Error::other(e.to_string()))?;
    conn.set_write_timeout(Some(ANSWER_WAIT))?;
    conn.write_all(&b)
}

fn no_answer() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "the console did not answer")
}

/// Attach over `conn`, taking the console over from another viewer
/// when `take_over`.
///
/// # Errors
/// [`Error::Viewed`], or the stream failed.
pub fn attach<C: Conn>(mut conn: C, take_over: bool) -> Result<Viewer<C>, Error> {
    ask(&mut conn, kind::ATTACH, &[u8::from(take_over)])?;
    let mut frames = Reader::new();
    let deadline = Instant::now() + ANSWER_WAIT;
    loop {
        let Some(f) = read_frame(&mut conn, &mut frames, deadline)? else {
            return Err(no_answer().into());
        };
        match f.kind {
            kind::ACCEPTED => return Ok(Viewer { conn, frames }),
            kind::REFUSED if f.body.first() == Some(&why::VIEWED) => return Err(Error::Viewed),
            kind::REFUSED => {
                return Err(io::Error::other("the console refused the viewer").into());
            }
            _ => {}
        }
    }
}

/// The screen as it is now, over `conn`.
///
/// # Errors
/// The stream failed or the console did not answer.
pub fn screen<C: Conn>(mut conn: C) -> io::Result<wire::Frame> {
    ask(&mut conn, kind::SCREEN, &[])?;
    let mut frames = Reader::new();
    let deadline = Instant::now() + ANSWER_WAIT;
    let got = loop {
        let Some(f) = read_frame(&mut conn, &mut frames, deadline)? else {
            break Err(no_answer());
        };
        if let Ok(Some(FromSystem::Frame(f))) = FromSystem::decode(&f) {
            break Ok(f);
        }
    };
    conn.close();
    got
}

impl<C: Conn> Viewer<C> {
    /// The next thing seen within `timeout`; `None` when nothing came.
    ///
    /// # Errors
    /// The stream ended or failed.
    pub fn recv(&mut self, timeout: Duration) -> io::Result<Option<Seen>> {
        let deadline = Instant::now() + timeout;
        loop {
            let f = match read_frame(&mut self.conn, &mut self.frames, deadline) {
                Ok(Some(f)) => f,
                Ok(None) => return Ok(None),
                Err(e) if timed_out(&e) => return Ok(None),
                Err(e) => return Err(e),
            };
            if f.kind == kind::ENDED {
                return Ok(Some(Seen::Ended(
                    String::from_utf8_lossy(&f.body).into_owned(),
                )));
            }
            match FromSystem::decode(&f) {
                Ok(Some(FromSystem::Frame(m))) => return Ok(Some(Seen::Frame(m))),
                Ok(Some(FromSystem::Update(m))) => return Ok(Some(Seen::Update(m))),
                Ok(Some(FromSystem::Hello(_)) | None) => {}
                Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidData, e.to_string())),
            }
        }
    }

    /// Send a key or a refresh.
    ///
    /// # Errors
    /// The stream failed.
    pub fn send(&mut self, m: &ToSystem) -> io::Result<()> {
        let b = m
            .encode()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        self.conn.set_write_timeout(Some(ANSWER_WAIT))?;
        self.conn.write_all(&b)
    }

    /// Detach.
    pub fn close(&self) {
        self.conn.close();
    }
}
