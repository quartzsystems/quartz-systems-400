// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! 5250 sessions (handoff steps 5-6b): the user's saved sessions (the
//! desktop's `.qsd` files), keyboard maps and macros; Verify
//! Connection; and the live display, a TN5250E client of the system's
//! own Telnet server run as the signed-on user.

use std::time::Duration;

use qsweb::msg::Message;

use crate::s5250::files::keymap::KeyMap;
use crate::s5250::files::qsd::SessionFile;
use crate::s5250::files::qsm::MacroFile;
use crate::s5250::screen::View;

/// A saved session.
#[derive(Clone, Debug, PartialEq)]
pub struct Saved {
    /// Its number among the user's sessions (0 for a new one).
    pub id: u32,
    /// The session file.
    pub file: SessionFile,
    /// When it was last started, seconds since 1970 (0: never).
    pub last_used: u64,
    /// Its macros.
    pub macros: Vec<SavedMacro>,
}

/// A macro of a session.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedMacro {
    /// The macro file.
    pub file: MacroFile,
    /// When it was recorded, seconds since 1970.
    pub recorded: u64,
    /// The group or `*PUBLIC` it is shared with; empty for none.
    pub shared: String,
}

/// One check of Verify Connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    /// What was checked.
    pub name: String,
    /// `Some(true)` OK, `Some(false)` failed, `None` skipped.
    pub ok: Option<bool>,
    /// What came back.
    pub message: String,
}

/// Where a live session is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Connected; the keyboard is the user's.
    Connected,
    /// Connected and signing on automatically.
    SigningOn,
    /// Not connected: the last screen stays.
    Disconnected,
}

/// A live session's screen and state.
#[derive(Clone, Debug, PartialEq)]
pub struct Live {
    /// The screen as it draws.
    pub view: View,
    /// Where it is.
    pub state: State,
    /// Why it is disconnected, or what the last action said.
    pub message: Option<Message>,
    /// Counts every change of the screen.
    pub version: u64,
}

/// What the user does to a live session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    /// A 5250 function by its keyboard-map name (`Enter`, `F:3`, ...).
    Key(String),
    /// Characters typed at the cursor.
    Text(String),
    /// The cursor moved to a 1-based row and column.
    Cursor(usize, usize),
}

/// 5250 sessions.
pub trait Session5250: Send + Sync {
    /// `user`'s saved sessions, by number.
    fn sessions(&self, user: &str) -> Vec<Saved>;
    /// Keep a session; a new one (`id` 0) gets the next number. The
    /// number it is kept under.
    fn put(&self, user: &str, saved: Saved) -> Result<u32, Message>;
    /// Delete a saved session.
    fn delete(&self, user: &str, id: u32) -> Result<(), Message>;
    /// The keyboard maps `user` has loaded, by name.
    fn keymaps(&self, user: &str) -> Vec<(String, KeyMap)>;
    /// Keep a keyboard map under `name`.
    fn put_keymap(&self, user: &str, name: &str, map: &KeyMap) -> Result<(), Message>;
    /// Verify Connection's checks of `system`; `sign_on` adds a test
    /// sign-on as `user`.
    fn verify(&self, user: &str, system: &str, sign_on: bool) -> Vec<Check>;
    /// Connect session `saved` as `user`.
    fn connect(&self, user: &str, saved: &Saved) -> Result<(), Message>;
    /// The live session `id`, if it was ever started.
    fn live(&self, user: &str, id: u32) -> Option<Live>;
    /// Wait up to `timeout` for the screen to move past `version`.
    fn wait(&self, user: &str, id: u32, version: u64, timeout: Duration) -> Option<Live>;
    /// Send input to the live session.
    fn input(&self, user: &str, id: u32, input: Input) -> Result<(), Message>;
    /// End the live session; the last screen stays.
    fn disconnect(&self, user: &str, id: u32);
}
