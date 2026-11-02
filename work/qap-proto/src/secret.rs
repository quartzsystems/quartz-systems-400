// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Passwords and session tokens. Neither prints (`Debug` shows only
//! that a value is there) and both are overwritten when dropped, so a
//! log line or a freed buffer does not hold one.

use std::fmt;
use std::hint::black_box;

use ipc::{Dec, Wire, WireError};

/// The length of a session token.
pub const TOKEN_LEN: usize = 32;

/// A password: a UTF-8 string on the wire.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wrap a password.
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// The password, for checking it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Secret {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(..)")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        let mut b = std::mem::take(&mut self.0).into_bytes();
        b.fill(0);
        black_box(&b);
    }
}

impl Wire for Secret {
    fn put(&self, out: &mut Vec<u8>) {
        self.0.put(out);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        String::get(d).map(Self)
    }
}

/// A session token: 256 random bits, kept by the server.
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct Token(pub [u8; TOKEN_LEN]);

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(..)")
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        self.0.fill(0);
        black_box(&self.0);
    }
}

impl Wire for Token {
    fn put(&self, out: &mut Vec<u8>) {
        self.0.put(out);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        <[u8; TOKEN_LEN]>::get(d).map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_printed() {
        let s = Secret::from("hunter2");
        let t = Token([7; TOKEN_LEN]);
        assert_eq!(format!("{s:?} {t:?}"), "Secret(..) Token(..)");
        assert_eq!(s.expose(), "hunter2");
    }
}
