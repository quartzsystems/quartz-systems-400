// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A console's viewer of the system's console over QSI: the console
//! family's stream at ID 0, its pieces applied to a grid, keys sent back.

use std::time::{Duration, Instant};

use qsi::any::{AnyReply, AnyRequest};
use qsi::host::console::{self, In, Out, Pieces};
use qsi::host::model::SYSTEM_ID;
use qsipart::console::{FromSystem, Grid, Key};
use qsisess::{Incoming, StreamId};
use qsisvr::client::Client;

use super::{Peer, WAIT};

/// One console's view of the system's console.
pub(crate) struct View {
    c: Client,
    stream: StreamId,
    pieces: Pieces,
    /// What it shows.
    pub grid: Grid,
    /// Whole frames and updates seen.
    pub frames: usize,
    /// Updates seen.
    pub updates: usize,
    /// Why it was ended, when it was.
    pub ended: Option<String>,
    /// The stream closed.
    pub closed: bool,
}

impl View {
    /// Attach `peer` on a new session; the refusal's identifier when
    /// refused.
    pub(crate) fn attach(
        peer: &Peer,
        port: u16,
        take_over: bool,
    ) -> Result<Result<Self, String>, String> {
        let mut c = peer.connect(port)?;
        let r = c.ask(
            &AnyRequest::Console(console::Request::Attach(console::Attach {
                id: SYSTEM_ID,
                take_over,
            })),
            WAIT,
        )?;
        if let Some((id, _)) = r.refusal() {
            let id = id.to_string();
            c.close();
            return Ok(Err(id));
        }
        match r.decode() {
            Ok(Some(AnyReply::Console(console::Reply::Attach(a)))) if !a.serial => {}
            other => return Err(format!("attach: {other:?}")),
        }
        let stream = r.stream.ok_or("attach opened no stream")?;
        Ok(Ok(Self {
            c,
            stream,
            pieces: Pieces::default(),
            grid: Grid::default(),
            frames: 0,
            updates: 0,
            ended: None,
            closed: false,
        }))
    }

    /// Take in what came for `wait`.
    pub(crate) fn pump(&mut self, wait: Duration) -> Result<(), String> {
        let end = Instant::now() + wait;
        loop {
            self.c.step();
            let stream = self.stream;
            let mut got = Vec::new();
            let mut closed = false;
            self.c.seen.retain(|i| match i {
                Incoming::Data { stream: s, bytes } if *s == stream => {
                    got.extend_from_slice(bytes);
                    false
                }
                Incoming::StreamClosed { stream: s } | Incoming::StreamReset { stream: s, .. }
                    if *s == stream =>
                {
                    closed = true;
                    false
                }
                _ => true,
            });
            if !got.is_empty() {
                let n = u32::try_from(got.len()).unwrap_or(u32::MAX);
                if !self.closed && !closed {
                    self.c.grant(stream, n)?;
                }
                self.pieces.push(&got);
                while let Some(p) = self.pieces.next_piece() {
                    match Out::decode(&p).map_err(|e| e.to_string())? {
                        Out::Frame(f) => {
                            self.frames += 1;
                            self.grid
                                .apply(&FromSystem::Frame(f))
                                .map_err(|e| e.to_string())?;
                        }
                        Out::Update(u) => {
                            self.updates += 1;
                            self.grid
                                .apply(&FromSystem::Update(u))
                                .map_err(|e| e.to_string())?;
                        }
                        Out::Ended(why) => self.ended = Some(why),
                        Out::Serial(_) | Out::Other(_) => {}
                    }
                }
            }
            self.closed |= closed || self.c.is_closed();
            if Instant::now() >= end {
                return Ok(());
            }
        }
    }

    /// The grid once it holds one of `needles`: which, and the text.
    pub(crate) fn wait_any(
        &mut self,
        needles: &[&str],
        within: Duration,
    ) -> Result<(usize, String), String> {
        let until = Instant::now() + within;
        loop {
            self.pump(Duration::from_millis(100))?;
            let t = self.grid.text();
            if let Some(i) = needles.iter().position(|n| t.contains(n)) {
                // Let the rest of the screen arrive.
                self.pump(Duration::from_millis(400))?;
                return Ok((i, self.grid.text()));
            }
            if Instant::now() > until || self.closed {
                return Err(format!(
                    "the viewer shows none of {needles:?} within {within:?}:\n{t}"
                ));
            }
        }
    }

    /// The grid once it holds `needle`.
    pub(crate) fn wait_text(&mut self, needle: &str, within: Duration) -> Result<String, String> {
        self.wait_any(&[needle], within).map(|(_, t)| t)
    }

    /// Send a key.
    pub(crate) fn key(&mut self, k: Key) -> Result<(), String> {
        let p = In::Key(k).piece().map_err(|e| e.to_string())?;
        let n = self.c.send_on(self.stream, &p)?;
        if n != p.len() {
            return Err(format!("the stream took {n} of {} bytes", p.len()));
        }
        self.pump(Duration::from_millis(30))
    }

    /// Type `text`.
    pub(crate) fn typed(&mut self, text: &str) -> Result<(), String> {
        text.chars().try_for_each(|c| self.key(Key::Char(c)))
    }

    /// End the session.
    pub(crate) fn close(self) {
        self.c.close();
    }
}
