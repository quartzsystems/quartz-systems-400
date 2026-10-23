// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The UIM APIs called from IR programs: an application opened over a
//! panel group, variables and list entries put, the panel shown on a
//! scripted display, the operator's option read back, the list's
//! attributes, printing, help and the error codes.

#![allow(clippy::too_many_lines)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use mi::exec::Program;
use mi::io::IoFail;
use mi::io::spool::{MemSpool, PrinterOpen, SpoolSink};
use sysapi::uiapi::UimHost;
use uim::PanelGroup;
use uim::engine::{Input, Terminal};
use uim::model::Key;
use uim::render::{Screen, Target};

/// Error code with 16 bytes provided.
const ERRC: &str = "static $err: char(16) inz(x\"00000010000000000000000000000000\")";

const GROUP: &str = "\
:PNLGRP.
:CLASS NAME=OPTCL BASETYPE=ACTION.
:ECLASS.
:CLASS NAME=NAMECL BASETYPE='CHAR 10'.
:ECLASS.
:CLASS NAME=NUMCL BASETYPE='BIN 31'.
:ECLASS.
:VAR NAME=OPT CLASS=OPTCL.
:VAR NAME=OBJ CLASS=NAMECL.
:VAR NAME=TYPE CLASS=NAMECL.
:VAR NAME=OWNER CLASS=NAMECL.
:VAR NAME=COUNT CLASS=NUMCL.
:VARRCD NAME=HDR VARS='OWNER COUNT'.
:VARRCD NAME=ROW VARS='OPT OBJ TYPE'.
:LISTDEF NAME=OBJS VARS='OPT OBJ TYPE'.
:KEYL NAME=KEYS.
:KEYI KEY=ENTER ACTION=ENTER.
:KEYI KEY=F3 ACTION=EXIT.F3=Exit
:KEYI KEY=F12 ACTION=CANCEL.F12=Cancel
:EKEYL.
:PANEL NAME=LST KEYL=KEYS ENTER='RETURN 1'.Work with Test Objects
:DATA DEPTH=2.
:DATAI VAR=OWNER USAGE=OUTPUT.Owner
:DATAI VAR=COUNT USAGE=OUTPUT.Objects
:EDATA.
:TOPINST.Type options, press Enter.
:LIST DEPTH='*' LISTDEF=OBJS.
:LISTACT OPTION=5 ENTER='RETURN 5'.5=Display
:LISTCOL VAR=OPT USAGE=INPUT.Opt
:LISTCOL VAR=OBJ USAGE=OUTPUT.Object
:LISTCOL VAR=TYPE USAGE=OUTPUT.Type
:ELIST.
:EPANEL.
:PRTPNL NAME=PRT.Test Objects Report
:DATA DEPTH=2.
:DATAI VAR=OWNER USAGE=OUTPUT.Owner
:EDATA.
:EPRTPNL.
:HELP NAME=HLP.Test help
:P.Words about the test panel.
:EHELP.
:EPNLGRP.
";

/// What the scripted display shared with the test.
#[derive(Default)]
struct Display {
    /// Keys, with the option to type on the n-th entry.
    steps: VecDeque<(Key, Option<(usize, String)>)>,
    shown: Vec<Screen>,
}

struct Term(Rc<RefCell<Display>>);

impl Terminal for Term {
    fn wide(&mut self) -> bool {
        false
    }

    fn exchange(&mut self, screen: &Screen) -> Option<Input> {
        let mut d = self.0.borrow_mut();
        d.shown.push(screen.clone());
        let (key, typed) = d.steps.pop_front()?;
        let values = screen
            .fields
            .iter()
            .map(|f| match (&f.target, &typed) {
                (Target::ListOption { row_index, .. }, Some((n, v))) if row_index == n => v.clone(),
                _ => f.value.clone(),
            })
            .collect();
        Some(Input {
            key: Some(key),
            cursor: (1, 1),
            values,
        })
    }
}

struct Host {
    group: Rc<PanelGroup>,
    display: Option<Rc<RefCell<Display>>>,
}

impl UimHost for Host {
    fn panel_group(&mut self, _lib: &str, name: &str) -> Result<(Rc<PanelGroup>, String), String> {
        if name == "TESTGRP" {
            Ok((Rc::clone(&self.group), "QGPL".into()))
        } else {
            Err(format!("no {name}"))
        }
    }

    fn terminal(&mut self) -> Option<Box<dyn Terminal>> {
        self.display
            .as_ref()
            .map(|d| Box::new(Term(Rc::clone(d))) as Box<dyn Terminal>)
    }
}

fn group() -> Rc<PanelGroup> {
    let c = uim::compile(
        GROUP,
        &uim::NoImbeds,
        &uim::NoImports,
        &uim::CompileOpts::default(),
    );
    assert!(c.ok(), "{}", c.listing);
    Rc::new(c.group.unwrap())
}

/// A spool the test can read after the run.
struct Shared(Rc<RefCell<MemSpool>>);

impl SpoolSink for Shared {
    fn open(&mut self, file: &str, lib: &str, actgrp: &str) -> Result<PrinterOpen, IoFail> {
        self.0.borrow_mut().open(file, lib, actgrp)
    }

    fn put(&mut self, id: u32, data: &[u8], pages: u32, records: u32) -> Result<(), IoFail> {
        self.0.borrow_mut().put(id, data, pages, records)
    }

    fn close(&mut self, id: u32, hold: bool) -> Result<(), IoFail> {
        self.0.borrow_mut().close(id, hold)
    }
}

/// The text of every spooled file, SCS decoded.
fn spooled(s: &Rc<RefCell<MemSpool>>) -> Vec<String> {
    s.borrow()
        .files
        .values()
        .map(|f| spool::view::text(&spool::scs::decode(&f.data).0))
        .collect()
}

thread_local! {
    static SPOOL: Rc<RefCell<MemSpool>> = Rc::new(RefCell::new(MemSpool::default()));
}

fn run(text: &str, display: Option<Rc<RefCell<Display>>>) -> Program {
    let mut p = Program::from_text(text).unwrap_or_else(|e| panic!("{e}"));
    p.job().spool = Some(Box::new(Shared(SPOOL.with(Rc::clone))));
    let host = Rc::new(RefCell::new(Host {
        group: group(),
        display,
    }));
    let host: sysapi::uiapi::SharedUimHost = host;
    sysapi::uiapi::install(p.job(), &host);
    p.call_entry().unwrap_or_else(|e| panic!("{e}"));
    p
}

fn bytes(p: &Program, name: &str) -> Vec<u8> {
    p.static_bytes(name).unwrap_or_else(|| panic!("no {name}"))
}

fn bin4(b: &[u8], off: usize) -> i32 {
    i32::from_be_bytes(b[off..off + 4].try_into().unwrap())
}

fn chars(b: &[u8], off: usize, n: usize) -> String {
    ccsid::decode_trimmed(&b[off..off + n])
}

/// The message identifier in an error code, blank when none.
fn errid(p: &Program, name: &str) -> String {
    let e = bytes(p, name);
    if bin4(&e, 4) == 0 {
        String::new()
    } else {
        chars(&e, 8, 7)
    }
}

const EXTERNS: &str = "
extern sysapi QUIOPNDA($h: char(8), $g: char(20), $s: i32, $x: i32, $f: char(1), $err: char(16))
extern sysapi QUIOPNPA($h: char(8), $g: char(20), $s: i32, $x: i32, $pf: char(20), $alt: char(10), $sh: char(1), $ud: char(10), $err: char(16))
extern sysapi QUICLOA($h: char(8), $o: char(1), $err: char(16))
extern sysapi QUIPUTV($h: char(8), $b: char(14), $l: i32, $r: char(10), $err: char(16))
extern sysapi QUIGETV($h: char(8), $b: char(14), $l: i32, $r: char(10), $err: char(16))
extern sysapi QUIADDLE($h: char(8), $b: char(22), $l: i32, $r: char(10), $n: char(10), $o: char(4), $e: char(4), $err: char(16))
extern sysapi QUIGETLE($h: char(8), $b: char(22), $l: i32, $r: char(10), $n: char(10), $p: char(4), $c: char(1), $sc: char(20), $sh: char(4), $x: char(1), $e: char(4), $err: char(16))
extern sysapi QUIRMVLE($h: char(8), $n: char(10), $x: char(1), $e: char(4), $err: char(16))
extern sysapi QUISETLA($h: char(8), $n: char(10), $c: char(4), $p: char(4), $t: char(1), $err: char(16))
extern sysapi QUIGETLA($h: char(8), $r: char(28), $l: i32, $f: char(8), $n: char(10), $err: char(16))
extern sysapi QUIDSPP($h: char(8), $fn: i32, $p: char(10), $r: char(1), $err: char(16))
extern sysapi QUIPRTP($h: char(8), $p: char(10), $e: char(1), $err: char(16))
extern sysapi QUHPRTH($ids: char(52), $n: i32, $err: char(16))
";

#[test]
fn a_list_is_shown_and_its_option_read_back() {
    let display = Rc::new(RefCell::new(Display::default()));
    // Enter with option 5 typed on the second entry.
    display
        .borrow_mut()
        .steps
        .push_back((Key::Enter, Some((1, "5".into()))));
    let p = run(
        &format!(
            "
module UI
{EXTERNS}
static $h: char(8)
static $grp: char(20) inz(\"TESTGRP   QGPL\")
static $scope: i32 inz(-1)
static $zero: i32 inz(0)
static $n: char(1) inz(\"N\")
{ERRC}
static $hdr: char(14) inz(x\"D8D7C7D4D9404040404000000003\")
static $l14: i32 inz(14)
static $rhdr: char(10) inz(\"HDR\")
static $e1: char(22) inz(\"  ALPHA     *FILE\")
static $e2: char(22) inz(\"  BETA      *PGM\")
static $e3: char(22) inz(\"  GAMMA     *DTAARA\")
static $l22: i32 inz(22)
static $rrow: char(10) inz(\"ROW\")
static $list: char(10) inz(\"OBJS\")
static $last: char(4) inz(\"LAST\")
static $eh: char(4)
static $all: char(4) inz(\"ALL\")
static $top: char(4) inz(\"TOP\")
static $fn: i32
static $pnl: char(10) inz(\"LST\")
static $got: char(22)
static $frst: char(4) inz(\"FRST\")
static $y: char(1) inz(\"Y\")
static $action: char(20) inz(\"ACTION\")
static $sh: char(4)
static $geh: char(4)
static $la: char(28)
static $l28: i32 inz(28)
static $fmt: char(8) inz(\"LIST0100\")
static $back: char(14)
static $m: char(1) inz(\"M\")
proc UI() entry {{
L0:
  sysapi QUIOPNDA($h, $grp, $scope, $zero, $n, $err)
  sysapi QUIPUTV($h, $hdr, $l14, $rhdr, $err)
  sysapi QUIADDLE($h, $e1, $l22, $rrow, $list, $last, $eh, $err)
  sysapi QUIADDLE($h, $e2, $l22, $rrow, $list, $last, $eh, $err)
  sysapi QUIADDLE($h, $e3, $l22, $rrow, $list, $last, $eh, $err)
  sysapi QUISETLA($h, $list, $all, $top, $n, $err)
  sysapi QUIDSPP($h, $fn, $pnl, $n, $err)
  sysapi QUIGETLE($h, $got, $l22, $rrow, $list, $frst, $y, $action, $sh, $n, $geh, $err)
  sysapi QUIGETLA($h, $la, $l28, $fmt, $list, $err)
  sysapi QUIGETV($h, $back, $l14, $rhdr, $err)
  sysapi QUICLOA($h, $m, $err)
  ret
}}
"
        ),
        Some(Rc::clone(&display)),
    );
    assert_eq!(errid(&p, "err"), "");
    assert_eq!(bin4(&bytes(&p, "fn"), 0), 5, "the option's RETURN 5");
    let got = bytes(&p, "got");
    assert_eq!(chars(&got, 0, 2), "5");
    assert_eq!(chars(&got, 2, 10), "BETA");
    assert_eq!(chars(&got, 12, 10), "*PGM");
    let la = bytes(&p, "la");
    assert_eq!(chars(&la, 8, 4), "ALL");
    assert_eq!(bin4(&la, 16), 3, "three entries");
    assert_eq!(bin4(&la, 20), 1, "one with an option");
    assert_eq!(&la[24..28], &bytes(&p, "geh")[..], "the current entry");
    let back = bytes(&p, "back");
    assert_eq!(chars(&back, 0, 10), "QPGMR");
    assert_eq!(bin4(&back, 10), 3);
    let shown = display.borrow().shown[0].text_rows();
    assert!(shown[0].contains("Work with Test Objects"), "{shown:?}");
    assert!(
        shown
            .iter()
            .any(|r| r.contains("Owner") && r.contains("QPGMR"))
    );
    assert!(
        shown
            .iter()
            .any(|r| r.contains("GAMMA") && r.contains("*DTAARA"))
    );
}

#[test]
fn errors_come_back_in_the_error_code() {
    let p = run(
        &format!(
            "
module UE
{EXTERNS}
static $h: char(8)
static $bad: char(8) inz(\"NOTOPEN\")
static $grp: char(20) inz(\"TESTGRP   QGPL\")
static $nogrp: char(20) inz(\"NOGRP     QGPL\")
static $scope: i32 inz(-1)
static $badscope: i32 inz(7)
static $zero: i32 inz(0)
static $n: char(1) inz(\"N\")
static $err1: char(16) inz(x\"00000010000000000000000000000000\")
static $err2: char(16) inz(x\"00000010000000000000000000000000\")
static $err3: char(16) inz(x\"00000010000000000000000000000000\")
static $err4: char(16) inz(x\"00000010000000000000000000000000\")
static $err5: char(16) inz(x\"00000010000000000000000000000000\")
static $err6: char(16) inz(x\"00000010000000000000000000000000\")
static $fn: i32
static $pnl: char(10) inz(\"NOPANEL\")
static $list: char(10) inz(\"OBJS\")
static $rrow: char(10) inz(\"ROW\")
static $l22: i32 inz(22)
static $got: char(22)
static $frst: char(4) inz(\"FRST\")
static $y: char(1) inz(\"Y\")
static $blank: char(20)
static $sh: char(4)
static $geh: char(4)
static $m: char(1) inz(\"M\")
proc UE() entry {{
L0:
  sysapi QUIOPNDA($h, $nogrp, $scope, $zero, $n, $err1)
  sysapi QUIOPNDA($h, $grp, $badscope, $zero, $n, $err2)
  sysapi QUIDSPP($bad, $fn, $pnl, $n, $err3)
  sysapi QUIOPNDA($h, $grp, $scope, $zero, $n, $err4)
  sysapi QUIDSPP($h, $fn, $pnl, $n, $err5)
  sysapi QUIGETLE($h, $got, $l22, $rrow, $list, $frst, $y, $blank, $sh, $n, $geh, $err6)
  sysapi QUICLOA($h, $m, $err4)
  ret
}}
"
        ),
        None,
    );
    assert_eq!(errid(&p, "err1"), "CPF6A12");
    assert_eq!(errid(&p, "err2"), "CPF6A2A");
    assert_eq!(errid(&p, "err3"), "CPF6A0B");
    assert_eq!(errid(&p, "err4"), "");
    assert_eq!(errid(&p, "err5"), "CPF6A3F");
    assert_eq!(errid(&p, "err6"), "CPF6A91");
}

#[test]
fn print_panels_and_help_are_spooled() {
    let p = run(
        &format!(
            "
module UP
{EXTERNS}
static $h: char(8)
static $grp: char(20) inz(\"TESTGRP   QGPL\")
static $scope: i32 inz(-1)
static $zero: i32 inz(0)
static $n: char(1) inz(\"N\")
static $prtf: char(20)
static $alt: char(10)
static $ud: char(10)
{ERRC}
static $hdr: char(14) inz(x\"D8D7C7D4D9404040404000000003\")
static $l14: i32 inz(14)
static $rhdr: char(10) inz(\"HDR\")
static $prt: char(10) inz(\"PRT\")
static $m: char(1) inz(\"M\")
static $ids: char(52) inz(\"TESTGRP   QGPL      HLP\")
static $one: i32 inz(1)
proc UP() entry {{
L0:
  sysapi QUIOPNPA($h, $grp, $scope, $zero, $prtf, $alt, $n, $ud, $err)
  sysapi QUIPUTV($h, $hdr, $l14, $rhdr, $err)
  sysapi QUIPRTP($h, $prt, $n, $err)
  sysapi QUICLOA($h, $m, $err)
  sysapi QUHPRTH($ids, $one, $err)
  ret
}}
"
        ),
        None,
    );
    assert_eq!(errid(&p, "err"), "");
    let files = SPOOL.with(spooled);
    assert!(
        files
            .iter()
            .any(|f| f.contains("Test Objects Report") && f.contains("QPGMR")),
        "{files:?}"
    );
    assert!(
        files
            .iter()
            .any(|f| f.contains("Test help") && f.contains("Words about")),
        "{files:?}"
    );
}
