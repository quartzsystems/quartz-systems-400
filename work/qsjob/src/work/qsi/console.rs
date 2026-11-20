// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system's console as the QSI server's console family sees it: a
//! viewer attached to the process that owns the console, its frames
//! and updates out, keys in. Until that source is there every attach
//! and screen is refused `QSI0301`.

use std::io;
use std::time::Duration;

use qsi::host::console::{Capture, In, Out};

/// Why the console could not be had: a message identifier and its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    /// `QSI0601` another viewer has it; `QSI0301` the source is not there.
    pub msgid: String,
    /// In words.
    pub text: String,
}

/// One viewer of the console.
#[derive(Debug)]
pub struct Viewer {
    _private: (),
}

fn not_there() -> Refused {
    Refused {
        msgid: "QSI0301".into(),
        text: "The system's console cannot be reached.".into(),
    }
}

/// Attach to the console; with `take_over`, in place of another viewer.
///
/// # Errors
/// Another viewer has it, or the source is not there.
pub fn attach(take_over: bool) -> Result<Viewer, Refused> {
    let _ = take_over;
    Err(not_there())
}

/// What the console shows now.
///
/// # Errors
/// The source is not there.
pub fn screen() -> Result<Capture, Refused> {
    Err(not_there())
}

// The source's shape before it is there.
#[allow(
    clippy::unused_self,
    clippy::unnecessary_wraps,
    clippy::needless_pass_by_value
)]
impl Viewer {
    /// The next thing from the console, waiting up to `timeout`.
    ///
    /// # Errors
    /// The source went away.
    pub fn recv(&mut self, timeout: Duration) -> io::Result<Option<Out>> {
        let _ = timeout;
        Ok(None)
    }

    /// A key, a refresh or bytes for the console.
    ///
    /// # Errors
    /// The source went away.
    pub fn send(&mut self, input: In) -> io::Result<()> {
        let _ = input;
        Ok(())
    }
}
