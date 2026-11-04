// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Panel help, written into the hand-made panel group sources once: for
//! every place F1 can land on that has no help yet, a `HELP=` on its tag
//! and a module before `:EPNLGRP.`. Data items and list columns take
//! the opening text of the command parameter with the same prompt where
//! one exists; options say what they run; key lists explain each key;
//! menu items show their command's help. The modules are ordinary
//! source after that and are edited by hand.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use uim::model::{
    Action, Area, DataNode, Display, KeyList, ListColNode, MenuItem, MenuNode, Panel, PanelGroup,
    Span,
};

use super::uimtext::{esc, name_attr, tagged};
use crate::{AuditError, Groups, Result, files, read, stem};

/// The hand-made sources this writes into (the generated menus and
/// command help have generators of their own).
pub const DIRS: &[&str] = &["ui/uim/pnlgrp", "ui/menus/sys"];

/// What the parameters of the shipped commands say, by prompt text.
pub struct Openings {
    by_prompt: BTreeMap<String, String>,
    /// Command name to its help group and module.
    pub commands: BTreeMap<String, (String, String, String)>,
}

fn norm(label: &str) -> String {
    label
        .trim()
        .trim_end_matches([':', '.', ' ', '>'])
        .trim_end_matches(" .")
        .trim()
        .to_ascii_lowercase()
}

/// The first paragraph of a module, as plain text.
fn first_para(h: &uim::model::HelpModule) -> Option<String> {
    h.body.iter().find_map(|b| match b {
        uim::model::Block::Para(s) => Some(Span::plain(s).split_whitespace().collect::<Vec<_>>().join(" ")),
        _ => None,
    })
}

impl Openings {
    /// From the command help groups and definitions.
    pub fn load(root: &Path, groups: &Groups) -> Result<Self> {
        let mut counts: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
        let mut commands = BTreeMap::new();
        for (def, _) in crate::commands::definitions(root)? {
            let group = super::cmdhelp::group_of(&def);
            let id = def.hlpid.clone().unwrap_or_else(|| def.name.clone());
            commands.insert(def.name.clone(), (group.clone(), id.clone(), def.prompt.clone()));
            let Some(g) = groups.get(&group) else {
                continue;
            };
            for p in &def.parms {
                let Some(h) = g.help(&format!("{id}/{}", p.kwd)) else {
                    continue;
                };
                let Some(t) = first_para(h) else {
                    continue;
                };
                // Text about "the command" reads wrongly on a display.
                if t.ends_with("for this command.")
                    || t.starts_with("Whether to ")
                    || t.contains("command")
                {
                    continue;
                }
                *counts
                    .entry(norm(p.prompt_text()))
                    .or_default()
                    .entry(t)
                    .or_default() += 1;
            }
        }
        let by_prompt = counts
            .into_iter()
            .filter_map(|(k, m)| m.into_iter().max_by_key(|(_, n)| *n).map(|(t, _)| (k, t)))
            .collect();
        Ok(Self { by_prompt, commands })
    }

    fn text_for(&self, label: &str) -> Option<&str> {
        self.by_prompt.get(&norm(label)).map(String::as_str)
    }
}

/// A module to add, and the tag lines that point at it.
struct Plan {
    modules: Vec<(String, String)>,
    defined: BTreeSet<String>,
    /// Source line index to the help name its tag gets.
    tags: BTreeMap<usize, String>,
    imports: BTreeSet<(String, String)>,
}

impl Plan {
    fn module(&mut self, name: &str, title: &str, body: &str) {
        if self.defined.insert(name.to_string()) {
            self.modules.push((name.to_string(), format!(
                ":HELP NAME={}.{}\n{body}:EHELP.\n",
                name_attr(name),
                esc(title)
            )));
        }
    }
}

fn plain(spans: &[Span]) -> String {
    Span::plain(spans).trim().to_string()
}

fn lc(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(_) if c.clone().next().is_some_and(char::is_uppercase) => s.to_string(),
        Some(f) => f.to_lowercase().chain(c).collect(),
        None => String::new(),
    }
}

fn label_of(spans: &[Span]) -> String {
    let t = plain(spans);
    let t = t.trim_end_matches([':', '.', ' ']);
    let t = t.strip_suffix(" .").unwrap_or(t).trim_end_matches([' ', '.']);
    t.trim().to_string()
}

fn field_text(o: &Openings, label: &str, input: bool, column: bool) -> String {
    if let Some(t) = o.text_for(label) {
        return t.to_string();
    }
    let what = lc(label);
    match (input, column) {
        (true, true) => format!("Type the {what} for the entry on the line."),
        (true, false) => format!("Type the {what}."),
        (false, true) => format!("The {what} of each entry in the list."),
        (false, false) => format!("The {what} shown for what the display is about."),
    }
}

/// The command of a `CMD` action and whether it prompts.
fn command_of(action: &Action) -> Option<(String, bool)> {
    let Action::Cmd(s) = action else {
        return None;
    };
    let s = s.trim();
    let (prompt, rest) = s.strip_prefix('?').map_or((false, s), |r| (true, r));
    let name = rest.split(|c: char| c.is_whitespace() || c == '(').next()?.to_ascii_uppercase();
    Some((name.rsplit('/').next().unwrap_or(&name).to_string(), prompt))
}

fn option_text(o: &Openings, n: &str, text: &str, action: Option<&Action>) -> String {
    let verb = lc(text);
    let base = format!("Type {n} next to an entry and press Enter to {verb} it.");
    match action.and_then(command_of) {
        Some((c, prompt)) => match o.commands.get(&c) {
            Some((_, _, title)) if prompt => {
                format!("{base} The {title} ({c}) command is prompted for the entry.")
            }
            Some((_, _, title)) => format!("{base} The {title} ({c}) command runs for the entry."),
            None => base,
        },
        None => base,
    }
}

/// What a function key does, by its text.
fn key_text(text: &str, action: &Action) -> String {
    let what = text.split_once('=').map_or(text, |(_, w)| w).trim();
    let known = match what.to_ascii_lowercase().as_str() {
        "exit" => Some("Ends the display and returns to where it was started from."),
        "cancel" => Some("Returns to the previous display without acting on what was typed."),
        "refresh" => Some("Shows the display again with the current information."),
        "retrieve" => Some("Puts the last command run back on the command line; press it again for the one before."),
        "prompt" => Some("Prompts the command on the command line, or the command of the option typed."),
        "more keys" => Some("Shows the other function keys of the display."),
        "help" => Some("Shows help for where the cursor is."),
        "print" => Some("Prints the display."),
        "command line" => Some("Shows a command line to run a command from."),
        "more options" => Some("Shows the other options of the list."),
        "display names only" => Some("Shows the list with the names alone, so more fit on a page."),
        "position to" => Some("Positions the list at the name typed on the command line."),
        "top" => Some("Moves to the top of the list."),
        "bottom" => Some("Moves to the bottom of the list."),
        "messages" => Some("Shows the messages of the job."),
        "change view" | "view" => Some("Shows other columns of the list."),
        "sort" => Some("Sorts the list by the column the cursor is on."),
        "restart statistics" => Some("Starts the statistics over from now."),
        "work with" => Some("Shows the work-with display of what the display is about."),
        "display details" | "details" => Some("Shows more about the entry."),
        _ => None,
    };
    if let Some(k) = known {
        return k.to_string();
    }
    match action {
        Action::Exit => "Ends the display.".into(),
        Action::Cancel => "Returns to the previous display.".into(),
        Action::Retrieve => "Puts the last command run back on the command line.".into(),
        Action::Cmd(c) => format!("Runs {}.", c.trim_start_matches('?')),
        Action::Menu(m) => format!("Shows menu {m}."),
        _ => format!("{}.", what.trim_end_matches('.')),
    }
}

fn keys_module(k: &KeyList) -> String {
    let mut b = String::from(":PARML.\n");
    for i in k.keys.iter().filter(|i| !i.text.trim().is_empty()) {
        let _ = writeln!(b, ":PT.{}", esc(i.text.trim()));
        b.push_str(&tagged(":PD.", &key_text(&i.text, &i.action)));
    }
    b.push_str(":EPARML.\n");
    b
}

fn menu_item(o: &Openings, plan: &mut Plan, panel: &Panel, i: &MenuItem) {
    let text = plain(&i.text);
    let n = match i.option {
        uim::model::MenuOption::Number(n) => n.to_string(),
        uim::model::MenuOption::Star => "0".into(),
    };
    if let Some((c, _)) = command_of(&i.action)
        && let Some((g, id, _)) = o.commands.get(&c)
        && !plan.defined.contains(id)
    {
        plan.imports.insert((id.clone(), g.clone()));
        plan.tags.insert(i.line as usize, id.clone());
        return;
    }
    let name = format!("{}/OPT{n}", panel.name);
    let body = match &i.action {
        Action::Menu(m) => format!("Shows menu {m}: {text}."),
        Action::Cmd(c) => match command_of(&i.action) {
            Some((name, _)) if c.trim_start_matches('?').trim().eq_ignore_ascii_case(&name) => {
                format!("Runs the {name} command: {text}.")
            }
            _ => format!("Runs {}: {text}.", c.trim_start_matches('?').trim()),
        },
        Action::HelpModule(_) => format!("Shows help: {text}."),
        _ => format!("{text}."),
    };
    plan.module(&name, &text, &tagged(":P.", &format!("Option {n}. {body}")));
    plan.tags.insert(i.line as usize, name);
}

fn data_items(o: &Openings, plan: &mut Plan, nodes: &[DataNode], area: bool) {
    for n in nodes {
        match n {
            DataNode::Group(g) => data_items(o, plan, &g.items, area),
            DataNode::Field(f) if f.help.is_none() && !area && f.display != Display::No => {
                let label = label_of(&f.prompt);
                let name = format!("FLD/{}", f.var);
                let title = if label.is_empty() { f.var.clone() } else { label.clone() };
                let body = tagged(":P.", &field_text(o, &title, f.usage.is_input(), false));
                plan.module(&name, &title, &body);
                plan.tags.insert(f.line as usize, name);
            }
            DataNode::Select(s) if s.help.is_none() && !area && s.display != Display::No => {
                let label = label_of(&s.prompt);
                let name = format!("FLD/{}", s.var);
                let mut body = tagged(":P.", &field_text(o, &label, true, false));
                body.push_str(":PARML.\n");
                for c in &s.choices {
                    let _ = writeln!(body, ":PT.{}", esc(&c.value));
                    body.push_str(&tagged(":PD.", &format!("{}.", plain(&c.text))));
                }
                body.push_str(":EPARML.\n");
                plan.module(&name, &label, &body);
                plan.tags.insert(s.line as usize, name);
            }
            _ => {}
        }
    }
}

#[allow(clippy::many_single_char_names, clippy::too_many_lines)] // one arm per area
fn plan_group(o: &Openings, groups: &Groups, g: &PanelGroup) -> Plan {
    let mut plan = Plan {
        modules: Vec::new(),
        defined: g.helps.iter().map(|h| h.name.clone()).collect(),
        tags: BTreeMap::new(),
        imports: BTreeSet::new(),
    };
    for p in &g.panels {
        if let Some(k) = g.key_lists.iter().find(|k| k.name.eq_ignore_ascii_case(&p.keyl))
            && k.keys.iter().any(|i| !i.text.trim().is_empty())
            && !groups.has_text(g, k.help.as_deref())
            && !plan.tags.values().any(|v| *v == format!("KEYS/{}", k.name))
        {
            let name = format!("KEYS/{}", k.name);
            plan.module(&name, "Function keys", &keys_module(k));
            plan.tags.insert(k.line as usize, name);
        }
        for a in &p.areas {
            match a {
                Area::Data(d) => {
                    let covered = groups.has_text(g, d.help.as_deref());
                    data_items(o, &mut plan, &d.items, covered);
                }
                Area::List(l) => {
                    if groups.has_text(g, l.help.as_deref()) {
                        continue;
                    }
                    for (col, grp) in l.columns.iter().flat_map(|n| match n {
                        ListColNode::Column(c) => vec![(c, None)],
                        ListColNode::Group(gr) => gr.columns.iter().map(|c| (c, Some(gr))).collect(),
                    }) {
                        if col.help.is_some() || col.display == Display::No {
                            continue;
                        }
                        let mut label = label_of(&col.heading);
                        if let Some(gr) = grp {
                            label = format!("{} {}", label_of(&gr.heading), lc(&label)).trim().to_string();
                        }
                        let (name, body) = if col.usage.is_input() && l.actions.iter().any(|a| a.option.is_some()) && plan_is_option(col, l) {
                            (
                                format!("{}/OPT", p.name),
                                tagged(":P.", "Type the number of an option next to an entry and press Enter. The options the list allows are shown above it; F1 on one of them explains it."),
                            )
                        } else {
                            (format!("FLD/{}", col.var), tagged(":P.", &field_text(o, if label.is_empty() { &col.var } else { &label }, col.usage.is_input(), true)))
                        };
                        let title = if name.ends_with("/OPT") { "Option".to_string() } else if label.is_empty() { col.var.clone() } else { label };
                        plan.module(&name, &title, &body);
                        plan.tags.insert(col.line as usize, name);
                    }
                    let opts: Vec<_> = l
                        .actions
                        .iter()
                        .filter(|a| a.option.is_some() && a.help.is_none() && !plain(&a.text).is_empty())
                        .collect();
                    if !opts.is_empty() {
                        let name = format!("{}/OPTIONS", p.name);
                        let mut body = tagged(":P.", "Type an option number next to one or more entries and press Enter. The option runs for each entry it is typed next to, in order.");
                        body.push_str(":PARML.\n");
                        for a in &opts {
                            let t = plain(&a.text);
                            let (n, what) = t.split_once('=').unwrap_or(("", t.as_str()));
                            let n = if n.is_empty() { a.option.clone().unwrap_or_default() } else { n.to_string() };
                            let _ = writeln!(body, ":PT.{}", esc(&t));
                            body.push_str(&tagged(":PD.", &option_text(o, &n, what, a.enter.as_ref())));
                        }
                        body.push_str(":EPARML.\n");
                        plan.module(&name, "Options", &body);
                        for a in opts {
                            plan.tags.insert(a.line as usize, name.clone());
                        }
                    }
                }
                Area::Menu(m) => {
                    if groups.has_text(g, m.help.as_deref()) {
                        continue;
                    }
                    for n in &m.items {
                        let (items, gh) = match n {
                            MenuNode::Item(i) => (std::slice::from_ref(i), None),
                            MenuNode::Group { items, help, .. } => (items.as_slice(), help.as_deref()),
                        };
                        if groups.has_text(g, gh) {
                            continue;
                        }
                        for i in items {
                            if i.help.is_none() && !plain(&i.text).is_empty() {
                                menu_item(o, &mut plan, p, i);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    plan
}

/// The option column of a list: its first input column.
fn plan_is_option(col: &uim::model::ListColumn, l: &uim::model::ListArea) -> bool {
    l.columns
        .iter()
        .flat_map(|n| match n {
            ListColNode::Column(c) => vec![c],
            ListColNode::Group(g) => g.columns.iter().collect(),
        })
        .find(|c| c.usage.is_input())
        .is_some_and(|c| c.var == col.var)
}

/// Put `HELP=name` on the tag at `line` (0-based, or the line after).
fn tag_line(lines: &mut [String], line: usize, name: &str) -> bool {
    for i in [line, line.saturating_sub(1), line + 1] {
        let Some(l) = lines.get_mut(i) else {
            continue;
        };
        if !l.starts_with(':') || l.to_ascii_uppercase().contains(" HELP=") {
            continue;
        }
        let end = l[1..].find([' ', '.']).map_or(l.len(), |e| e + 1);
        let tag = l[1..end].to_ascii_uppercase();
        if !matches!(
            tag.as_str(),
            "DATAI" | "DATASLT" | "LISTCOL" | "LISTACT" | "MENUI" | "KEYL"
        ) {
            continue;
        }
        l.insert_str(end, &format!(" HELP={}", name_attr(name)));
        return true;
    }
    false
}

/// Write the panel help into every hand-made source; the files changed.
pub fn write(root: &Path) -> Result<Vec<String>> {
    let groups = Groups::load(root)?;
    let o = Openings::load(root, &groups)?;
    let mut changed = Vec::new();
    for dir in DIRS {
        for p in files(&root.join(dir), "pnlgrp")? {
            let name = stem(&p);
            if name.starts_with("QH") && name.ends_with("CMD") || name == "QTOCMD" || name.starts_with("QHDC") {
                continue;
            }
            let Some(g) = groups.get(&name) else {
                continue;
            };
            let plan = plan_group(&o, &groups, g);
            if plan.tags.is_empty() && plan.modules.is_empty() {
                continue;
            }
            let text = read(&p)?.replace("\r\n", "\n");
            let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
            for (line, module) in &plan.tags {
                if !tag_line(&mut lines, *line, module) {
                    return Err(AuditError::Compile(
                        format!("{dir}/{name}.pnlgrp"),
                        format!("no tag for help {module} at line {line}"),
                    ));
                }
            }
            let at = lines
                .iter()
                .rposition(|l| l.trim().eq_ignore_ascii_case(":EPNLGRP."))
                .unwrap_or(lines.len());
            let mut add: Vec<String> = Vec::new();
            for (m, g) in &plan.imports {
                add.push(format!(":IMPORT NAME={m} PNLGRP={g}."));
            }
            // Imports go before the first panel; modules at the end.
            let first_panel = lines
                .iter()
                .position(|l| {
                    let u = l.to_ascii_uppercase();
                    u.starts_with(":PANEL ") || u.starts_with(":KEYL ")
                })
                .unwrap_or(at);
            let mut out: Vec<String> = Vec::new();
            out.extend(lines[..first_panel].iter().cloned());
            out.extend(add);
            out.extend(lines[first_panel..at].iter().cloned());
            if !plan.modules.is_empty() {
                out.push(".* Item, key and option help.".to_string());
                for (_, m) in &plan.modules {
                    out.extend(m.lines().map(str::to_string));
                }
            }
            out.extend(lines[at..].iter().cloned());
            let mut s = out.join("\n");
            s.push('\n');
            std::fs::write(&p, s).map_err(|e| AuditError::Io(p.clone(), e))?;
            changed.push(format!("{dir}/{name}"));
        }
    }
    Ok(changed)
}
