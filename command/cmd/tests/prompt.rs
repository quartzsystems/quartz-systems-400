// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The prompter's displays against golden files under `tests/golden/`:
//! the first display of a command, the F10, F9, F11 and F24 states, an
//! error state, a selective prompt, and the command strings the
//! session builds. `QS400_UPDATE_GOLDEN=1` rewrites them. The goldens
//! are this system's own renders (the real box's captures are in
//! `CMD_OPEN_QUESTIONS.md`).

use std::path::PathBuf;

use cmd::analyze::{Env, MemSource, analyze};
use cmd::prompt::{Prompter, Step};

const CRTLIB: &str = "\
 CMD PROMPT('Create Library') MAXPOS(2)
 PARM KWD(LIB) TYPE(*SNAME) LEN(10) MIN(1) PROMPT('Library')
 PARM KWD(TYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) DFT(*PROD) VALUES(*PROD *TEST) PROMPT('Library type')
 PARM KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) SPCVAL((*BLANK ' ')) PROMPT('Description')
 PARM KWD(ASP) TYPE(*INT2) DFT(1) RANGE(1 32) PMTCTL(*PMTRQS) PROMPT('Auxiliary storage pool')
 PARM KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Public authority')
 PARM KWD(CRTAUT) TYPE(*NAME) LEN(10) DFT(*SYSVAL) SPCVAL((*SYSVAL) (*CHANGE) (*ALL) (*USE) (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Authority for new objects')
";

const SBMJOB: &str = "\
 CMD PROMPT('Submit Job') MAXPOS(1)
 PARM KWD(CMD) TYPE(*CMDSTR) LEN(3000) PROMPT('Command to run')
 PARM KWD(JOB) TYPE(*NAME) LEN(10) DFT(*JOBD) SPCVAL((*JOBD)) PROMPT('Job name')
 PARM KWD(JOBD) TYPE(Q1) DFT(*USRPRF) SNGVAL((*USRPRF)) PROMPT('Job description')
 PARM KWD(JOBQ) TYPE(Q1) DFT(*JOBD) SNGVAL((*JOBD)) PROMPT('Job queue')
 PARM KWD(JOBPTY) TYPE(*INT2) DFT(*JOBD) RANGE(1 9) SPCVAL((*JOBD 0)) PROMPT('Job priority')
 PARM KWD(LOG) TYPE(E1) DFT(*JOBD) SNGVAL((*JOBD)) PMTCTL(*PMTRQS) PROMPT('Message logging')
 PARM KWD(INLLIBL) TYPE(*NAME) LEN(10) MAX(250) DFT(*JOBD) SNGVAL((*JOBD) (*SYSVAL) (*NONE) (*CURRENT)) PMTCTL(*PMTRQS) PROMPT('Initial library list')
 PARM KWD(HOLD) TYPE(*CHAR) LEN(5) RSTD(*YES) DFT(*JOBD) VALUES(*JOBD *NO *YES) PMTCTL(*PMTRQS) PROMPT('Hold on job queue')
 PARM KWD(SCDDATE) TYPE(*DATE) DFT(*CURRENT) SPCVAL((*CURRENT) (*MONTHSTR) (*MONTHEND)) PMTCTL(*PMTRQS) PROMPT('Schedule date')
 Q1: QUAL TYPE(*NAME) LEN(10)
 QUAL TYPE(*NAME) LEN(10) DFT(*LIBL) SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
 E1: ELEM TYPE(*INT2) DFT(4) RANGE(0 4) PROMPT('Level')
 ELEM TYPE(*INT2) DFT(0) RANGE(0 99) PROMPT('Severity')
 ELEM TYPE(*CHAR) LEN(7) RSTD(*YES) DFT(*NOLIST) VALUES(*MSG *SECLVL *NOLIST) PROMPT('Text')
";

const CRTSBSD: &str = "\
 CMD PROMPT('Create Subsystem Description') MAXPOS(1)
 PARM KWD(SBSD) TYPE(Q1) MIN(1) PROMPT('Subsystem description')
 PARM KWD(POOLS) TYPE(E1) MAX(10) DFT((1 *BASE)) PROMPT('Storage pools')
 PARM KWD(MAXJOBS) TYPE(*INT4) DFT(*NOMAX) RANGE(0 32767) SPCVAL((*NOMAX -1)) PROMPT('Maximum jobs')
 PARM KWD(SGNDSPF) TYPE(Q1) DFT(*QDSIGNON) SNGVAL((*QDSIGNON)) PMTCTL(P1) PROMPT('Sign-on display file')
 PARM KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) SPCVAL((*BLANK ' ')) PROMPT('Description')
 Q1: QUAL TYPE(*NAME) LEN(10)
 QUAL TYPE(*NAME) LEN(10) DFT(*CURLIB) SPCVAL((*CURLIB) (*LIBL)) PROMPT('Library')
 E1: ELEM TYPE(*INT2) RANGE(1 10) PROMPT('Pool identifier')
 ELEM TYPE(*CHAR) LEN(10) PROMPT('Storage size')
 ELEM TYPE(*INT4) DFT(1) RANGE(0 32767) PROMPT('Activity level')
 P1: PMTCTL CTL(MAXJOBS) COND((*NE *NOMAX))
";

fn source() -> MemSource {
    let mut s = MemSource::default();
    s.add("CRTLIB", CRTLIB).unwrap();
    s.add("SBMJOB", SBMJOB).unwrap();
    s.add("CRTSBSD", CRTSBSD).unwrap();
    s
}

fn golden(name: &str, text: &str) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let path = dir.join(format!("{name}.txt"));
    if std::env::var("QS400_UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, text).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (QS400_UPDATE_GOLDEN=1 writes it)", path.display()));
    assert_eq!(
        want.replace("\r\n", "\n"),
        text,
        "{name} differs from its golden"
    );
}

fn prompter(text: &str) -> (MemSource, Prompter) {
    let mut src = source();
    let r = {
        let mut env = Env::new(&mut src);
        analyze(&mut env, text).unwrap()
    };
    let p = Prompter::new(&r);
    (src, p)
}

#[test]
fn crtlib_states() {
    let (mut src, mut p) = prompter("CRTLIB TESTLIB TEXT('Test library')");
    golden("crtlib-first", &p.screen().text());
    let mut env = Env::new(&mut src);
    let fields: Vec<String> = p.screen().fields.iter().map(|f| f.value.clone()).collect();
    assert_eq!(p.key(10, &fields, &mut env), Step::Show);
    golden("crtlib-f10", &p.screen().text());
    assert_eq!(p.key(11, &fields, &mut env), Step::Show);
    golden("crtlib-f11", &p.screen().text());
    assert_eq!(p.key(11, &fields, &mut env), Step::Show);
    assert_eq!(p.key(24, &fields, &mut env), Step::Show);
    golden("crtlib-f24", &p.screen().text());
    assert_eq!(p.key(24, &fields, &mut env), Step::Show);
    // A bad value: the message, the cursor and reverse image on it.
    let fields: Vec<String> = p.screen().fields.iter().map(|f| f.value.clone()).collect();
    let mut bad = fields.clone();
    bad[1] = "*NOPE".into();
    assert_eq!(p.key(0, &bad, &mut env), Step::Show);
    let s = p.screen();
    golden("crtlib-error", &s.text());
    let err = s
        .fields
        .iter()
        .find(|f| f.reverse)
        .expect("a field in error");
    assert_eq!(err.value, "*NOPE");
    assert_eq!(s.cursor, Some((err.row, err.col)));
    assert_eq!(p.key(15, &bad, &mut env), Step::Show);
    golden("crtlib-f15", &p.screen().text());
    assert_eq!(p.key(0, &[], &mut env), Step::Show);
    // Back to good values: Enter runs.
    let mut good = fields.clone();
    good[1] = "*TEST".into();
    good[3] = "2".into();
    assert_eq!(
        p.key(0, &good, &mut env),
        Step::Done("CRTLIB LIB(TESTLIB) TYPE(*TEST) TEXT('Test library') ASP(2)".into())
    );
    assert_eq!(p.key(3, &good, &mut env), Step::Cancel);
}

#[test]
fn sbmjob_groups_lists_and_selective() {
    let (mut src, mut p) = prompter("SBMJOB CMD(CRTLIB LIB(X)) JOBD(QGPL/QBATCH)");
    let mut env = Env::new(&mut src);
    let fields: Vec<String> = p.screen().fields.iter().map(|f| f.value.clone()).collect();
    assert_eq!(p.key(10, &fields, &mut env), Step::Show);
    golden("sbmjob-f10", &p.screen().text());
    assert_eq!(p.key(8, &fields, &mut env), Step::Show);
    golden("sbmjob-page2", &p.screen().text());
    assert_eq!(p.key(7, &fields, &mut env), Step::Show);
    // The + field of the list adds an entry.
    let s = p.screen();
    let fields: Vec<String> = s
        .fields
        .iter()
        .map(|f| {
            if f.len == 1
                && f.path
                    .as_ref()
                    .is_some_and(|p| p.steps.last() == Some(&usize::MAX))
            {
                "+".into()
            } else {
                f.value.clone()
            }
        })
        .collect();
    assert_eq!(p.key(0, &fields, &mut env), Step::Show);
    golden("sbmjob-more", &p.screen().text());
    assert_eq!(p.key(14, &[], &mut env), Step::Show);
    golden("sbmjob-f14", &p.screen().text());
    assert_eq!(p.key(0, &[], &mut env), Step::Show);
    assert_eq!(p.key(13, &[], &mut env), Step::Show);
    golden("help", &p.screen().text());
    assert_eq!(p.key(0, &[], &mut env), Step::Show);
    let (mut src, p) =
        prompter("?SBMJOB ??CMD(CRTLIB LIB(X)) ?*JOB(NIGHT) ?-LOG ?<HOLD(*YES) ?&JOBPTY(3)");
    let _ = &mut src;
    golden("sbmjob-selective", &p.screen().text());
    assert_eq!(
        p.command_text(),
        "SBMJOB CMD(CRTLIB LIB(X)) JOB(NIGHT) JOBPTY(3) HOLD(*YES)"
    );
}

#[test]
fn crtsbsd_pmtctl_two_step() {
    let (mut src, mut p) = prompter("CRTSBSD SBSD(QGPL/MYSBS)");
    let mut env = Env::new(&mut src);
    golden("crtsbsd-first", &p.screen().text());
    // SGNDSPF shows only when MAXJOBS is not *NOMAX: the first Enter
    // with a changed MAXJOBS brings the display back with it.
    let s = p.screen();
    let fields: Vec<String> = s
        .fields
        .iter()
        .map(|f| {
            if f.value == "*NOMAX" {
                "5".into()
            } else {
                f.value.clone()
            }
        })
        .collect();
    assert_eq!(p.key(0, &fields, &mut env), Step::Show);
    golden("crtsbsd-after-enter", &p.screen().text());
    let fields: Vec<String> = p.screen().fields.iter().map(|f| f.value.clone()).collect();
    assert_eq!(
        p.key(0, &fields, &mut env),
        Step::Done("CRTSBSD SBSD(QGPL/MYSBS) MAXJOBS(5)".into())
    );
}
