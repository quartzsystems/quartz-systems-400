// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Trap actions by signal (0 is `EXIT`).

use std::collections::BTreeMap;

/// What a trap does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Run this command text.
    Run(String),
    /// Ignore the signal (`trap '' sig`).
    Ignore,
}

/// The trap table.
#[derive(Clone, Debug, Default)]
pub struct Traps {
    map: BTreeMap<i32, Action>,
}

impl Traps {
    /// The action for a signal.
    pub fn get(&self, sig: i32) -> Option<&Action> {
        self.map.get(&sig)
    }

    /// Set or (with `None`) reset an action.
    pub fn set(&mut self, sig: i32, a: Option<Action>) {
        match a {
            Some(a) => {
                self.map.insert(sig, a);
            }
            None => {
                self.map.remove(&sig);
            }
        }
    }

    /// Every action in signal order.
    pub fn iter(&self) -> impl Iterator<Item = (&i32, &Action)> {
        self.map.iter()
    }

    /// The table a subshell starts with: ignored signals stay ignored,
    /// commands are reset.
    #[must_use]
    pub fn for_subshell(&self) -> Self {
        Self {
            map: self
                .map
                .iter()
                .filter(|(_, a)| **a == Action::Ignore)
                .map(|(k, a)| (*k, a.clone()))
                .collect(),
        }
    }
}
