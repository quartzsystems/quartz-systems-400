// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The corpus under `tests/pnlgrp/`: every group compiled, its listing
//! compared with `tests/expected/<name>.lst`, the clean groups checked
//! as models and round-tripped through the payload. `.IM` members live
//! in `tests/pnlgrp/imbed/`. `QS400_UPDATE_UIM=1` rewrites the expected
//! listings after a deliberate change.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use uim::model::{Action, Area, Block, DataNode, Depth, Key, MenuNode, MenuOption, Span};
use uim::payload::{self, Payload};
use uim::{Compilation, CompileOpts, Imbed, MapImports, NoImports, compile};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

/// Members from `tests/pnlgrp/imbed/<name>.pnlgrp`.
struct FileImbeds;

impl Imbed for FileImbeds {
    fn imbed(&self, name: &str) -> Option<String> {
        let path = root()
            .join("pnlgrp/imbed")
            .join(format!("{}.pnlgrp", name.to_ascii_lowercase()));
        std::fs::read_to_string(path).ok()
    }
}

const CLEAN: &[&str] = &[
    "data",
    "list",
    "menu",
    "help",
    "imports_a",
    "render-data",
    "render-help",
    "render-list",
    "render-menu",
];
const FAILING: &[&str] = &["errors", "imports_b", "nopnlgrp"];

/// Every group compiled, imports resolved from the other groups' exports.
fn compile_all() -> BTreeMap<String, Compilation> {
    let mut sources = BTreeMap::new();
    for entry in std::fs::read_dir(root().join("pnlgrp")).expect("corpus") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|e| e == "pnlgrp") {
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            sources.insert(stem, std::fs::read_to_string(&path).expect("read"));
        }
    }
    let mut imports = MapImports::new();
    for (stem, text) in &sources {
        let c = compile(text, &FileImbeds, &NoImports, &CompileOpts::default());
        if let Some(g) = &c.group {
            let name = stem.to_ascii_uppercase();
            imports.add(&name, g.exports());
            imports.add(&format!("QGPL/{name}"), g.exports());
        }
    }
    sources
        .into_iter()
        .map(|(stem, text)| {
            let name = stem.to_ascii_uppercase();
            let opts = CompileOpts {
                name: Some(name.clone()),
                library: Some("QGPL".into()),
                source: Some(format!("QGPL/QPNLSRC({name})")),
                timestamp: None,
            };
            (stem, compile(&text, &FileImbeds, &imports, &opts))
        })
        .collect()
}

#[test]
fn listings_match_expected() {
    let all = compile_all();
    assert_eq!(all.len(), CLEAN.len() + FAILING.len(), "corpus size");
    let dir = root().join("expected");
    std::fs::create_dir_all(&dir).unwrap();
    let update = std::env::var_os("QS400_UPDATE_UIM").is_some();
    let mut diffs = Vec::new();
    for (stem, c) in &all {
        let path = dir.join(format!("{stem}.lst"));
        if update {
            std::fs::write(&path, &c.listing).unwrap();
            continue;
        }
        let want = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e} (QS400_UPDATE_UIM=1 writes it)", path.display()))
            .replace("\r\n", "\n");
        for (i, (a, b)) in want.lines().zip(c.listing.lines()).enumerate() {
            if a != b {
                diffs.push(format!("{stem} line {}: expected {a:?}, got {b:?}", i + 1));
            }
        }
        if want.lines().count() != c.listing.lines().count() {
            diffs.push(format!(
                "{stem}: {} expected lines, {} produced",
                want.lines().count(),
                c.listing.lines().count()
            ));
        }
    }
    assert!(diffs.is_empty(), "\n{}", diffs.join("\n"));
}

#[test]
fn clean_groups_compile_and_failing_ones_do_not() {
    let all = compile_all();
    for stem in CLEAN {
        let c = &all[*stem];
        assert!(
            c.ok(),
            "{stem} should compile:\n{}",
            c.diagnostics
                .items()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert!(c.payload().is_some());
    }
    for stem in FAILING {
        let c = &all[*stem];
        assert!(!c.ok(), "{stem} should fail");
        assert!(c.payload().is_none());
    }
}

#[test]
fn payload_round_trips() {
    let all = compile_all();
    for stem in CLEAN {
        let c = &all[*stem];
        // The header holds ten characters of the name.
        let p = Payload {
            name: stem.to_ascii_uppercase().chars().take(10).collect(),
            library: "QGPL".into(),
            group: c.group.clone().unwrap(),
            source: c.source.lines().to_vec(),
        };
        let bytes = payload::encode(&p);
        let back = payload::decode(&bytes).unwrap();
        assert_eq!(back, p, "{stem}");
        assert_eq!(
            payload::decode(&c.payload().unwrap()).unwrap().group,
            p.group
        );
    }
}

#[test]
fn data_panel_model() {
    let all = compile_all();
    let g = all["data"].group.as_ref().unwrap();
    assert_eq!(g.dftmsgf.as_deref(), Some("QGPL/MYMSGF"));
    assert_eq!(
        g.copyright.as_deref(),
        Some("Copyright (C) 2026 Quartz Systems")
    );
    assert_eq!(g.classes.len(), 5);
    let yn = g.class("YNCL").unwrap();
    assert_eq!(yn.translations.len(), 2);
    assert_eq!(yn.translations[0].text, "*YES");
    assert_eq!(yn.checks[0].kind, "NOTBLANK");
    assert_eq!(g.variables.len(), 7);
    assert_eq!(g.var_records[0].noput, ["NOTE"]);
    assert_eq!(g.conditions.len(), 2);
    let keyl = g.key_list("DATAKEYS").unwrap();
    assert_eq!(keyl.keys.len(), 8);
    assert_eq!(keyl.keys[3].key, Key::F(5));
    assert_eq!(keyl.keys[3].action, Action::Return(5));
    assert_eq!(keyl.keys[3].cond.as_deref(), Some("ISACTIVE"));
    assert_eq!(keyl.keys[3].text, "F5=Refresh");
    let p = g.panel("CUSTDTA").unwrap();
    assert_eq!(Span::plain(&p.title), "Work with Customer");
    assert_eq!(p.enter, Some(Action::Return(1)));
    assert_eq!(p.panelid.as_deref(), Some("PNLID"));
    assert_eq!(p.areas.len(), 3);
    let Area::Data(d) = &p.areas[1] else {
        panic!("{:?}", p.areas[1])
    };
    assert_eq!(d.depth, Depth::Lines(12));
    assert_eq!(Span::plain(&d.title), "Customer details");
    assert_eq!(d.items.len(), 6);
    assert!(matches!(d.items[0], DataNode::Column(30)));
    let DataNode::Field(lib) = &d.items[2] else {
        panic!("{:?}", d.items[2])
    };
    assert_eq!(lib.var, "CUSTLIB");
    assert_eq!(lib.extenders.len(), 1);
    assert_eq!(lib.extenders[0].var, "QTY");
    assert_eq!(Span::plain(&lib.extenders[0].text), "Quantity");
    let DataNode::Group(grp) = &d.items[4] else {
        panic!("{:?}", d.items[4])
    };
    assert!(grp.compact);
    assert_eq!(grp.items.len(), 3);
    let DataNode::Select(sel) = &d.items[5] else {
        panic!("{:?}", d.items[5])
    };
    assert_eq!(sel.choices.len(), 2);
    assert_eq!(sel.choices[1].cond.as_deref(), Some("ISACTIVE"));
    let Area::BotInst(b) = &p.areas[2] else {
        panic!("{:?}", p.areas[2])
    };
    assert_eq!(
        *b,
        vec![
            Span::Text("Current library is ".into()),
            Span::Var("CUSTLIB".into()),
            Span::Text(".".into())
        ]
    );
    assert_eq!(g.helps.len(), 7);
}

#[test]
fn list_panel_model() {
    let all = compile_all();
    let g = all["list"].group.as_ref().unwrap();
    assert_eq!(g.truth_tables[0].rows.len(), 3);
    assert_eq!(g.condition("HASOBJS").unwrap().expr.lists(), ["OBJLIST"]);
    let p = g.panel("WRKOBJ").unwrap();
    let Area::List(l) = &p.areas[2] else {
        panic!("{:?}", p.areas[2])
    };
    assert_eq!(l.depth, Depth::Fill);
    assert_eq!(l.views, Some(2));
    assert_eq!(l.list_views.len(), 2);
    assert_eq!(l.actions.len(), 5);
    assert_eq!(l.actions[1].confirm.as_deref(), Some("CFMDLT"));
    assert_eq!(l.actions[1].usrexit, Some(Action::Call("PGMNAME".into())));
    assert_eq!(l.actions[4].option, None);
    assert_eq!(l.all_columns().len(), 7);
    assert_eq!(l.columns.len(), 6);
    assert_eq!(l.all_columns()[1].name, "NAME");
    assert_eq!(Span::plain(&l.all_columns()[6].heading), "Changed");
    assert_eq!(
        l.botinst.as_ref().map(|b| Span::plain(b)).as_deref(),
        Some("More...")
    );
    assert_eq!(g.panel("CFMDLT").unwrap().areas.len(), 2);
}

#[test]
fn menu_panel_model() {
    let all = compile_all();
    let g = all["menu"].group.as_ref().unwrap();
    let p = g.panel("MAIN").unwrap();
    let Area::Menu(menu) = &p.areas[0] else {
        panic!("{:?}", p.areas[0])
    };
    assert_eq!(menu.items.len(), 2);
    let items = menu.all_items();
    assert_eq!(items.len(), 7);
    assert_eq!(items[0].action, Action::Call("PGM".into()));
    assert_eq!(items[1].action, Action::Panel("OFFICE".into()));
    assert_eq!(items[2].action, Action::Menu("GENERAL".into()));
    assert_eq!(items[3].action, Action::Msg("MSGVAR".into()));
    assert_eq!(items[6].option, MenuOption::Star);
    assert_eq!(items[6].action, Action::CmdLine);
    assert!(
        matches!(&menu.items[1], MenuNode::Group { heading, .. } if Span::plain(heading) == "Operator tasks")
    );
    let cmd = p.cmdline().unwrap();
    assert_eq!(cmd.name.as_deref(), Some("CMD"));
    assert_eq!(Span::plain(&cmd.prompt), "Selection or command");
    let office = g.panel("OFFICE").unwrap();
    let Area::Info(i) = &office.areas[0] else {
        panic!("{:?}", office.areas[0])
    };
    assert_eq!(i.body.len(), 2);
    let Area::Text(t) = &office.areas[1] else {
        panic!("{:?}", office.areas[1])
    };
    assert_eq!(
        t.body,
        vec![Block::Para(vec![Span::Text(
            "Fixed text at the bottom of the panel.".into()
        )])]
    );
}

#[test]
fn help_document_model() {
    let all = compile_all();
    let g = all["help"].group.as_ref().unwrap();
    assert_eq!(g.synonyms.len(), 2);
    assert_eq!(g.synonyms[0].words, ["OBJECTS", "THING", "THINGS"]);
    assert_eq!(g.helps.len(), 3, "the imbedded module counts");
    assert!(g.help("HLPCOMMON").is_some());
    assert_eq!(
        Span::plain(&g.help("HLPMORE").unwrap().title),
        "More Information"
    );
    let h = g.help("HLPINTRO").unwrap();
    assert_eq!(h.topics.len(), 1);
    assert_eq!(h.topics[0].roots, ["OBJECT", "LIBRARY"]);
    let kinds: Vec<&str> = h
        .body
        .iter()
        .map(|b| match b {
            Block::Heading { .. } => "H",
            Block::Para(_) => "P",
            Block::ParaCont(_) => "PC",
            Block::Example(_) => "XMP",
            Block::Lines(_) => "LINES",
            Block::Figure(_) => "FIG",
            Block::List { .. } => "LIST",
            Block::DefList { .. } => "DL",
            Block::ParmList(_) => "PARML",
            Block::Note(_) => "NT",
            Block::ImHelp(_) => "IMHELP",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "H", "P", "PC", "H", "XMP", "H", "LIST", "LIST", "H", "DL", "PARML", "NT", "NT",
            "LINES", "FIG", "P", "IMHELP"
        ]
    );
    let Block::Para(first) = &h.body[1] else {
        panic!("{:?}", h.body[1])
    };
    let highlights = first.iter().filter(|s| matches!(s, Span::Hp(..))).count();
    assert_eq!(highlights, 10);
    assert!(first.iter().any(|s| matches!(s, Span::Rt(_))));
    assert!(first.iter().any(|s| matches!(s, Span::Pk(_))));
    assert!(first.iter().any(|s| matches!(s, Span::Pv(_))));
    let Block::Example(lines) = &h.body[4] else {
        panic!("{:?}", h.body[4])
    };
    assert_eq!(lines.len(), 3);
    assert!(lines[1].is_empty(), "the blank line is kept");
    assert_eq!(
        Span::plain(&lines[0]),
        "   CRTLIB LIB(MYLIB) TEXT('My library')"
    );
    let Block::List { items, .. } = &h.body[6] else {
        panic!("{:?}", h.body[6])
    };
    assert_eq!(items.len(), 2);
    assert!(matches!(items[1][1], Block::List { compact: true, .. }));
    let Block::DefList { heads, items, .. } = &h.body[9] else {
        panic!("{:?}", h.body[9])
    };
    assert!(heads.is_some());
    assert_eq!(items.len(), 2);
    assert_eq!(items[1].def.len(), 2);
    let Block::Para(last) = &h.body[15] else {
        panic!("{:?}", h.body[15])
    };
    assert!(
        last.iter()
            .any(|s| matches!(s, Span::Link { perform, .. } if perform == "DSPHELP HLPMORE"))
    );
    assert!(Span::plain(last).contains(
        "[like these], braces {and these}, a colon: a period. a comma, and an ampersand &."
    ));
}

#[test]
fn error_identifiers() {
    let all = compile_all();
    let ids: Vec<&str> = all["errors"]
        .diagnostics
        .items()
        .iter()
        .map(|d| d.id)
        .collect();
    for want in [
        "UIM0101", "UIM0106", "UIM0202", "UIM0203", "UIM0204", "UIM0205", "UIM0207", "UIM0208",
        "UIM0301", "UIM0302", "UIM0303", "UIM1001", "UIM1002", "UIM1003", "UIM1004", "UIM1005",
        "UIM1006", "UIM1007", "UIM1008", "UIM1009", "UIM1010", "UIM1011", "UIM1012", "UIM1013",
        "UIM1014", "UIM1015", "UIM1016", "UIM1017", "UIM1018", "UIM1019", "UIM2001", "UIM2002",
        "UIM2003", "UIM2004", "UIM2005", "UIM2006", "UIM2007", "UIM2010", "UIM3001", "UIM3002",
        "UIM3003", "UIM3004",
    ] {
        assert!(ids.contains(&want), "{want} missing from {ids:?}");
    }
    let b: Vec<&str> = all["imports_b"]
        .diagnostics
        .items()
        .iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(b, ["UIM2011", "UIM2006"]);
    let n: Vec<&str> = all["nopnlgrp"]
        .diagnostics
        .items()
        .iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(n, ["UIM0206", "UIM0204"]);
}
