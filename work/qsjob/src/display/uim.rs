// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The UIM dialog engine: a panel group's variables, lists and
//! conditions, the panel rendered through `uim::render` onto the job's
//! screen, and the operator's keys turned into what the caller acts
//! on (an option typed on a list entry, a command on the command line,
//! a menu option, a `RETURN` number, exit or cancel). Paging, help,
//! `F24=More keys` and the message line are the engine's.

use std::collections::BTreeMap;
use std::rc::Rc;

use tn5250::ds::field::{Ffw, Shift as FfwShift};
use tn5250::ds::{aid, attr};
use tn5250::screen::{Read, ScreenModel};
use uim::model::{Action, Area, Key, KeyItem, MenuOption, Panel, PanelGroup};
use uim::render::{Attr, Dialog, Field, ListState, Options, Screen, Target};

use super::surface::Surface;

/// A list's entries and page.
#[derive(Clone, Debug, Default)]
pub struct UimList {
    /// Every entry, `(variable, value)` pairs.
    pub rows: Vec<Vec<(String, String)>>,
    /// The index of the first entry on the page.
    pub top: usize,
    /// The program has added every entry.
    pub complete: bool,
    /// Options typed on entries, by entry index.
    pub options: BTreeMap<usize, String>,
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
    pub action: Option<Action>,
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
    /// `PRINT`.
    Print,
    /// The display is gone or the job is to end.
    Gone,
}

/// Why a panel could not be shown.
#[derive(Debug, thiserror::Error)]
pub enum UimError {
    /// The renderer refused.
    #[error(transparent)]
    Render(#[from] uim::RenderError),
}

/// The 5250 attribute of a rendered attribute.
fn attr_of(a: Attr) -> u8 {
    match a {
        Attr::Normal => attr::GREEN,
        Attr::HighIntensity => attr::WHITE,
        Attr::Blue => attr::BLUE,
        Attr::Reverse => attr::GREEN_RI,
        Attr::Underline => attr::GREEN_UL,
        Attr::Nondisplay => attr::NONDISPLAY,
    }
}

/// The field format word of a rendered field.
fn ffw_of(f: &Field) -> Ffw {
    Ffw {
        bypass: !f.usage.is_input(),
        shift: match f.shift {
            uim::model::Shift::Numeric => FfwShift::SignedNumeric,
            _ => FfwShift::AlphaShift,
        },
        mandatory_entry: f.required,
        auto_enter: f.autoskip,
        monocase: false,
        ..Ffw::default()
    }
}

/// Put a rendered screen on the model.
pub fn apply(s: &Screen, m: &mut ScreenModel) {
    m.clear();
    for t in &s.texts {
        m.put(t.row, t.col, &t.text, attr_of(t.attr));
    }
    for f in &s.fields {
        let a = match f.attr {
            Attr::Normal => attr::GREEN_UL,
            other => attr_of(other),
        };
        m.field(f.row, f.col, f.len, a, &f.value, ffw_of(f), &[]);
    }
    if let Some((r, c)) = s.cursor {
        m.set_cursor(r, c);
    }
}

/// The UIM key of an attention identifier.
fn key_of(a: u8) -> Option<Key> {
    match a {
        aid::ENTER | aid::AUTO_ENTER => Some(Key::Enter),
        aid::ROLL_UP => Some(Key::PageDown),
        aid::ROLL_DOWN => Some(Key::PageUp),
        aid::HELP => Some(Key::Help),
        aid::PRINT => Some(Key::Print),
        other => aid::function_of(other).map(Key::F),
    }
}

/// The dialog of one panel group.
pub struct UimDialog {
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
    /// The message for the message line, and whether it is an error.
    pub message: Option<(String, bool)>,
    /// The system name for `TOPSEP=SYSNAM`.
    pub system: String,
    /// Show the group's copyright on the message line when no message.
    pub copyright: bool,
}

impl Dialog for UimDialog {
    fn var(&self, name: &str) -> Option<String> {
        self.vars.get(&name.to_ascii_uppercase()).cloned()
    }

    fn list(&self, name: &str) -> Option<ListState> {
        let l = self.lists.get(&name.to_ascii_uppercase())?;
        Some(ListState {
            rows: l.rows.get(l.top..).unwrap_or_default().to_vec(),
            top: l.top,
            total: l.complete.then_some(l.rows.len()),
            complete: l.complete,
        })
    }

    fn cond(&self, name: &str) -> bool {
        self.conds
            .get(&name.to_ascii_uppercase())
            .copied()
            .unwrap_or(false)
    }
}

/// The command line's value when `:CMDLINE.` names no variable.
pub const CMDLINE_VAR: &str = "*CMDLINE";

impl UimDialog {
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
            message: None,
            system: system.to_string(),
            copyright: false,
        }
    }

    /// Set a variable.
    pub fn set(&mut self, name: &str, value: &str) {
        self.vars
            .insert(name.to_ascii_uppercase(), value.to_string());
    }

    /// A variable's value, blank when unset.
    #[must_use]
    #[allow(dead_code)] // the panel drivers' API
    pub fn get(&self, name: &str) -> String {
        self.var(name).unwrap_or_default()
    }

    /// Set an imported condition.
    #[allow(dead_code)] // the panel drivers' API
    pub fn set_cond(&mut self, name: &str, value: bool) {
        self.conds.insert(name.to_ascii_uppercase(), value);
    }

    /// A list, made when first named.
    pub fn list_mut(&mut self, name: &str) -> &mut UimList {
        self.lists.entry(name.to_ascii_uppercase()).or_default()
    }

    /// Empty a list.
    #[allow(dead_code)] // the panel drivers' API
    pub fn clear_list(&mut self, name: &str) {
        let l = self.list_mut(name);
        l.rows.clear();
        l.top = 0;
        l.options.clear();
        l.complete = false;
    }

    /// Add an entry to a list.
    #[allow(dead_code)] // the panel drivers' API
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
                while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_') {
                    end += 1;
                }
                if end > start {
                    let name: String = chars[start..end].iter().collect::<String>().to_ascii_uppercase();
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

    fn options(&mut self, wide: bool) -> Options {
        Options {
            system: self.system.clone(),
            show_copyright: self.copyright,
            copyright: self.group.copyright.clone().unwrap_or_default(),
            wide,
            view: self.view,
            more_keys: self.more_keys,
            message: self.message.take(),
            page: self.page,
        }
    }

    /// Take what the operator typed into the variables, lists and
    /// command line.
    fn take_input(&mut self, fields: &[(Field, String)]) -> (String, String) {
        let mut cmdline = String::new();
        let mut selection = String::new();
        for (f, value) in fields {
            match &f.target {
                Target::Var(name) => {
                    if f.usage.is_input() {
                        self.set(name, value);
                    }
                }
                Target::ListOption { list, row_index } => {
                    let l = self.list_mut(list);
                    if value.trim().is_empty() {
                        l.options.remove(row_index);
                    } else {
                        l.options.insert(*row_index, value.trim().to_string());
                    }
                }
                Target::ListCell {
                    list,
                    row_index,
                    var,
                } => {
                    let l = self.list_mut(list);
                    if let Some(row) = l.rows.get_mut(*row_index) {
                        match row.iter_mut().find(|(v, _)| v == var) {
                            Some(cell) => cell.1.clone_from(value),
                            None => row.push((var.clone(), value.clone())),
                        }
                    }
                }
                Target::CmdLine => {
                    cmdline = value.trim().to_string();
                    self.set(CMDLINE_VAR, "");
                }
                Target::MenuSelection => selection = value.trim().to_string(),
                Target::PositionTo(var) => self.set(var, value),
            }
        }
        (cmdline, selection)
    }

    /// The key's action on the panel, `None` when the key is not in
    /// the key list (or its condition is off).
    fn action_of(&self, panel: &Panel, key: Key) -> Option<Action> {
        let keyl = self.group.key_list(&panel.keyl)?;
        let item: Option<&KeyItem> = keyl.keys.iter().find(|k| {
            k.key == key
                && k.cond
                    .as_deref()
                    .is_none_or(|c| uim::render::condition(&self.group, self, c))
        });
        match item {
            Some(k) => Some(k.action.clone()),
            None if key == Key::Enter => Some(Action::Enter),
            None => None,
        }
    }

    /// Typed options on the panel's lists, in list order.
    fn choices(&self, panel: &Panel) -> Vec<Choice> {
        let mut out = Vec::new();
        for a in &panel.areas {
            let Area::List(l) = a else {
                continue;
            };
            let Some(list) = self.lists.get(&l.listdef.to_ascii_uppercase()) else {
                continue;
            };
            for (row, option) in &list.options {
                let action = l
                    .actions
                    .iter()
                    .find(|a| a.option.as_deref().is_some_and(|o| o.eq_ignore_ascii_case(option)))
                    .and_then(|a| a.enter.clone());
                out.push(Choice {
                    list: l.listdef.clone(),
                    row: *row,
                    option: option.clone(),
                    action,
                });
            }
        }
        out
    }

    /// Enter on a menu panel: the option typed, or the command.
    fn menu_enter(&mut self, panel: &Panel, typed: &str) -> Option<Response> {
        let menu = panel.areas.iter().find_map(|a| match a {
            Area::Menu(m) => Some(m),
            _ => None,
        })?;
        if typed.is_empty() {
            return None;
        }
        let Ok(n) = typed.parse::<u16>() else {
            return Some(Response::Command(typed.to_string()));
        };
        let item = menu
            .all_items()
            .into_iter()
            .find(|i| matches!(i.option, MenuOption::Number(k) if k == n));
        let Some(i) = item else {
            self.message = Some((format!("Selection {n} is not valid."), true));
            return None;
        };
        Some(match &i.action {
            Action::Menu(name) => Response::Menu(name.clone()),
            Action::Cmd(c) => Response::Command(c.clone()),
            Action::Return(r) => Response::Return(*r),
            Action::Panel(p) => Response::Panel(p.clone()),
            _ => Response::Return(0),
        })
    }

    /// Page every list on the panel and the areas; whether anything
    /// moved.
    fn page(&mut self, panel: &Panel, screen: &Screen, down: bool) -> bool {
        let mut moved = false;
        for a in &panel.areas {
            match a {
                Area::List(l) => {
                    let shown = screen.list_shown.get(&l.listdef).copied().unwrap_or(1).max(1);
                    let list = self.list_mut(&l.listdef);
                    if down {
                        if list.top + shown < list.rows.len() {
                            list.top += shown;
                            moved = true;
                        }
                    } else if list.top > 0 {
                        list.top = list.top.saturating_sub(shown);
                        moved = true;
                    }
                }
                Area::Menu(_) | Area::Data(_) | Area::Info(_) => {
                    if down {
                        if screen.more.is_some_and(|m| !m.bottom) {
                            self.page += 1;
                            moved = true;
                        }
                    } else if self.page > 0 {
                        self.page -= 1;
                        moved = true;
                    }
                }
                _ => {}
            }
        }
        if !moved {
            self.message = Some((
                if down {
                    "Already at the bottom of the area."
                } else {
                    "Already at the top of the area."
                }
                .into(),
                true,
            ));
        }
        moved
    }

    /// Show a help module until F3, F12 or Enter.
    fn help(&mut self, surface: &mut Surface, module: Option<&str>) -> Option<()> {
        let Some(module) = module else {
            self.message = Some(("No help is available for this display.".into(), true));
            return Some(());
        };
        let wide = surface.wide();
        let opts = self.options(wide);
        let Ok(screen) = uim::render_help(&self.group, module, &opts) else {
            self.message = Some(("No help is available for this display.".into(), true));
            return Some(());
        };
        loop {
            surface.with_model(|m| apply(&screen, m));
            surface.flush(Read::MdtFields);
            let (a, _) = surface.wait()?;
            if let Some(Key::Enter | Key::F(3 | 12)) = key_of(a) {
                return Some(());
            }
        }
    }

    /// Show a panel and take the operator's answer.
    #[allow(clippy::too_many_lines)] // one arm per action
    pub fn show(&mut self, surface: &mut Surface, panel_name: &str) -> Result<Response, UimError> {
        let group = Rc::clone(&self.group);
        let panel = group
            .panel(panel_name)
            .ok_or_else(|| uim::RenderError::NoPanel(panel_name.to_string()))?;
        loop {
            let wide = surface.wide();
            let opts = self.options(wide);
            let screen = uim::render(&group, panel_name, self, &opts)?;
            surface.with_model(|m| apply(&screen, m));
            surface.flush(Read::MdtFields);
            let Some((a, _cursor)) = surface.wait() else {
                return Ok(Response::Gone);
            };
            let values: Vec<(Field, String)> = surface.with_model(|m| {
                screen
                    .fields
                    .iter()
                    .map(|f| {
                        let v = m
                            .field_index_at(f.row, f.col)
                            .map(|i| m.fields()[i].text())
                            .unwrap_or_default();
                        (f.clone(), v)
                    })
                    .collect()
            });
            let (cmdline, selection) = self.take_input(&values);
            let typed = if cmdline.is_empty() { selection } else { cmdline };
            let Some(key) = key_of(a) else {
                continue;
            };
            let Some(action) = self.action_of(panel, key) else {
                self.message = Some(("Function key not allowed.".into(), true));
                continue;
            };
            let action = if action == Action::Enter {
                panel.enter.clone().unwrap_or(Action::Return(0))
            } else {
                action
            };
            match action {
                Action::Return(n) if key == Key::Enter => {
                    if let Some(r) = self.menu_enter(panel, &typed) {
                        return Ok(r);
                    }
                    if self.message.is_some() {
                        continue;
                    }
                    let choices = self.choices(panel);
                    if !choices.is_empty() {
                        return Ok(Response::Options(choices));
                    }
                    if !typed.is_empty() && panel.cmdline().is_some() {
                        return Ok(Response::Command(typed));
                    }
                    if panel.enter.is_none() && panel.cmdline().is_some() {
                        continue;
                    }
                    return Ok(Response::Return(n));
                }
                Action::Return(n) => {
                    if n == 4 && !typed.is_empty() && panel.cmdline().is_some() {
                        return Ok(Response::Command(format!("?{typed}")));
                    }
                    self.set(CMDLINE_VAR, &typed);
                    return Ok(Response::Return(n));
                }
                Action::Exit => return Ok(Response::Exit),
                Action::Cancel => return Ok(Response::Cancel),
                Action::Cmd(c) => return Ok(Response::Command(c)),
                Action::Menu(m) => return Ok(Response::Menu(m)),
                Action::Panel(p) => return Ok(Response::Panel(p)),
                Action::Retrieve => return Ok(Response::Retrieve),
                Action::Print => return Ok(Response::Print),
                Action::PageDown | Action::NxtPage => {
                    self.page(panel, &screen, true);
                }
                Action::PageUp | Action::PrvPage => {
                    self.page(panel, &screen, false);
                }
                Action::Top | Action::Home => {
                    for a in &panel.areas {
                        if let Area::List(l) = a {
                            self.list_mut(&l.listdef).top = 0;
                        }
                    }
                    self.page = 0;
                }
                Action::Bottom => {
                    for a in &panel.areas {
                        if let Area::List(l) = a {
                            let shown = screen.list_shown.get(&l.listdef).copied().unwrap_or(1).max(1);
                            let list = self.list_mut(&l.listdef);
                            list.top = list.rows.len().saturating_sub(shown);
                        }
                    }
                }
                Action::MoreKeys => self.more_keys = !self.more_keys,
                Action::Help | Action::ExtHelp | Action::FldHelp => {
                    let module = panel.help.clone();
                    if self.help(surface, module.as_deref()).is_none() {
                        return Ok(Response::Gone);
                    }
                }
                Action::KeysHelp => {
                    let module = group.key_list(&panel.keyl).and_then(|k| k.help.clone());
                    if self.help(surface, module.as_deref()).is_none() {
                        return Ok(Response::Gone);
                    }
                }
                Action::HelpModule(m) => {
                    if self.help(surface, Some(&m)).is_none() {
                        return Ok(Response::Gone);
                    }
                }
                Action::CmdLine => {
                    self.set(CMDLINE_VAR, &typed);
                }
                Action::Enter
                | Action::About
                | Action::Actions
                | Action::PullDown
                | Action::PrvPnl
                | Action::NxtPnl
                | Action::Call(_)
                | Action::Msg(_) => {
                    self.message = Some(("That function is not available in this release.".into(), true));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GROUP: &str = "\
:PNLGRP.
:CLASS NAME=OPTCL BASETYPE='CHAR 3'.
:ECLASS.
:CLASS NAME=NAMECL BASETYPE='CHAR 10'.
:ECLASS.
:VAR NAME=OPTION CLASS=OPTCL.
:VAR NAME=PANELID CLASS=NAMECL VALUE='TEST'.
:VAR NAME=JOB CLASS=NAMECL.
:KEYL NAME=KEYS.
:KEYI KEY=ENTER ACTION=ENTER.
:KEYI KEY=F3 ACTION=EXIT.F3=Exit
:KEYI KEY=F12 ACTION=CANCEL.F12=Cancel
:EKEYL.
:PANEL NAME=TEST HELP=HLP KEYL=KEYS ENTER='RETURN 1' TOPSEP=SYSNAM PANELID=PANELID.A Test Menu
:MENU DEPTH='*' SCROLL=YES OPTIONS=OPTION.Select one of the following:
:MENUI OPTION=1 ACTION='MENU USER'.User tasks
:MENUI OPTION=2 ACTION='CMD DSPJOB JOB(&JOB)'.Display a job
:EMENU.
:CMDLINE SIZE=LONG.Selection or command
:EPANEL.
:HELP NAME=HLP.Help
:P.Some help.
:EHELP.
:EPNLGRP.
";

    fn group() -> Rc<PanelGroup> {
        let c = uim::compile(GROUP, &uim::NoImbeds, &uim::NoImports, &uim::CompileOpts::default());
        Rc::new(c.group.unwrap_or_else(|| panic!("{}", c.listing)))
    }

    #[test]
    fn substitution() {
        let mut d = UimDialog::new(group(), "QS400");
        d.set("JOB", "QPADEV0001");
        assert_eq!(d.substitute("DSPJOB JOB(&JOB)", None), "DSPJOB JOB(QPADEV0001)");
        assert_eq!(d.substitute("A && B &NOPE.", None), "A & B ");
        d.add_row("L", vec![("JOB".into(), "OTHER".into())]);
        assert_eq!(d.substitute("&JOB.X", Some(("L", 0))), "OTHERX");
        assert_eq!(d.get("PANELID"), "TEST");
    }

    #[test]
    fn menu_choice() {
        let g = group();
        let mut d = UimDialog::new(Rc::clone(&g), "QS400");
        let p = g.panel("TEST").unwrap();
        assert_eq!(d.menu_enter(p, "1"), Some(Response::Menu("USER".into())));
        assert_eq!(
            d.menu_enter(p, "2"),
            Some(Response::Command("DSPJOB JOB(&JOB)".into()))
        );
        assert_eq!(d.menu_enter(p, "wrkactjob"), Some(Response::Command("wrkactjob".into())));
        assert_eq!(d.menu_enter(p, "7"), None);
        assert!(d.message.is_some());
        let screen = uim::render(&g, "TEST", &d, &Options::default()).unwrap();
        let mut m = ScreenModel::new(false);
        apply(&screen, &mut m);
        let rows = m.text_rows();
        assert!(rows[0].contains("A Test Menu"));
        assert!(rows[1].contains("System:"));
        assert!(rows[4].contains("1. User tasks"));
        assert_eq!(m.fields().len(), 1);
    }
}
