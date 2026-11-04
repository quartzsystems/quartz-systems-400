// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Steps 120 to 122: national language support. 120 installs the
//! Spanish scaffold with `INSLNG` and a job that changes its language
//! finds the library ahead of `QSYS` and the hardware resource display
//! in Spanish; 121 puts the library at the front of `QSYSLIBL` and a new
//! session's sign-on is Spanish; 122 changes the job's date and time
//! formats and a display's date and time follow them.

use tn5250_driver::ScriptOptions;
use tn5250_driver::script::Context;

use crate::phase5::SECOFR_PASSWORD;
use crate::phase8::{connect, run, secofr};
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

fn want(out: &str, what: &str, words: &[&str]) -> Result<(), String> {
    if words.iter().all(|w| out.contains(w)) {
        Ok(())
    } else {
        Err(format!("{what}:\n{out}"))
    }
}

/// The screen's rows as one text.
fn screen(d: &tn5250_driver::session::Session) -> String {
    d.term.text_rows().join("\n")
}

/// The system library list as `QSYSLIBL` holds it.
const SYSLIBL: &str = "QSYS QSYS2 QHLPSYS QUSRSYS";

impl Suite {
    /// Step 120: `INSLNG LNG(2931)`, then a job with `LANGID(ESP)`.
    #[allow(clippy::many_single_char_names)]
    pub(crate) fn install_language(&self, s: &mut Session) -> Result<String, String> {
        let out = s.command("INSLNG LNG(2931)", CMD)?;
        want(&out, "INSLNG", &["2931", "QSYS2931"])?;
        let again = s.command("INSLNG LNG(2931)", CMD)?;
        want(&again, "INSLNG again", &["QSYS2931", " 0 "])?;
        let none = s.command("INSLNG LNG(2999)", CMD)?;
        want(&none, "INSLNG of a language not shipped", &["TXT0203"])?;
        let objs = s.command("svc.obj ls QSYS2931", CMD)?;
        want(&objs, "QSYS2931", &["QCPFMSG", "QSTXTMSG", "QDSIGNON"])?;
        let port = self.telnet_port().ok_or("the launch has no telnet port")?;
        let mut d = connect(port, "QSECOFR", SECOFR_PASSWORD)?;
        let c = tn5250_driver::Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
        let options = ScriptOptions::default();
        let mut ctx = Context::new(&c, &options);
        secofr(&mut d, &mut ctx)?;
        run(
            &mut d,
            &mut ctx,
            "wait \"Main Menu\"\ntype call qcmd\nkey enter\nwait \"Command Entry\"\n\
             type CHGJOB LANGID(ESP) CNTRYID(ES)\nkey enter\nsettle 1500\n\
             type DSPLIBL\nkey enter\nsettle 2000\n",
        )?;
        let libl = screen(&d);
        let (es, sys) = (libl.find("QSYS2931"), libl.find("QSYS      "));
        if !matches!((es, sys), (Some(a), Some(b)) if a < b) {
            return Err(format!("DSPLIBL after CHGJOB LANGID(ESP):\n{libl}"));
        }
        run(&mut d, &mut ctx, "key f3\nsettle 1000\ntype WRKHDWRSC TYPE(*CMN)\nkey enter\nsettle 3000\n")?;
        let hw = screen(&d);
        want(&hw, "WRKHDWRSC in Spanish", &["Operativo", "Puerto Ethernet"])?;
        run(&mut d, &mut ctx, "key f3\nsettle 1000\ntype signoff\nkey enter\nsettle 2000\n")?;
        d.disconnect();
        Ok("QSYS2931 installed; LANGID(ESP) puts it ahead of QSYS; WRKHDWRSC in Spanish".into())
    }

    /// Step 121: `QSYS2931` at the front of `QSYSLIBL`, a Spanish sign-on.
    pub(crate) fn spanish_sign_on(&self, s: &mut Session) -> Result<String, String> {
        let set = s.command(&format!("CHGSYSVAL SYSVAL(QSYSLIBL) VALUE('QSYS2931 {SYSLIBL}')"), CMD)?;
        let port = self.telnet_port().ok_or("the launch has no telnet port")?;
        let shown = connect(port, "QSECOFR", SECOFR_PASSWORD).and_then(|mut d| {
            let c = tn5250_driver::Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
            let options = ScriptOptions::default();
            let mut ctx = Context::new(&c, &options);
            let r = run(&mut d, &mut ctx, "wait \"Inicio de sesi\"\n").map(|()| screen(&d));
            d.disconnect();
            r
        });
        // Back to the shipped list whatever the sign-on showed.
        let back = s.command(&format!("CHGSYSVAL SYSVAL(QSYSLIBL) VALUE('{SYSLIBL}')"), CMD)?;
        let shown = shown.map_err(|e| format!("CHGSYSVAL: {set}\n{e}"))?;
        want(&shown, "the Spanish sign-on", &["Inicio de sesi", "Usuario", "Contrase", "Biblioteca actual"])?;
        if back.contains("CPF") {
            return Err(format!("restoring QSYSLIBL: {back}"));
        }
        Ok("QDSIGNON from QSYS2931 at the front of QSYSLIBL".into())
    }

    /// Step 122: `CHGJOB DATFMT(*YMD) DATSEP('-') TIMSEP('.')`, then
    /// `WRKACTJOB`'s date and time.
    pub(crate) fn date_formats(&self, _s: &mut Session) -> Result<String, String> {
        let port = self.telnet_port().ok_or("the launch has no telnet port")?;
        let mut d = connect(port, "QSECOFR", SECOFR_PASSWORD)?;
        let c = tn5250_driver::Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
        let options = ScriptOptions::default();
        let mut ctx = Context::new(&c, &options);
        secofr(&mut d, &mut ctx)?;
        run(
            &mut d,
            &mut ctx,
            "wait \"Main Menu\"\ntype call qcmd\nkey enter\nwait \"Command Entry\"\n\
             type CHGJOB DATFMT(*YMD) DATSEP('-') TIMSEP('.')\nkey enter\nsettle 1500\n\
             type WRKACTJOB\nkey enter\nsettle 3000\n",
        )?;
        let shown = screen(&d);
        let top: String = shown.lines().take(3).collect::<Vec<_>>().join("\n");
        let words: Vec<&str> = top.split_whitespace().collect();
        let shape = |w: &str, sep: u8| {
            let b = w.as_bytes();
            b.len() == 8 && b[2] == sep && b[5] == sep && b.iter().enumerate().all(|(i, c)| i == 2 || i == 5 || c.is_ascii_digit())
        };
        if !words.iter().any(|w| shape(w, b'-')) || !words.iter().any(|w| shape(w, b'.')) {
            return Err(format!("WRKACTJOB's date and time after CHGJOB:\n{top}"));
        }
        run(&mut d, &mut ctx, "key f3\nsettle 1000\ntype signoff\nkey enter\nsettle 2000\n")?;
        d.disconnect();
        Ok("WRKACTJOB shows YY-MM-DD and HH.MM.SS".into())
    }
}
