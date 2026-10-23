// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Panels every menu uses: a numbered submenu, a notice, a confirmation
//! that runs an action on F10, and a one-column selection list.

use console::Screen;

use crate::dst::{INVALID_SELECTION, NOT_AVAILABLE, selection};
use crate::env::DstEnv;
use crate::panel::{Flow, Outcome, Panel};
use crate::ui;

/// What an option on a submenu does.
pub type Action = Box<dyn Fn(&mut dyn DstEnv) -> Flow>;

/// A numbered submenu whose options are closures.
pub struct Submenu {
    name: &'static str,
    title: &'static str,
    options: Vec<(u8, &'static str, Option<Action>)>,
}

impl Submenu {
    /// A menu with these options; `None` for an option that is not
    /// available in this release.
    #[must_use]
    pub fn new(
        name: &'static str,
        title: &'static str,
        options: Vec<(u8, &'static str, Option<Action>)>,
    ) -> Self {
        Self {
            name,
            title,
            options,
        }
    }
}

impl Panel for Submenu {
    fn name(&self) -> &'static str {
        self.name
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let options: Vec<(u8, &str)> = self.options.iter().map(|(n, t, _)| (*n, *t)).collect();
        ui::menu(
            self.title,
            &env.system_name(),
            &options,
            "F3=Exit   F12=Cancel",
        )
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let Some(n) = selection(fields) else {
            return Flow::Error(INVALID_SELECTION.into());
        };
        match self.options.iter().find(|(number, _, _)| *number == n) {
            Some((_, _, Some(action))) => action(env),
            Some((_, _, None)) => Flow::Message(NOT_AVAILABLE.into()),
            None => Flow::Error(INVALID_SELECTION.into()),
        }
    }
}

/// Lines a notice shows per page.
pub const PAGE_LINES: usize = 17;

/// Text to read, paged with F7 and F8; Enter, F3 or F12 go back.
pub struct Notice {
    name: &'static str,
    title: &'static str,
    lines: Vec<String>,
    page: usize,
}

impl Notice {
    /// With these lines.
    #[must_use]
    pub fn new(name: &'static str, title: &'static str, lines: Vec<String>) -> Self {
        Self {
            name,
            title,
            lines,
            page: 0,
        }
    }

    fn pages(&self) -> usize {
        self.lines.len().div_ceil(PAGE_LINES).max(1)
    }
}

impl Panel for Notice {
    fn name(&self) -> &'static str {
        self.name
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let start = self.page * PAGE_LINES;
        let shown: Vec<String> = self
            .lines
            .iter()
            .skip(start)
            .take(PAGE_LINES)
            .cloned()
            .collect();
        let mut s = ui::text(
            self.title,
            &env.system_name(),
            &shown,
            None,
            "F3=Exit   F7=Previous page   F8=Next page   F12=Cancel",
        );
        let more = if self.page + 1 < self.pages() {
            "More..."
        } else {
            "Bottom"
        };
        s.put(
            ui::KEYS_ROW - 1,
            console::COLS - more.len(),
            more,
            console::Attr::GREEN,
        );
        // One field so Enter goes back too.
        s.field(ui::KEYS_ROW - 1, ui::BODY_COL, 1, false);
        s.put(
            ui::KEYS_ROW - 1,
            ui::BODY_COL + 3,
            "Press Enter to continue.",
            console::Attr::GREEN,
        );
        s
    }

    fn enter(&mut self, _fields: &[String], _env: &mut dyn DstEnv) -> Flow {
        Flow::Pop
    }

    fn function(&mut self, key: u8, _env: &mut dyn DstEnv) -> Flow {
        match key {
            3 => Flow::PopAll,
            12 => Flow::Pop,
            7 => {
                self.page = self.page.saturating_sub(1);
                Flow::Stay
            }
            8 => {
                if self.page + 1 < self.pages() {
                    self.page += 1;
                }
                Flow::Stay
            }
            _ => Flow::Stay,
        }
    }
}

/// What an options list does with the rows and the option typed beside
/// each.
pub type Optioned = Box<dyn FnMut(&[(usize, u8)], &mut dyn DstEnv) -> Flow>;
/// What F6 does on an options list.
pub type Create = Box<dyn Fn(&mut dyn DstEnv) -> Flow>;

/// A list whose option column takes any of a set of option numbers.
pub struct Options {
    name: &'static str,
    title: &'static str,
    prompt: &'static str,
    headings: String,
    rows: Vec<String>,
    allowed: Vec<u8>,
    fkeys: &'static str,
    on_enter: Optioned,
    on_create: Option<Create>,
}

impl Options {
    /// With these rows, the options allowed, and the actions.
    #[must_use]
    #[allow(clippy::too_many_arguments)] // one per part of the display
    pub fn new(
        name: &'static str,
        title: &'static str,
        prompt: &'static str,
        headings: &str,
        rows: Vec<String>,
        allowed: &[u8],
        fkeys: &'static str,
        on_enter: Optioned,
        on_create: Option<Create>,
    ) -> Self {
        Self {
            name,
            title,
            prompt,
            headings: headings.to_string(),
            rows,
            allowed: allowed.to_vec(),
            fkeys,
            on_enter,
            on_create,
        }
    }
}

/// Parse an option column: `(row, option)` for every row with one.
pub fn parse_options(fields: &[String], allowed: &[u8]) -> Result<Vec<(usize, u8)>, String> {
    let mut chosen = Vec::new();
    for (i, f) in fields.iter().enumerate() {
        let f = f.trim();
        if f.is_empty() {
            continue;
        }
        match f.parse::<u8>() {
            Ok(n) if allowed.contains(&n) => chosen.push((i, n)),
            _ => return Err(format!("Option '{f}' is not valid")),
        }
    }
    if chosen.is_empty() {
        return Err("Type an option beside an entry and press Enter".into());
    }
    Ok(chosen)
}

impl Panel for Options {
    fn name(&self) -> &'static str {
        self.name
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let mut s = ui::list(
            self.title,
            &env.system_name(),
            self.prompt,
            &self.headings,
            &self.rows,
            self.fkeys,
        );
        if self.rows.is_empty() {
            s.put(
                ui::BODY_ROW + 3,
                ui::BODY_COL + 4,
                "(None)",
                console::Attr::GREEN,
            );
            s.field(ui::KEYS_ROW - 1, ui::BODY_COL, 1, false);
        }
        s
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        if self.rows.is_empty() {
            return Flow::Pop;
        }
        let mut chosen = Vec::new();
        for (i, f) in fields.iter().enumerate() {
            let f = f.trim();
            if f.is_empty() {
                continue;
            }
            match f.parse::<u8>() {
                Ok(n) if self.allowed.contains(&n) => chosen.push((i, n)),
                _ => return Flow::Error(format!("Option '{f}' is not valid")),
            }
        }
        if chosen.is_empty() {
            return Flow::Error("Type an option beside an entry and press Enter".into());
        }
        (self.on_enter)(&chosen, env)
    }

    fn function(&mut self, key: u8, env: &mut dyn DstEnv) -> Flow {
        match (key, self.on_create.as_ref()) {
            (6, Some(create)) => create(env),
            (3, _) => Flow::PopAll,
            (12, _) => Flow::Pop,
            _ => Flow::Stay,
        }
    }
}

/// What a confirmation goes on to after its action.
pub type Next = Box<dyn Fn(&mut dyn DstEnv) -> Flow>;

/// What a confirmation does on F10.
pub type Confirmed = Box<dyn FnMut(&mut dyn DstEnv) -> Result<String, String>>;

/// A confirmation: the action runs on F10 and its message shows on the
/// panel underneath; F12 cancels.
pub struct Confirm {
    name: &'static str,
    title: &'static str,
    lines: Vec<String>,
    action: Confirmed,
    /// Panels to pop after the action, including this one.
    unwind: usize,
    /// End the application with this outcome after the action.
    exit: Option<Outcome>,
    /// Go on to another panel after the action.
    next: Option<Next>,
}

impl Confirm {
    /// With these lines and this action.
    #[must_use]
    pub fn new(
        name: &'static str,
        title: &'static str,
        lines: Vec<String>,
        action: Confirmed,
    ) -> Self {
        Self {
            name,
            title,
            lines,
            action,
            unwind: 1,
            exit: None,
            next: None,
        }
    }

    /// End the application with `outcome` once the action succeeds.
    #[must_use]
    pub fn exiting(mut self, outcome: Outcome) -> Self {
        self.exit = Some(outcome);
        self
    }

    /// Go on with `next` once the action succeeds.
    #[must_use]
    pub fn then(mut self, next: impl Fn(&mut dyn DstEnv) -> Flow + 'static) -> Self {
        self.next = Some(Box::new(next));
        self
    }

    /// Pop `n` panels after the action (the confirmation and the
    /// selection lists under it).
    #[must_use]
    pub fn unwinding(mut self, n: usize) -> Self {
        self.unwind = n;
        self
    }
}

impl Panel for Confirm {
    fn name(&self) -> &'static str {
        self.name
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        ui::confirm(self.title, &env.system_name(), &self.lines)
    }

    fn enter(&mut self, _fields: &[String], _env: &mut dyn DstEnv) -> Flow {
        Flow::Message("Press F10 to continue or F12 to cancel".into())
    }

    fn function(&mut self, key: u8, env: &mut dyn DstEnv) -> Flow {
        match key {
            10 => match (self.action)(env) {
                Ok(_) if self.exit.is_some() => Flow::Exit(self.exit.expect("checked")),
                Ok(_) if self.next.is_some() => (self.next.as_ref().expect("checked"))(env),
                Ok(m) => Flow::Unwind(self.unwind, Some((m, false))),
                Err(e) => Flow::Error(e),
            },
            3 => Flow::PopAll,
            12 => Flow::Pop,
            _ => Flow::Stay,
        }
    }
}

/// Run a change now, or, when the environment asks for confirmation
/// first (SST), on F10 of a confirmation showing `lines`. Its message
/// shows `unwind` panels back from the one asking (0: on it).
pub fn guarded(
    env: &mut dyn DstEnv,
    name: &'static str,
    title: &'static str,
    lines: Vec<String>,
    unwind: usize,
    mut action: Confirmed,
) -> Flow {
    if env.confirm_changes() {
        return Flow::Push(Box::new(
            Confirm::new(name, title, lines, action).unwinding(unwind + 1),
        ));
    }
    match action(env) {
        Ok(m) if unwind == 0 => Flow::Message(m),
        Ok(m) => Flow::Unwind(unwind, Some((m, false))),
        Err(e) => Flow::Error(e),
    }
}

/// What a selection list does with the chosen rows.
pub type Chosen = Box<dyn FnMut(&[usize], &mut dyn DstEnv) -> Flow>;

/// A list with an option column: type `1` beside rows and press Enter.
pub struct Select {
    name: &'static str,
    title: &'static str,
    prompt: &'static str,
    headings: String,
    rows: Vec<String>,
    on_enter: Chosen,
}

impl Select {
    /// With these rows and this action.
    #[must_use]
    pub fn new(
        name: &'static str,
        title: &'static str,
        prompt: &'static str,
        headings: &str,
        rows: Vec<String>,
        on_enter: Chosen,
    ) -> Self {
        Self {
            name,
            title,
            prompt,
            headings: headings.to_string(),
            rows,
            on_enter,
        }
    }
}

impl Panel for Select {
    fn name(&self) -> &'static str {
        self.name
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let mut s = ui::list(
            self.title,
            &env.system_name(),
            self.prompt,
            &self.headings,
            &self.rows,
            "F3=Exit   F12=Cancel",
        );
        if self.rows.is_empty() {
            s.put(
                ui::BODY_ROW + 3,
                ui::BODY_COL + 4,
                "(No units)",
                console::Attr::GREEN,
            );
            s.field(ui::KEYS_ROW - 1, ui::BODY_COL, 1, false);
        }
        s
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        if self.rows.is_empty() {
            return Flow::Pop;
        }
        let chosen: Vec<usize> = fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f.trim() == "1")
            .map(|(i, _)| i)
            .collect();
        if fields.iter().any(|f| !matches!(f.trim(), "" | "1")) {
            return Flow::Error("Only option 1 is valid".into());
        }
        if chosen.is_empty() {
            return Flow::Error("Type 1 beside a unit and press Enter".into());
        }
        (self.on_enter)(&chosen, env)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MemEnv;
    use crate::panel::{Outcome, Runner};

    fn f(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn confirm_runs_its_action_on_f10_and_unwinds() {
        let mut env = MemEnv::new("QS400");
        let menu = Submenu::new(
            "m",
            "Menu",
            vec![
                (
                    1,
                    "Do it",
                    Some(Box::new(|_env: &mut dyn DstEnv| {
                        Flow::Push(Box::new(Confirm::new(
                            "c",
                            "Confirm",
                            vec!["Sure?".into()],
                            Box::new(|env: &mut dyn DstEnv| {
                                env.add_unit("/dev/vdb").map(|n| format!("unit {n} added"))
                            }),
                        )))
                    })),
                ),
                (2, "Nothing", None),
            ],
        );
        let mut r = Runner::new(Box::new(menu));
        assert_eq!(r.enter(&f(&["2"]), &mut env), None);
        assert!(r.screen(&mut env).dump().contains(NOT_AVAILABLE));
        assert_eq!(r.enter(&f(&["1"]), &mut env), None);
        assert_eq!(r.top(), "c");
        assert_eq!(r.function(12, &mut env), None);
        assert_eq!(r.top(), "m");
        r.enter(&f(&["1"]), &mut env);
        assert_eq!(r.function(10, &mut env), None);
        assert_eq!(r.top(), "m");
        assert!(r.screen(&mut env).dump().contains("unit 0 added"));
        assert_eq!(env.actions, vec!["add /dev/vdb"]);
        assert_eq!(r.function(3, &mut env), Some(Outcome::Exit));
    }

    #[test]
    fn select_reports_the_chosen_rows() {
        let mut env = MemEnv::new("QS400");
        let list = Select::new(
            "s",
            "Pick",
            "Type 1 to select.",
            "Opt  Unit",
            vec!["one".into(), "two".into(), "three".into()],
            Box::new(|chosen: &[usize], _env: &mut dyn DstEnv| {
                Flow::Message(format!("{chosen:?}"))
            }),
        );
        let mut r = Runner::new(Box::new(list));
        assert_eq!(r.screen(&mut env).fields.len(), 3);
        r.enter(&f(&["", "1", "1"]), &mut env);
        assert!(r.screen(&mut env).dump().contains("[1, 2]"));
        r.enter(&f(&["2", "", ""]), &mut env);
        assert!(r.screen(&mut env).dump().contains("Only option 1"));
    }
}
