// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Showing a panel: render, exchange with the terminal, take what was
//! typed, and act on the key (§8 of `docs/UIM.md`).

use std::collections::BTreeSet;
use std::rc::Rc;

use super::{CMDLINE_VAR, Choice, Engine, EngineError, Response, Terminal};
use crate::model::{Action, Area, DataNode, Key, KeyItem, ListArea, MenuOption, Panel};
use crate::render::{Field, Screen, Target};

/// What a show ends with: a response, or a request for more entries of
/// an incomplete list (only with `incomplete_returns`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The operator's answer.
    Response(Response),
    /// The operator paged past the last entry of this incomplete list.
    More(String),
}

/// The message for options no `:LISTACT.` has.
pub const OPTION_NOT_VALID: &str = "Option not valid for this list.";

impl Engine {
    /// Show a panel and take the operator's answer.
    pub fn show<T: Terminal + ?Sized>(
        &mut self,
        term: &mut T,
        panel_name: &str,
    ) -> Result<Response, EngineError> {
        loop {
            match self.show_event(term, panel_name)? {
                Event::Response(r) => return Ok(r),
                Event::More(_) => {}
            }
        }
    }

    /// Show a panel: the answer, or a request for more entries.
    #[allow(clippy::too_many_lines)] // one arm per action
    pub fn show_event<T: Terminal + ?Sized>(
        &mut self,
        term: &mut T,
        panel_name: &str,
    ) -> Result<Event, EngineError> {
        let group = Rc::clone(&self.group);
        let panel = group
            .panel(panel_name)
            .ok_or_else(|| crate::RenderError::NoPanel(panel_name.to_string()))?;
        loop {
            self.subsets(panel);
            let wide = term.wide();
            let screen = self.screen(panel_name, wide)?;
            let Some(input) = term.exchange(&screen) else {
                return Ok(Event::Response(Response::Gone));
            };
            self.cursor = input.cursor;
            let values: Vec<(Field, String)> = screen
                .fields
                .iter()
                .cloned()
                .zip(
                    input
                        .values
                        .iter()
                        .cloned()
                        .chain(std::iter::repeat(String::new())),
                )
                .collect();
            let (cmdline, selection) = self.take_input(&values);
            let typed = if cmdline.is_empty() {
                selection
            } else {
                cmdline
            };
            let Some(key) = input.key else {
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
                    if let Some(r) = self.menu_enter(term, panel, &typed) {
                        return Ok(Event::Response(r));
                    }
                    if self.message.is_some() || std::mem::take(&mut self.redisplay) {
                        continue;
                    }
                    if !self.validate(panel) {
                        continue;
                    }
                    let choices = self.choices(panel);
                    if !choices.is_empty() {
                        match self.confirm(term, panel, &choices)? {
                            Some(Response::Cancel) => continue,
                            Some(r) => return Ok(Event::Response(r)),
                            None => return Ok(Event::Response(Response::Options(choices))),
                        }
                    }
                    if !typed.is_empty() && panel.cmdline().is_some() {
                        return Ok(Event::Response(Response::Command(typed)));
                    }
                    if panel.enter.is_none() && panel.cmdline().is_some() {
                        continue;
                    }
                    return Ok(Event::Response(Response::Return(n)));
                }
                Action::Return(n) => {
                    if n == 4 && !typed.is_empty() && panel.cmdline().is_some() {
                        return Ok(Event::Response(Response::Command(format!("?{typed}"))));
                    }
                    self.set(CMDLINE_VAR, &typed);
                    return Ok(Event::Response(Response::Return(n)));
                }
                Action::Exit => return Ok(Event::Response(Response::Exit)),
                Action::Cancel => return Ok(Event::Response(Response::Cancel)),
                Action::Cmd(c) => return Ok(Event::Response(Response::Command(c))),
                Action::Menu(m) => return Ok(Event::Response(Response::Menu(m))),
                Action::Panel(p) => {
                    // A pop-up panel shows over this one, in the engine.
                    if group.panel(&p).is_some_and(crate::render::is_window) {
                        if let Some(r) = self.popup(term, panel, &p)? {
                            return Ok(Event::Response(r));
                        }
                        continue;
                    }
                    return Ok(Event::Response(Response::Panel(p)));
                }
                Action::PrvPnl => {
                    if let Some(p) = &panel.prvpnl {
                        return Ok(Event::Response(Response::Panel(p.clone())));
                    }
                    self.not_available();
                }
                Action::NxtPnl => {
                    if let Some(p) = &panel.nxtpnl {
                        return Ok(Event::Response(Response::Panel(p.clone())));
                    }
                    self.not_available();
                }
                Action::Retrieve => return Ok(Event::Response(Response::Retrieve)),
                Action::Print => {
                    let title = crate::model::Span::plain(&panel.title);
                    let rows = screen
                        .text_rows()
                        .into_iter()
                        .map(|r| r.trim_end().to_string())
                        .collect::<Vec<_>>();
                    match term.print(title.trim(), &rows) {
                        Ok(m) => self.message = Some((m, false)),
                        Err(_) => return Ok(Event::Response(Response::Print)),
                    }
                }
                Action::PageDown | Action::NxtPage => {
                    if let Some(list) = self.page(panel, &screen, true) {
                        return Ok(Event::More(list));
                    }
                }
                Action::PageUp | Action::PrvPage => {
                    self.page(panel, &screen, false);
                }
                Action::Top | Action::Home => self.top(panel),
                Action::Bottom => {
                    if let Some(list) = self.bottom(panel, &screen) {
                        return Ok(Event::More(list));
                    }
                }
                Action::MoreKeys => {
                    self.more_keys = !self.more_keys;
                    self.key_set += 1;
                }
                Action::Help | Action::FldHelp => {
                    let module = help_at(&screen, self.cursor).or_else(|| panel.help.clone());
                    if self.contextual_help(term, panel, module, &screen).is_none() {
                        return Ok(Event::Response(Response::Gone));
                    }
                }
                Action::ExtHelp => {
                    if self.extended_help(term, panel).is_none() {
                        return Ok(Event::Response(Response::Gone));
                    }
                }
                Action::KeysHelp => {
                    let module = group.key_list(&panel.keyl).and_then(|k| k.help.clone());
                    if self.contextual_help(term, panel, module, &screen).is_none() {
                        return Ok(Event::Response(Response::Gone));
                    }
                }
                Action::HelpModule(m) => {
                    if self
                        .contextual_help(term, panel, Some(m), &screen)
                        .is_none()
                    {
                        return Ok(Event::Response(Response::Gone));
                    }
                }
                Action::About => {
                    if self.about(term, panel, &screen).is_none() {
                        return Ok(Event::Response(Response::Gone));
                    }
                }
                Action::CmdLine => {
                    self.set(CMDLINE_VAR, &typed);
                }
                Action::Enter
                | Action::Actions
                | Action::PullDown
                | Action::Call(_)
                | Action::Msg(_) => {
                    self.not_available();
                }
            }
        }
    }

    fn not_available(&mut self) {
        self.message = Some((
            "That function is not available in this release.".into(),
            true,
        ));
    }

    /// Render a panel: a pop-up panel over the screen shown before it.
    pub(crate) fn screen(&mut self, panel: &str, wide: bool) -> Result<Screen, EngineError> {
        let opts = self.options(wide);
        let p = self
            .group
            .panel(panel)
            .ok_or_else(|| crate::RenderError::NoPanel(panel.to_string()))?;
        let mut screen = if crate::render::is_window(p) {
            let under = self.under.clone().unwrap_or_else(|| {
                let (rows, cols) = if wide { (27, 132) } else { (24, 80) };
                Screen::new(rows, cols)
            });
            crate::render::render_window(&self.group, panel, self, &opts, &under, self.cursor)?
        } else {
            let s = crate::render(&self.group, panel, self, &opts)?;
            self.under = Some(s.clone());
            s
        };
        if let Some(name) = self.cursor_field.take() {
            let name = name.to_ascii_uppercase();
            if let Some(f) = screen
                .fields
                .iter()
                .find(|f| matches!(&f.target, Target::Var(v) if *v == name))
            {
                screen.cursor = Some((f.row, f.col));
            }
        }
        Ok(screen)
    }

    /// Take what the operator typed into the variables, lists and
    /// command line: the command line and the menu selection.
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
                    let Some(real) = l.visible().get(*row_index).copied() else {
                        continue;
                    };
                    l.errors.remove(&real);
                    if value.trim().is_empty() {
                        l.options.remove(&real);
                    } else {
                        l.options.insert(real, value.trim().to_string());
                    }
                }
                Target::ListCell {
                    list,
                    row_index,
                    var,
                } => {
                    let l = self.list_mut(list);
                    let Some(real) = l.visible().get(*row_index).copied() else {
                        continue;
                    };
                    if let Some(row) = l.rows.get_mut(real) {
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
                    .is_none_or(|c| crate::render::condition(&self.group, self, c))
        });
        match item {
            Some(k) => Some(k.action.clone()),
            None if key == Key::Enter => Some(Action::Enter),
            // The Help key always helps.
            None if key == Key::Help => Some(Action::Help),
            None => None,
        }
    }

    /// The list areas of a panel.
    fn list_areas(panel: &Panel) -> impl Iterator<Item = &ListArea> {
        panel.areas.iter().filter_map(|a| match a {
            Area::List(l) => Some(l),
            _ => None,
        })
    }

    /// Typed options on the panel's lists, in list order.
    pub(crate) fn choices(&self, panel: &Panel) -> Vec<Choice> {
        let mut out = Vec::new();
        for l in Self::list_areas(panel) {
            let Some(list) = self.list_ref(&l.listdef) else {
                continue;
            };
            let visible: BTreeSet<usize> = list.visible().into_iter().collect();
            for (row, option) in &list.options {
                if !visible.contains(row) {
                    continue;
                }
                let action = l
                    .actions
                    .iter()
                    .find(|a| {
                        a.option
                            .as_deref()
                            .is_some_and(|o| o.eq_ignore_ascii_case(option))
                    })
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

    /// Mark options no `:LISTACT.` has; whether every option is valid.
    fn validate(&mut self, panel: &Panel) -> bool {
        let mut ok = true;
        for l in Self::list_areas(panel) {
            let known: Vec<String> = l
                .actions
                .iter()
                .filter_map(|a| a.option.as_ref().map(|o| o.to_ascii_uppercase()))
                .collect();
            if known.is_empty() {
                continue;
            }
            let list = self.list_mut(&l.listdef);
            let bad: Vec<usize> = list
                .options
                .iter()
                .filter(|(_, o)| !known.contains(&o.to_ascii_uppercase()))
                .map(|(k, _)| *k)
                .collect();
            if bad.is_empty() {
                continue;
            }
            ok = false;
            // The page shows the first entry in error.
            let first = bad[0];
            list.errors.extend(bad);
            let visible = list.visible();
            if let Some(pos) = visible.iter().position(|i| *i == first)
                && pos < list.top
            {
                list.top = pos;
            }
        }
        if !ok {
            self.message = Some((OPTION_NOT_VALID.into(), true));
        }
        ok
    }

    /// Confirmation panels for options whose `:LISTACT.` has `CONFIRM`:
    /// `None` when there is nothing to confirm or Enter confirmed;
    /// `Cancel` to go back to the list; another response to pass on.
    fn confirm<T: Terminal + ?Sized>(
        &mut self,
        term: &mut T,
        panel: &Panel,
        choices: &[Choice],
    ) -> Result<Option<Response>, EngineError> {
        for l in Self::list_areas(panel) {
            for a in &l.actions {
                let (Some(confirm), Some(opt)) = (&a.confirm, &a.option) else {
                    continue;
                };
                let rows: BTreeSet<usize> = choices
                    .iter()
                    .filter(|c| c.list == l.listdef && c.option.eq_ignore_ascii_case(opt))
                    .map(|c| c.row)
                    .collect();
                if rows.is_empty() || self.group.panel(confirm).is_none() {
                    continue;
                }
                let list = self.list_mut(&l.listdef);
                let saved = list.top;
                list.only = Some(rows);
                list.top = 0;
                let r = self.confirm_panel(term, confirm);
                let list = self.list_mut(&l.listdef);
                list.only = None;
                list.top = saved;
                match r? {
                    Response::Options(_) | Response::Return(_) => {}
                    other => return Ok(Some(other)),
                }
            }
        }
        Ok(None)
    }

    /// The confirmation panel itself: Enter is the answer, F12 cancels.
    fn confirm_panel<T: Terminal + ?Sized>(
        &mut self,
        term: &mut T,
        name: &str,
    ) -> Result<Response, EngineError> {
        let group = Rc::clone(&self.group);
        let panel = group
            .panel(name)
            .ok_or_else(|| crate::RenderError::NoPanel(name.to_string()))?;
        loop {
            let screen = self.screen(name, term.wide())?;
            let Some(input) = term.exchange(&screen) else {
                return Ok(Response::Gone);
            };
            self.cursor = input.cursor;
            let values: Vec<(Field, String)> = screen
                .fields
                .iter()
                .cloned()
                .zip(
                    input
                        .values
                        .iter()
                        .cloned()
                        .chain(std::iter::repeat(String::new())),
                )
                .collect();
            self.take_input(&values);
            let Some(key) = input.key else {
                continue;
            };
            match self.action_of(panel, key) {
                Some(Action::Enter | Action::Return(_)) if key == Key::Enter => {
                    return Ok(Response::Return(0));
                }
                Some(Action::Cancel) => return Ok(Response::Cancel),
                Some(Action::Exit) => return Ok(Response::Exit),
                Some(Action::PageDown) => {
                    self.page(panel, &screen, true);
                }
                Some(Action::PageUp) => {
                    self.page(panel, &screen, false);
                }
                Some(Action::Help | Action::FldHelp | Action::ExtHelp) => {
                    let module = help_at(&screen, self.cursor).or_else(|| panel.help.clone());
                    if self.contextual_help(term, panel, module, &screen).is_none() {
                        return Ok(Response::Gone);
                    }
                }
                Some(_) => self.not_available(),
                None => self.message = Some(("Function key not allowed.".into(), true)),
            }
        }
    }

    /// Lists filtered by data fields that share a list column's
    /// variable (a subset field, `Subset by type`).
    fn subsets(&mut self, panel: &Panel) {
        let mut fields: Vec<String> = Vec::new();
        for a in &panel.areas {
            if let Area::Data(d) = a {
                collect_inputs(&d.items, &mut fields);
            }
        }
        for l in Self::list_areas(panel) {
            let vars: Vec<String> = self
                .group
                .list_def(&l.listdef)
                .map(|d| d.vars.clone())
                .unwrap_or_default();
            let subset: Vec<(String, String)> = fields
                .iter()
                .filter(|f| vars.contains(f))
                .map(|f| (f.clone(), self.get(f)))
                .collect();
            let list = self.list_mut(&l.listdef);
            if list.subset != subset {
                list.subset = subset;
                list.top = 0;
            }
        }
    }

    /// Position the panel's lists (`:LIST PARMS=`): the page starts at
    /// the first entry whose first column is at or after the record's
    /// first variable and whose next columns match the record's next
    /// variables (`Type`). Whether any was asked.
    fn position(&mut self, panel: &Panel) -> bool {
        let mut any = false;
        for l in Self::list_areas(panel) {
            let Some(rec) = l
                .parms
                .as_deref()
                .and_then(|r| self.group.var_records.iter().find(|v| v.name == r))
            else {
                continue;
            };
            let Some((first, extras)) = rec.vars.split_first() else {
                continue;
            };
            let want = self.get(first).trim().to_ascii_uppercase();
            // The record's variables go with the list's columns in
            // order, the option column left out.
            let cols = crate::render::position_columns(&self.group, l);
            let extras: Vec<(String, String)> = extras
                .iter()
                .zip(cols.iter().skip(1))
                .map(|(v, col)| (col.clone(), self.get(v)))
                .filter(|(_, x)| !x.trim().is_empty())
                .collect();
            if want.is_empty() && extras.is_empty() {
                continue;
            }
            any = true;
            let key = cols.first().cloned();
            let list = self.list_mut(&l.listdef);
            let visible = list.visible();
            let found = visible.iter().position(|i| {
                let row = &list.rows[*i];
                let cell = |var: &str| {
                    row.iter()
                        .find(|(v, _)| v == var)
                        .map_or(String::new(), |(_, x)| x.trim_end().to_ascii_uppercase())
                };
                let at = key.as_deref().is_none_or(|k| cell(k) >= want);
                at && extras
                    .iter()
                    .all(|(v, x)| super::subset_matches(&cell(v), x))
            });
            if let Some(pos) = found {
                list.top = pos;
            } else {
                list.top = visible.len().saturating_sub(1);
                self.message = Some((format!("No entry at or after {want}."), true));
            }
        }
        any
    }

    /// A pop-up panel over `base`: Enter positions `base`'s lists from
    /// the pop-up's variables (the `Position the List` window), F12
    /// goes back; F3 is answered.
    fn popup<T: Terminal + ?Sized>(
        &mut self,
        term: &mut T,
        base: &Panel,
        name: &str,
    ) -> Result<Option<Response>, EngineError> {
        let group = Rc::clone(&self.group);
        let panel = group
            .panel(name)
            .ok_or_else(|| crate::RenderError::NoPanel(name.to_string()))?;
        loop {
            let screen = self.screen(name, term.wide())?;
            let Some(input) = term.exchange(&screen) else {
                return Ok(Some(Response::Gone));
            };
            self.cursor = input.cursor;
            let values: Vec<(Field, String)> = screen
                .fields
                .iter()
                .cloned()
                .zip(
                    input
                        .values
                        .iter()
                        .cloned()
                        .chain(std::iter::repeat(String::new())),
                )
                .collect();
            self.take_input(&values);
            let Some(key) = input.key else {
                continue;
            };
            match self.action_of(panel, key) {
                Some(Action::Enter | Action::Return(_)) if key == Key::Enter => {
                    self.position(base);
                    return Ok(None);
                }
                Some(Action::Cancel) => return Ok(None),
                Some(Action::Exit) => return Ok(Some(Response::Exit)),
                Some(Action::Help | Action::FldHelp | Action::ExtHelp) => {
                    let module = help_at(&screen, self.cursor).or_else(|| panel.help.clone());
                    if self.contextual_help(term, panel, module, &screen).is_none() {
                        return Ok(Some(Response::Gone));
                    }
                }
                Some(_) => self.not_available(),
                None => self.message = Some(("Function key not allowed.".into(), true)),
            }
        }
    }

    /// Enter on a menu panel: the option typed, or the command.
    pub(crate) fn menu_enter<T: Terminal + ?Sized>(
        &mut self,
        term: &mut T,
        panel: &Panel,
        typed: &str,
    ) -> Option<Response> {
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
            Action::Exit => Response::Exit,
            Action::Cancel => Response::Cancel,
            Action::HelpModule(m) => {
                let empty = Screen::new(24, 80);
                let under = self.under.clone().unwrap_or(empty);
                self.contextual_help(term, panel, Some(m.clone()), &under)?;
                // A shown help leaves the menu where it was.
                self.redisplay = true;
                return None;
            }
            Action::Help | Action::ExtHelp | Action::FldHelp => {
                self.extended_help(term, panel)?;
                self.redisplay = true;
                return None;
            }
            _ => Response::Return(0),
        })
    }

    /// Page every list on the panel and the areas; a list to extend
    /// when paging past an incomplete one.
    fn page(&mut self, panel: &Panel, screen: &Screen, down: bool) -> Option<String> {
        let mut moved = false;
        let incomplete_returns = self.incomplete_returns;
        for a in &panel.areas {
            match a {
                Area::List(l) => {
                    let shown = screen
                        .list_shown
                        .get(&l.listdef)
                        .copied()
                        .unwrap_or(1)
                        .max(1);
                    let list = self.list_mut(&l.listdef);
                    let count = list.visible().len();
                    if down {
                        if list.top + shown < count {
                            list.top += shown;
                            moved = true;
                        } else if !list.complete && incomplete_returns {
                            return Some(l.listdef.clone());
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
        None
    }

    /// `TOP` (F17): every list and area to its start.
    fn top(&mut self, panel: &Panel) {
        for l in Self::list_areas(panel) {
            self.list_mut(&l.listdef).top = 0;
        }
        self.page = 0;
    }

    /// `BOTTOM` (F18): every list to its last page; an incomplete list
    /// to extend first.
    fn bottom(&mut self, panel: &Panel, screen: &Screen) -> Option<String> {
        let incomplete_returns = self.incomplete_returns;
        for l in Self::list_areas(panel) {
            let shown = screen
                .list_shown
                .get(&l.listdef)
                .copied()
                .unwrap_or(1)
                .max(1);
            let list = self.list_mut(&l.listdef);
            if !list.complete && incomplete_returns {
                return Some(l.listdef.clone());
            }
            list.top = list.visible().len().saturating_sub(shown);
        }
        None
    }
}

/// The input variables of data items.
fn collect_inputs(items: &[DataNode], out: &mut Vec<String>) {
    for n in items {
        match n {
            DataNode::Field(f) if f.usage.is_input() => out.push(f.var.clone()),
            DataNode::Group(g) => collect_inputs(&g.items, out),
            _ => {}
        }
    }
}

/// The help module of the screen region under the cursor.
pub(crate) fn help_at(screen: &Screen, cursor: (usize, usize)) -> Option<String> {
    let (row, col) = cursor;
    screen
        .helps
        .iter()
        .rev()
        .find(|h| h.row == row && h.col <= col && col <= h.end)
        .map(|h| h.module.clone())
}
