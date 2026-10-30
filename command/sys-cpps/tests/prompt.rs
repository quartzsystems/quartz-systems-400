// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The first prompt display of the shipped commands, against golden
//! files under `tests/golden/`: the layout rules of the prompter over
//! real definitions. `QS400_UPDATE_GOLDEN=1` rewrites them. The
//! goldens are this system's own renders; the real box's captures
//! are an open question (`docs/CMD_OPEN_QUESTIONS.md`).

use std::path::PathBuf;

use cmd::analyze::{Env, MemSource, analyze};
use cmd::prompt::{Prompter, Step};

/// The commands prompted and the string each prompt starts from.
const PROMPTED: &[(&str, &str)] = &[
    ("crtpf", "CRTPF FILE(QGPL/CUST) RCDLEN(80)"),
    ("crtlf", "CRTLF FILE(QGPL/CUSTL1)"),
    ("crtclpgm", "CRTCLPGM PGM(QGPL/HELLO) SRCFILE(QGPL/QCLSRC)"),
    ("crtbndcl", "CRTBNDCL PGM(QGPL/HELLO)"),
    ("crtbndrpg", "CRTBNDRPG PGM(QGPL/HELLO)"),
    ("crtrpgmod", "CRTRPGMOD MODULE(QGPL/HELLO)"),
    ("crtsqlrpgi", "CRTSQLRPGI OBJ(QGPL/HELLO)"),
    ("sbmjob", "SBMJOB CMD(CALL QGPL/HELLO) JOB(HELLO)"),
    ("chgjob", "CHGJOB"),
    ("crtusrprf", "CRTUSRPRF USRPRF(ANNE) PASSWORD(SECRET)"),
    ("grtobjaut", "GRTOBJAUT OBJ(QGPL/CUST) OBJTYPE(*FILE) USER(ANNE) AUT(*USE)"),
    ("cpyf", "CPYF FROMFILE(QGPL/CUST) TOFILE(QGPL/CUST2)"),
    ("wrkactjob", "WRKACTJOB"),
    ("ovrdbf", "OVRDBF FILE(CUST) TOFILE(QGPL/CUST2)"),
    ("dspfd", "DSPFD FILE(QGPL/CUST)"),
    ("sndpgmmsg", "SNDPGMMSG MSG('hello')"),
    ("monmsg", "MONMSG MSGID(CPF0000)"),
    ("crtcmd", "CRTCMD CMD(QGPL/MYCMD) PGM(QGPL/MYPGM)"),
    ("addrtge", "ADDRTGE SBSD(QGPL/MYSBS) SEQNBR(10) CMPVAL(QCMDI) PGM(QSYS/QCMD)"),
    ("strjrnpf", "STRJRNPF FILE(QGPL/CUST) JRN(QGPL/CUSTJRN)"),
    ("addjobscde", "ADDJOBSCDE JOB(NIGHTLY) CMD(CALL QGPL/NIGHT) FRQ(*WEEKLY)"),
    ("chgsysval", "CHGSYSVAL SYSVAL(QINACTITV) VALUE('60')"),
    ("dspobjaut", "DSPOBJAUT OBJ(QGPL/CUST) OBJTYPE(*FILE)"),
    ("wrksplf", "WRKSPLF"),
    ("crtjrnrcv", "CRTJRNRCV JRNRCV(QGPL/CUSTRCV)"),
    ("rgzpfm", "RGZPFM FILE(QGPL/CUST)"),
    ("call", "CALL PGM(QGPL/HELLO) PARM('x' 12)"),
    ("pwrdwnsys", "PWRDWNSYS"),
    ("crtlib", "CRTLIB LIB(TESTLIB)"),
    ("dltlib", "DLTLIB LIB(TESTLIB)"),
    ("crtdtaara", "CRTDTAARA DTAARA(QGPL/FLAG) TYPE(*CHAR) LEN(10)"),
    ("chgdtaara", "CHGDTAARA DTAARA(QGPL/FLAG) VALUE('on')"),
    ("dspdtaara", "DSPDTAARA DTAARA(QGPL/FLAG)"),
    ("crtjobd", "CRTJOBD JOBD(QGPL/NIGHT)"),
    ("crtsbsd", "CRTSBSD SBSD(QGPL/MYSBS) POOLS((1 *BASE))"),
    ("strsbs", "STRSBS SBSD(QGPL/MYSBS)"),
    ("endsbs", "ENDSBS SBS(MYSBS)"),
    ("crtoutq", "CRTOUTQ OUTQ(QGPL/REPORTS)"),
    ("strprtwtr", "STRPRTWTR DEV(QSCAPTURE)"),
    ("crtmsgq", "CRTMSGQ MSGQ(QGPL/MYQ)"),
    ("sndmsg", "SNDMSG MSG('note') TOUSR(*SYSOPR)"),
    ("rcvmsg", "RCVMSG MSGTYPE(*ANY)"),
    ("dspjoblog", "DSPJOBLOG"),
    ("chgusrprf", "CHGUSRPRF USRPRF(ANNE)"),
    ("crtautl", "CRTAUTL AUTL(PAYROLL)"),
    ("dspaudjrne", "DSPAUDJRNE ENTTYP(AF)"),
    ("runsqlstm", "RUNSQLSTM SRCFILE(QGPL/QSQLSRC) SRCMBR(SETUP)"),
    ("crtmsgf", "CRTMSGF MSGF(QGPL/MYMSGF)"),
    ("addmsgd", "ADDMSGD MSGID(USR0001) MSGF(QGPL/MYMSGF) MSG('hello &1')"),
    ("crtpgm", "CRTPGM PGM(QGPL/HELLO)"),
    ("strdbg", "STRDBG PGM(QGPL/HELLO)"),
    ("dspjob", "DSPJOB"),
    ("chglibl", "CHGLIBL LIBL(QGPL QTEMP)"),
    ("addrdbdire", "ADDRDBDIRE RDB(REMOTE1) RMTLOCNAME(remote.example.com *IP)"),
    ("chgddmtcpa", "CHGDDMTCPA"),
];

fn source() -> MemSource {
    let mut s = MemSource::default();
    for (name, _) in sys_cpps::defs::DEFS {
        let o = sys_cpps::install::object(name, 0).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        s.put(o.def);
    }
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

#[test]
fn first_displays() {
    let mut src = source();
    let mut env = Env::new(&mut src);
    env.setting.program = true;
    assert!(PROMPTED.len() >= 40);
    for (name, text) in PROMPTED {
        let r = analyze(&mut env, text).unwrap_or_else(|e| panic!("{text}: {e}"));
        let p = Prompter::new(&r);
        let screen = p.screen();
        let rendered = screen.text();
        assert_eq!(rendered.lines().count(), 24, "{name}: 24 rows");
        assert!(
            rendered.lines().all(|l| l.chars().count() == 80),
            "{name}: 80 columns"
        );
        golden(&format!("{name}-first"), &rendered);
    }
}

/// `Enter` on an unchanged display gives back a command that analyzes
/// to the same values.
#[test]
fn enter_round_trips() {
    let mut src = source();
    for (name, text) in PROMPTED {
        let r = {
            let mut env = Env::new(&mut src);
            env.setting.program = true;
            analyze(&mut env, text).unwrap_or_else(|e| panic!("{text}: {e}"))
        };
        let mut p = Prompter::new(&r);
        let fields: Vec<String> = p.screen().fields.iter().map(|f| f.value.clone()).collect();
        let mut env = Env::new(&mut src);
        env.setting.program = true;
        match p.key(0, &fields, &mut env) {
            Step::Done(line) => {
                let back = analyze(&mut env, &line).unwrap_or_else(|e| panic!("{name}: {line}: {e}"));
                assert_eq!(back.text(), r.text(), "{name}: {line}");
            }
            Step::Show => {
                // A required value the string did not give: the prompt
                // stays up with its message.
                let s = p.screen();
                assert!(!s.message.is_empty(), "{name}: shown again without a message");
            }
            Step::Cancel => panic!("{name}: cancelled"),
        }
    }
}
