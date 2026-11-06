// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 104, commands typed across the rows of the multi-row command
//! lines, and step 103, `ENDSBS *ALL *IMMED` from the console while
//! jobs hold locks: the restricted state with the console still usable.

use std::time::{Duration, Instant};

use tn5250_driver::script::Context;
use tn5250_driver::{Connect, ScriptOptions};

use crate::phase5::{SECOFR_PASSWORD, screen_wait, secofr_command_entry};
use crate::phase6::entry;
use crate::phase8::{connect, run, secofr};
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The batch job's program: it holds the data area and waits.
const HOLDER: &str = "\
             PGM
             ALCOBJ     OBJ((QGPL/RSTDLCK *DTAARA *EXCL))
             DLYJOB     DLY(600)
             ENDPGM
";

/// The command of the report: 77 characters, wrapping on MAIN.
const CRTLINETH: &str =
    "CRTLINETH LIND(ETHLINE) RSRCNAME(CMN01) ONLINE(*YES) TEXT('Primary Ethernet')";

/// Poll a service console command until `done`, `secs` at most.
fn poll(s: &mut Session, cmd: &str, secs: u64, done: impl Fn(&str) -> bool) -> Result<String, String> {
    let until = Instant::now() + Duration::from_secs(secs);
    loop {
        let out = s.command(cmd, CMD)?;
        if done(&out) || Instant::now() > until {
            return Ok(out);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// A job's row in the text form of `WRKACTJOB`.
fn has_job(list: &str, name: &str) -> bool {
    list.lines().any(|l| l.split_whitespace().next() == Some(name))
}

/// A data area's value as `DSPDTAARA` shows it.
fn dtaara(s: &mut Session, name: &str) -> Result<String, String> {
    s.command(&format!("DSPDTAARA DTAARA(QGPL/{name})"), CMD)
}

impl Suite {
    /// Step 104: long commands typed across the rows of MAIN's command
    /// line (153 cells over two rows) and Command Entry's (313 over
    /// four), over telnet and at the console, run as typed.
    #[allow(clippy::many_single_char_names)] // s, d, c, m, e as the other steps name them
    pub(crate) fn wrapped_command_lines(&self, s: &mut Session) -> Result<String, String> {
        let _ = s.command("DLTDTAARA DTAARA(QGPL/WRAPM)", CMD);
        let _ = s.command("DLTDTAARA DTAARA(QGPL/WRAPE)", CMD);
        let _ = s.command("DLTDTAARA DTAARA(QGPL/WRAPC)", CMD);
        let port = self.telnet_port().ok_or("the launch has no telnet port")?;
        let mut d = connect(port, "QSECOFR", SECOFR_PASSWORD)?;
        let c = Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
        let options = ScriptOptions::default();
        let mut ctx = Context::new(&c, &options);
        secofr(&mut d, &mut ctx)?;
        run(&mut d, &mut ctx, "wait \"Main Menu\"\n")?;
        // MAIN: 74 cells on row 20, the rest from row 21 column 1.
        let main_cmd = "CRTDTAARA DTAARA(QGPL/WRAPM) TYPE(*CHAR) LEN(60) VALUE('typed on MAIN across the row end')";
        run(&mut d, &mut ctx, &format!("type {main_cmd}\nkey enter\nsettle 2000\n"))?;
        let main_screen = d.term.text();
        let m = dtaara(s, "WRAPM")?;
        if !m.contains("typed on MAIN across the row end") {
            return Err(format!("MAIN's wrapped command did not run:\n{main_screen}\n--- DSPDTAARA\n{m}"));
        }
        // The report's command: whatever it says, not that it is wrong.
        run(&mut d, &mut ctx, &format!("type {CRTLINETH}\nkey enter\nsettle 3000\n"))?;
        let lineth = d.term.text();
        if lineth.contains("Errors were found") {
            return Err(format!("CRTLINETH typed on MAIN was misread:\n{lineth}"));
        }
        // Command Entry: past two row ends.
        run(&mut d, &mut ctx, "type call qcmd\nkey enter\nwait \"Command Entry\"\n")?;
        let long_value = "x".repeat(150);
        let entry_cmd = format!(
            "CRTDTAARA DTAARA(QGPL/WRAPE) TYPE(*CHAR) LEN(200) VALUE('{long_value}') TEXT('three rows')"
        );
        run(&mut d, &mut ctx, &format!("type {entry_cmd}\nkey enter\nsettle 2000\n"))?;
        let e = dtaara(s, "WRAPE")?;
        let entry_screen = d.term.text();
        if !e.contains("xxxxxxxxxxxxxxxxxxxx") {
            return Err(format!("Command Entry's wrapped command did not run:\n{entry_screen}\n--- DSPDTAARA\n{e}"));
        }
        run(&mut d, &mut ctx, "key f3\nsettle 1000\ntype signoff\nkey enter\nwait \"Sign On\"\n")?;
        d.disconnect();
        // The console's Command Entry, typed through the framebuffer client.
        secofr_command_entry(s)?;
        let console_cmd = "CRTDTAARA DTAARA(QGPL/WRAPC) TYPE(*CHAR) LEN(60) VALUE('typed at the console across the row end')";
        entry(s, console_cmd)?;
        let found = poll(s, "DSPDTAARA DTAARA(QGPL/WRAPC)", 20, |o| o.contains("across the row end"))?;
        if !found.contains("across the row end") {
            let screen = s.command("screen", CMD)?;
            return Err(format!("the console's wrapped command did not run:\n{screen}\n--- DSPDTAARA\n{found}"));
        }
        entry(s, "signoff")?;
        screen_wait(s, "Sign On")?;
        for n in ["WRAPM", "WRAPE", "WRAPC"] {
            let _ = s.command(&format!("DLTDTAARA DTAARA(QGPL/{n})"), CMD);
        }
        Ok("MAIN (2 rows), Command Entry (3 rows) over telnet and the console's Command Entry ran their wrapped commands; CRTLINETH read whole".into())
    }

    /// Step 103: a batch job holds a data area and a telnet session is
    /// signed on; `ENDSBS *ALL *IMMED` typed at the console ends every
    /// job but the console's, every subsystem ends, the lock is free and
    /// the console still runs commands; the controlling subsystem then
    /// starts again.
    pub(crate) fn restricted_state(&self, s: &mut Session) -> Result<String, String> {
        let _ = s.command("CRTDTAARA DTAARA(QGPL/RSTDLCK) TYPE(*CHAR) LEN(10)", CMD);
        let _ = s.command("CRTSRCPF FILE(QGPL/QCLSRC)", CMD);
        crate::qir::qemu::push(s, "RSTDSRC", HOLDER)?;
        let put = s.command("svc.srcput QGPL/QCLSRC RSTDHOLD SRCTYPE(CLLE) FROMSTMF(RSTDSRC)", CMD)?;
        if !put.contains("written") {
            return Err(format!("srcput: {put}"));
        }
        let made = s.command("CRTBNDCL PGM(QGPL/RSTDHOLD) SRCFILE(QGPL/QCLSRC)", Duration::from_secs(60))?;
        if !made.contains("created") {
            return Err(format!("CRTBNDCL: {made}"));
        }
        let _ = s.command("STRSBS SBSD(QSYS/QBATCH)", CMD);
        s.command("SBMJOB CMD(CALL PGM(QGPL/RSTDHOLD)) JOB(RSTDHOLD) MSGQ(*NONE)", CMD)?;
        let held = poll(s, "WRKOBJLCK OBJ(QGPL/RSTDLCK) OBJTYPE(*DTAARA)", 40, |o| o.contains("RSTDHOLD"))?;
        if !held.contains("RSTDHOLD") {
            let active = s.command("wrkactjob", CMD)?;
            return Err(format!("RSTDHOLD does not hold the data area:\n{held}\n{active}"));
        }
        // A telnet session at Command Entry.
        let port = self.telnet_port().ok_or("the launch has no telnet port")?;
        let mut d = connect(port, "QSECOFR", SECOFR_PASSWORD)?;
        let c = Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
        let options = ScriptOptions::default();
        let mut ctx = Context::new(&c, &options);
        secofr(&mut d, &mut ctx)?;
        run(&mut d, &mut ctx, "wait \"Main Menu\"\n")?;
        let before = s.command("wrkactjob", CMD)?;
        // The console ends every subsystem.
        secofr_command_entry(s)?;
        entry(s, "ENDSBS SBS(*ALL) OPTION(*IMMED)")?;
        let ended = poll(s, "wrksbs", 150, |o| !o.lines().any(|l| l.contains("ACTIVE") || l.contains("ENDING")))?;
        let after = poll(s, "wrkactjob", 60, |o| !has_job(o, "RSTDHOLD") && !o.contains("QPADEV"))?;
        let screen = s.command("screen", CMD)?;
        let report = format!("--- before\n{before}\n--- wrksbs\n{ended}\n--- wrkactjob\n{after}\n--- console\n{screen}");
        if ended.lines().any(|l| l.contains("ACTIVE") || l.contains("ENDING")) {
            return Err(format!("subsystems are still active:\n{report}"));
        }
        if has_job(&after, "RSTDHOLD") || after.contains("QPADEV") {
            return Err(format!("jobs survived ENDSBS *ALL:\n{report}"));
        }
        if !has_job(&after, "QSCONSOLE") {
            return Err(format!("the console's job ended:\n{report}"));
        }
        // The lock went with its job.
        let alc = s.command("ALCOBJ OBJ((QGPL/RSTDLCK *DTAARA *EXCL))", CMD)?;
        let _ = s.command("DLCOBJ OBJ((QGPL/RSTDLCK *DTAARA *EXCL))", CMD);
        if !alc.contains("allocated") {
            return Err(format!("the data area is still held: {alc}\n{report}"));
        }
        // The console still runs commands in the restricted state.
        screen_wait(s, "Command Entry")?;
        entry(s, "CHGDTAARA DTAARA(QGPL/RSTDLCK) VALUE('RESTRICTD')")?;
        let v = poll(s, "DSPDTAARA DTAARA(QGPL/RSTDLCK)", 20, |o| o.contains("RESTRICTD"))?;
        if !v.contains("RESTRICTD") {
            let screen = s.command("screen", CMD)?;
            return Err(format!("the console did not run a command in the restricted state:\n{screen}"));
        }
        // Back: the controlling subsystem from the console.
        let ctl = s.command("svc.sysval QCTLSBSD", CMD)?;
        let name = if ctl.contains("QCTL") { "QCTL" } else { "QBASE" };
        entry(s, &format!("STRSBS SBSD(QSYS/{name})"))?;
        let up = poll(s, "wrksbs", 60, |o| o.contains(name))?;
        if !up.contains(name) {
            return Err(format!("{name} did not start again:\n{up}"));
        }
        entry(s, "signoff")?;
        let _ = screen_wait(s, "Sign On");
        drop(d);
        let _ = s.command("DLTDTAARA DTAARA(QGPL/RSTDLCK)", CMD);
        let _ = s.command("DLTPGM PGM(QGPL/RSTDHOLD)", CMD);
        Ok(format!(
            "RSTDHOLD and the telnet session ended, every subsystem ended, the lock is free, the console ran a command, {name} started again"
        ))
    }
}

/// Step 142's batch job: every subsystem ends but it, it works alone
/// for a while, leaves a data area and ends.
const BATCH_RSTD: &str = "\
             PGM
             ENDSBS     SBS(*ALL) OPTION(*IMMED) BCHTIMLMT(5)
             DLYJOB     DLY(30)
             CRTDTAARA  DTAARA(QGPL/BRSTDRAN) TYPE(*CHAR) LEN(10) +
                          VALUE('ALONE')
             ENDPGM
";

impl Suite {
    /// Step 142: the batch restricted state. `BCHTIMLMT` from the console
    /// is `CPF1052`; a batch job of the controlling subsystem ends every
    /// subsystem with it and keeps running alone; when it ends, the
    /// controlling subsystem starts again.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn batch_restricted(&self, s: &mut Session) -> Result<String, String> {
        let refused = s.command("ENDSBS SBS(*ALL) BCHTIMLMT(5)", CMD)?;
        if !refused.contains("CPF1052") {
            return Err(format!("BCHTIMLMT from the console: {refused}"));
        }
        let _ = s.command("DLTDTAARA DTAARA(QGPL/BRSTDRAN)", CMD);
        let _ = s.command("CRTSRCPF FILE(QGPL/QCLSRC)", CMD);
        crate::qir::qemu::push(s, "BRSTDSRC", BATCH_RSTD)?;
        let put = s.command("svc.srcput QGPL/QCLSRC BRSTD SRCTYPE(CLLE) FROMSTMF(BRSTDSRC)", CMD)?;
        if !put.contains("written") {
            return Err(format!("srcput: {put}"));
        }
        let made = s.command("CRTBNDCL PGM(QGPL/BRSTD) SRCFILE(QGPL/QCLSRC)", Duration::from_secs(60))?;
        if !made.contains("created") {
            return Err(format!("CRTBNDCL: {made}"));
        }
        let ctl = s.command("svc.sysval QCTLSBSD", CMD)?;
        // The value after the name: `QCTLSBSD: QBASE QSYS`.
        let value = ctl.split_once(':').map_or("", |(_, v)| v);
        let (name, jobq) = if value.contains("QCTL") { ("QCTL", "QSYS/QCTL") } else { ("QBASE", "QGPL/QBASE") };
        s.command(&format!("SBMJOB CMD(CALL PGM(QGPL/BRSTD)) JOB(BRSTD) JOBQ({jobq}) MSGQ(*NONE)"), CMD)?;
        // Alone: every other subsystem gone, the job still there.
        let alone = poll(s, "wrksbs", 60, |o| !o.lines().any(|l| l.contains("ACTIVE")))?;
        let active = s.command("wrkactjob", CMD)?;
        let report = format!("--- wrksbs\n{alone}\n--- wrkactjob\n{active}");
        if alone.lines().any(|l| l.contains("ACTIVE")) {
            return Err(format!("subsystems stayed active:\n{report}"));
        }
        if !has_job(&active, "BRSTD") {
            return Err(format!("the batch job did not stay:\n{report}"));
        }
        // It ends; the controlling subsystem comes back.
        let ran = poll(s, "DSPDTAARA DTAARA(QGPL/BRSTDRAN)", 90, |o| o.contains("ALONE"))?;
        if !ran.contains("ALONE") {
            return Err(format!("the job did not finish alone:\n{ran}\n{report}"));
        }
        let up = poll(s, "wrksbs", 90, |o| o.lines().any(|l| l.contains(name) && l.contains("ACTIVE")))?;
        if !up.lines().any(|l| l.contains(name) && l.contains("ACTIVE")) {
            let log = s.command("dsplog", CMD)?;
            return Err(format!("{name} did not start again:\n{up}\n--- dsplog\n{log}"));
        }
        let _ = s.command("DLTDTAARA DTAARA(QGPL/BRSTDRAN)", CMD);
        let _ = s.command("DLTPGM PGM(QGPL/BRSTD)", CMD);
        Ok(format!("CPF1052 from the console; BRSTD ran alone after ENDSBS *ALL BCHTIMLMT(5); {name} started again when it ended"))
    }
}
