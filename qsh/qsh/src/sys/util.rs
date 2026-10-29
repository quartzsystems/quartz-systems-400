// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Utilities run in-process: the trait the `qsh-utils` crates
//! implement and the registry a `Sys` looks programs up in.

use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;

use super::stream::{Fd, write_all};
use super::{Sys, SysError};

/// What a utility runs with: its arguments, environment, working
/// directory, umask, descriptors and the system.
pub struct UtilCtx<'a> {
    /// The arguments, `argv[0]` first.
    pub argv: Vec<String>,
    /// The environment.
    pub env: Vec<(String, String)>,
    /// The working directory.
    pub cwd: String,
    /// The file mode creation mask.
    pub umask: u32,
    /// Open descriptors by number.
    pub fds: BTreeMap<u32, Fd>,
    /// The system.
    pub sys: &'a dyn Sys,
}

impl UtilCtx<'_> {
    /// Descriptor `n`.
    pub fn fd(&self, n: u32) -> Option<&Fd> {
        self.fds.get(&n)
    }

    /// Write to standard output.
    pub fn out(&self, s: &str) -> io::Result<()> {
        self.write(1, s.as_bytes())
    }

    /// Write to standard error.
    pub fn err(&self, s: &str) -> io::Result<()> {
        self.write(2, s.as_bytes())
    }

    /// Write bytes to descriptor `n`.
    pub fn write(&self, n: u32, b: &[u8]) -> io::Result<()> {
        match self.fds.get(&n) {
            Some(fd) => write_all(&**fd, b),
            None => Err(io::Error::from(SysError::new(super::Errno::EBADF))),
        }
    }

    /// An environment variable.
    pub fn getenv(&self, name: &str) -> Option<&str> {
        self.env
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// A path made absolute against the working directory.
    pub fn path(&self, p: &str) -> String {
        crate::pattern::path::absolute(&self.cwd, p)
    }
}

/// A program run in-process.
pub trait Utility: Send + Sync {
    /// Run; the result is the exit status.
    fn run(&self, cx: &mut UtilCtx<'_>) -> i32;
}

impl<F> Utility for F
where
    F: Fn(&mut UtilCtx<'_>) -> i32 + Send + Sync,
{
    fn run(&self, cx: &mut UtilCtx<'_>) -> i32 {
        self(cx)
    }
}

/// Utilities by absolute path (`/usr/bin/ls`).
#[derive(Clone, Default)]
pub struct Registry {
    map: BTreeMap<String, Arc<dyn Utility>>,
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a utility at a path.
    pub fn add(&mut self, path: &str, u: Arc<dyn Utility>) {
        self.map.insert(path.to_string(), u);
    }

    /// The utility at a path.
    pub fn get(&self, path: &str) -> Option<Arc<dyn Utility>> {
        self.map.get(path).cloned()
    }

    /// Every registered path.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(String::as_str)
    }
}
