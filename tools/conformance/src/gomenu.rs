// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 165: `GO` of each kind of menu over TN5250. A `TYPE(*PGM)`
//! menu calls its program with the menu's name, the library it was found
//! in and a return code (pub400: 0 calls it again, -1 leaves); a
//! `TYPE(*DSPF)` menu shows its display file's record with the command
//! line at row 22, and option 1 runs message `USR0001`'s text. A menu
//! that is not there is `CPF9801`; one whose program is missing says
//! something else.

use tn5250_driver::script::Context;
use tn5250_driver::session::Session as Display;
use tn5250_driver::{Connect, ScriptOptions};

use crate::phase8::{connect, run};
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

const LIB: &str = "MNU165";

/// The step's user and password.
const USER: &str = "MNU165U";
const PASSWORD: &str = "MNU165PW";

/// The `*PGM` menu's program: what it was passed into `SEEN`, the calls
/// counted in `CALLS`; 0 the first time (call again), -1 the second.
const MENUPGM: &str = "
             PGM        PARM(&MENU &MLIB &RC)
             DCL        VAR(&MENU) TYPE(*CHAR) LEN(10)
             DCL        VAR(&MLIB) TYPE(*CHAR) LEN(10)
             DCL        VAR(&RC) TYPE(*INT) LEN(2)
             DCL        VAR(&N) TYPE(*DEC) LEN(3 0)
             RTVDTAARA  DTAARA(MNU165/CALLS) RTNVAR(&N)
             CHGVAR     VAR(&N) VALUE(&N + 1)
             CHGDTAARA  DTAARA(MNU165/CALLS) VALUE(&N)
             CHGDTAARA  DTAARA(MNU165/SEEN) VALUE(&MENU *CAT &MLIB)
             IF         COND(&N *LT 2) THEN(CHGVAR VAR(&RC) VALUE(0))
             ELSE       CMD(CHGVAR VAR(&RC) VALUE(-1))
             ENDPGM
";

/// The `*DSPF` menu's display file: one record named like the file.
const MENUDSP: &str = "
     A                                      DSPSIZ(24 80 *DS3)
     A                                      INDARA
     A          R MENUDSP
     A                                  1  2'Step 165 menu'
     A                                  3  5'1. Mark the data area'
";

/// The screen as text, one row a line.
fn screen(d: &Display) -> String {
    let mut out = String::new();
    for row in d.term.text_rows() {
        out.push('|');
        out.push_str(&row);
        out.push_str("|\n");
    }
    out
}

/// A console command whose output must hold `want`.
fn expect(s: &mut Session, line: &str, want: &str) -> Result<String, String> {
    let out = s.command(line, CMD)?;
    if out.contains(want) && !out.contains("not created") {
        Ok(out)
    } else {
        Err(format!("{line}: no {want:?} in:\n{out}"))
    }
}

/// A data area's value as `dtaara get` shows it.
fn dtaara(s: &mut Session, name: &str) -> Result<String, String> {
    s.command(&format!("svc.dtaara get {LIB}/{name}"), CMD)
}

/// GO typed on the main menu's command line.
fn go(d: &mut Display, ctx: &mut Context, menu: &str) -> Result<(), String> {
    // A GO that failed leaves its command on the line: erase it first.
    run(
        d,
        ctx,
        &format!("key eraseinput\ntype go {menu}\nkey enter\nsettle 2500\n"),
    )
}

impl Suite {
    /// Step 165.
    #[allow(clippy::too_many_lines)] // one block per kind of menu
    pub(crate) fn go_menus(&self, s: &mut Session) -> Result<String, String> {
        let port = self.telnet_port().ok_or("the launch has no telnet port")?;
        let made = s.command(
            &format!("CRTUSRPRF USRPRF({USER}) PASSWORD({PASSWORD}) PWDEXP(*NO)"),
            CMD,
        )?;
        if !made.contains("created") && !made.contains("CPF2214") {
            return Err(format!("CRTUSRPRF {USER}: {made}"));
        }
        let _ = s.command(&format!("DLTLIB LIB({LIB})"), CMD);
        expect(s, &format!("CRTLIB LIB({LIB})"), "created")?;
        expect(s, &format!("CRTSRCPF FILE({LIB}/QCLSRC)"), "created")?;
        expect(s, &format!("CRTSRCPF FILE({LIB}/QDDSSRC)"), "created")?;
        for (name, ty, text) in [("MENUPGM", "CLLE", MENUPGM), ("MENUDSP", "DSPF", MENUDSP)] {
            crate::qir::qemu::push(s, &format!("{name}S"), text.trim_start_matches('\n'))?;
            let file = if ty == "CLLE" { "QCLSRC" } else { "QDDSSRC" };
            expect(
                s,
                &format!("svc.srcput {LIB}/{file} {name} SRCTYPE({ty}) FROMSTMF({name}S)"),
                "written",
            )?;
        }
        expect(
            s,
            &format!("CRTBNDCL PGM({LIB}/MENUPGM) SRCFILE({LIB}/QCLSRC)"),
            "created",
        )?;
        expect(
            s,
            &format!("CRTDSPF FILE({LIB}/MENUDSP) SRCFILE({LIB}/QDDSSRC)"),
            "created",
        )?;
        expect(
            s,
            &format!("CRTDTAARA DTAARA({LIB}/CALLS) TYPE(*DEC) LEN(3 0) VALUE(0)"),
            "created",
        )?;
        expect(
            s,
            &format!("CRTDTAARA DTAARA({LIB}/SEEN) TYPE(*CHAR) LEN(20)"),
            "created",
        )?;
        expect(s, &format!("CRTMSGF MSGF({LIB}/MENUMSG)"), "created")?;
        let _ = s.command(
            &format!(
                "ADDMSGD MSGID(USR0001) MSGF({LIB}/MENUMSG) MSG('CHGDTAARA DTAARA({LIB}/SEEN) VALUE(OPTION1)')"
            ),
            CMD,
        )?;
        for (menu, kind) in [
            ("MENUPGM", format!("TYPE(*PGM) PGM({LIB}/MENUPGM)")),
            (
                "MENUDSP",
                format!("TYPE(*DSPF) DSPF({LIB}/MENUDSP) MSGF({LIB}/MENUMSG)"),
            ),
            ("NOPGM", format!("TYPE(*PGM) PGM({LIB}/NOSUCH)")),
        ] {
            expect(s, &format!("CRTMNU MENU({LIB}/{menu}) {kind}"), "created")?;
        }
        // The step's user may use and change all of it.
        let mut grants = vec![format!(
            "GRTOBJAUT OBJ({LIB}) OBJTYPE(*LIB) USER({USER}) AUT(*ALL)"
        )];
        for (obj, ty) in [
            ("CALLS", "*DTAARA"),
            ("SEEN", "*DTAARA"),
            ("MENUPGM", "*PGM"),
            ("MENUDSP", "*FILE"),
            ("MENUMSG", "*MSGF"),
            ("MENUPGM", "*MENU"),
            ("MENUDSP", "*MENU"),
            ("NOPGM", "*MENU"),
        ] {
            grants.push(format!(
                "GRTOBJAUT OBJ({LIB}/{obj}) OBJTYPE({ty}) USER({USER}) AUT(*ALL)"
            ));
        }
        for grant in grants {
            expect(s, &grant, "given")?;
        }

        let c = Connect::qs400(port, USER, PASSWORD);
        let options = ScriptOptions {
            golden: None,
            update: false,
            release: String::new(),
            timeout_ms: 20000,
        };
        let mut ctx = Context::new(&c, &options);
        let mut d = connect(port, USER, PASSWORD)?;
        run(&mut d, &mut ctx, "wait \"Sign On\"\nsignon\nsettle 1500\n")?;
        if d.term.contains("Sign-on Information") {
            run(&mut d, &mut ctx, "key enter\nsettle 1000\n")?;
        }
        run(&mut d, &mut ctx, "wait \"Main Menu\"\n")?;

        // *PGM, through the library list: the real library is passed,
        // the program called again after 0 and left after -1.
        run(
            &mut d,
            &mut ctx,
            &format!("type addlible {LIB}\nkey enter\nsettle 1500\n"),
        )?;
        go(&mut d, &mut ctx, "MENUPGM")?;
        if !d.term.contains("Main Menu") {
            return Err(format!("GO MENUPGM did not come back:\n{}", screen(&d)));
        }
        let calls = dtaara(s, "CALLS")?;
        let n = calls
            .rsplit(':')
            .next()
            .and_then(|v| v.trim().parse::<i64>().ok());
        if n != Some(2) {
            return Err(format!("the menu program ran other than twice: {calls}"));
        }
        let seen = dtaara(s, "SEEN")?;
        if !seen.contains("MENUPGM") || !seen.contains(LIB) {
            return Err(format!("the menu program was passed other names: {seen}"));
        }

        // *DSPF: the record, the command line at row 22, option 1.
        go(&mut d, &mut ctx, &format!("{LIB}/MENUDSP"))?;
        let rows = d.term.text_rows();
        if !d.term.contains("Step 165 menu")
            || !rows.get(21).is_some_and(|r| r.starts_with(" ===>"))
        {
            return Err(format!("GO MENUDSP shows:\n{}", screen(&d)));
        }
        let shown = screen(&d);
        run(&mut d, &mut ctx, "type 1\nkey enter\nsettle 2000\n")?;
        let seen = dtaara(s, "SEEN")?;
        if !seen.contains("OPTION1") {
            return Err(format!("option 1 did not run USR0001's command: {seen}"));
        }
        run(&mut d, &mut ctx, "key f3\nsettle 1500\n")?;
        if !d.term.contains("Main Menu") {
            return Err(format!("F3 did not leave the menu:\n{}", screen(&d)));
        }

        // Missing: the menu (CPF9801); the program of one (not CPF9801).
        go(&mut d, &mut ctx, &format!("{LIB}/NOSUCH"))?;
        let missing = d.term.text_rows().last().cloned().unwrap_or_default();
        if !missing.contains("NOSUCH") {
            return Err(format!("GO of a missing menu said: {missing}"));
        }
        go(&mut d, &mut ctx, &format!("{LIB}/NOPGM"))?;
        let nopgm = d.term.text_rows().last().cloned().unwrap_or_default();
        if nopgm.trim().is_empty() || nopgm.contains("Menu NOPGM") {
            return Err(format!("GO of a menu without its program said: {nopgm:?}"));
        }
        run(&mut d, &mut ctx, "type signoff\nkey enter\nsettle 1000\n")?;
        let _ = s.command(&format!("DLTLIB LIB({LIB})"), CMD);
        Ok(format!(
            "*PGM menu called twice with MENUPGM and {LIB}; *DSPF menu:\n{shown}option 1 ran; missing menu: {}; missing program: {}",
            missing.trim(),
            nopgm.trim()
        ))
    }
}
