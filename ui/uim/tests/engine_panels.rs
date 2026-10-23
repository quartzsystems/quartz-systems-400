// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The dialog engine's own displays against the pub400 goldens of
//! `batch8-uim` and `batch9-uimpos`: help titles and keys, the
//! position-to pop-up window and an option that is not valid. The help
//! text itself is ours and is not compared; the rows that are the
//! compatibility interface (titles, labels, keys, borders) are.

mod common;

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::rc::Rc;

use uim::engine::{Engine, Input, Response, Terminal};
use uim::model::Key;
use uim::render::{Attr, Screen, Target};
use uim::{CompileOpts, NoImbeds, NoImports, PanelGroup, compile};

/// A list panel shaped as `WRKOBJ` is, with help on the options and the
/// keys, and the `Position the List` window behind F17.
const GROUP: &str = "\
:PNLGRP.
:CLASS NAME=OPTCL BASETYPE=ACTION.
:ECLASS.
:CLASS NAME=NAMECL BASETYPE='CHAR 10'.
:ECLASS.
:CLASS NAME=TYPECL BASETYPE='CHAR 8'.
:ECLASS.
:CLASS NAME=HINTCL BASETYPE='CHAR 20'.
:ECLASS.
:VAR NAME=OPT CLASS=OPTCL.
:VAR NAME=OBJ CLASS=NAMECL.
:VAR NAME=TYPE CLASS=TYPECL.
:VAR NAME=POSOBJ CLASS=NAMECL.
:VAR NAME=POSTYPE CLASS=TYPECL.
:VAR NAME=HINT CLASS=HINTCL VALUE='Starting characters'.
:VARRCD NAME=POSREC VARS='POSOBJ POSTYPE'.
:LISTDEF NAME=OBJS VARS='OPT OBJ TYPE'.
:KEYL NAME=KEYS HELP=HKEYS.
:KEYI KEY=ENTER ACTION=ENTER.
:KEYI KEY=HELP ACTION=HELP.
:KEYI KEY=F1 ACTION=HELP.
:KEYI KEY=F2 ACTION=EXTHELP.
:KEYI KEY=F3 ACTION=EXIT.F3=Exit
:KEYI KEY=F12 ACTION=CANCEL.F12=Cancel
:KEYI KEY=F17 ACTION='PANEL POSLIST'.F17=Position to
:EKEYL.
:KEYL NAME=POSKEYS.
:KEYI KEY=ENTER ACTION=ENTER.
:KEYI KEY=F6 ACTION='RETURN 6'.F6=Retrieve previous
:KEYI KEY=F12 ACTION=CANCEL.F12=Cancel
:KEYI KEY=F16 ACTION='RETURN 16'.F16=Repeat position to
:EKEYL.
:PANEL NAME=WRK KEYL=KEYS HELP=HWRK ENTER='RETURN 1'.Work with Objects
:TOPINST.Type options, press Enter.
:LIST DEPTH='*' LISTDEF=OBJS PARMS=POSREC HELP=HOPTS.
:LISTACT OPTION=4 ENTER='RETURN 4'.4=Delete
:LISTACT OPTION=5 ENTER='RETURN 5'.5=Display
:LISTCOL VAR=OPT USAGE=INPUT.Opt
:LISTCOL VAR=OBJ USAGE=OUTPUT.Object
:LISTCOL VAR=TYPE USAGE=OUTPUT.Type
:ELIST.
:EPANEL.
:PANEL NAME=POSLIST KEYL=POSKEYS DEPTH=10 WIDTH=74 ENTER='RETURN 1'.Position the List
:DATA DEPTH='*'.
:DATACOL WIDTH=26.
:DATACOL WIDTH=72.
:DATAI VAR=POSOBJ USAGE=INPUT.Position to
:DATAIX VAR=HINT USAGE=OUTPUT.
:DATAGRP GRPSEP=NONE.
:DATAI VAR=POSTYPE USAGE=INPUT.Type
:EDATAGRP.
:EDATA.
:EPANEL.
:HELP NAME=HWRK.Work with Objects
:P.Our words about the list of objects.
:EHELP.
:HELP NAME=HOPTS.Options
:P.Our words about the options.
:EHELP.
:HELP NAME=HKEYS.Function Keys
:P.Our words about the keys.
:EHELP.
:EPNLGRP.
";

fn group() -> Rc<PanelGroup> {
    let c = compile(GROUP, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "{}", c.listing);
    Rc::new(c.group.expect("group"))
}

/// A terminal answering from a script, keeping what it showed.
#[derive(Default)]
struct Script {
    steps: VecDeque<(Key, (usize, usize))>,
    shown: Vec<Screen>,
}

impl Script {
    fn keys(steps: &[(Key, (usize, usize))]) -> Self {
        Self {
            steps: steps.iter().copied().collect(),
            shown: Vec::new(),
        }
    }
}

impl Terminal for Script {
    fn wide(&mut self) -> bool {
        false
    }

    fn exchange(&mut self, screen: &Screen) -> Option<Input> {
        self.shown.push(screen.clone());
        let (key, cursor) = self.steps.pop_front()?;
        Some(Input {
            key: Some(key),
            cursor,
            values: screen.fields.iter().map(|f| f.value.clone()).collect(),
        })
    }
}

fn dialog() -> Engine {
    let mut d = Engine::new(group(), "QS400");
    d.full_screen_help = true;
    for (o, t) in [("SETUSER", "*PGM"), ("EXCELAFBA", "*SQLPKG")] {
        d.add_row(
            "OBJS",
            vec![
                ("OPT".into(), String::new()),
                ("OBJ".into(), o.into()),
                ("TYPE".into(), t.into()),
            ],
        );
    }
    d.list_mut("OBJS").complete = true;
    d
}

/// A golden row with the `+number` mask's `N` standing for digits.
fn same(golden: &str, ours: &str) -> bool {
    let g: Vec<char> = golden.chars().collect();
    let o: Vec<char> = ours.chars().collect();
    g.len() == o.len()
        && g.iter()
            .zip(&o)
            .all(|(a, b)| a == b || (*a == 'N' && b.is_ascii_digit()))
}

fn check_rows(name: &str, screen: &Screen, rows: &[usize]) {
    let golden = common::golden_rows(name);
    let ours = screen.text_rows();
    let mut bad = String::new();
    for &r in rows {
        if !same(&golden[r - 1], &ours[r - 1]) {
            let _ = writeln!(bad, "row {r}:\n  golden |{}|\n  ours   |{}|", golden[r - 1], ours[r - 1]);
        }
    }
    assert!(bad.is_empty(), "{name}:\n{bad}");
}

#[test]
fn uim_help_field() {
    let mut d = dialog();
    // F1 on the first option field: the options' help, full screen.
    let mut t = Script::keys(&[(Key::F(1), (8, 3)), (Key::F(3), (1, 1))]);
    let _ = d.show(&mut t, "WRK");
    let help = &t.shown[1];
    check_rows("uim-help-field", help, &[1, 22, 23]);
    check_rows("uim-help-field-2", help, &[21]);
}

#[test]
fn uim_exthelp() {
    let mut d = dialog();
    let mut t = Script::keys(&[(Key::F(2), (8, 3)), (Key::F(3), (1, 1))]);
    let _ = d.show(&mut t, "WRK");
    check_rows("uim-exthelp", &t.shown[1], &[1, 22, 23]);
    // A further module's title is a heading at column 2.
    assert!(t.shown[1].text_rows().iter().any(|r| r.trim_end() == " Options"));
}

#[test]
fn uim_help_title() {
    let mut d = dialog();
    // F1 on the title row is the extended help.
    let mut t = Script::keys(&[(Key::F(1), (1, 40)), (Key::F(3), (1, 1))]);
    let _ = d.show(&mut t, "WRK");
    check_rows("uim-help-title", &t.shown[1], &[1, 22, 23]);
}

#[test]
fn uim_help_keys() {
    let mut d = dialog();
    let mut t = Script::keys(&[(Key::F(1), (23, 3)), (Key::F(3), (1, 1))]);
    let _ = d.show(&mut t, "WRK");
    check_rows("uim-help-keys", &t.shown[1], &[1, 22, 23]);
}

#[test]
fn uim_position_window() {
    let mut d = dialog();
    let mut t = Script::keys(&[(Key::F(17), (8, 3)), (Key::F(12), (1, 1)), (Key::F(3), (1, 1))]);
    assert_eq!(d.show(&mut t, "WRK").unwrap(), Response::Exit);
    let win = &t.shown[1];
    check_rows("uim-position-window", win, &[2, 3, 4, 5, 6, 7, 11, 12, 13]);
    let attrs = |r: usize, c: usize| win.fields.iter().find(|f| (f.row, f.col) == (r, c)).map(|f| (f.len, f.target.clone()));
    assert_eq!(attrs(5, 32), Some((10, Target::Var("POSOBJ".into()))));
    assert_eq!(attrs(6, 34), Some((8, Target::Var("POSTYPE".into()))));
}

/// A script that types `99` into one field on the first screen.
struct Typing(Script, (usize, usize));
impl Terminal for Typing {
    fn wide(&mut self) -> bool {
        false
    }
    fn exchange(&mut self, screen: &Screen) -> Option<Input> {
        let mut i = self.0.exchange(screen)?;
        if self.0.shown.len() == 1 {
            for (f, v) in screen.fields.iter().zip(i.values.iter_mut()) {
                if (f.row, f.col) == self.1 {
                    *v = "99".into();
                }
            }
        }
        Some(i)
    }
}

#[test]
fn uim_option_not_valid() {
    let mut d = dialog();
    let first = uim::render(&d.group, "WRK", &d, &uim::Options::default()).unwrap();
    let at = first
        .fields
        .iter()
        .find(|f| matches!(f.target, Target::ListOption { .. }))
        .map(|f| (f.row, f.col))
        .unwrap();
    let mut t = Typing(Script::keys(&[(Key::Enter, at), (Key::F(3), (1, 1))]), at);
    assert_eq!(d.show(&mut t, "WRK").unwrap(), Response::Exit);
    let again = &t.0.shown[1];
    let f = again.fields.iter().find(|f| (f.row, f.col) == at).unwrap();
    // The golden shows the field underlined and reversed (attribute 0x25).
    let attr = std::fs::read_to_string(common::golden_dir().join("uim-option-not-valid.attr")).unwrap();
    let row8: Vec<char> = attr.lines().nth(7).unwrap().chars().collect();
    assert_eq!((row8[1], row8[2]), ('5', '5'));
    assert_eq!(f.attr, Attr::Reverse);
    assert_eq!(f.value, "99");
}
