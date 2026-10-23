// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

#![allow(clippy::many_single_char_names)] // dialogs, terminals and rows

use std::collections::VecDeque;

use super::*;
use crate::render::{Attr, Target};

const MENU: &str = "\
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
:MENUI OPTION=3 ACTION='HELP HLPTHREE'.Read about three
:EMENU.
:CMDLINE SIZE=LONG.Selection or command
:EPANEL.
:HELP NAME=HLP.Help
:P.Some help.
:EHELP.
:HELP NAME=HLPTHREE.Three
:P.About three.
:EHELP.
:EPNLGRP.
";

const LIST: &str = "\
:PNLGRP.
:COPYR.(C) TEST 2026.
:CLASS NAME=OPTCL BASETYPE=ACTION.
:ECLASS.
:CLASS NAME=NAMECL BASETYPE='CHAR 10'.
:ECLASS.
:CLASS NAME=TEXTCL BASETYPE='CHAR 30'.
:ECLASS.
:VAR NAME=OPT CLASS=OPTCL.
:VAR NAME=OBJ CLASS=NAMECL.
:VAR NAME=TYPE CLASS=NAMECL.
:VAR NAME=TEXT CLASS=TEXTCL.
:VAR NAME=POSTO CLASS=NAMECL.
:VAR NAME=NEWNAME CLASS=NAMECL.
:VAR NAME=POSTYPE CLASS=NAMECL.
:VARRCD NAME=POSREC VARS='POSTO POSTYPE'.
:LISTDEF NAME=OBJS VARS='OPT OBJ TYPE TEXT'.
:KEYL NAME=KEYS HELP=HKEYS.
:KEYI KEY=ENTER ACTION=ENTER.
:KEYI KEY=HELP ACTION=HELP.
:KEYI KEY=PAGEUP ACTION=PAGEUP.
:KEYI KEY=PAGEDOWN ACTION=PAGEDOWN.
:KEYI KEY=F1 ACTION=HELP.
:KEYI KEY=F2 ACTION=EXTHELP.
:KEYI KEY=F3 ACTION=EXIT.F3=Exit
:KEYI KEY=F6 ACTION=PRINT.F6=Print
:KEYI KEY=F12 ACTION=CANCEL.F12=Cancel
:KEYI KEY=F14 ACTION=ABOUT.F14=About
:KEYI KEY=F16 ACTION=TOP.F16=Top
:KEYI KEY=F17 ACTION='PANEL POS'.F17=Position to
:KEYI KEY=F18 ACTION=BOTTOM.F18=Bottom
:EKEYL.
:KEYL NAME=POSKEYS.
:KEYI KEY=ENTER ACTION=ENTER.
:KEYI KEY=F12 ACTION=CANCEL.F12=Cancel
:EKEYL.
:PANEL NAME=WRK KEYL=KEYS HELP=HWRK ENTER='RETURN 1'.Work with Things
:DATA DEPTH=1.
:DATAI VAR=TYPE USAGE=INPUT.Subset by type
:EDATA.
:TOPINST.Type options, press Enter.
:LIST DEPTH='*' LISTDEF=OBJS PARMS=POSREC HELP=HOPTS.
:LISTACT OPTION=4 ENTER='RETURN 4' CONFIRM=CNF HELP=HOPTS.4=Delete
:LISTACT OPTION=5 ENTER='RETURN 5' HELP=HOPTS.5=Display
:LISTCOL VAR=OPT USAGE=INPUT.Opt
:LISTCOL VAR=OBJ USAGE=OUTPUT HELP=HOBJ.Object
:LISTCOL VAR=TYPE USAGE=OUTPUT.Type
:LISTCOL VAR=TEXT USAGE=OUTPUT.Text
:ELIST.
:EPANEL.
:PANEL NAME=CNF KEYL=KEYS HELP=HWRK ENTER='RETURN 1'.Confirm Delete of Things
:TOPINST.Press Enter to delete them, F12 to change your choices.
:LIST DEPTH='*' LISTDEF=OBJS.
:LISTACT OPTION=4 ENTER='RETURN 4'.4=Delete
:LISTCOL VAR=OPT USAGE=OUTPUT.Opt
:LISTCOL VAR=OBJ USAGE=OUTPUT.Object
:ELIST.
:EPANEL.
:PANEL NAME=POS KEYL=POSKEYS DEPTH=10 WIDTH=74 ENTER='RETURN 1'.Position the List
:DATA DEPTH='*'.
:DATACOL WIDTH=26.
:DATACOL WIDTH=72.
:DATAI VAR=POSTO USAGE=INPUT.Position to
:DATAGRP GRPSEP=NONE.
:DATAI VAR=POSTYPE USAGE=INPUT.Type
:EDATAGRP.
:EDATA.
:EPANEL.
:PANEL NAME=POP KEYL=KEYS DEPTH=8 WIDTH=40 ENTER='RETURN 1'.Rename
:DATA DEPTH='*'.
:DATAI VAR=NEWNAME USAGE=INPUT.New name
:EDATA.
:EPANEL.
:HELP NAME=HWRK.Work with Things
:P.The things.
:EHELP.
:HELP NAME=HOPTS.Options
:P.Type an option. See :LINK PERFORM='DSPHELP HOBJ'.objects:ELINK. too.
:EHELP.
:HELP NAME=HOBJ.Object
:P.The object's name.
:EHELP.
:HELP NAME=HKEYS.Function Keys
:P.The keys.
:EHELP.
:EPNLGRP.
";

fn compile(text: &str) -> Rc<PanelGroup> {
    let c = crate::compile(
        text,
        &crate::NoImbeds,
        &crate::NoImports,
        &crate::CompileOpts::default(),
    );
    Rc::new(c.group.unwrap_or_else(|| panic!("{}", c.listing)))
}

/// A key, the cursor, and values typed by field position.
type Step = (Key, (usize, usize), Vec<((usize, usize), String)>);

/// A terminal that answers from a script and keeps what it showed.
#[derive(Default)]
struct Script {
    /// Keys with the cursor and typed values by field position.
    steps: VecDeque<Step>,
    shown: Vec<Screen>,
    printed: Vec<Vec<String>>,
    /// A 27 by 132 display.
    wide: bool,
}

impl Script {
    fn step(
        &mut self,
        key: Key,
        cursor: (usize, usize),
        typed: &[((usize, usize), &str)],
    ) -> &mut Self {
        self.steps.push_back((
            key,
            cursor,
            typed.iter().map(|(p, v)| (*p, (*v).to_string())).collect(),
        ));
        self
    }

    fn rows(&self, i: usize) -> Vec<String> {
        self.shown[i].text_rows()
    }
}

impl Terminal for Script {
    fn wide(&mut self) -> bool {
        self.wide
    }

    fn exchange(&mut self, screen: &Screen) -> Option<Input> {
        self.shown.push(screen.clone());
        let (key, cursor, typed) = self.steps.pop_front()?;
        let values = screen
            .fields
            .iter()
            .map(|f| {
                typed
                    .iter()
                    .find(|(p, _)| *p == (f.row, f.col))
                    .map_or_else(|| f.value.clone(), |(_, v)| v.clone())
            })
            .collect();
        Some(Input {
            key: Some(key),
            cursor,
            values,
        })
    }

    fn print(&mut self, _title: &str, rows: &[String]) -> Result<String, String> {
        self.printed.push(rows.to_vec());
        Ok("Printed.".into())
    }
}

fn things(e: &mut Engine, n: usize) {
    for i in 0..n {
        e.add_row(
            "OBJS",
            vec![
                ("OPT".into(), String::new()),
                ("OBJ".into(), format!("OBJ{i:02}")),
                (
                    "TYPE".into(),
                    if i % 2 == 0 { "*FILE" } else { "*PGM" }.into(),
                ),
                ("TEXT".into(), format!("thing {i}")),
            ],
        );
    }
    e.list_mut("OBJS").complete = true;
}

/// The first option field's position on a screen.
fn option_at(s: &Screen, n: usize) -> (usize, usize) {
    let f = s
        .fields
        .iter()
        .filter(|f| matches!(f.target, Target::ListOption { .. }))
        .nth(n)
        .expect("an option field");
    (f.row, f.col)
}

#[test]
fn substitution() {
    let mut d = Engine::new(compile(MENU), "QS400");
    d.set("JOB", "QPADEV0001");
    assert_eq!(
        d.substitute("DSPJOB JOB(&JOB)", None),
        "DSPJOB JOB(QPADEV0001)"
    );
    assert_eq!(d.substitute("A && B &NOPE.", None), "A & B ");
    d.add_row("L", vec![("JOB".into(), "OTHER".into())]);
    assert_eq!(d.substitute("&JOB.X", Some(("L", 0))), "OTHERX");
    assert_eq!(d.get("PANELID"), "TEST");
}

#[test]
fn menu_choice_and_help_action() {
    let g = compile(MENU);
    let mut d = Engine::new(Rc::clone(&g), "QS400");
    let p = g.panel("TEST").unwrap();
    let mut t = Script::default();
    assert_eq!(
        d.menu_enter(&mut t, p, "1"),
        Some(Response::Menu("USER".into()))
    );
    assert_eq!(
        d.menu_enter(&mut t, p, "2"),
        Some(Response::Command("DSPJOB JOB(&JOB)".into()))
    );
    assert_eq!(
        d.menu_enter(&mut t, p, "wrkactjob"),
        Some(Response::Command("wrkactjob".into()))
    );
    assert_eq!(d.menu_enter(&mut t, p, "7"), None);
    assert!(d.message.is_some());
    // Option 3 shows its help module and the menu stays.
    let mut t = Script::default();
    t.step(Key::Enter, (21, 7), &[((21, 7), "3")])
        .step(Key::F(12), (1, 1), &[])
        .step(Key::F(3), (1, 1), &[]);
    assert_eq!(d.show(&mut t, "TEST").unwrap(), Response::Exit);
    assert!(
        t.rows(1).iter().any(|r| r.contains("Three - Help")),
        "{:?}",
        t.rows(1)
    );
    assert!(t.rows(2)[0].contains("A Test Menu"));
}

#[test]
fn options_not_valid_show_in_reverse() {
    let g = compile(LIST);
    let mut d = Engine::new(g, "QS400");
    things(&mut d, 3);
    let mut t = Script::default();
    let first = crate::render(&d.group, "WRK", &d, &Options::default()).unwrap();
    let at = option_at(&first, 1);
    t.step(Key::Enter, at, &[(at, "9")])
        .step(Key::F(3), (1, 1), &[]);
    assert_eq!(d.show(&mut t, "WRK").unwrap(), Response::Exit);
    let again = &t.shown[1];
    let f = again.fields.iter().find(|f| (f.row, f.col) == at).unwrap();
    assert_eq!(f.attr, Attr::Reverse);
    assert_eq!(f.value, "9");
    assert!(again.text_rows()[23].contains(super::show::OPTION_NOT_VALID));
}

#[test]
fn confirmation_panel() {
    let g = compile(LIST);
    let mut d = Engine::new(g, "QS400");
    things(&mut d, 4);
    let first = crate::render(&d.group, "WRK", &d, &Options::default()).unwrap();
    let a = option_at(&first, 1);
    let b = option_at(&first, 2);
    let mut t = Script::default();
    // Enter, F12 on the confirmation, Enter again, Enter to confirm.
    t.step(Key::Enter, a, &[(a, "4"), (b, "5")])
        .step(Key::F(12), (1, 1), &[])
        .step(Key::Enter, a, &[])
        .step(Key::Enter, (1, 1), &[]);
    let r = d.show(&mut t, "WRK").unwrap();
    let Response::Options(c) = r else {
        panic!("{r:?}");
    };
    assert_eq!(c.len(), 2);
    assert_eq!((c[0].row, c[0].option.as_str()), (1, "4"));
    let cnf = t.rows(1);
    assert!(cnf[0].contains("Confirm Delete of Things"));
    assert_eq!(
        cnf.iter().filter(|r| r.contains("OBJ0")).count(),
        1,
        "{cnf:?}"
    );
    // F12 went back with both options still typed.
    let back = &t.shown[2];
    assert!(back.fields.iter().any(|f| f.value == "4"));
    assert!(back.fields.iter().any(|f| f.value == "5"));
}

#[test]
fn position_to_and_subset() {
    let g = compile(LIST);
    let mut d = Engine::new(g, "QS400");
    things(&mut d, 30);
    let first = crate::render(&d.group, "WRK", &d, &Options::default()).unwrap();
    let subset = first
        .fields
        .iter()
        .find(|f| f.target == Target::Var("TYPE".into()))
        .map(|f| (f.row, f.col))
        .unwrap();
    let mut t = Script::default();
    // F17 shows the window; Enter there positions the list. The
    // subset is typed with Enter, which returns, and holds when shown.
    t.step(Key::F(17), (1, 1), &[])
        .step(Key::Enter, (5, 32), &[((5, 32), "OBJ2"), ((6, 34), "*PGM")])
        .step(Key::Enter, subset, &[(subset, "*PGM")]);
    assert_eq!(d.show(&mut t, "WRK").unwrap(), Response::Return(1));
    let win = t.rows(1);
    assert!(win[2].contains("Position the List"), "{win:?}");
    assert!(win[1].starts_with(" ....."));
    assert!(win[4].contains("Position to  . . . . . ."), "{win:?}");
    let rows = t.rows(2);
    let entry = rows.iter().position(|r| r.contains("OBJ")).unwrap();
    assert!(rows[entry].contains("OBJ21"), "{rows:?}");
    t.step(Key::F(18), (1, 1), &[])
        .step(Key::F(16), (1, 1), &[])
        .step(Key::F(3), (1, 1), &[]);
    d.show(&mut t, "WRK").unwrap();
    // The subset shows programs only.
    let rows = t.rows(3);
    assert!(
        rows.iter()
            .filter(|r| r.contains("OBJ"))
            .all(|r| r.contains("*PGM"))
    );
    // F18 shows the last page, F16 (top here) the first.
    assert!(t.rows(4).iter().any(|r| r.contains("OBJ29")));
    assert!(t.rows(5).iter().any(|r| r.contains("OBJ01")));
}

#[test]
fn field_help_extended_help_and_links() {
    let g = compile(LIST);
    let mut d = Engine::new(g, "QS400");
    d.full_screen_help = true;
    things(&mut d, 3);
    let first = crate::render(&d.group, "WRK", &d, &Options::default()).unwrap();
    let obj = first
        .texts
        .iter()
        .find(|t| t.text == "OBJ00")
        .map(|t| (t.row, t.col))
        .unwrap();
    let opt = option_at(&first, 0);
    let mut t = Script::default();
    t.step(Key::F(1), obj, &[])
        .step(Key::F(12), (1, 1), &[])
        // F1 on an option: the options' help; Enter on its link.
        .step(Key::F(1), opt, &[])
        .step(Key::Enter, (0, 0), &[]);
    // The link's position is known only once shown: fill it in below.
    let mut d2 = Engine::new(Rc::clone(&d.group), "QS400");
    d2.full_screen_help = true;
    things(&mut d2, 3);
    let _ = d.show(&mut t, "WRK");
    assert!(t.rows(1)[0].contains("Object - Help"), "{:?}", t.rows(1));
    assert!(t.rows(1)[21].contains("F2=Extended help"));
    let opts_help = &t.shown[3];
    assert!(opts_help.text_rows()[0].contains("Options - Help"));
    let link = opts_help.links.first().expect("a link").clone();
    assert_eq!(link.module, "HOBJ");
    let mut t = Script::default();
    t.step(Key::F(1), opt, &[])
        .step(Key::Enter, (link.row, link.col), &[])
        .step(Key::F(12), (1, 1), &[])
        .step(Key::F(2), (1, 1), &[])
        .step(Key::PageDown, (1, 1), &[])
        .step(Key::F(3), (1, 1), &[])
        .step(Key::F(3), (1, 1), &[]);
    assert_eq!(d2.show(&mut t, "WRK").unwrap(), Response::Exit);
    assert!(t.rows(2)[0].contains("Object - Help"));
    assert!(t.rows(3)[0].contains("Options - Help"));
    let ext = t.rows(4);
    assert!(ext[0].contains("Work with Things - Help"), "{ext:?}");
    assert!(ext.iter().any(|r| r.trim() == "Options"));
    assert!(!ext[21].contains("F2="));
}

#[test]
fn help_window_and_keys_help() {
    let g = compile(LIST);
    let mut d = Engine::new(g, "QS400");
    things(&mut d, 3);
    let mut t = Script::default();
    t.step(Key::F(1), (23, 3), &[])
        .step(Key::F(24), (1, 1), &[])
        .step(Key::F(20), (1, 1), &[])
        .step(Key::F(3), (1, 1), &[])
        .step(Key::F(3), (1, 1), &[]);
    let _ = d.show(&mut t, "WRK");
    let win = t.rows(1);
    assert!(
        win.iter().any(|r| r.contains("Function Keys - Help")),
        "{win:?}"
    );
    assert!(win.iter().any(|r| r.contains("F20=Enlarge")));
    assert!(win.iter().any(|r| r.contains("....")));
    assert!(t.rows(2).iter().any(|r| r.contains("F14=Print help")));
    // F20 makes it full screen.
    assert!(t.rows(3)[0].contains("Function Keys - Help"));
}

#[test]
fn print_about_and_popups() {
    let g = compile(LIST);
    let mut d = Engine::new(g, "QS400");
    things(&mut d, 2);
    let mut t = Script::default();
    t.step(Key::F(6), (1, 1), &[])
        .step(Key::F(14), (5, 5), &[])
        .step(Key::F(12), (1, 1), &[])
        .step(Key::F(3), (1, 1), &[]);
    assert_eq!(d.show(&mut t, "WRK").unwrap(), Response::Exit);
    assert_eq!(t.printed.len(), 1);
    assert!(t.printed[0][0].contains("Work with Things"));
    assert!(t.rows(1)[23].contains("Printed."));
    assert!(t.rows(2).iter().any(|r| r.contains("About")));
    // A pop-up panel over the list.
    let mut t = Script::default();
    t.step(Key::F(12), (1, 1), &[])
        .step(Key::F(12), (1, 1), &[]);
    d.cursor = (8, 3);
    let _ = d.show(&mut t, "WRK");
    let _ = d.show(&mut t, "POP");
    let pop = t.rows(1);
    assert!(pop.iter().any(|r| r.contains("Rename")), "{pop:?}");
    assert!(pop.iter().any(|r| r.contains("Work with Things")));
    assert!(
        t.shown[1]
            .fields
            .iter()
            .any(|f| f.target == Target::Var("NEWNAME".into()))
    );
}

#[test]
fn incomplete_lists_ask_for_more() {
    let g = compile(LIST);
    let mut d = Engine::new(g, "QS400");
    things(&mut d, 5);
    d.list_mut("OBJS").complete = false;
    d.incomplete_returns = true;
    let mut t = Script::default();
    t.step(Key::PageDown, (1, 1), &[]);
    assert_eq!(
        d.show_event(&mut t, "WRK").unwrap(),
        Event::More("OBJS".into())
    );
}

#[test]
fn handles_follow_inserts_and_removes() {
    let mut l = UimList::default();
    l.rows.push(vec![("A".into(), "1".into())]);
    l.sync_handles();
    let h = l.insert(0, vec![("A".into(), "0".into())]);
    l.options.insert(1, "4".into());
    assert_eq!(l.by_handle(h), Some(0));
    assert_eq!(l.by_handle(1), Some(1));
    l.remove(0);
    assert_eq!(l.by_handle(1), Some(0));
    assert_eq!(l.options.get(&0).map(String::as_str), Some("4"));
}

#[test]
fn wide_panels_on_a_wide_display() {
    let text = LIST.replace(
        ":PANEL NAME=CNF KEYL=KEYS",
        ":PANEL NAME=WIDE KEYL=KEYS DEPTH=27 WIDTH=132 ENTER='RETURN 1'.Wide Things
:LIST DEPTH='*' LISTDEF=OBJS.
:LISTCOL VAR=OPT USAGE=INPUT.Opt
:LISTCOL VAR=OBJ USAGE=OUTPUT.Object
:LISTCOL VAR=TEXT USAGE=OUTPUT.Text
:ELIST.
:EPANEL.
:PANEL NAME=CNF KEYL=KEYS",
    );
    let mut d = Engine::new(compile(&text), "QS400");
    things(&mut d, 40);
    let mut t = Script {
        wide: true,
        ..Script::default()
    };
    t.step(Key::F(1), (5, 3), &[])
        .step(Key::F(3), (1, 1), &[])
        .step(Key::F(3), (1, 1), &[]);
    assert_eq!(d.show(&mut t, "WIDE").unwrap(), Response::Exit);
    let s = &t.shown[0];
    assert_eq!((s.rows, s.cols), (27, 132));
    assert!(s.text_rows()[0].contains("Wide Things"));
    // More entries than 24 rows hold.
    assert!(s.text_rows().iter().filter(|r| r.contains("OBJ")).count() >= 20);
    // Its help fills the wide screen too.
    assert_eq!(t.shown[1].cols, 132);
    // A narrow display refuses the panel.
    let mut n = Script::default();
    assert!(d.show(&mut n, "WIDE").is_err());
}

/// A script whose search index holds the list group's topics.
struct Indexed(Script, Rc<PanelGroup>);

impl Terminal for Indexed {
    fn wide(&mut self) -> bool {
        false
    }

    fn exchange(&mut self, screen: &Screen) -> Option<Input> {
        self.0.exchange(screen)
    }

    fn group(&mut self, _lib: &str, _name: &str) -> Option<Rc<PanelGroup>> {
        Some(Rc::clone(&self.1))
    }

    fn search_index(&mut self, _lib: &str, _name: &str) -> Option<Vec<IndexEntry>> {
        Some(IndexEntry::of_group(&self.1, "QGPL/THINGS"))
    }
}

#[test]
fn the_search_index_finds_a_topic() {
    let text = LIST
        .replace(":PNLGRP.", ":PNLGRP SCHIDX='QGPL/THINGIDX'.")
        .replace(
            ":HELP NAME=HOBJ.Object\n",
            ":HELP NAME=HOBJ.Object\n:ISCH ROOTS='object name'.Naming objects\n",
        );
    let g = compile(&text);
    assert_eq!(IndexEntry::of_group(&g, "QGPL/THINGS").len(), 1);
    let mut d = Engine::new(Rc::clone(&g), "QS400");
    d.full_screen_help = true;
    things(&mut d, 2);
    let mut t = Indexed(Script::default(), g);
    // Field help, F11, the words, option 5 on the topic, out again.
    t.0.step(Key::F(1), (23, 3), &[])
        .step(Key::F(11), (1, 1), &[])
        .step(Key::Enter, (1, 1), &[])
        .step(Key::Enter, (1, 1), &[])
        .step(Key::F(3), (1, 1), &[])
        .step(Key::F(12), (1, 1), &[])
        .step(Key::F(12), (1, 1), &[])
        .step(Key::F(3), (1, 1), &[])
        .step(Key::F(3), (1, 1), &[]);
    let _ = d.show(&mut t, "WRK");
    let shown: Vec<Vec<String>> = t.0.shown.iter().map(Screen::text_rows).collect();
    assert!(shown[1][21].contains("F11=Search index"), "{:?}", shown[1]);
    assert!(shown[2][0].contains("Search Help Index"), "{:?}", shown[2]);
    assert!(
        shown[3].iter().any(|r| r.contains("Naming objects")),
        "{:?}",
        shown[3]
    );
}
