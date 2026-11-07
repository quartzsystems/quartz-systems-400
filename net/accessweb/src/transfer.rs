// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The transfer queue of Browse files, per user, as the desktop's
//! Integrated File System window keeps it: uploads and downloads with
//! their mode, how far they got and how they ended. A failed or waiting
//! upload keeps its data, so Retry and Replace write it again; a
//! download that stopped is fetched again, resumed with `Range` by the
//! browser.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Transfers kept per user; the oldest finished ones go first.
pub const KEEP: usize = 50;

/// Which way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// To the system.
    Upload,
    /// From the system.
    Download,
}

/// Line endings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endings {
    /// `LF`.
    Lf,
    /// `CR LF`.
    CrLf,
}

/// Binary, or text converted between the file's CCSID and UTF-8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The bytes as they are.
    Binary,
    /// Text: the file's CCSID and the line endings on the far side.
    Text(u16, Endings),
}

impl Mode {
    /// As the queue shows it.
    pub fn shown(self, dir: Direction) -> String {
        match (self, dir) {
            (Mode::Binary, _) => "Binary".into(),
            (Mode::Text(c, _), Direction::Upload)
            | (Mode::Text(c, Endings::Lf), Direction::Download) => format!("Text, CCSID {c}, LF"),
            (Mode::Text(c, Endings::CrLf), Direction::Download) => {
                format!("Text, CCSID {c}, CR LF")
            }
        }
    }
}

/// Where a transfer is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Queued, not started (a download the browser fetches from its
    /// link).
    Ready,
    /// Under way; `part` names the partial file it is written as.
    Running(Option<String>),
    /// Done.
    Complete,
    /// Stopped by the system: its message ID and text.
    Failed(String, String),
    /// Cancelled before it ended.
    Cancelled,
    /// An upload whose target exists, waiting for Replace or Skip.
    Exists,
    /// Left out because the target existed.
    Skipped,
}

impl State {
    /// Whether it has ended one way or another.
    pub fn finished(&self) -> bool {
        matches!(self, State::Complete | State::Skipped | State::Cancelled)
    }
}

/// One transfer.
#[derive(Clone, Debug)]
pub struct Transfer {
    /// Its number in the user's queue.
    pub id: u32,
    /// The file on the system.
    pub path: String,
    /// Which way.
    pub direction: Direction,
    /// How.
    pub mode: Mode,
    /// Bytes in all.
    pub size: u64,
    /// Bytes moved.
    pub done: u64,
    /// Where it is.
    pub state: State,
    /// An upload's data while it may be written again.
    pub data: Option<Arc<Vec<u8>>>,
}

impl Transfer {
    /// A new transfer of `size` bytes.
    pub fn new(path: &str, direction: Direction, mode: Mode, size: u64) -> Self {
        Self {
            id: 0,
            path: path.to_string(),
            direction,
            mode,
            size,
            done: 0,
            state: State::Ready,
            data: None,
        }
    }
}

/// Every user's queue.
#[derive(Debug, Default)]
pub struct Queue {
    users: Mutex<HashMap<String, (u32, Vec<Transfer>)>>,
}

impl Queue {
    /// `user`'s transfers, oldest first.
    pub fn list(&self, user: &str) -> Vec<Transfer> {
        self.users
            .lock()
            .expect("queue")
            .get(user)
            .map(|q| q.1.clone())
            .unwrap_or_default()
    }

    /// One of `user`'s transfers.
    pub fn get(&self, user: &str, id: u32) -> Option<Transfer> {
        self.list(user).into_iter().find(|t| t.id == id)
    }

    /// Add `t` to `user`'s queue: its number.
    pub fn add(&self, user: &str, mut t: Transfer) -> u32 {
        let mut all = self.users.lock().expect("queue");
        let q = all.entry(user.to_string()).or_default();
        q.0 += 1;
        t.id = q.0;
        q.1.push(t);
        while q.1.len() > KEEP {
            let i = q.1.iter().position(|t| t.state.finished()).unwrap_or(0);
            q.1.remove(i);
        }
        q.0
    }

    /// Change transfer `id` of `user`; whether it is there.
    pub fn update(&self, user: &str, id: u32, f: impl FnOnce(&mut Transfer)) -> bool {
        let mut all = self.users.lock().expect("queue");
        match all
            .get_mut(user)
            .and_then(|q| q.1.iter_mut().find(|t| t.id == id))
        {
            Some(t) => {
                f(t);
                true
            }
            None => false,
        }
    }

    /// Drop `user`'s finished transfers.
    pub fn clear_finished(&self, user: &str) {
        if let Some(q) = self.users.lock().expect("queue").get_mut(user) {
            q.1.retain(|t| !t.state.finished());
        }
    }
}

/// A size as the queue and My files show it: bytes under 1 KB, whole
/// KB under 1 MB, then MB, GB and TB with one decimal (none when it is
/// `.0`). Units are of 1000.
pub fn size(n: u64) -> String {
    if n == 0 {
        return "0".into();
    }
    if n < 1000 {
        return format!("{n} B");
    }
    if n < 1_000_000 {
        return format!("{} KB", (n + 500) / 1000);
    }
    #[allow(clippy::cast_precision_loss)]
    let mut v = n as f64 / 1_000_000.0;
    let mut unit = "MB";
    for u in ["GB", "TB"] {
        if v < 1000.0 {
            break;
        }
        v /= 1000.0;
        unit = u;
    }
    let s = format!("{v:.1}");
    format!("{} {unit}", s.strip_suffix(".0").unwrap_or(&s))
}

/// `1,844`: digits grouped by three.
pub fn grouped(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(size(240_112), "240 KB");
        assert_eq!(size(41_200), "41 KB");
        assert_eq!(size(12_400_000), "12.4 MB");
        assert_eq!(size(88_000_000), "88 MB");
        assert_eq!(size(4_100_000_000), "4.1 GB");
        assert_eq!(size(0), "0");
        assert_eq!(grouped(240_112), "240,112");
        assert_eq!(grouped(412), "412");
        assert_eq!(grouped(1_844), "1,844");
    }

    #[test]
    fn queue_keeps_the_newest() {
        let q = Queue::default();
        for i in 0..KEEP + 3 {
            let mut t = Transfer::new(&format!("/f{i}"), Direction::Download, Mode::Binary, 1);
            t.state = State::Complete;
            q.add("U", t);
        }
        let l = q.list("U");
        assert_eq!(l.len(), KEEP);
        assert_eq!(l[0].path, "/f3");
        assert!(q.update("U", l[0].id, |t| t.done = 1));
        q.clear_finished("U");
        assert!(q.list("U").is_empty());
    }
}
