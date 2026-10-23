// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The analyzer over hand-written definitions of `CRTLIB`, `CPYF` and
//! `SBMJOB`: a table of command strings with the identifiers each must
//! produce, and the values the good ones bind.

use cmd::analyze::{Env, MemSource, Setting};
use cmd::value::Value;
use cmd::{Sel, analyze};

const CRTLIB: &str = "\
 CMD PROMPT('Create Library') MAXPOS(2) ALLOW(*ALL)
 PARM KWD(LIB) TYPE(*SNAME) LEN(10) MIN(1) PROMPT('Library')
 PARM KWD(TYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) DFT(*PROD) VALUES(*PROD *TEST) PROMPT('Library type')
 PARM KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) SPCVAL((*BLANK ' ')) PROMPT('Text')
 PARM KWD(ASP) TYPE(*INT2) DFT(1) RANGE(1 32) PROMPT('Auxiliary storage pool')
 PARM KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Authority')
 PARM KWD(CRTAUT) TYPE(*NAME) LEN(10) DFT(*SYSVAL) SPCVAL((*SYSVAL) (*CHANGE) (*ALL) (*USE) (*EXCLUDE)) PMTCTL(*PMTRQS)
";

const CPYF: &str = "\
 CMD PROMPT('Copy File') MAXPOS(2)
 PARM KWD(FROMFILE) TYPE(Q1) MIN(1) FILE(*IN) PROMPT('From file')
 PARM KWD(TOFILE) TYPE(Q2) DFT(*PRINT) SNGVAL((*PRINT)) PROMPT('To file')
 PARM KWD(FROMMBR) TYPE(*NAME) LEN(10) DFT(*FIRST) SPCVAL((*FIRST) (*ALL)) PROMPT('From member')
 PARM KWD(TOMBR) TYPE(*NAME) LEN(10) DFT(*FIRST) SPCVAL((*FIRST) (*FROMMBR)) PROMPT('To member')
 PARM KWD(MBROPT) TYPE(*CHAR) LEN(8) RSTD(*YES) DFT(*NONE) VALUES(*NONE *ADD *REPLACE *UPDADD) PROMPT('Replace or add records')
 PARM KWD(CRTFILE) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*NO) VALUES(*NO *YES) PROMPT('Create file')
 PARM KWD(FROMRCD) TYPE(*DEC) LEN(10) DFT(*START) RANGE(1 4294967288) SPCVAL((*START 0)) PMTCTL(*PMTRQS)
 PARM KWD(NBRRCDS) TYPE(*DEC) LEN(10) DFT(*END) REL(*GT 0) SPCVAL((*END 0)) PMTCTL(*PMTRQS)
 PARM KWD(INCREL) TYPE(E1) MAX(50) DFT(*NONE) SNGVAL((*NONE)) PMTCTL(*PMTRQS)
 PARM KWD(FMTOPT) TYPE(*CHAR) LEN(8) RSTD(*YES) DFT(*NONE) VALUES(*NONE *NOCHK *CVTSRC *MAP *DROP) MAX(2) PMTCTL(*PMTRQS)
 PARM KWD(SRCOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) DFT(*SAME) VALUES(*SAME *SEQNBR *DATE) MAX(2) PMTCTL(*PMTRQS)
 PARM KWD(ERRLVL) TYPE(*DEC) LEN(10) DFT(0) RANGE(0 2147483647) SPCVAL((*NOMAX 2147483647)) PMTCTL(*PMTRQS)
 Q1: QUAL TYPE(*NAME) LEN(10)
 QUAL TYPE(*NAME) LEN(10) DFT(*LIBL) SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
 Q2: QUAL TYPE(*NAME) LEN(10)
 QUAL TYPE(*NAME) LEN(10) DFT(*LIBL) SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
 E1: ELEM TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*IF) VALUES(*IF *AND *OR) PROMPT('Relationship')
 ELEM TYPE(*NAME) LEN(10) MIN(1) PROMPT('Field')
 ELEM TYPE(*CHAR) LEN(3) RSTD(*YES) MIN(1) VALUES(*EQ *NE *GT *LT *GE *LE *CT) PROMPT('Operator')
 ELEM TYPE(*CHAR) LEN(256) MIN(1) EXPR(*YES) PROMPT('Value')
 DEP CTL(&CRTFILE *EQ *YES) PARM((&MBROPT *EQ *NONE)) NBRTRUE(*NONE) MSGID(CPD2918)
 DEP CTL(&FROMRCD) PARM(&NBRRCDS &TOFILE) NBRTRUE((*GE 1)) MSGID(CPD2919)
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
 PARM KWD(RTGDTA) TYPE(*CHAR) LEN(80) DFT(QCMDB) SPCVAL((*JOBD) (*RQSDTA)) PMTCTL(*PMTRQS) PROMPT('Routing data')
 PARM KWD(SCDDATE) TYPE(*DATE) DFT(*CURRENT) SPCVAL((*CURRENT) (*MONTHSTR) (*MONTHEND)) PMTCTL(*PMTRQS) PROMPT('Schedule date')
 PARM KWD(SCDTIME) TYPE(*TIME) DFT(*CURRENT) SPCVAL((*CURRENT)) PMTCTL(*PMTRQS) PROMPT('Schedule time')
 PARM KWD(HOLD) TYPE(*CHAR) LEN(5) RSTD(*YES) DFT(*JOBD) VALUES(*JOBD *NO *YES) PMTCTL(*PMTRQS) PROMPT('Hold on job queue')
 PARM KWD(MSGQ) TYPE(Q1) DFT(*USRPRF) SNGVAL((*USRPRF) (*NONE)) PMTCTL(*PMTRQS)
 Q1: QUAL TYPE(*NAME) LEN(10)
 QUAL TYPE(*NAME) LEN(10) DFT(*LIBL) SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
 E1: ELEM TYPE(*INT2) DFT(4) RANGE(0 4) PROMPT('Level')
 ELEM TYPE(*INT2) DFT(0) RANGE(0 99) PROMPT('Severity')
 ELEM TYPE(*CHAR) LEN(7) RSTD(*YES) DFT(*NOLIST) VALUES(*MSG *SECLVL *NOLIST) PROMPT('Text')
";

fn source() -> MemSource {
    let mut s = MemSource::default();
    s.add("CRTLIB", CRTLIB).unwrap();
    s.add("CPYF", CPYF).unwrap();
    s.add("SBMJOB", SBMJOB).unwrap();
    s
}

/// The table: a command string and the identifiers expected, in
/// order; `CPF0001` last for failures, nothing for success.
const TABLE: &[(&str, &[&str])] = &[
    ("CRTLIB LIB(TESTLIB)", &[]),
    ("crtlib testlib", &[]),
    ("CRTLIB TESTLIB *TEST", &[]),
    ("CRTLIB TESTLIB *TEST 'text'", &["CPD0065", "CPF0001"]),
    ("CRTLIB", &["CPD0071", "CPF0001"]),
    ("CRTLIB LIB(TESTLIB) TYPE(*PRODX)", &["CPD0084", "CPF0001"]),
    ("CRTLIB LIB(TESTLIB) TYPE(*prod)", &[]),
    ("CRTLIB LIB(1LIB)", &["CPD0078", "CPF0001"]),
    ("CRTLIB LIB(ABCDEFGHIJK)", &["CPD0074", "CPF0001"]),
    ("CRTLIB LIB(X) FOO(1)", &["CPD0043", "CPF0001"]),
    ("CRTLIB LIB(X) LIB(Y)", &["CPD0042", "CPF0001"]),
    ("CRTLIB LIB(X) ASP(33)", &["CPD0085", "CPF0001"]),
    ("CRTLIB LIB(X) ASP(1.5)", &["CPD0085", "CPF0001"]),
    ("CRTLIB LIB(X) ASP(abc)", &["CPD0076", "CPF0001"]),
    ("CRTLIB LIB(X) TEXT('it''s a (lib)')", &[]),
    ("CRTLIB LIB(X) TEXT(*BLANK)", &[]),
    ("CRTLIB LIB(X) AUT(*ALL)", &[]),
    ("CRTLIB LIB(X) AUT(*NOPE)", &["CPD0078", "CPF0001"]),
    ("CRTLIB LIB(X) AUT(PAYROLL)", &[]),
    ("CRTLIB LIB(X) TEXT(", &["CPD0014", "CPF0001"]),
    ("CRTLIB LIB(X) TEXT('abc)", &["CPD0014", "CPF0001"]),
    ("CRTLIB LIB(X) LIB", &["CPD0011", "CPF0001"]),
    ("CRTLIBX LIB(X)", &["CPD0030", "CPF0001"]),
    ("QGPL/CRTLIB LIB(X)", &["CPD0030", "CPF0001"]),
    ("CRTLIB LIB(&X)", &["CPD0103", "CPF0001"]),
    ("CPYF FROMFILE(QGPL/A) TOFILE(QGPL/B) MBROPT(*ADD)", &[]),
    ("CPYF A B", &[]),
    ("CPYF (A QGPL) *PRINT", &[]),
    ("CPYF FROMFILE(A/B/C)", &["CPD0045", "CPF0001"]),
    (
        "CPYF FROMFILE(QGPL/A) INCREL((*IF NAME *EQ 'x') (*AND AGE *GT 5))",
        &[],
    ),
    (
        "CPYF FROMFILE(QGPL/A) INCREL((*IF NAME *EQ 'x') (*XOR AGE *GT 5))",
        &["CPD0084", "CPF0001"],
    ),
    (
        "CPYF FROMFILE(QGPL/A) INCREL((*IF NAME))",
        &["CPD0072", "CPF0001"],
    ),
    ("CPYF FROMFILE(QGPL/A) INCREL(*NONE)", &[]),
    ("CPYF FROMFILE(QGPL/A) FMTOPT(*MAP *DROP)", &[]),
    (
        "CPYF FROMFILE(QGPL/A) FMTOPT(*MAP *DROP *NOCHK)",
        &["CPD0016", "CPF0001"],
    ),
    ("CPYF FROMFILE(QGPL/A) FROMRCD(0)", &["CPD0085", "CPF0001"]),
    ("CPYF FROMFILE(QGPL/A) NBRRCDS(0)", &["CPD0086", "CPF0001"]),
    ("CPYF FROMFILE(QGPL/A) NBRRCDS(*END)", &[]),
    ("CPYF FROMFILE(QGPL/A) ERRLVL(*NOMAX)", &[]),
    (
        "CPYF FROMFILE(QGPL/A) TOFILE(QGPL/B) MBROPT(*NONE) CRTFILE(*YES)",
        &["CPD2918", "CPF0001"],
    ),
    (
        "CPYF FROMFILE(QGPL/A) TOFILE(QGPL/B) MBROPT(*ADD) CRTFILE(*YES)",
        &[],
    ),
    ("CPYF FROMFILE(QGPL/A) FROMRCD(5)", &["CPD2919", "CPF0001"]),
    ("CPYF FROMFILE(QGPL/A) FROMRCD(5) NBRRCDS(2)", &[]),
    ("CPYF FROMFILE(QGPL/A) TOFILE(*PRINT)", &[]),
    (
        "CPYF FROMFILE(QGPL/A) INCREL((*IF NAME *EQ ('a' *CAT 'b')))",
        &[],
    ),
    (
        "CPYF FROMFILE(QGPL/A) INCREL((*IF NAME *EQ 'a' *CAT 'b'))",
        &["CPD0016", "CPF0001"],
    ),
    (
        "SBMJOB CMD(CALL PGM(X))",
        &["CPD0030", "CPD0084", "CPF0001"],
    ),
    ("SBMJOB CMD(CRTLIB LIB(X)) JOB(NIGHT)", &[]),
    (
        "SBMJOB CMD(CRTLIB LIB(X) TYPE(*Q)) JOB(NIGHT)",
        &["CPD0084", "CPD0084", "CPF0001"],
    ),
    (
        "SBMJOB CMD(CRTLIB LIB(X)) JOBD(QGPL/QBATCH) JOBQ(*JOBD) JOBPTY(5)",
        &[],
    ),
    (
        "SBMJOB CMD(CRTLIB LIB(X)) JOBPTY(10)",
        &["CPD0085", "CPF0001"],
    ),
    ("SBMJOB CMD(CRTLIB LIB(X)) LOG(4 00 *SECLVL)", &[]),
    (
        "SBMJOB CMD(CRTLIB LIB(X)) LOG(5 00 *SECLVL)",
        &["CPD0085", "CPF0001"],
    ),
    (
        "SBMJOB CMD(CRTLIB LIB(X)) LOG(4 00 *SECLVL *MORE)",
        &["CPD0016", "CPF0001"],
    ),
    ("SBMJOB CMD(CRTLIB LIB(X)) INLLIBL(QGPL QTEMP MYLIB)", &[]),
    ("SBMJOB CMD(CRTLIB LIB(X)) INLLIBL(*NONE)", &[]),
    (
        "SBMJOB CMD(CRTLIB LIB(X)) SCDDATE(123126) SCDTIME(2330)",
        &[],
    ),
    (
        "SBMJOB CMD(CRTLIB LIB(X)) SCDDATE(133126)",
        &["CPD0082", "CPF0001"],
    ),
    (
        "SBMJOB CMD(CRTLIB LIB(X)) HOLD(*MAYBE)",
        &["CPD0084", "CPF0001"],
    ),
    ("SBMJOB CMD(CRTLIB LIB(X)) MSGQ(*NONE)", &[]),
    ("SBMJOB CMD(CRTLIB LIB(X)) RTGDTA('abc def')", &[]),
    ("SBMJOB CMD(CRTLIB LIB(X)) JOB(*JOBD)", &[]),
    (
        "?SBMJOB ??CMD(CRTLIB LIB(X)) ?*JOB(X) ?-LOG ?<HOLD(*YES) ?&JOBPTY(3)",
        &[],
    ),
    (
        "SBMJOB CMD(CRTLIB LIB(X)) JOB(A) JOB(B)",
        &["CPD0042", "CPF0001"],
    ),
];

#[test]
fn table() {
    let mut src = source();
    let mut env = Env::new(&mut src);
    let mut failed = Vec::new();
    for (text, want) in TABLE {
        let got: Vec<String> = match analyze(&mut env, text) {
            Ok(_) => Vec::new(),
            Err(e) => e.msgids().iter().map(|s| (*s).to_string()).collect(),
        };
        if got != *want {
            failed.push(format!("{text}\n   want {want:?}\n   got  {got:?}"));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn bound_values() {
    let mut src = source();
    let mut env = Env::new(&mut src);
    let r = analyze(&mut env, "crtlib testlib").unwrap();
    assert_eq!(r.name, "CRTLIB");
    assert_eq!(r.lib, "QSYS");
    let lib = r.get("LIB").unwrap();
    assert!(lib.specified);
    assert_eq!(lib.value, Some(Value::Char("TESTLIB".into())));
    let ty = r.get("TYPE").unwrap();
    assert!(!ty.specified);
    assert_eq!(ty.value, Some(Value::Char("*PROD".into())));
    assert_eq!(r.get("TEXT").unwrap().value, Some(Value::Char(" ".into())));
    assert_eq!(r.get("ASP").unwrap().value.as_ref().unwrap().source(), "1");
    assert_eq!(r.text(), "CRTLIB LIB(TESTLIB)");

    let r = analyze(
        &mut env,
        "CPYF A B INCREL((*IF NAME *EQ 'x') (*AND AGE *GT 5))",
    )
    .unwrap();
    assert_eq!(
        r.get("FROMFILE").unwrap().value,
        Some(Value::Qual(vec![
            Value::Char("A".into()),
            Value::Char("*LIBL".into())
        ]))
    );
    let rel = r.get("INCREL").unwrap().value.as_ref().unwrap();
    assert_eq!(rel.source(), "(*IF NAME *EQ 'x') (*AND AGE *GT 5)");
    assert_eq!(
        r.get("FROMRCD").unwrap().value.as_ref().unwrap().source(),
        "0"
    );

    let r = analyze(
        &mut env,
        "SBMJOB CMD(CRTLIB LIB(X) TYPE(*TEST)) LOG(2 *N *MSG) SCDDATE(123126) SCDTIME(2330)",
    )
    .unwrap();
    assert_eq!(
        r.get("CMD").unwrap().value.as_ref().unwrap().text(),
        Some("CRTLIB LIB(X) TYPE(*TEST)")
    );
    assert_eq!(
        r.get("LOG").unwrap().value.as_ref().unwrap().source(),
        "(2 0 *MSG)"
    );
    assert_eq!(
        r.get("SCDDATE").unwrap().value.as_ref().unwrap().text(),
        Some("1261231")
    );
    assert_eq!(
        r.get("SCDTIME").unwrap().value.as_ref().unwrap().text(),
        Some("233000")
    );
    assert_eq!(
        r.get("JOBPTY").unwrap().value.as_ref().unwrap().source(),
        "0"
    );
    assert_eq!(
        r.get("INLLIBL").unwrap().value,
        Some(Value::Char("*JOBD".into()))
    );

    let r = analyze(
        &mut env,
        "?SBMJOB ??CMD(CRTLIB LIB(X)) ?*JOB(X) ?-LOG ?<HOLD(*YES) ?&JOBPTY(3)",
    )
    .unwrap();
    assert!(r.prompt && r.selective());
    assert_eq!(r.get("CMD").unwrap().sel, Some(Sel::Prompt));
    assert_eq!(r.get("JOB").unwrap().sel, Some(Sel::Show));
    assert_eq!(r.get("LOG").unwrap().sel, Some(Sel::Omit));
    assert_eq!(r.get("HOLD").unwrap().sel, Some(Sel::Default));
    assert_eq!(r.get("JOBPTY").unwrap().sel, Some(Sel::Hidden));
}

#[test]
fn settings_and_limits() {
    let mut src = source();
    let mut env = Env::new(&mut src);
    env.limited = true;
    assert_eq!(
        analyze(&mut env, "CRTLIB X").unwrap_err().msgid(),
        "CPD0043"
    );
    env.limited = false;
    env.mode = cmd::def::Mode::DEBUG;
    assert!(analyze(&mut env, "CRTLIB X").is_ok());
    let mut src = MemSource::default();
    src.add(
        "BATCHONLY",
        " CMD PROMPT('x') ALLOW(*BATCH *BPGM) MODE(*PROD)\n PARM KWD(A) TYPE(*CHAR)\n",
    )
    .unwrap();
    let mut env = Env::new(&mut src);
    assert_eq!(
        analyze(&mut env, "BATCHONLY").unwrap_err().msgid(),
        "CPD0031"
    );
    env.setting = Setting {
        interactive: false,
        ..Setting::default()
    };
    assert!(analyze(&mut env, "BATCHONLY").is_ok());
    env.mode = cmd::def::Mode::DEBUG;
    assert_eq!(
        analyze(&mut env, "BATCHONLY").unwrap_err().msgid(),
        "CPD0032"
    );
}

#[test]
fn variables_in_a_program() {
    use cmd::analyze::{VarInfo, VarType};
    let mut src = source();
    let vars = |name: &str| -> Option<VarInfo> {
        match name {
            "LIB" => Some(VarInfo {
                ty: VarType::Char,
                len: 10,
                dec: 0,
            }),
            "N" => Some(VarInfo {
                ty: VarType::Dec,
                len: 5,
                dec: 0,
            }),
            _ => None,
        }
    };
    let mut env = Env::new(&mut src);
    env.vars = Some(&vars);
    env.setting.program = true;
    let r = analyze(&mut env, "CRTLIB LIB(&LIB) ASP(&N)").unwrap();
    assert_eq!(r.get("LIB").unwrap().value, Some(Value::Var("LIB".into())));
    assert_eq!(r.get("ASP").unwrap().value, Some(Value::Var("N".into())));
    assert_eq!(
        analyze(&mut env, "CRTLIB LIB(&N)").unwrap_err().msgid(),
        "CPD0084"
    );
    assert_eq!(
        analyze(&mut env, "CRTLIB LIB(&NOPE)").unwrap_err().msgid(),
        "CPD0727"
    );
    let r = analyze(&mut env, "CPYF A B INCREL((*IF NAME *EQ (&LIB *CAT 'x')))").unwrap();
    let rel = r.get("INCREL").unwrap().value.as_ref().unwrap();
    assert!(
        matches!(rel, Value::List(items) if matches!(&items[0], Value::Elems(e) if matches!(e[3], Some(Value::Expr(_)))))
    );
}
