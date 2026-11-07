// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every option of every shipped menu leads somewhere: a menu that is
//! shipped, a command with a definition in `command/sys-cpps/cmds`, or
//! one `menus::unavailable` lists, which answers that it is not in this
//! release. A command menu's reserved option (no text) answers the same.

use std::collections::BTreeSet;
use std::path::Path;

use menus::unavailable;
use uim::model::{Action, Area};
use uim::{CompileOpts, NoImbeds, NoImports};

fn defined() -> BTreeSet<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../command/sys-cpps/cmds");
    std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter_map(|e| {
            let p = e.path();
            (p.extension()? == "cmd")
                .then(|| p.file_stem()?.to_str().map(str::to_ascii_uppercase))?
        })
        .collect()
}

fn shipped() -> BTreeSet<String> {
    menus::shipped::FILES
        .iter()
        .map(|(n, _)| n.to_ascii_uppercase())
        .collect()
}

/// Every option's target: (menu, option, action, has text).
fn options() -> Vec<(String, String, Action, bool)> {
    let mut out = Vec::new();
    for (name, text) in menus::shipped::FILES {
        let c = uim::compile(text, &NoImbeds, &NoImports, &CompileOpts::default());
        let g = c.group.unwrap_or_else(|| panic!("{name} does not compile"));
        for p in &g.panels {
            for a in &p.areas {
                let Area::Menu(m) = a else { continue };
                for i in m.all_items() {
                    out.push((
                        (*name).to_string(),
                        format!("{:?}", i.option),
                        i.action.clone(),
                        !i.text.is_empty(),
                    ));
                }
            }
        }
    }
    out
}

#[test]
fn every_option_leads_somewhere() {
    let cmds = defined();
    let menus_shipped = shipped();
    let mut bad = Vec::new();
    for (menu, opt, action, text) in options() {
        match &action {
            Action::Menu(m) => {
                let m = m.to_ascii_uppercase();
                if !menus_shipped.contains(&m) && unavailable::menu(&m).is_none() {
                    bad.push(format!("{menu} {opt}: menu {m} is not shipped"));
                }
            }
            Action::Cmd(c) => {
                let name = unavailable::command_name(c);
                // A reserved option names a command left out by design.
                if !text || cmds.contains(&name) || unavailable::command(c).is_some() {
                    continue;
                }
                bad.push(format!("{menu} {opt}: command {name} has no definition"));
            }
            _ => {}
        }
    }
    assert!(
        bad.is_empty(),
        "{} options lead nowhere:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// The lists hold nothing a later change shipped, nothing no option
/// names, and every name is in `docs/MENUS.md`'s table.
#[test]
fn the_unavailable_lists_stay_true() {
    let cmds = defined();
    let menus_shipped = shipped();
    let opts = options();
    let doc =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/MENUS.md"))
            .expect("docs/MENUS.md");
    let mut bad = Vec::new();
    for m in unavailable::MENUS {
        if menus_shipped.contains(*m) {
            bad.push(format!("menu {m} is shipped now"));
        }
        if !opts
            .iter()
            .any(|(_, _, a, _)| matches!(a, Action::Menu(x) if x.eq_ignore_ascii_case(m)))
        {
            bad.push(format!("no option names menu {m}"));
        }
        if !doc.contains(&format!("| `{m}` |")) {
            bad.push(format!("menu {m} is not in the table of docs/MENUS.md"));
        }
    }
    for c in unavailable::COMMANDS {
        if cmds.contains(*c) {
            bad.push(format!("command {c} has a definition now"));
        }
        if !opts
            .iter()
            .any(|(_, _, a, _)| matches!(a, Action::Cmd(x) if unavailable::command_name(x) == *c))
        {
            bad.push(format!("no option names command {c}"));
        }
        if !doc.contains(&format!("| `{c}` |")) {
            bad.push(format!("command {c} is not in the table of docs/MENUS.md"));
        }
    }
    assert!(bad.is_empty(), "\n{}", bad.join("\n"));
}

/// The major command menu's curated groups against the pub400 captures
/// of every page: the same option numbers lead to the same commands and
/// menus (the rows' texts are ours or blank, as in the generated menus).
#[test]
fn curated_groups_follow_the_captures() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/golden/addendum/menus");
    let groups = [
        "CMDOBJMGT",
        "CMDFILE",
        "CMDSAVRST",
        "CMDWRKMGT",
        "CMDDTAMGT",
        "CMDSYSCTL",
        "CMDCMNMGT",
        "CMDDSTSRV",
        "CMDPRBMGT",
        "CMDMSGHDL",
    ];
    let mut bad = Vec::new();
    for g in groups {
        let mut theirs = std::collections::BTreeMap::new();
        for page in ["", "-2", "-3", "-4", "-5"] {
            let Ok(text) = std::fs::read_to_string(
                dir.join(format!("menu-{}{page}.txt", g.to_ascii_lowercase())),
            ) else {
                continue;
            };
            for row in text.lines().skip(3).take(17) {
                let words: Vec<&str> = row.split_whitespace().collect();
                if let (Some(n), Some(name)) = (
                    words.first().and_then(|w| w.strip_suffix('.')),
                    words.last(),
                ) && let Ok(n) = n.parse::<u16>()
                    && words.len() > 2
                {
                    theirs.insert(n, (*name).to_string());
                }
            }
        }
        let source = menus::shipped::source(g).unwrap_or_else(|| panic!("{g} is not shipped"));
        let c = uim::compile(source, &NoImbeds, &NoImports, &CompileOpts::default());
        let group = c.group.expect("group");
        let mut ours = std::collections::BTreeMap::new();
        for a in &group.panel(g).expect("panel").areas {
            let Area::Menu(m) = a else { continue };
            for i in m.all_items() {
                let uim::model::MenuOption::Number(n) = i.option else {
                    continue;
                };
                let name = match &i.action {
                    Action::Menu(x) => x.clone(),
                    Action::Cmd(x) => unavailable::command_name(x),
                    _ => continue,
                };
                ours.insert(n, name);
            }
        }
        if theirs.is_empty() || theirs != ours {
            bad.push(format!("{g}:\n  ours   {ours:?}\n  theirs {theirs:?}"));
        }
    }
    assert!(bad.is_empty(), "\n{}", bad.join("\n"));
}
