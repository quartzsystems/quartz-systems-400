// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Steps 146 to 148, the V1R1M0 items D4 and D3. 146: CL exit programs
//! on `QIBM_QSY_CRT_PROFILE`, `QIBM_QSY_DLT_PROFILE`,
//! `QIBM_QSY_CHK_PASSWRD` and `QIBM_QWC_PRERESTRICT` are called and
//! heard (`ENDSBS *ALL` refused `CPF18C3`). 147: the function usage
//! registry (`WRKFCNUSG`, `CHGFCNUSG`, `DSPFCNUSG`, `CPF228C`), the
//! security attributes (`CHGSECA`, `DSPSECA`) and `STRPASTHR` refused.
//! 148: `CMPJRNIMG` over a journaled file, `RCVJRNE` handing its entries
//! to a CL exit program, and `SAVSAVFDTA` writing a save file to a
//! virtual tape that `DSPTAP` reads and `RSTLIB` restores from.

use std::time::Duration;

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

const LONG: Duration = Duration::from_secs(90);

/// A profile exit program: the format and profile it is called with go
/// to the job log (`PRFEXIT saw CRTP0100NAME`).
const PRF_SRC: &str = "             PGM        PARM(&INFO)
             DCL        VAR(&INFO) TYPE(*CHAR) LEN(38)
             DCL        VAR(&SEEN) TYPE(*CHAR) LEN(18)
             CHGVAR     VAR(&SEEN) VALUE(%SST(&INFO 21 18))
             SNDPGMMSG  MSG('PRFEXIT saw' *BCAT &SEEN) TOPGMQ(*EXT)
             ENDPGM";

/// A password check exit program (`CHKP0100`): the profile to the job
/// log, the password accepted.
const CHK_SRC: &str = "             PGM        PARM(&INFO &RC)
             DCL        VAR(&INFO) TYPE(*CHAR) LEN(68)
             DCL        VAR(&RC) TYPE(*CHAR) LEN(1)
             DCL        VAR(&SEEN) TYPE(*CHAR) LEN(10)
             CHGVAR     VAR(&SEEN) VALUE(%SST(&INFO 33 10))
             SNDPGMMSG  MSG('CHKEXIT saw' *BCAT &SEEN) TOPGMQ(*EXT)
             CHGVAR     VAR(&RC) VALUE('0')
             ENDPGM";

/// A pre-restricted state program that says no on the check.
const PRER_SRC: &str = "             PGM        PARM(&RTN &FMT &ACT &DLY)
             DCL        VAR(&RTN) TYPE(*CHAR) LEN(12)
             DCL        VAR(&FMT) TYPE(*CHAR) LEN(8)
             DCL        VAR(&ACT) TYPE(*CHAR) LEN(1)
             DCL        VAR(&DLY) TYPE(*CHAR) LEN(4)
             IF         COND(&ACT *EQ '1') THEN(CHGVAR VAR(&RTN) +
                          VALUE(X'0000000C0000000000000000'))
             ENDPGM";

/// RCVJRNE's exit program: each entry's code and type to the job log.
const RCV_SRC: &str = "             PGM        PARM(&ENTRY &IND)
             DCL        VAR(&ENTRY) TYPE(*CHAR) LEN(300)
             DCL        VAR(&IND) TYPE(*CHAR) LEN(3)
             DCL        VAR(&T) TYPE(*CHAR) LEN(3)
             IF         COND(%SST(&IND 1 1) *EQ '1') THEN(DO)
             CHGVAR     VAR(&T) VALUE(%SST(&ENTRY 16 3))
             SNDPGMMSG  MSG('RCVEXIT got' *BCAT &T) TOPGMQ(*EXT)
             ENDDO
             ENDPGM";

/// A service console command whose output holds every word of `want`.
fn expect(s: &mut Session, line: &str, want: &[&str]) -> Result<String, String> {
    let out = s.command(line, CMD)?;
    if want.iter().all(|w| out.contains(w)) {
        Ok(out)
    } else {
        Err(format!("{line}: no {want:?} in:\n{out}"))
    }
}

/// A CL program `QGPL/name` from `src`.
fn program(s: &mut Session, name: &str, src: &str) -> Result<(), String> {
    let _ = s.command("CRTSRCPF FILE(QGPL/QCLSRC)", CMD);
    let buf = format!("{name}SRC");
    crate::qir::qemu::push(s, &buf, src)?;
    expect(s, &format!("svc.srcput QGPL/QCLSRC {name} SRCTYPE(CLLE) FROMSTMF({buf})"), &["written"])?;
    let _ = s.command(&format!("DLTPGM PGM(QGPL/{name})"), CMD);
    let out = s.command(&format!("CRTBNDCL PGM(QGPL/{name}) SRCFILE(QGPL/QCLSRC) SRCMBR({name})"), LONG)?;
    if out.contains("CPF0C") || out.contains("not created") {
        return Err(format!("{name} did not compile:\n{out}"));
    }
    Ok(())
}

fn add(s: &mut Session, point: &str, format: &str, pgm: &str) -> Result<(), String> {
    let _ = s.command(&format!("RMVEXITPGM EXITPNT({point}) FORMAT({format}) PGMNBR(1)"), CMD);
    expect(
        s,
        &format!("ADDEXITPGM EXITPNT({point}) FORMAT({format}) PGMNBR(1) PGM(QGPL/{pgm})"),
        &["added to exit point"],
    )
    .map(|_| ())
}

fn remove(s: &mut Session, point: &str, format: &str) {
    let _ = s.command(&format!("RMVEXITPGM EXITPNT({point}) FORMAT({format}) PGMNBR(1)"), CMD);
}

#[allow(clippy::unused_self)] // the steps share one signature
impl Suite {
    /// Step 146: the security and work management exit points.
    pub(crate) fn exit_points(&self, s: &mut Session) -> Result<String, String> {
        let points = s.command("WRKREGINF EXITPNT(QIBM_QSY*)", CMD)?;
        for p in ["QIBM_QSY_VLD_PASSWRD", "QIBM_QSY_CRT_PROFILE", "QIBM_QSY_DLT_PROFILE", "QIBM_QSY_CHK_PASSWRD"] {
            if !points.contains(p) {
                return Err(format!("WRKREGINF has no {p}:\n{points}"));
            }
        }
        program(s, "PRFEXIT", PRF_SRC)?;
        program(s, "CHKEXIT", CHK_SRC)?;
        program(s, "PREREXIT", PRER_SRC)?;
        add(s, "QIBM_QSY_CRT_PROFILE", "CRTP0100", "PRFEXIT")?;
        add(s, "QIBM_QSY_DLT_PROFILE", "DLTP0100", "PRFEXIT")?;
        add(s, "QIBM_QSY_CHK_PASSWRD", "CHKP0100", "CHKEXIT")?;
        let _ = s.command("DLTUSRPRF USRPRF(EXITUSR1)", CMD);
        // The program itself, called by hand: its data area takes what it saw.
        expect(
            s,
            "CALL PGM(QGPL/PRFEXIT) PARM('QIBM_QSY_CRT_PROFILECRTP0100BYHAND    ')",
            &["PRFEXIT saw CRTP0100BYHAND"],
        )?;
        let r = (|| -> Result<String, String> {
            expect(s, "CRTUSRPRF USRPRF(EXITUSR1) PASSWORD(Exit146pw) PWDEXP(*NO)", &["EXITUSR1"])?;
            expect(s, "DLTUSRPRF USRPRF(EXITUSR1)", &["EXITUSR1"])?;
            expect(
                s,
                "dspjoblog",
                &["PRFEXIT saw CRTP0100EXITUSR1", "CHKEXIT saw EXITUSR1", "PRFEXIT saw DLTP0100EXITUSR1"],
            )
        })();
        remove(s, "QIBM_QSY_CRT_PROFILE", "CRTP0100");
        remove(s, "QIBM_QSY_DLT_PROFILE", "DLTP0100");
        remove(s, "QIBM_QSY_CHK_PASSWRD", "CHKP0100");
        if let Err(e) = r {
            let log = s.command("dspjoblog", CMD).unwrap_or_default();
            let tail: Vec<&str> = log.lines().rev().take(30).collect();
            let tail: Vec<&str> = tail.into_iter().rev().collect();
            return Err(format!("{e}\njob log:\n{}", tail.join("\n")));
        }
        // A pre-restricted state program that says no stops ENDSBS *ALL.
        add(s, "QIBM_QWC_PRERESTRICT", "PRER0100", "PREREXIT")?;
        let ended = s.command("ENDSBS SBS(*ALL) OPTION(*IMMED)", LONG);
        remove(s, "QIBM_QWC_PRERESTRICT", "PRER0100");
        let ended = ended?;
        if !ended.contains("CPF18C3") {
            return Err(format!("ENDSBS *ALL went on past a PRER0100 program that said no:\n{ended}"));
        }
        let sbs = s.command("WRKSBS", CMD)?;
        if !sbs.contains("QINTER") && !sbs.contains("QBASE") && !sbs.contains("QCTL") {
            return Err(format!("the subsystems ended anyway:\n{sbs}"));
        }
        Ok("CRTP0100, DLTP0100 and CHKP0100 programs logged EXITUSR1; a PRER0100 program's no stopped ENDSBS *ALL (CPF18C3)".into())
    }

    /// Step 147: function usage, security attributes, pass-through.
    pub(crate) fn function_usage(&self, s: &mut Session) -> Result<String, String> {
        expect(s, "WRKFCNUSG", &["QIBM_DB_SQLADM", "QIBM_ACCESS_ALLOBJ_JOBLOG", "QIBM_QTMF_SERVER_REQ_10"])?;
        let _ = s.command("DLTUSRPRF USRPRF(FCNUSR1)", CMD);
        expect(s, "CRTUSRPRF USRPRF(FCNUSR1) PASSWORD(*NONE)", &["FCNUSR1"])?;
        let r = (|| -> Result<(), String> {
            s.command("CHGFCNUSG FCNID(QIBM_DB_SQLADM) USER(FCNUSR1) USAGE(*ALLOWED)", CMD)?;
            expect(s, "DSPFCNUSG FCNID(QIBM_DB_SQLADM)", &["FCNUSR1", "*ALLOWED", "*DENIED", "*NOTUSED"])?;
            s.command("CHGFCNUSG FCNID(QIBM_DB_SQLADM) USER(FCNUSR1) USAGE(*NONE)", CMD)?;
            let after = s.command("DSPFCNUSG FCNID(QIBM_DB_SQLADM)", CMD)?;
            if after.contains("FCNUSR1") {
                return Err(format!("USAGE(*NONE) left FCNUSR1:\n{after}"));
            }
            expect(s, "CHGFCNUSG FCNID(QIBM_NOSUCH_FUNCTION) DEFAULT(*ALLOWED)", &["CPF228C"])?;
            Ok(())
        })();
        let _ = s.command("DLTUSRPRF USRPRF(FCNUSR1)", CMD);
        r?;
        expect(s, "CHGSECA UID(5000) GID(6000)", &["5000", "6000"])?;
        let seca = expect(s, "DSPSECA", &["5000", "6000"])?;
        let pass = s.command("STRPASTHR RMTLOCNAME(QSNOSUCH)", CMD)?;
        if !pass.contains("CPF8933") && !pass.contains("CPD0031") && !pass.contains("not allowed") {
            return Err(format!("STRPASTHR was not refused:\n{pass}"));
        }
        let _ = seca;
        Ok("WRKFCNUSG listed the V7R1M0 functions; CHGFCNUSG gave and took FCNUSR1's QIBM_DB_SQLADM; CPF228C for an unknown ID; CHGSECA and DSPSECA at 5000/6000; STRPASTHR refused".into())
    }

    /// Step 148: `CMPJRNIMG`, `RCVJRNE` and `SAVSAVFDTA`.
    pub(crate) fn journal_and_savf_data(&self, s: &mut Session) -> Result<String, String> {
        let _ = s.command("DLTLIB LIB(RELJRN)", LONG);
        for (line, want) in [
            ("CRTLIB LIB(RELJRN)", "RELJRN"),
            ("CRTJRNRCV JRNRCV(RELJRN/RCV0001)", "RCV0001"),
            ("CRTJRN JRN(RELJRN/JRN) JRNRCV(RELJRN/RCV0001)", "JRN"),
            ("RUNSQL SQL('CREATE TABLE RELJRN.ITEMS (NAME CHAR(10))') COMMIT(*NONE)", ""),
            ("STRJRNPF FILE(RELJRN/ITEMS) JRN(RELJRN/JRN) IMAGES(*BOTH)", ""),
            ("RUNSQL SQL('INSERT INTO RELJRN.ITEMS VALUES(''APPLE'')') COMMIT(*NONE)", ""),
            ("RUNSQL SQL('UPDATE RELJRN.ITEMS SET NAME = ''APRICOT''') COMMIT(*NONE)", ""),
        ] {
            let out = s.command(line, LONG)?;
            if !want.is_empty() && !out.contains(want) {
                return Err(format!("{line}:\n{out}"));
            }
        }
        expect(s, "CMPJRNIMG FILE(RELJRN/ITEMS)", &["QPCMPIMG"])?;
        expect(s, "WRKJRNRCV JRNRCV(RELJRN/*ALL)", &[])?;
        program(s, "RCVEXIT", RCV_SRC)?;
        let rcv = s.command("RCVJRNE JRN(RELJRN/JRN) EXITPGM(QGPL/RCVEXIT) TOENT(*LAST)", LONG)?;
        let log = s.command("dspjoblog", CMD)?;
        let got = log.matches("RCVEXIT got").count();
        if got == 0 || !log.contains("RCVEXIT got RUP") {
            return Err(format!("RCVJRNE handed the program no update entry:\n{rcv}\n{log}"));
        }
        // SAVSAVFDTA to a virtual tape; DSPTAP shows the library.
        for (line, want) in [
            ("CRTSAVF FILE(RELJRN/SAVED)", ""),
            ("SAVLIB LIB(RELJRN) DEV(*SAVF) SAVF(RELJRN/SAVED)", "RELJRN"),
            ("CRTDEVTAP DEVD(Q148TAP) RSRCNAME(*VRT)", ""),
            ("CRTIMGCLG IMGCLG(Q148TAP) DIR('/q148/tap') TYPE(*TAP)", ""),
            ("ADDIMGCLGE IMGCLG(Q148TAP) FROMFILE(*NEW) TOFILE(VOL1) VOLNAM(Q148V1) IMGSIZ(8)", ""),
            ("LODIMGCLG IMGCLG(Q148TAP) DEV(Q148TAP)", ""),
            ("VRYCFG CFGOBJ(Q148TAP) CFGTYPE(*DEV) STATUS(*ON)", ""),
            ("INZTAP DEV(Q148TAP) NEWVOL(Q148V1) CHECK(*NO)", ""),
        ] {
            let out = s.command(line, LONG)?;
            if !want.is_empty() && !out.contains(want) {
                return Err(format!("{line}:\n{out}"));
            }
        }
        expect(s, "SAVSAVFDTA SAVF(RELJRN/SAVED) DEV(Q148TAP)", &["Q148V1"])?;
        expect(s, "DSPTAP DEV(Q148TAP) DATA(*LABELS)", &["RELJRN"])?;
        let _ = s.command("DLTLIB LIB(RELJRN)", LONG);
        expect(s, "RSTLIB SAVLIB(RELJRN) DEV(Q148TAP)", &["RELJRN"])?;
        expect(s, "RUNSQL SQL('SELECT * FROM RELJRN.ITEMS') COMMIT(*NONE)", &[])?;
        Ok(format!(
            "CMPJRNIMG printed QPCMPIMG; RCVJRNE fed a CL program {got} entries; SAVSAVFDTA wrote RELJRN to Q148TAP, DSPTAP showed it, RSTLIB restored it"
        ))
    }
}
