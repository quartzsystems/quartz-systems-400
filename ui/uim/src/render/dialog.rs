// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the renderer asks the dialog: variable values, list contents
//! and the conditions it cannot evaluate itself.

use std::collections::BTreeMap;

/// The visible part of a list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListState {
    /// The entries from the top of the page on: `(variable, value)`
    /// pairs per entry.
    pub rows: Vec<Vec<(String, String)>>,
    /// The index of the first entry of `rows` in the whole list.
    pub top: usize,
    /// How many entries the list holds, `None` while incomplete.
    pub total: Option<usize>,
    /// Whether the program has added every entry.
    pub complete: bool,
}

impl ListState {
    /// The number of entries known so far.
    #[must_use]
    pub fn count(&self) -> usize {
        self.total.unwrap_or(self.top + self.rows.len())
    }
}

/// The dialog's current state.
pub trait Dialog {
    /// A variable's value, `None` when the dialog has not set it.
    fn var(&self, name: &str) -> Option<String>;
    /// A list's visible entries.
    fn list(&self, name: &str) -> Option<ListState>;
    /// A condition the group does not define (an imported one).
    fn cond(&self, _name: &str) -> bool {
        false
    }
    /// The option typed on an entry, by its index among the entries
    /// shown; `None` shows the entry's own value.
    fn option(&self, _list: &str, _index: usize) -> Option<String> {
        None
    }
    /// Whether the option typed on an entry is not valid (shown in
    /// reverse image).
    fn option_error(&self, _list: &str, _index: usize) -> bool {
        false
    }
}

/// A dialog from maps, for tests and simple callers.
#[derive(Clone, Debug, Default)]
pub struct MapDialog {
    vars: BTreeMap<String, String>,
    lists: BTreeMap<String, ListState>,
    conds: BTreeMap<String, bool>,
}

impl MapDialog {
    /// Empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a variable.
    pub fn set(&mut self, name: &str, value: &str) -> &mut Self {
        self.vars
            .insert(name.to_ascii_uppercase(), value.to_string());
        self
    }

    /// Set a list.
    pub fn set_list(&mut self, name: &str, list: ListState) -> &mut Self {
        self.lists.insert(name.to_ascii_uppercase(), list);
        self
    }

    /// Set an imported condition.
    pub fn set_cond(&mut self, name: &str, value: bool) -> &mut Self {
        self.conds.insert(name.to_ascii_uppercase(), value);
        self
    }
}

impl Dialog for MapDialog {
    fn var(&self, name: &str) -> Option<String> {
        self.vars.get(&name.to_ascii_uppercase()).cloned()
    }

    fn list(&self, name: &str) -> Option<ListState> {
        self.lists.get(&name.to_ascii_uppercase()).cloned()
    }

    fn cond(&self, name: &str) -> bool {
        self.conds
            .get(&name.to_ascii_uppercase())
            .copied()
            .unwrap_or(false)
    }
}
