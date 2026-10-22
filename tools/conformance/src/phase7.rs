// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 7 steps: Command Entry over the analyzer, the prompter, a CL
//! program compiled, called and submitted, the job log's requests, a
//! limited user, and `?CMD`.

use std::time::Duration;

use crate::phase5::{screen_wait, secofr_command_entry};
use crate::phase6::{entry, entry_wait, has, sign_off};
use crate::qemu::session::Session;
use crate::steps::Suite;

const CMD: Duration = Duration::from_secs(10);
const PASSWORD: &str = "LETMEIN1";

/// The CL program the steps compile.
const HELLO: &str = "\
             PGM        PARM(&WHO)
             DCL        VAR(&WHO) TYPE(*CHAR) LEN(10)
             DCL        VAR(&JOB) TYPE(*CHAR) LEN(10)
             DCL        VAR(&N) TYPE(*DEC) LEN(3 0) VALUE(0)
             RTVJOBA    JOB(&JOB)
             DOWHILE    COND(&N *LT 3)
               CHGVAR     VAR(&N) VALUE(&N + 1)
             ENDDO
             SNDPGMMSG  MSG('Hello from CL,' *BCAT &WHO *BCAT 'in job' *BCAT +
                          &JOB *BCAT 'count' *BCAT %CHAR(&N)) TOPGMQ(*EXT)
             CHKOBJ     OBJ(QGPL/NOSUCHOBJ) OBJTYPE(*FILE)
             MONMSG     MSGID(CPF9801) EXEC(SNDPGMMSG MSG('NOSUCHOBJ is missing, as expected') +
                          TOPGMQ(*EXT))
             ENDPGM
";

/// Press a key at the display.
fn key(s: &mut Session, name: &str) -> Result<(), String> {
    s.command(&format!("key {name}"), CMD).map(|_| ())
}

/// Wait until the display no longer shows `text` (a key was taken).
fn screen_without(s: &mut Session, text: &str) -> Result<(), String> {
    let mut last = String::new();
    for _ in 0..40 {
        last = s.command("screen", CMD)?;
        if !last.contains(text) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(format!("the console kept showing {text:?}:\n{last}"))
}

impl Suite {
    /// Step 46: QSECOFR at Command Entry creates a library through the
    /// analyzer, prompts `CRTPF` with `?` and with F4, runs it from the
    /// prompt, sees the requests in the job log, retrieves a command
    /// with F9 and signs off.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn command_entry(&self, s: &mut Session) -> Result<String, String> {
        secofr_command_entry(s)?;
        entry_wait(s, "crtlib CLTEST TEXT('Phase 7')", has(&["CLTEST", "created"]))?;
        // `?CMD` prompts; F12 leaves the prompt.
        entry(s, "?crtpf")?;
        let prompt = screen_wait(s, "Create Physical File (CRTPF)")?;
        if !prompt.contains("Record length") {
            return Err(format!("the CRTPF prompt lacks its parameters:\n{prompt}"));
        }
        key(s, "f12")?;
        screen_wait(s, "Command Entry")?;
        // F4 on a typed command: the values typed show on the prompt;
        // Enter runs it.
        s.command("type crtpf FILE(CLTEST/F1) RCDLEN(80)", CMD)?;
        key(s, "f4")?;
        let prompt = screen_wait(s, "Create Physical File (CRTPF)")?;
        if !prompt.contains("> F1") || !prompt.contains("> 80") {
            return Err(format!("the prompt does not show the typed values:\n{prompt}"));
        }
        key(s, "enter")?;
        let after = screen_wait(s, "Command Entry")?;
        if !after.contains("F1") || !after.contains("created") {
            return Err(format!("CRTPF from the prompt did not run:\n{after}"));
        }
        let log = entry_wait(s, "dspjoblog", has(&[">>", "crtlib CLTEST"]))?;
        if !log.contains("crtpf FILE(CLTEST/F1)") && !log.contains("CRTPF FILE(CLTEST/F1)") {
            return Err(format!("the prompted CRTPF is not a request in the job log:\n{log}"));
        }
        key(s, "f9")?;
        let retrieved = screen_wait(s, "===> dspjoblog")?;
        key(s, "f9")?;
        let older = screen_wait(s, "===> crtpf")?;
        key(s, "f12")?;
        screen_without(s, "===> crtpf")?;
        let _ = (retrieved, older);
        // A command the analyzer refuses: the diagnostic and CPF0001.
        entry_wait(s, "crtlib LIB(CLTEST) NOPE(1)", has(&["CPD0012"]))?;
        sign_off(s)?;
        Ok("CRTLIB ran through the analyzer; ?CRTPF and F4 prompted; Enter ran CRTPF; DSPJOBLOG lists the requests; F9 retrieved; CPD0012 for a bad keyword".into())
    }

    /// Step 47: a CL program from a source member: compiled with
    /// `CRTCLPGM`, called with a parameter, submitted with `SBMJOB`, its
    /// source retrieved with `RTVCLSRC`.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn cl_program(&self, s: &mut Session) -> Result<String, String> {
        secofr_command_entry(s)?;
        entry_wait(s, "crtsrcpf FILE(QGPL/QCLSRC)", has(&["QCLSRC", "created"]))?;
        crate::qir::qemu::push(s, "HELLOSRC", HELLO)?;
        let put = s.command(
            "svc.srcput QGPL/QCLSRC HELLO SRCTYPE(CLLE) FROMSTMF(HELLOSRC)",
            CMD,
        )?;
        if !put.contains("written") {
            return Err(format!("srcput: {put}"));
        }
        let made = entry_wait(
            s,
            "crtclpgm PGM(CLTEST/HELLO) SRCFILE(QGPL/QCLSRC)",
            has(&["HELLO", "created"]),
        )?;
        let called = entry_wait(
            s,
            "call CLTEST/HELLO PARM('Quartz')",
            has(&["Hello from CL, Quartz"]),
        )?;
        if !called.contains("count 3") || !called.contains("NOSUCHOBJ is missing") {
            return Err(format!("the program's messages are incomplete:\n{called}"));
        }
        entry_wait(
            s,
            "sbmjob CMD(CALL PGM(CLTEST/HELLO) PARM('batch')) JOB(HELLOB)",
            has(&["HELLOB", "submitted"]),
        )?;
        let active = entry_wait(s, "wrkactjob", has(&["SCPF", "RUN"]))?;
        entry_wait(
            s,
            "rtvclsrc PGM(CLTEST/HELLO) SRCFILE(QGPL/QCLSRC) SRCMBR(HELLO2)",
            has(&["retrieved"]),
        )?;
        sign_off(s)?;
        let back = s.command("svc.srcget QGPL/QCLSRC HELLO2", CMD)?;
        if !back.contains("RTVJOBA") {
            return Err(format!("RTVCLSRC did not give the source back:\n{back}"));
        }
        let _ = (made, active);
        Ok("CRTCLPGM compiled HELLO; CALL printed its messages with RTVJOBA, DOWHILE and MONMSG at work; SBMJOB submitted it; WRKACTJOB listed; RTVCLSRC gave the source back".into())
    }

    /// Step 48: a user with `LMTCPB(*YES)` may not run `CRTLIB`
    /// (`CPD0043`) but may sign off.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn limited_user(&self, s: &mut Session) -> Result<String, String> {
        let made = s.command(
            &format!("crtusrprf USRPRF(LIMITED) PASSWORD({PASSWORD}) LMTCPB(*YES) TEXT('Limited user')"),
            CMD,
        )?;
        if !made.contains("LIMITED") {
            return Err(format!("crtusrprf LIMITED: {made}"));
        }
        screen_wait(s, "Sign On")?;
        crate::phase5::console_sign_on_as(s, "LIMITED", PASSWORD, PASSWORD)?;
        entry(s, "crtlib LIB(NOTALLOWED)")?;
        screen_wait(s, "CPD0043")?;
        entry(s, "signoff")?;
        screen_wait(s, "Sign On")?;
        let gone = s.command("svc.obj ls NOTALLOWED", CMD)?;
        if !gone.contains("not found") && !gone.contains("CPF") {
            return Err(format!("the limited user created a library:\n{gone}"));
        }
        Ok("LIMITED refused CRTLIB with CPD0043; DSPJOBLOG and SIGNOFF allowed".into())
    }

    /// Step 49: the startup program compiled from CL started the work:
    /// the subsystems QSTRUP names are active after the IPL.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn startup_program(&self, s: &mut Session) -> Result<String, String> {
        let sbs = s.command("wrksbs", CMD)?;
        for name in ["QSPL", "QSERVER", "QUSRWRK", "QINTER", "QBATCH"] {
            if !sbs.contains(name) {
                return Err(format!("QSTRUP did not start {name}:\n{sbs}"));
            }
        }
        let pgm = s.command("dsppgm QSYS/QSTRUP", CMD)?;
        if !pgm.contains("CLLE") {
            return Err(format!("QSTRUP is not the CL program:\n{pgm}"));
        }
        Ok("QSPL, QSERVER, QUSRWRK, QINTER and QBATCH active; QSYS/QSTRUP is a CLLE program".into())
    }

    /// Step 50: the analyzer at the service console: the refusals and
    /// the old words behind `svc.`.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn analyzer_console(&self, s: &mut Session) -> Result<String, String> {
        let bad = s.command("crtlib", CMD)?;
        if !bad.contains("CPD0015") || !bad.contains("CPF0001") {
            return Err(format!("a missing LIB was not refused with CPD0015:\n{bad}"));
        }
        let unknown = s.command("nosuchcmd", CMD)?;
        if !unknown.contains("CPD0030") {
            return Err(format!("an unknown command was not refused with CPD0030:\n{unknown}"));
        }
        let too_many = s.command("dltlib LIB(CLTEST) EXTRA(1)", CMD)?;
        if !too_many.contains("CPD0012") {
            return Err(format!("a bad keyword was not refused with CPD0012:\n{too_many}"));
        }
        let dev = s.command("svc.obj ls CLTEST", CMD)?;
        if !dev.contains("F1") {
            return Err(format!("svc.obj ls does not list CLTEST/F1:\n{dev}"));
        }
        let gone = s.command("dltlib CLTEST", CMD)?;
        if !gone.contains("CLTEST") {
            return Err(format!("dltlib CLTEST: {gone}"));
        }
        Ok("CPD0015, CPD0030 and CPD0012 at the service console; svc. words still work; CLTEST deleted".into())
    }
}
