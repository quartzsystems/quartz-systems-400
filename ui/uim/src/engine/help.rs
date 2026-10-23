// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Help in the engine: field help (F1, in a window or full screen),
//! extended help (F2), the keys' help, links between modules, the
//! search index (F11), the Information Assistant (F13), printing help
//! (F14), `About` and moving to the top (F10).

use std::collections::BTreeSet;
use std::rc::Rc;

use super::{Engine, IndexEntry, Terminal};
use crate::model::{Area, DataNode, Document, Key, ListColNode, Panel, PanelGroup, Span};
use crate::render::{HelpPart, HelpView, HelpWindow, Screen, help_lines, help_title, render_view};

/// One module of a help display, with the group it belongs to.
#[derive(Clone)]
struct Part {
    group: Rc<PanelGroup>,
    title: String,
    body: Document,
}

/// A help display on the stack.
struct Frame {
    parts: Vec<Part>,
    title: String,
    page: usize,
    /// Extended help (no `F2`).
    extended: bool,
    /// In a window over the panel.
    window: bool,
}

/// The search index panel group.
const SEARCH_GROUP: &str = "QUHSCHIDX";

/// The message when nothing has help.
const NO_HELP: &str = "No help is available for this display.";

impl Engine {
    /// A help module by name: this group's, an import's, or a qualified
    /// `GROUP/MODULE` or `LIB/GROUP/MODULE` through the terminal.
    fn resolve<T: Terminal + ?Sized>(&self, term: &mut T, name: &str) -> Option<Part> {
        let name = name.trim().to_ascii_uppercase();
        let parts: Vec<&str> = name.split('/').collect();
        let (group, module) = match parts.as_slice() {
            [m] => {
                if self.group.help(m).is_some() {
                    (Rc::clone(&self.group), (*m).to_string())
                } else {
                    let imp = self
                        .group
                        .imports
                        .iter()
                        .filter(|i| i.name == *m || i.name == "*")
                        .find_map(|i| {
                            let (lib, grp) = split_qualified(&i.pnlgrp);
                            term.group(&lib, &grp).filter(|g| g.help(m).is_some())
                        })?;
                    (imp, (*m).to_string())
                }
            }
            [g, m] => (term.group("*LIBL", g)?, (*m).to_string()),
            [l, g, m] => (term.group(l, g)?, (*m).to_string()),
            _ => return None,
        };
        let h = group.help(&module)?;
        Some(Part {
            title: Span::plain(&h.title).trim().to_string(),
            body: h.body.clone(),
            group: Rc::clone(&group),
        })
    }

    /// F1: the help of the region under the cursor, the panel's when
    /// the cursor is on the title row (extended help, as the goldens
    /// show it).
    pub(crate) fn contextual_help<T: Terminal + ?Sized>(
        &mut self,
        term: &mut T,
        panel: &Panel,
        module: Option<String>,
        under: &Screen,
    ) -> Option<()> {
        let Some(module) = module else {
            self.message = Some((NO_HELP.into(), true));
            return Some(());
        };
        if self.cursor.0 == 1 || panel.help.as_deref() == Some(module.as_str()) {
            return self.extended_help(term, panel);
        }
        let Some(part) = self.resolve(term, &module) else {
            self.message = Some((NO_HELP.into(), true));
            return Some(());
        };
        let frame = Frame {
            title: help_title(&part.title),
            parts: vec![part],
            page: 0,
            extended: false,
            window: !self.full_screen_help,
        };
        self.help_session(term, Some(panel), frame, under)
    }

    /// Help modules by name (`QUIDSPH`, `QUHDSPH`): this group's or
    /// qualified `LIB/GROUP/MODULE`; extended help joins them under the
    /// first one's title, field help shows the first. False when none
    /// resolves; `None` when the display is gone.
    pub fn show_help<T: Terminal + ?Sized>(
        &mut self,
        term: &mut T,
        names: &[String],
        extended: bool,
        full_screen: bool,
    ) -> Option<bool> {
        let mut parts: Vec<Part> = names.iter().filter_map(|n| self.resolve(term, n)).collect();
        if parts.is_empty() {
            return Some(false);
        }
        if !extended {
            parts.truncate(1);
        }
        let frame = Frame {
            title: help_title(&parts[0].title),
            parts,
            page: 0,
            extended,
            window: !full_screen && !extended,
        };
        let under = self.under.clone().unwrap_or_else(|| {
            if term.wide() {
                Screen::new(27, 132)
            } else {
                Screen::new(24, 80)
            }
        });
        self.help_session(term, None, frame, &under)?;
        Some(true)
    }

    /// The text of help modules as lines, for printing them (`QUHPRTH`);
    /// `None` when none resolves.
    pub fn help_text<T: Terminal + ?Sized>(
        &mut self,
        term: &mut T,
        names: &[String],
    ) -> Option<Vec<String>> {
        let parts: Vec<Part> = names.iter().filter_map(|n| self.resolve(term, n)).collect();
        let first = parts.first()?;
        let title = help_title(&first.title);
        let view: Vec<HelpPart> = parts
            .iter()
            .map(|p| HelpPart {
                group: &p.group,
                title: p.title.clone(),
                body: p.body.clone(),
            })
            .collect();
        let mut lines = vec![title, String::new()];
        lines.extend(help_lines(&view, &crate::render::Options::default(), 80));
        Some(lines)
    }

    /// F2: the panel's help and every module its areas name, in order.
    pub(crate) fn extended_help<T: Terminal + ?Sized>(
        &mut self,
        term: &mut T,
        panel: &Panel,
    ) -> Option<()> {
        let names = extended_modules(&self.group, panel);
        let parts: Vec<Part> = names.iter().filter_map(|n| self.resolve(term, n)).collect();
        if parts.is_empty() {
            self.message = Some((NO_HELP.into(), true));
            return Some(());
        }
        let title = panel
            .helptitle
            .clone()
            .unwrap_or_else(|| help_title(&Span::plain(&panel.title)));
        let frame = Frame {
            parts,
            title,
            page: 0,
            extended: true,
            window: false,
        };
        let under = self.under.clone().unwrap_or_else(|| Screen::new(24, 80));
        self.help_session(term, Some(panel), frame, &under)
    }

    /// `ABOUT`: the panel group, the panel and the copyright.
    pub(crate) fn about<T: Terminal + ?Sized>(
        &mut self,
        term: &mut T,
        panel: &Panel,
        under: &Screen,
    ) -> Option<()> {
        let mut body: Document = vec![crate::model::Block::Para(vec![Span::Text(format!(
            "Panel: {}",
            panel.name
        ))])];
        if let Some(c) = &self.group.copyright {
            body.push(crate::model::Block::Para(vec![Span::Text(c.clone())]));
        }
        let frame = Frame {
            parts: vec![Part {
                group: Rc::clone(&self.group),
                title: "About".into(),
                body,
            }],
            title: "About".into(),
            page: 0,
            extended: false,
            window: true,
        };
        self.help_session(term, Some(panel), frame, under)
    }

    /// Show help frames until the operator leaves help.
    #[allow(clippy::too_many_lines)] // one arm per help key
    fn help_session<T: Terminal + ?Sized>(
        &mut self,
        term: &mut T,
        panel: Option<&Panel>,
        first: Frame,
        under: &Screen,
    ) -> Option<()> {
        let mut stack = vec![first];
        let mut more_keys = false;
        let mut message: Option<(String, bool)> = None;
        while let Some(frame) = stack.last_mut() {
            let wide = term.wide();
            let (rows, cols) = if wide { (27, 132) } else { (24, 80) };
            let search = self.group.schidx.is_some();
            let view = HelpView {
                title: frame.title.clone(),
                keys: keys(frame, search, more_keys),
                page: frame.page,
                window: frame.window.then(|| help_window(rows, cols, self.cursor)),
            };
            let parts: Vec<HelpPart> = frame
                .parts
                .iter()
                .map(|p| HelpPart {
                    group: &p.group,
                    title: p.title.clone(),
                    body: p.body.clone(),
                })
                .collect();
            let mut opts = self.options(wide);
            opts.message = message.take();
            let screen = render_view(&parts, &view, &opts, rows, cols, Some(under));
            let input = term.exchange(&screen)?;
            let Some(key) = input.key else {
                continue;
            };
            let at_bottom = screen.more.is_none_or(|m| m.bottom);
            match key {
                Key::Enter => {
                    let link = screen
                        .links
                        .iter()
                        .find(|l| {
                            l.row == input.cursor.0
                                && l.col <= input.cursor.1
                                && input.cursor.1 <= l.end
                        })
                        .map(|l| l.module.clone());
                    match link.and_then(|m| self.resolve(term, &m)) {
                        Some(part) => {
                            let window = frame.window;
                            stack.push(Frame {
                                title: help_title(&part.title),
                                parts: vec![part],
                                page: 0,
                                extended: false,
                                window,
                            });
                        }
                        None => {
                            stack.pop();
                        }
                    }
                }
                Key::F(3) => return Some(()),
                Key::F(12) => {
                    stack.pop();
                }
                Key::F(10) => frame.page = 0,
                Key::PageDown => {
                    if at_bottom {
                        message = Some(("Already at the bottom of the help.".into(), true));
                    } else {
                        frame.page += 1;
                    }
                }
                Key::PageUp => {
                    if frame.page == 0 {
                        message = Some(("Already at the top of the help.".into(), true));
                    } else {
                        frame.page -= 1;
                    }
                }
                Key::F(2) if !frame.extended => {
                    let Some(panel) = panel else {
                        message = Some((NO_HELP.into(), true));
                        continue;
                    };
                    let names = extended_modules(&self.group, panel);
                    let parts: Vec<Part> =
                        names.iter().filter_map(|n| self.resolve(term, n)).collect();
                    if parts.is_empty() {
                        message = Some((NO_HELP.into(), true));
                    } else {
                        let title = panel
                            .helptitle
                            .clone()
                            .unwrap_or_else(|| help_title(&Span::plain(&panel.title)));
                        stack.push(Frame {
                            parts,
                            title,
                            page: 0,
                            extended: true,
                            window: false,
                        });
                    }
                }
                Key::F(20) if frame.window => frame.window = false,
                Key::F(24) if frame.window => more_keys = !more_keys,
                Key::F(11) if search => {
                    self.search(term)?;
                }
                Key::F(13) => {
                    if !self.assistant(term)? {
                        message =
                            Some(("The Information Assistant is not available.".into(), true));
                    }
                }
                Key::F(14) => {
                    let mut lines = vec![frame.title.clone(), String::new()];
                    lines.extend(help_lines(&parts, &opts, cols));
                    message = Some(match term.print(&frame.title, &lines) {
                        Ok(m) => (m, false),
                        Err(e) => (e, true),
                    });
                }
                _ => message = Some(("Function key not allowed.".into(), true)),
            }
        }
        Some(())
    }

    /// F13: the Information Assistant (`QUIINFO`); false when the
    /// terminal cannot open it.
    fn assistant<T: Terminal + ?Sized>(&mut self, term: &mut T) -> Option<bool> {
        let Some(g) = term.group("QSYS", "QUIINFO") else {
            return Some(false);
        };
        let Some(first) = g.panels.first().map(|p| p.name.clone()) else {
            return Some(false);
        };
        let mut e = Engine::new(g, &self.system);
        e.full_screen_help = true;
        loop {
            match e.show(term, &first).ok()? {
                super::Response::Gone => return None,
                super::Response::Exit | super::Response::Cancel => return Some(true),
                _ => {}
            }
        }
    }

    /// F11: search the group's index (`SCHIDX`) and show a topic.
    fn search<T: Terminal + ?Sized>(&mut self, term: &mut T) -> Option<()> {
        let name = self.group.schidx.clone().unwrap_or_default();
        let (lib, idx) = split_qualified(&name);
        let Some(entries) = term.search_index(&lib, &idx) else {
            self.message = Some((format!("Search index {idx} was not found."), true));
            return Some(());
        };
        let Some(g) = search_group() else {
            return Some(());
        };
        let mut e = Engine::new(g, &self.system);
        e.full_screen_help = true;
        e.set("SCHIDX", &idx);
        loop {
            match e.show(term, "SEARCH").ok()? {
                super::Response::Gone => return None,
                super::Response::Exit | super::Response::Cancel => return Some(()),
                _ => {}
            }
            let words: Vec<String> = e
                .get("WORDS")
                .split_whitespace()
                .map(str::to_ascii_uppercase)
                .collect();
            let found: Vec<&IndexEntry> = entries
                .iter()
                .filter(|x| {
                    words.is_empty()
                        || words
                            .iter()
                            .any(|w| x.roots.iter().any(|r| r.starts_with(w.as_str())))
                })
                .collect();
            e.clear_list("TOPICS");
            for x in &found {
                e.add_row(
                    "TOPICS",
                    vec![
                        ("OPT".into(), String::new()),
                        ("TOPIC".into(), x.title.clone()),
                    ],
                );
            }
            e.list_mut("TOPICS").complete = true;
            loop {
                match e.show(term, "TOPICS").ok()? {
                    super::Response::Gone => return None,
                    super::Response::Exit => return Some(()),
                    super::Response::Cancel => break,
                    super::Response::Options(choices) => {
                        for c in choices {
                            let Some(x) = found.get(c.row) else {
                                continue;
                            };
                            let qualified = format!("{}/{}", x.pnlgrp, x.module);
                            if let Some(part) = self.resolve(term, &qualified) {
                                let frame = Frame {
                                    title: help_title(&part.title),
                                    parts: vec![part],
                                    page: 0,
                                    extended: true,
                                    window: false,
                                };
                                let under = Screen::new(24, 80);
                                self.help_session(term, None, frame, &under)?;
                            }
                        }
                        e.list_mut("TOPICS").options.clear();
                    }
                    _ => {}
                }
            }
        }
    }
}

/// The compiled search panel group, shipped as source.
fn search_group() -> Option<Rc<PanelGroup>> {
    let text = crate::shipped::source(SEARCH_GROUP)?;
    let c = crate::compile(
        text,
        &crate::NoImbeds,
        &crate::NoImports,
        &crate::CompileOpts::default(),
    );
    c.group.map(Rc::new)
}

/// `LIB/NAME` or `NAME` (`*LIBL`).
fn split_qualified(name: &str) -> (String, String) {
    match name.split_once('/') {
        Some((l, n)) => (l.trim().to_ascii_uppercase(), n.trim().to_ascii_uppercase()),
        None => ("*LIBL".into(), name.trim().to_ascii_uppercase()),
    }
}

/// The keys of a help display (R22): field help adds `F2`, a window
/// `F20` and `F24` with its second set, a group with a search index
/// `F11`.
fn keys(frame: &Frame, search: bool, more: bool) -> Vec<String> {
    let mut k: Vec<&str> = Vec::new();
    if frame.window {
        if more {
            k.extend(["F3=Exit help", "F14=Print help", "F24=More keys"]);
        } else {
            if !frame.extended {
                k.push("F2=Extended help");
            }
            k.push("F10=Move to top");
            if search {
                k.push("F11=Search index");
            }
            k.extend([
                "F12=Cancel",
                "F13=Information Assistant",
                "F20=Enlarge",
                "F24=More keys",
            ]);
        }
    } else {
        if !frame.extended {
            k.push("F2=Extended help");
        }
        k.extend(["F3=Exit help", "F10=Move to top"]);
        if search {
            k.push("F11=Search index");
        }
        k.extend(["F12=Cancel", "F13=Information Assistant", "F14=Print help"]);
    }
    k.into_iter().map(str::to_string).collect()
}

/// R23: a help window: 72 columns wide inside its borders, 12 rows
/// (fewer on a short screen), under the cursor's row when it fits.
fn help_window(rows: usize, cols: usize, cursor: (usize, usize)) -> HelpWindow {
    let inner_cols = 72.min(cols - 4);
    let inner_rows = 12.min(rows - 4);
    let (top, _) = crate::render::place(rows, cols, inner_rows, inner_cols, cursor);
    let left = (cols - inner_cols - 2) / 2;
    HelpWindow {
        top,
        left: left.max(1),
        rows: inner_rows,
        cols: inner_cols,
    }
}

/// The modules extended help shows for a panel: its own, then each
/// area's and item's in order, then the keys'.
fn extended_modules(group: &PanelGroup, panel: &Panel) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen = BTreeSet::new();
    let mut add = |m: &Option<String>, out: &mut Vec<String>| {
        if let Some(m) = m
            && seen.insert(m.clone())
        {
            out.push(m.clone());
        }
    };
    add(&panel.help, &mut out);
    for a in &panel.areas {
        match a {
            Area::Data(d) => {
                add(&d.help, &mut out);
                data_helps(&d.items, &mut |m| add(m, &mut out));
            }
            Area::List(l) => {
                add(&l.help, &mut out);
                for x in &l.actions {
                    add(&x.help, &mut out);
                }
                for c in &l.columns {
                    match c {
                        ListColNode::Column(c) => add(&c.help, &mut out),
                        ListColNode::Group(g) => {
                            add(&g.help, &mut out);
                            for c in &g.columns {
                                add(&c.help, &mut out);
                            }
                        }
                    }
                }
            }
            Area::Menu(m) => {
                add(&m.help, &mut out);
                for i in m.all_items() {
                    add(&i.help, &mut out);
                }
            }
            Area::Info(i) => add(&i.help, &mut out),
            _ => {}
        }
    }
    if let Some(k) = group.key_list(&panel.keyl) {
        add(&k.help, &mut out);
    }
    out
}

fn data_helps(items: &[DataNode], f: &mut dyn FnMut(&Option<String>)) {
    for n in items {
        match n {
            DataNode::Field(x) => f(&x.help),
            DataNode::Select(x) => f(&x.help),
            DataNode::Const(x) => f(&x.help),
            DataNode::Group(g) => {
                f(&g.help);
                data_helps(&g.items, f);
            }
            DataNode::Column(_) => {}
        }
    }
}
