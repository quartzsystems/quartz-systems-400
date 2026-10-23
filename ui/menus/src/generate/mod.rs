// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The command-group menus as UIM panel-group sources: every `CMDxxx`
//! menu of IBM's inventory, numbered over IBM's command list, with our
//! titles, plus `MAJOR`, `VERB` and `SUBJECT`. `genmenus` writes them
//! to `ui/menus/pnlgrp/`; `tests/generated.rs` keeps them in step.

pub mod emit;
pub mod fixed;
pub mod inventory;
pub mod member;
pub mod titles;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub use inventory::{CmdEntry, MenuEntry};
pub use member::Classifier;

use crate::Result;

/// The libraries whose commands the menus list.
const LIBRARIES: &[&str] = &["QSYS", "QTCP"];

/// What the generator reads.
#[derive(Clone, Debug)]
pub struct Inputs {
    /// IBM's menus.
    pub menus: Vec<MenuEntry>,
    /// IBM's commands.
    pub commands: Vec<CmdEntry>,
    /// Our command titles, by name.
    pub titles: BTreeMap<String, String>,
}

impl Inputs {
    /// The menu inventory, from the repository root.
    pub const MENU_LIST: &'static str = "ui/menus/inventory/ibm-menu-list.txt";
    /// The command inventory.
    pub const CMD_LIST: &'static str = "tools/conformance/oracle/ibm-cmd-list.txt";
    /// Our definitions.
    pub const CMD_DIR: &'static str = "command/sys-cpps/cmds";

    /// Read the three inputs under the repository root.
    pub fn read(root: &Path) -> Result<Self> {
        let menus = std::fs::read_to_string(root.join(Self::MENU_LIST))?;
        let commands = std::fs::read_to_string(root.join(Self::CMD_LIST))?;
        Ok(Self {
            menus: inventory::parse_menu_list(&menus),
            commands: inventory::parse_cmd_list(&commands),
            titles: titles::read_titles(&root.join(Self::CMD_DIR))?,
        })
    }
}

/// How a menu's members were chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Names starting with the verb.
    Verb(String),
    /// The verb, then the subject.
    Composite {
        /// The verb.
        verb: String,
        /// The subject.
        subject: String,
    },
    /// The code as a token after the verb.
    Subject(String),
    /// `MAJOR`, `VERB`, `SUBJECT`.
    Fixed,
}

/// One option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// The option number.
    pub option: u16,
    /// The command, or the menu of a fixed menu.
    pub name: String,
    /// Our title; `None` for a command QS/400 does not ship, whose row
    /// is blank.
    pub title: Option<String>,
}

/// One generated menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    /// The name.
    pub name: String,
    /// The title.
    pub title: String,
    /// How the members were chosen.
    pub kind: Kind,
    /// The options in order.
    pub items: Vec<Item>,
    /// The UIM source.
    pub source: String,
}

impl Menu {
    /// `NAME.pnlgrp`.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!("{}.pnlgrp", self.name)
    }
}

/// The inventory reconciliation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// `CMDxxx` menus in IBM's list.
    pub inventory: usize,
    /// Of them, listed on 7.1.
    pub inventory_71: usize,
    /// Menus generated, the three fixed ones included.
    pub generated: usize,
    /// Inventory menus left for hand-written sources.
    pub hand_written: Vec<String>,
    /// Generated menus with no command.
    pub empty: Vec<String>,
    /// Commands of IBM's list the menus consider.
    pub commands: usize,
    /// Options over every command menu.
    pub placements: usize,
    /// Of them, with a title (a command QS/400 ships).
    pub shipped_placements: usize,
    /// IBM commands no menu lists.
    pub unplaced: Vec<String>,
    /// Our commands IBM's list lacks.
    pub ours_only: Vec<String>,
}

/// The generator's output.
#[derive(Clone, Debug)]
pub struct Generated {
    /// The menus, command menus first in name order, then `MAJOR`,
    /// `VERB`, `SUBJECT`.
    pub menus: Vec<Menu>,
    /// The counts.
    pub report: Report,
}

fn cmp_names(a: &str, b: &str) -> std::cmp::Ordering {
    ccsid::ebcdic_cmp_str(a, b)
}

/// The commands the menus list: `QSYS` and `QTCP`, one row per name,
/// in EBCDIC order; with a release, those listed on it.
fn commands(inputs: &Inputs, release: Option<u8>) -> Vec<&CmdEntry> {
    let mut seen = BTreeSet::new();
    let mut out: Vec<&CmdEntry> = inputs
        .commands
        .iter()
        .filter(|c| LIBRARIES.contains(&c.lib.as_str()))
        .filter(|c| release.is_none_or(|r| c.releases.contains(&r)))
        .filter(|c| seen.insert(c.name.clone()))
        .collect();
    out.sort_by(|a, b| cmp_names(&a.name, &b.name));
    out
}

fn kind_of(code: &str) -> Kind {
    if let Some((_, verb, subject)) = member::COMPOSITES.iter().find(|(c, _, _)| *c == code) {
        return Kind::Composite {
            verb: (*verb).to_string(),
            subject: (*subject).to_string(),
        };
    }
    if member::VERBS.contains(&code) {
        return Kind::Verb(code.to_string());
    }
    Kind::Subject(code.to_string())
}

fn is_member(classifier: &Classifier, menu: &str, kind: &Kind, name: &str) -> bool {
    let curated = member::CURATED
        .iter()
        .any(|(m, names)| *m == menu && names.contains(&name));
    curated
        || match kind {
            Kind::Verb(v) => Classifier::verb_member(name, v),
            Kind::Composite { verb, subject } => classifier.composite_member(name, verb, subject),
            Kind::Subject(s) => classifier.subject_member(name, s),
            Kind::Fixed => false,
        }
}

/// Is the menu on `VERB`?
#[must_use]
pub fn on_verb_menu(name: &str) -> bool {
    let Some(code) = name.strip_prefix("CMD") else {
        return false;
    };
    member::VERBS.contains(&code)
        || member::VERB_LISTED.contains(&code)
        || member::COMPOSITES.iter().any(|(c, _, _)| *c == code)
}

fn help_for(kind: &Kind, title: &str) -> Vec<String> {
    let how = match kind {
        Kind::Verb(v) => format!("every command whose name starts with {v}"),
        Kind::Composite { verb, subject } => {
            format!("every command whose name starts with {verb} and carries {subject}")
        }
        Kind::Subject(s) => format!("every command whose name carries {s} after its verb"),
        Kind::Fixed => String::new(),
    };
    vec![
        format!(
            "{title}: {how}, in name order, with the command name at the right of its row. The option numbers follow the full command set of the systems this one is compatible with, so a blank row is a command this release does not provide."
        ),
        "Type an option number and press Enter to prompt the command, or type a command on the command line. F12 returns to the previous menu, F3 to the menu where you started and F16 to the major command groups.".to_string(),
    ]
}

fn command_menu(
    entry: &MenuEntry,
    classifier: &Classifier,
    all: &[&CmdEntry],
    titles: &BTreeMap<String, String>,
) -> Menu {
    let code = &entry.name[3..];
    let kind = kind_of(code);
    let items: Vec<Item> = all
        .iter()
        .filter(|c| is_member(classifier, &entry.name, &kind, &c.name))
        .enumerate()
        .map(|(i, c)| Item {
            option: u16::try_from(i + 1).expect("under a thousand commands"),
            name: c.name.clone(),
            title: titles.get(&c.name).cloned(),
        })
        .collect();
    let title = entry.text.trim();
    let panel = emit::Panel {
        name: &entry.name,
        title,
        sysname: false,
        info_key: false,
        f16: ("MENU MAJOR", "F16=Major menu"),
        group: Some("Commands"),
        items: items
            .iter()
            .map(|i| {
                let text = i
                    .title
                    .as_deref()
                    .map(|t| emit::item_text(t, &i.name))
                    .unwrap_or_default();
                (i.option, text, format!("CMD ?{}", i.name))
            })
            .collect(),
        help: help_for(&kind, title),
    };
    Menu {
        name: entry.name.clone(),
        title: title.to_string(),
        kind,
        items,
        source: emit::source(&panel),
    }
}

fn fixed_menu(
    name: &str,
    title: &str,
    sysname: bool,
    rows: &[(String, String, String)],
    help: &str,
) -> Menu {
    let items: Vec<Item> = rows
        .iter()
        .enumerate()
        .map(|(i, (text, n, _))| Item {
            option: u16::try_from(i + 1).expect("few groups"),
            name: n.clone(),
            title: Some(text.clone()),
        })
        .collect();
    let (f16, f16_text) = if sysname {
        ("MENU MAIN", "F16=System Main menu")
    } else {
        ("MENU MAJOR", "F16=Major menu")
    };
    let panel = emit::Panel {
        name,
        title,
        sysname,
        info_key: sysname,
        f16: (f16, f16_text),
        group: None,
        items: rows
            .iter()
            .enumerate()
            .map(|(i, (text, n, action))| {
                (
                    u16::try_from(i + 1).expect("few groups"),
                    emit::item_text(text, n),
                    action.clone(),
                )
            })
            .collect(),
        help: vec![
            help.to_string(),
            "Type an option number and press Enter to show that menu, or type a command on the command line. F12 returns to the previous menu and F3 to the menu where you started.".to_string(),
        ],
    };
    Menu {
        name: name.to_string(),
        title: title.to_string(),
        kind: Kind::Fixed,
        items,
        source: emit::source(&panel),
    }
}

/// Generate every menu, numbered over IBM's full command list.
#[must_use]
pub fn generate(inputs: &Inputs) -> Generated {
    generate_for(inputs, None)
}

/// Generate every menu, numbered over the commands of one release
/// (71 or 75) when given; the goldens of pub400 are 75.
#[must_use]
pub fn generate_for(inputs: &Inputs, release: Option<u8>) -> Generated {
    let cmd_menus: Vec<&MenuEntry> = inputs
        .menus
        .iter()
        .filter(|m| m.name.len() > 3 && m.name.starts_with("CMD"))
        .collect();
    let classifier = Classifier::new(cmd_menus.iter().map(|m| m.name[3..].to_string()));
    let all = commands(inputs, release);
    let mut report = Report {
        inventory: cmd_menus.len(),
        inventory_71: cmd_menus
            .iter()
            .filter(|m| m.releases.contains(&71))
            .count(),
        commands: all.len(),
        ..Report::default()
    };
    let mut menus = Vec::new();
    let mut placed: BTreeSet<&str> = BTreeSet::new();
    let mut sorted: Vec<&MenuEntry> = cmd_menus.clone();
    sorted.sort_by(|a, b| cmp_names(&a.name, &b.name));
    for entry in sorted {
        if member::HAND_WRITTEN.contains(&entry.name.as_str()) {
            report.hand_written.push(entry.name.clone());
            continue;
        }
        let menu = command_menu(entry, &classifier, &all, &inputs.titles);
        if menu.items.is_empty() {
            report.empty.push(menu.name.clone());
        }
        report.placements += menu.items.len();
        report.shipped_placements += menu.items.iter().filter(|i| i.title.is_some()).count();
        for i in &menu.items {
            if let Some(c) = all.iter().find(|c| c.name == i.name) {
                placed.insert(c.name.as_str());
            }
        }
        menus.push(menu);
    }
    report.unplaced = all
        .iter()
        .filter(|c| !placed.contains(c.name.as_str()))
        .map(|c| c.name.clone())
        .collect();
    report.ours_only = inputs
        .titles
        .keys()
        .filter(|n| !all.iter().any(|c| &c.name == *n))
        .cloned()
        .collect();

    let major: Vec<(String, String, String)> = fixed::MAJOR
        .iter()
        .map(|(t, n, a)| ((*t).to_string(), (*n).to_string(), (*a).to_string()))
        .collect();
    menus.push(fixed_menu(
        "MAJOR",
        fixed::MAJOR_TITLE,
        true,
        &major,
        "The command menus by group. Option 1 finds a command by name; options 2 and 3 list the menus by verb and by subject; the others are the groups of commands by what they manage.",
    ));
    let listing = |pick: &dyn Fn(&Menu) -> bool| -> Vec<(String, String, String)> {
        menus
            .iter()
            .filter(|m| m.kind != Kind::Fixed && pick(m))
            .map(|m| (m.title.clone(), m.name.clone(), format!("MENU {}", m.name)))
            .collect()
    };
    let verb = listing(&|m| on_verb_menu(&m.name));
    let subject = listing(&|m| !on_verb_menu(&m.name));
    menus.push(fixed_menu(
        "VERB",
        fixed::VERB_TITLE,
        false,
        &verb,
        "The command menus by verb, one per leading word of a command name, in name order.",
    ));
    menus.push(fixed_menu(
        "SUBJECT",
        fixed::SUBJECT_TITLE,
        false,
        &subject,
        "The command menus by subject, one per object or topic a command name carries, in name order.",
    ));
    report.generated = menus.len();
    Generated { menus, report }
}

/// Write `NAME.pnlgrp` for every menu under `dir`, dropping sources of
/// menus no longer generated.
pub fn write(generated: &Generated, dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let keep: BTreeSet<String> = generated.menus.iter().map(Menu::file_name).collect();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path.file_name().map(|n| n.to_string_lossy().to_string());
        if path.extension().is_some_and(|x| x == "pnlgrp")
            && name.is_some_and(|n| !keep.contains(&n))
        {
            std::fs::remove_file(&path)?;
        }
    }
    for m in &generated.menus {
        std::fs::write(dir.join(m.file_name()), &m.source)?;
    }
    Ok(())
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "menus: {} CMDxxx in IBM's list ({} on 7.1), {} generated (3 fixed), {} hand-written later, {} empty",
            self.inventory,
            self.inventory_71,
            self.generated,
            self.hand_written.len(),
            self.empty.len()
        )?;
        writeln!(
            f,
            "commands: {} considered, {} options placed ({} shipped), {} in no menu, {} ours only",
            self.commands,
            self.placements,
            self.shipped_placements,
            self.unplaced.len(),
            self.ours_only.len()
        )?;
        if !self.hand_written.is_empty() {
            writeln!(f, "hand-written later: {}", self.hand_written.join(" "))?;
        }
        if !self.empty.is_empty() {
            writeln!(f, "empty: {}", self.empty.join(" "))?;
        }
        if !self.unplaced.is_empty() {
            writeln!(f, "in no menu: {}", self.unplaced.join(" "))?;
        }
        if !self.ours_only.is_empty() {
            writeln!(f, "ours only: {}", self.ours_only.join(" "))?;
        }
        Ok(())
    }
}
