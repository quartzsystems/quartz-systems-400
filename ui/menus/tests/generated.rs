// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The committed `pnlgrp/` is what the generator writes, every source
//! compiles, the numbering of four menus and the three fixed menus
//! follow the pub400 goldens, and the inventory is covered.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use menus::generate::{Generated, Inputs, Kind, generate, member, write};
use uim::{CompileOpts, NoImbeds, NoImports};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn inputs() -> &'static Inputs {
    static INPUTS: OnceLock<Inputs> = OnceLock::new();
    INPUTS.get_or_init(|| Inputs::read(&root()).expect("the inventories and the definitions"))
}

fn generated() -> &'static Generated {
    static G: OnceLock<Generated> = OnceLock::new();
    G.get_or_init(|| generate(inputs()))
}

fn sources(dir: &Path) -> BTreeMap<String, String> {
    std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "pnlgrp"))
        .map(|p| {
            let name = p.file_name().expect("name").to_string_lossy().to_string();
            let text = std::fs::read_to_string(&p)
                .expect("read")
                .replace("\r\n", "\n");
            (name, text)
        })
        .collect()
}

#[test]
fn committed_sources_are_what_the_generator_writes() {
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("pnlgrp");
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp).expect("clear");
    }
    write(generated(), &tmp).expect("write");
    let fresh = sources(&tmp);
    let committed = sources(&root().join("ui/menus/pnlgrp"));
    let fresh_names: BTreeSet<&String> = fresh.keys().collect();
    let committed_names: BTreeSet<&String> = committed.keys().collect();
    assert_eq!(
        fresh_names, committed_names,
        "the menu set changed: run cargo run -p menus --bin genmenus"
    );
    for (name, text) in &fresh {
        assert!(
            committed[name] == *text,
            "{name} differs: run cargo run -p menus --bin genmenus"
        );
    }
}

#[test]
fn every_source_compiles() {
    for m in &generated().menus {
        let opts = CompileOpts {
            name: Some(m.name.clone()),
            library: Some("QSYS".into()),
            ..CompileOpts::default()
        };
        let c = uim::compile(&m.source, &NoImbeds, &NoImports, &opts);
        assert!(c.ok(), "{}: {:?}", m.name, c.diagnostics.into_vec());
        let group = c.group.as_ref().expect("a group");
        assert!(
            group.panels.iter().any(|p| p.name == m.name),
            "{}: no panel of that name",
            m.name
        );
        assert!(c.payload().is_some());
    }
}

/// `(option, NAME)` of every `  n. text  NAME` row of a golden.
fn golden_rows(file: &str) -> Vec<(u16, String)> {
    let path = root()
        .join("tools/conformance/golden/verified/pub400")
        .join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .filter_map(|line| {
            let line = line.trim_end_matches('\r').trim_end();
            let (n, rest) = line.trim_start().split_once(". ")?;
            let n: u16 = n.parse().ok()?;
            let (_, name) = rest.rsplit_once("  ")?;
            let name = name.trim();
            (!name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
            .then(|| (n, name.to_string()))
        })
        .collect()
}

fn golden_menu(files: &[&str]) -> BTreeMap<String, u16> {
    files
        .iter()
        .flat_map(|f| golden_rows(f))
        .map(|(n, name)| (name, n))
        .collect()
}

fn ours(g: &Generated, menu: &str) -> BTreeMap<String, u16> {
    g.menus
        .iter()
        .find(|m| m.name == menu)
        .unwrap_or_else(|| panic!("{menu} generated"))
        .items
        .iter()
        .map(|i| (i.name.clone(), i.option))
        .collect()
}

fn by_option(map: &BTreeMap<String, u16>, keep: &BTreeSet<&String>) -> Vec<String> {
    let mut v: Vec<(&u16, &String)> = map
        .iter()
        .filter(|(n, _)| keep.contains(n))
        .map(|(n, o)| (o, n))
        .collect();
    v.sort();
    v.into_iter().map(|(_, n)| n.clone()).collect()
}

/// A command menu's expectations against its goldens: the names IBM
/// lists that the rule does not, the names IBM lists out of name
/// order, and how many leading options are numbered as IBM's before a
/// command one list lacks shifts the rest (the 7.1 oracle carries
/// proxies pub400 has not, pub400 hides commands its user may not
/// see).
struct Case {
    menu: &'static str,
    files: &'static [&'static str],
    ibm_only: &'static [&'static str],
    misplaced: &'static [&'static str],
    exact_prefix: u16,
}

const CASES: &[Case] = &[
    Case {
        menu: "CMDADD",
        files: &["menu-cmdadd.txt", "menu-cmdadd-2.txt"],
        ibm_only: &[],
        misplaced: &[],
        exact_prefix: 8,
    },
    Case {
        menu: "CMDPF",
        files: &["menu-cmdpf.txt", "menu-cmdpf-2.txt"],
        ibm_only: &[],
        misplaced: &[],
        exact_prefix: 4,
    },
    Case {
        menu: "CMDWRK",
        files: &["cmdwrk-menu.txt", "cmdwrk-menu-page2.txt"],
        // IBM lists NETSTAT first, under its title Work with TCP/IP
        // Network Sts, so every number is one higher there; WRKARMJOB
        // comes before WRKAPPNSTS.
        ibm_only: &["NETSTAT"],
        misplaced: &["WRKARMJOB"],
        exact_prefix: 0,
    },
    Case {
        menu: "CMDJOB",
        files: &["menu-cmdjob.txt", "menu-cmdjob-2.txt"],
        // IBM's Job menu is curated: autostart entries, group
        // attributes, Data, Delete Trace and a watcher definition;
        // DSPJVMJOB comes before DSPJOB.
        ibm_only: &["ADDAJE", "ADDJWDFN", "CHGAJE", "CHGGRPA", "DATA", "DLTTRC"],
        misplaced: &["DSPJVMJOB"],
        exact_prefix: 0,
    },
];

#[test]
fn numbering_follows_the_goldens() {
    for case in CASES {
        let gold = golden_menu(case.files);
        let mine = ours(generated(), case.menu);
        let common: BTreeSet<&String> = gold.keys().filter(|n| mine.contains_key(*n)).collect();
        let ibm_only: BTreeSet<&str> = gold
            .keys()
            .filter(|n| !mine.contains_key(*n))
            .map(String::as_str)
            .collect();
        let expected_only: BTreeSet<&str> = case.ibm_only.iter().copied().collect();
        assert_eq!(
            ibm_only, expected_only,
            "{}: names IBM lists that the rule does not",
            case.menu
        );

        // The common names come in the same order, the ones IBM
        // misplaces aside.
        let ordered: BTreeSet<&String> = common
            .iter()
            .filter(|n| !case.misplaced.contains(&n.as_str()))
            .copied()
            .collect();
        for m in case.misplaced {
            assert!(
                common.iter().any(|n| n == m),
                "{}: {m} is not common",
                case.menu
            );
        }
        let gold_order = by_option(&gold, &ordered);
        let mine_order = by_option(&mine, &ordered);
        assert_eq!(
            gold_order, mine_order,
            "{}: order of the common names",
            case.menu
        );

        let mismatched: Vec<String> = common
            .iter()
            .filter(|n| gold[**n] != mine[**n])
            .map(|n| format!("{n} IBM {} ours {}", gold[*n], mine[*n]))
            .collect();
        println!(
            "{}: {} golden rows, {} common, {} numbered as IBM; {} IBM-only",
            case.menu,
            gold.len(),
            common.len(),
            common.len() - mismatched.len(),
            ibm_only.len()
        );
        for m in &mismatched {
            println!("  {m}");
        }
        for n in common.iter().filter(|n| gold[**n] <= case.exact_prefix) {
            assert_eq!(gold[*n], mine[*n], "{}: option of {n}", case.menu);
        }
    }
}

#[test]
fn fixed_menus_follow_the_goldens() {
    let g = generated();
    let major = golden_menu(&["menu-major.txt", "menu-major-2.txt"]);
    assert_eq!(major, ours(g, "MAJOR"));
    let verb = golden_menu(&["menu-verb.txt", "menu-verb-2.txt"]);
    let mine = ours(g, "VERB");
    for (name, n) in &verb {
        assert_eq!(mine.get(name), Some(n), "VERB: {name}");
    }
    let subject = golden_menu(&["menu-subject.txt", "menu-subject-2.txt"]);
    let mine = ours(g, "SUBJECT");
    let mut off = Vec::new();
    for (name, n) in &subject {
        if mine.get(name) != Some(n) {
            off.push(format!("{name} IBM {n} ours {:?}", mine.get(name)));
        }
    }
    println!(
        "SUBJECT: {} golden rows, {} numbered differently",
        subject.len(),
        off.len()
    );
    for o in &off {
        println!("  {o}");
    }
    assert!(
        subject.keys().all(|n| mine.contains_key(n)),
        "SUBJECT lists every golden menu"
    );
}

#[test]
fn inventory_is_covered() {
    let committed: BTreeSet<String> = sources(&root().join("ui/menus/pnlgrp"))
        .keys()
        .map(|n| n.trim_end_matches(".pnlgrp").to_string())
        .collect();
    let inventory: BTreeSet<&str> = inputs().menus.iter().map(|m| m.name.as_str()).collect();
    for m in &inputs().menus {
        let is_cmd = m.name.len() > 3 && m.name.starts_with("CMD");
        if !is_cmd || !m.releases.contains(&71) || member::HAND_WRITTEN.contains(&m.name.as_str()) {
            continue;
        }
        assert!(
            committed.contains(&m.name),
            "{} has no generated source",
            m.name
        );
    }
    for name in &committed {
        assert!(
            ["MAJOR", "VERB", "SUBJECT"].contains(&name.as_str())
                || inventory.contains(name.as_str()),
            "{name} is not in the inventory"
        );
    }
    let hand: Vec<&&str> = member::HAND_WRITTEN
        .iter()
        .filter(|h| inventory.contains(**h))
        .collect();
    assert_eq!(hand.len(), generated().report.hand_written.len());
}

#[test]
fn items_are_sorted_and_numbered() {
    for m in &generated().menus {
        for (i, item) in m.items.iter().enumerate() {
            assert_eq!(
                usize::from(item.option),
                i + 1,
                "{}: option {}",
                m.name,
                item.name
            );
        }
        if m.kind != Kind::Fixed {
            for w in m.items.windows(2) {
                assert!(
                    ccsid::ebcdic_cmp_str(&w[0].name, &w[1].name).is_lt(),
                    "{}: {} before {}",
                    m.name,
                    w[0].name,
                    w[1].name
                );
            }
        }
    }
    let r = &generated().report;
    println!("{r}");
    assert!(r.shipped_placements > 500);
}
