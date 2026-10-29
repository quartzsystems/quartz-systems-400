// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shell's descriptor table.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::sys::Fd;

/// Descriptor numbers and their open descriptions.
#[derive(Clone, Default)]
pub struct FdTable {
    map: BTreeMap<u32, Fd>,
}

/// What a descriptor held before a temporary redirection.
pub type Saved = Vec<(u32, Option<Fd>)>;

impl FdTable {
    /// Descriptor `n`.
    pub fn get(&self, n: u32) -> Option<Fd> {
        self.map.get(&n).cloned()
    }

    /// Point `n` at `fd`, returning what it held.
    pub fn set(&mut self, n: u32, fd: Fd) -> Option<Fd> {
        self.map.insert(n, fd)
    }

    /// Close `n`, returning what it held.
    pub fn close(&mut self, n: u32) -> Option<Fd> {
        self.map.remove(&n)
    }

    /// Restore saved descriptors, last saved first.
    pub fn restore(&mut self, saved: Saved) {
        for (n, fd) in saved.into_iter().rev() {
            match fd {
                Some(f) => {
                    self.map.insert(n, f);
                }
                None => {
                    self.map.remove(&n);
                }
            }
        }
    }

    /// The table as a descriptor map for a child.
    pub fn snapshot(&self) -> Vec<(u32, Fd)> {
        self.map.iter().map(|(k, v)| (*k, Arc::clone(v))).collect()
    }
}
