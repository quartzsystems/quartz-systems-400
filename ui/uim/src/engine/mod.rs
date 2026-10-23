// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The dialog engine: a panel group's variables, lists and conditions,
//! panels rendered through [`crate::render`] onto a [`Terminal`], and
//! the operator's keys turned into what the caller acts on (an option
//! typed on a list entry, a command, a menu option, a `RETURN` number,
//! exit or cancel). Paging, help, confirmation, position-to, subsets,
//! `F24=More keys` and the message line are the engine's.
//! `docs/UIM.md` §8 describes it.

mod help;
mod show;

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use crate::model::{Key, PanelGroup, SearchTopic};
use crate::render::{Dialog, ListState, Options, Screen};

pub use show::Event;

/// A list's entries and page.
#[derive(Clone, Debug, Default)]
pub struct UimList {
    /// Every entry, `(variable, value)` pairs.
    pub rows: Vec<Vec<(String, String)>>,
    /// The index of the first entry on the page, among the entries shown.
    pub top: usize,
    /// The program has added every entry.
    pub complete: bool,
    /// Options typed on entries, by entry index.
    pub options: BTreeMap<usize, String>,
    /// Entries whose typed option is not valid, by entry index.
    pub errors: BTreeSet<usize>,
    /// Entry handles for the APIs, parallel to `rows` once assigned.
    pub handles: Vec<u32>,
    /// The next handle to hand out.
    pub next_handle: u32,
    /// Only these entries show (a confirmation panel).
    pub only: Option<BTreeSet<usize>>,
    /// A subset: entries whose variable starts with the value.
    pub subset: Vec<(String, String)>,
    /// The entry the API's `SAME` positioning refers to.
    pub current: Option<usize>,
}

impl UimList {
    /// Give every entry a handle.
    pub fn sync_handles(&mut self) {
        if self.next_handle == 0 {
            self.next_handle = 1;
        }
        while self.handles.len() < self.rows.len() {
            self.handles.push(self.next_handle);
            self.next_handle += 1;
        }
        self.handles.truncate(self.rows.len());
    }

    /// The entry with a handle.
    #[must_use]
    pub fn by_handle(&self, handle: u32) -> Option<usize> {
        self.handles.iter().position(|h| *h == handle)
    }

    /// Insert an entry at `index`; its new handle.
    pub fn insert(&mut self, index: usize, row: Vec<(String, String)>) -> u32 {
        self.sync_handles();
        let index = index.min(self.rows.len());
        self.rows.insert(index, row);
        let h = self.next_handle;
        self.next_handle += 1;
        self.handles.insert(index, h);
        self.shift_marks(index, true);
        h
    }

    /// Remove the entry at `index`.
    pub fn remove(&mut self, index: usize) {
        self.sync_handles();
        if index >= self.rows.len() {
            return;
        }
        self.rows.remove(index);
        self.handles.remove(index);
        self.options.remove(&index);
        self.errors.remove(&index);
        self.shift_marks(index, false);
        if self.current == Some(index) {
            self.current = None;
        }
    }

    /// Typed options and errors after `index` move by one.
    fn shift_marks(&mut self, index: usize, up: bool) {
        let move_key = |k: usize| {
            if k < index {
                k
            } else if up {
                k + 1
            } else {
                k - 1
            }
        };
        self.options = std::mem::take(&mut self.options)
            .into_iter()
            .filter(|(k, _)| up || *k != index)
            .map(|(k, v)| (move_key(k), v))
            .collect();
        self.errors = std::mem::take(&mut self.errors)
            .into_iter()
            .filter(|k| up || *k != index)
            .map(move_key)
            .collect();
        if let Some(c) = self.current
            && c >= index
            && (up || c > index)
        {
            self.current = Some(move_key(c));
        }
    }

    /// The indexes of the entries shown, in order.
    #[must_use]
    pub fn visible(&self) -> Vec<usize> {
        (0..self.rows.len())
            .filter(|i| self.only.as_ref().is_none_or(|o| o.contains(i)))
            .filter(|i| {
                self.subset.iter().all(|(var, want)| {
                    let have = self.rows[*i]
                        .iter()
                        .find(|(v, _)| v == var)
                        .map_or("", |(_, x)| x.as_str());
                    subset_matches(have, want)
                })
            })
            .collect()
    }
}

/// Whether a value passes a subset: blank and `*ALL` pass everything,
/// `ABC*` is a prefix, anything else compares with trailing blanks
/// ignored.
pub(crate) fn subset_matches(have: &str, want: &str) -> bool {
    let want = want.trim();
    if want.is_empty() || want.eq_ignore_ascii_case("*ALL") {
        return true;
    }
    let have = have.trim_end().to_ascii_uppercase();
    let want = want.to_ascii_uppercase();
    match want.strip_suffix('*') {
        Some(prefix) => have.starts_with(prefix),
        None => have == want,
    }
}

/// An option typed on a list entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    /// The list.
    pub list: String,
    /// The entry's index in the whole list.
    pub row: usize,
    /// What was typed.
    pub option: String,
    /// The `:LISTACT.` it matched, with its action.
    pub action: Option<crate::model::Action>,
}

/// What the operator did that the caller must act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Response {
    /// `RETURN n` (Enter without anything typed returns the panel's
    /// `ENTER` number, 0 when it has none).
    Return(u32),
    /// `EXIT` (F3).
    Exit,
    /// `CANCEL` (F12).
    Cancel,
    /// A command typed on the command line, or a `CMD` action.
    Command(String),
    /// A `MENU name` action: a menu option or a key.
    Menu(String),
    /// Another panel of the group (`PANEL name`).
    Panel(String),
    /// Options typed on list entries, in list order.
    Options(Vec<Choice>),
    /// `RETRIEVE` (F9): the caller fills the command line.
    Retrieve,
    /// `PRINT` when the terminal does not print.
    Print,
    /// The display is gone or the job is to end.
    Gone,
}

/// Why a panel could not be shown.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// The renderer refused.
    #[error(transparent)]
    Render(#[from] crate::RenderError),
}

/// What the operator sent back after a screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Input {
    /// The key, `None` for one the engine has no use for.
    pub key: Option<Key>,
    /// Where the cursor was, 1-based.
    pub cursor: (usize, usize),
    /// The fields' contents, one per `Screen::fields` entry.
    pub values: Vec<String>,
}

/// One entry of a search index, as the help index shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexEntry {
    /// The topic's title.
    pub title: String,
    /// The words that find it, upper case.
    pub roots: Vec<String>,
    /// The panel group holding the help, `LIB/NAME`.
    pub pnlgrp: String,
    /// The help module.
    pub module: String,
}

impl IndexEntry {
    /// The entries a group's `:ISCH.` topics make.
    #[must_use]
    pub fn of_group(group: &PanelGroup, qualified: &str) -> Vec<Self> {
        let mut out = Vec::new();
        for h in &group.helps {
            for t in &h.topics {
                let SearchTopic { roots, text } = t;
                let title = crate::model::Span::plain(text);
                out.push(Self {
                    title: if title.trim().is_empty() {
                        crate::model::Span::plain(&h.title).trim().to_string()
                    } else {
                        title.trim().to_string()
                    },
                    roots: roots.iter().map(|r| r.to_ascii_uppercase()).collect(),
                    pnlgrp: qualified.to_ascii_uppercase(),
                    module: h.name.clone(),
                });
            }
        }
        out
    }
}

/// The screen the engine draws on and what it may ask of its owner.
pub trait Terminal {
    /// Whether the screen is 27 by 132.
    fn wide(&mut self) -> bool;
    /// Show a screen and wait for the operator; `None` when the display
    /// is gone.
    fn exchange(&mut self, screen: &Screen) -> Option<Input>;
    /// Another panel group, for qualified help names (`LIB/GROUP/MODULE`
    /// or `GROUP/MODULE`) and the Information Assistant.
    fn group(&mut self, _lib: &str, _name: &str) -> Option<Rc<PanelGroup>> {
        None
    }
    /// A search index's entries.
    fn search_index(&mut self, _lib: &str, _name: &str) -> Option<Vec<IndexEntry>> {
        None
    }
    /// Print rows to `QSYSPRT`: the message to show, or why not.
    fn print(&mut self, _title: &str, _rows: &[String]) -> Result<String, String> {
        Err(PRINT_UNAVAILABLE.to_string())
    }
}

/// The message when the terminal cannot print.
pub const PRINT_UNAVAILABLE: &str = "Printing is not available for this display.";

/// The command line's value when `:CMDLINE.` names no variable.
pub const CMDLINE_VAR: &str = "*CMDLINE";

/// The dialog of one panel group.
#[allow(clippy::struct_excessive_bools)] // independent switches
pub struct Engine {
    /// The group.
    pub group: Rc<PanelGroup>,
    vars: BTreeMap<String, String>,
    lists: BTreeMap<String, UimList>,
    conds: BTreeMap<String, bool>,
    /// The list view, from 0.
    pub view: usize,
    /// The page of a data, menu or information area, from 0.
    pub page: usize,
    /// `F24` pressed: the second set of keys shows.
    pub more_keys: bool,
    /// How many times `F24` was pressed: the key set that shows, for a
    /// key list with more than two.
    pub key_set: usize,
    /// The message for the message line, and whether it is an error.
    pub message: Option<(String, bool)>,
    /// The system name for `TOPSEP=SYSNAM`.
    pub system: String,
    /// Show the group's copyright on the message line when no message.
    pub copyright: bool,
    /// Where the cursor was when the operator last pressed a key.
    pub cursor: (usize, usize),
    /// Help shows full screen rather than in a window.
    pub full_screen_help: bool,
    /// Paging past the end of an incomplete list returns to the caller
    /// ([`Event::More`]) instead of stopping there.
    pub incomplete_returns: bool,
    /// The variable whose field the cursor goes to on the next show
    /// (`QUISETSA`).
    pub cursor_field: Option<String>,
    /// The screen under a pop-up panel or help window.
    pub(crate) under: Option<Screen>,
    /// A menu option's help was shown: show the menu again.
    pub(crate) redisplay: bool,
}

impl Dialog for Engine {
    fn var(&self, name: &str) -> Option<String> {
        self.vars.get(&name.to_ascii_uppercase()).cloned()
    }

    fn list(&self, name: &str) -> Option<ListState> {
        let l = self.lists.get(&name.to_ascii_uppercase())?;
        let visible = l.visible();
        let filtered = visible.len() != l.rows.len();
        Some(ListState {
            rows: visible
                .iter()
                .skip(l.top)
                .map(|i| l.rows[*i].clone())
                .collect(),
            top: l.top,
            total: (l.complete || filtered).then_some(visible.len()),
            complete: l.complete || filtered,
        })
    }

    fn cond(&self, name: &str) -> bool {
        self.conds
            .get(&name.to_ascii_uppercase())
            .copied()
            .unwrap_or(false)
    }

    fn option(&self, list: &str, index: usize) -> Option<String> {
        let l = self.lists.get(&list.to_ascii_uppercase())?;
        let real = *l.visible().get(index)?;
        l.options.get(&real).cloned()
    }

    fn option_error(&self, list: &str, index: usize) -> bool {
        self.lists
            .get(&list.to_ascii_uppercase())
            .and_then(|l| l.visible().get(index).map(|r| l.errors.contains(r)))
            .unwrap_or(false)
    }
}

impl Engine {
    /// A dialog over a group: the variables take their `VALUE`s.
    #[must_use]
    pub fn new(group: Rc<PanelGroup>, system: &str) -> Self {
        let mut vars = BTreeMap::new();
        for v in &group.variables {
            if let Some(val) = &v.value {
                vars.insert(v.name.to_ascii_uppercase(), val.clone());
            }
        }
        Self {
            group,
            vars,
            lists: BTreeMap::new(),
            conds: BTreeMap::new(),
            view: 0,
            page: 0,
            more_keys: false,
            key_set: 0,
            message: None,
            system: system.to_string(),
            copyright: false,
            cursor: (1, 1),
            full_screen_help: false,
            incomplete_returns: false,
            cursor_field: None,
            under: None,
            redisplay: false,
        }
    }

    /// Set a variable.
    pub fn set(&mut self, name: &str, value: &str) {
        self.vars
            .insert(name.to_ascii_uppercase(), value.to_string());
    }

    /// A variable's value, blank when unset.
    #[must_use]
    pub fn get(&self, name: &str) -> String {
        self.var(name).unwrap_or_default()
    }

    /// Set an imported condition.
    pub fn set_cond(&mut self, name: &str, value: bool) {
        self.conds.insert(name.to_ascii_uppercase(), value);
    }

    /// A list, made when first named.
    pub fn list_mut(&mut self, name: &str) -> &mut UimList {
        self.lists.entry(name.to_ascii_uppercase()).or_default()
    }

    /// A list, when it exists.
    #[must_use]
    pub fn list_ref(&self, name: &str) -> Option<&UimList> {
        self.lists.get(&name.to_ascii_uppercase())
    }

    /// Delete a list: it has no entries and no attributes.
    pub fn delete_list(&mut self, name: &str) -> bool {
        self.lists.remove(&name.to_ascii_uppercase()).is_some()
    }

    /// Empty a list.
    pub fn clear_list(&mut self, name: &str) {
        let l = self.list_mut(name);
        l.rows.clear();
        l.handles.clear();
        l.top = 0;
        l.options.clear();
        l.errors.clear();
        l.complete = false;
        l.current = None;
        l.only = None;
    }

    /// Add an entry to a list.
    pub fn add_row(&mut self, name: &str, row: Vec<(String, String)>) {
        self.list_mut(name).rows.push(row);
    }

    /// A list entry's variables.
    #[must_use]
    pub fn row(&self, list: &str, index: usize) -> Vec<(String, String)> {
        self.lists
            .get(&list.to_ascii_uppercase())
            .and_then(|l| l.rows.get(index).cloned())
            .unwrap_or_default()
    }

    /// Text with `&VAR` replaced: the entry's variables first, then the
    /// dialog's. `&&` is a literal ampersand.
    #[must_use]
    pub fn substitute(&self, text: &str, entry: Option<(&str, usize)>) -> String {
        let row = entry.map(|(l, i)| self.row(l, i)).unwrap_or_default();
        let mut out = String::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '&' {
                if chars.get(i + 1) == Some(&'&') {
                    out.push('&');
                    i += 2;
                    continue;
                }
                let start = i + 1;
                let mut end = start;
                while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_')
                {
                    end += 1;
                }
                if end > start {
                    let name: String = chars[start..end]
                        .iter()
                        .collect::<String>()
                        .to_ascii_uppercase();
                    let value = row
                        .iter()
                        .find(|(v, _)| v.eq_ignore_ascii_case(&name))
                        .map(|(_, val)| val.clone())
                        .or_else(|| self.var(&name))
                        .unwrap_or_default();
                    out.push_str(value.trim_end());
                    i = end;
                    if chars.get(i) == Some(&'.') {
                        i += 1;
                    }
                    continue;
                }
            }
            out.push(chars[i]);
            i += 1;
        }
        out
    }

    /// The render options; the message is taken.
    fn options(&mut self, wide: bool) -> Options {
        Options {
            system: self.system.clone(),
            show_copyright: self.copyright,
            copyright: self.group.copyright.clone().unwrap_or_default(),
            wide,
            view: self.view,
            more_keys: self.more_keys,
            key_set: self.key_set,
            message: self.message.take(),
            page: self.page,
        }
    }

    /// A panel as it would show now, as text rows: what printing the
    /// panel (`PRINT`, `QUIPRTP`) writes.
    pub fn image(&mut self, panel: &str, wide: bool) -> Result<Vec<String>, EngineError> {
        let mut opts = self.options(wide);
        opts.message = None;
        let screen = crate::render(&self.group, panel, self, &opts)?;
        Ok(screen
            .text_rows()
            .into_iter()
            .map(|r| r.trim_end().to_string())
            .collect())
    }
}

#[cfg(test)]
mod tests;
