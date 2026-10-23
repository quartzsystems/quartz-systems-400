// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Phase 8 steps: the user interface over a real TN5250E session
//! on the forwarded telnet port, driven with the same scripted display
//! that captures the oracle systems.

use std::time::{Duration, Instant};

use tn5250::term::Key;
use tn5250_driver::script::{self, Context};
use tn5250_driver::session::Session as Display;
use tn5250_driver::{Connect, ScriptOptions};

use crate::phase5::SECOFR_PASSWORD;
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// Connect to the guest's telnet port, retrying while the server job
/// comes up.
pub(crate) fn connect(port: u16, user: &str, password: &str) -> Result<Display, String> {
    let c = Connect::qs400(port, user, password);
    let until = Instant::now() + Duration::from_secs(90);
    let mut last = String::new();
    while Instant::now() < until {
        match Display::connect(&c) {
            Ok(d) => return Ok(d),
            Err(e) => {
                last = format!("{e:#}");
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
    Err(format!("telnet port {port}: {last}"))
}

/// Run a script's text over a display.
pub(crate) fn run(d: &mut Display, ctx: &mut Context, text: &str) -> Result<(), String> {
    script::run(d, ctx, text).map_err(|e| {
        let mut out = format!("{e:#}\n--- screen ---\n");
        for row in d.term.text_rows() {
            out.push('|');
            out.push_str(&row);
            out.push_str("|\n");
        }
        out
    })
}

/// Sign on as `QSECOFR` through telnet, whatever state the password is
/// in: the first sign-on changes it, later ones use the changed one.
pub(crate) fn secofr(d: &mut Display, ctx: &mut Context) -> Result<(), String> {
    run(
        d,
        ctx,
        "wait \"Sign On\"\nsignon QSECOFR Quartz2026\nsettle 1500\n",
    )?;
    if d.term.contains("not correct") {
        run(d, ctx, "signon QSECOFR QSECOFR\nsettle 1500\n")?;
    }
    if d.term.contains("Change Password") {
        run(
            d,
            ctx,
            &format!(
                "type QSECOFR\nkey tab\ntype {SECOFR_PASSWORD}\nkey tab\ntype {SECOFR_PASSWORD}\nkey enter\nsettle 1500\n"
            ),
        )?;
    }
    if d.term.contains("Sign-on Information") {
        run(d, ctx, "key enter\nsettle 1000\n")?;
    }
    Ok(())
}

impl Suite {
    /// Step 51: a TN5250E session signs on, reaches the main menu and
    /// Command Entry, runs a command whose output names the session's
    /// own job and the server, and signs off; the virtual display and
    /// its controller show in `WRKCFGSTS`.
    pub(crate) fn telnet_sign_on(&self, s: &mut Session) -> Result<String, String> {
        let port = self
            .launch
            .telnet_port
            .ok_or("the launch has no telnet port")?;
        let connect = Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
        let options = ScriptOptions::default();
        let mut ctx = Context::new(&connect, &options);
        let mut d = connect_display(port)?;
        if !d.term.contains("Sign On") {
            run(&mut d, &mut ctx, "wait \"Sign On\"\n")?;
        }
        let rows = d.term.text_rows();
        // `DEVNAME` of QDSIGNON: row 4, column 68.
        let device = rows
            .get(3)
            .and_then(|r| r.get(67..77))
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        if !device.starts_with("QPADEV") {
            return Err(format!(
                "the sign-on display names no virtual device:\n{}",
                rows.join("\n")
            ));
        }
        secofr(&mut d, &mut ctx)?;
        run(
            &mut d,
            &mut ctx,
            "wait \"Main Menu\"\ntype call qcmd\nkey enter\nwait \"Command Entry\"\ntype wrkactjob\nkey enter\n",
        )?;
        // The list is long: the server's jobs are at its end, which
        // Command Entry shows; the session's own job is checked through
        // the configuration status below.
        run(&mut d, &mut ctx, "wait \"QTVTELNET\"\n")?;
        if !d.term.contains("QTVDEVICE") {
            return Err(format!(
                "WRKACTJOB does not list the session job:\n{}",
                d.term.text()
            ));
        }
        let cfg = s.command("WRKCFGSTS CFGTYPE(*DEV)", CMD)?;
        let line = cfg
            .lines()
            .find(|l| l.starts_with(&device))
            .ok_or_else(|| format!("WRKCFGSTS does not list {device}:\n{cfg}"))?;
        if !line.contains("ACTIVE") {
            return Err(format!("{device} is not active: {line}"));
        }
        run(
            &mut d,
            &mut ctx,
            "type signoff\nkey enter\nwait \"Sign On\"\n",
        )?;
        d.disconnect();
        let gone = (0..40).any(|_| {
            std::thread::sleep(Duration::from_millis(250));
            s.command("WRKCFGSTS CFGTYPE(*DEV)", CMD).is_ok_and(|c| {
                c.lines()
                    .any(|l| l.starts_with(&device) && l.contains("VARIED ON"))
            })
        });
        if !gone {
            return Err(format!(
                "{device} did not return to varied on after the session ended"
            ));
        }
        let _ = Key::Enter;
        Ok(format!(
            "{device} on QPACTL01: sign-on, MAIN, Command Entry, WRKACTJOB, sign-off, device freed"
        ))
    }
}

impl Suite {
    /// Step 52: Command Entry over the session, compared with the
    /// oracle's capture (characters only: the panel is the interim
    /// one, not yet the display file).
    pub(crate) fn telnet_command_entry(&self, _s: &mut Session) -> Result<String, String> {
        let port = self
            .launch
            .telnet_port
            .ok_or("the launch has no telnet port")?;
        let connect = Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
        let golden =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("golden/verified/pub400");
        let options = ScriptOptions {
            golden: Some(golden),
            update: false,
            release: String::new(),
            timeout_ms: 20000,
        };
        let mut ctx = Context::new(&connect, &options);
        let mut d = connect_display(port)?;
        secofr(&mut d, &mut ctx)?;
        run(
            &mut d,
            &mut ctx,
            r#"wait "Main Menu"
mask reqlvl Request level: +\d+ => Request level:   N
mask title-system ^(.{71})([A-Z0-9]+ *)$ => SYSNAME 2
type call qcmd
key enter
wait "Command Entry"
snap command-entry -attrs
type signoff
key enter
wait "Sign On"
"#,
        )?;
        d.disconnect();
        ctx.finish().map_err(|e| format!("{e:#}"))?;
        Ok("Command Entry matches the oracle's characters".into())
    }
}

impl Suite {
    /// Step 53: device recovery. A session drops without signing off:
    /// under `QDEVRCYACN(*DSCMSG)` the job is kept and comes back when
    /// the user signs on again, with its screen; under `*ENDJOB` the job
    /// ends and the device returns to varied on.
    pub(crate) fn telnet_recovery(&self, s: &mut Session) -> Result<String, String> {
        let port = self
            .launch
            .telnet_port
            .ok_or("the launch has no telnet port")?;
        let connect = Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
        let options = ScriptOptions::default();
        let mut ctx = Context::new(&connect, &options);
        s.command("CHGSYSVAL SYSVAL(QDEVRCYACN) VALUE(*DSCMSG)", CMD)?;
        let mut d = connect_display(port)?;
        secofr(&mut d, &mut ctx)?;
        run(
            &mut d,
            &mut ctx,
            "wait \"Main Menu\"\ntype call qcmd\nkey enter\nwait \"Command Entry\"\ntype dsplibl\nkey enter\nwait \"> dsplibl\"\n",
        )?;
        let device = device_of(s)?;
        d.disconnect();
        drop(d);
        let pending = (0..40).any(|_| {
            std::thread::sleep(Duration::from_millis(250));
            s.command("WRKCFGSTS CFGTYPE(*DEV)", CMD).is_ok_and(|c| {
                c.lines()
                    .any(|l| l.starts_with(&device) && l.contains("RCYPND"))
            })
        });
        if !pending {
            return Err(format!(
                "{device} did not go to recovery pending after the drop"
            ));
        }
        let mut d = connect_display(port)?;
        secofr(&mut d, &mut ctx)?;
        run(&mut d, &mut ctx, "wait \"Command Entry\"\n")?;
        if !d.term.contains("> dsplibl") {
            return Err(format!(
                "the disconnected job did not come back with its screen:\n{}",
                d.term.text()
            ));
        }
        run(
            &mut d,
            &mut ctx,
            "type signoff\nkey enter\nwait \"Sign On\"\n",
        )?;
        d.disconnect();
        drop(d);
        s.command("CHGSYSVAL SYSVAL(QDEVRCYACN) VALUE(*ENDJOB)", CMD)?;
        let mut d = connect_display(port)?;
        secofr(&mut d, &mut ctx)?;
        run(&mut d, &mut ctx, "wait \"Main Menu\"\n")?;
        let device = device_of(s)?;
        d.disconnect();
        drop(d);
        let freed = (0..40).any(|_| {
            std::thread::sleep(Duration::from_millis(250));
            s.command("WRKCFGSTS CFGTYPE(*DEV)", CMD).is_ok_and(|c| {
                c.lines()
                    .any(|l| l.starts_with(&device) && l.contains("VARIED ON"))
            })
        });
        if !freed {
            return Err(format!(
                "{device} did not return to varied on under *ENDJOB"
            ));
        }
        let active = s.command("wrkactjob", CMD)?;
        if active
            .lines()
            .any(|l| l.contains(&device) && l.contains("QSECOFR"))
        {
            return Err(format!(
                "the job survived the drop under *ENDJOB:\n{active}"
            ));
        }
        s.command("CHGSYSVAL SYSVAL(QDEVRCYACN) VALUE(*DSCMSG)", CMD)?;
        Ok("*DSCMSG kept the job and brought its screen back; *ENDJOB ended it".into())
    }
}

/// The DDS of the step's display file: a prompt with a field the
/// program fills, an input field and an echo line.
const CUSTDSP: &str = "
     A                                      DSPSIZ(24 80 *DS3)
     A                                      CF03(03 'Exit')
     A          R PROMPT
     A                                  1 30'Customer prompt'
     A                                      DSPATR(HI)
     A                                  5  5'Name . . . . :'
     A            NAME          20A  B  5 20
     A            ECHO          40A  O  8  5
     A                                 23  5'F3=Exit'
";

/// The CL program behind it: `SNDRCVF` until F3, echoing the name.
const DSPPGM: &str = "
             PGM
             DCLF       FILE(UITEST/CUSTDSP)
             CHGVAR     VAR(&NAME) VALUE('Quartz')
 LOOP:       SNDRCVF    RCDFMT(PROMPT)
             IF         COND(&IN03) THEN(GOTO CMDLBL(DONE))
             CHGVAR     VAR(&ECHO) VALUE('You typed ' *CAT &NAME)
             GOTO       CMDLBL(LOOP)
 DONE:       SNDPGMMSG  MSG('Display file done') TOPGMQ(*EXT)
             ENDPGM
";

impl Suite {
    /// Step 54: a display file. `CRTDSPF` compiles DDS from a source
    /// member; a CL program's `SNDRCVF` shows its format over the
    /// session with the field the program filled, takes what was typed
    /// and shows it back; `F3` ends it through `&IN03`.
    pub(crate) fn telnet_display_file(&self, s: &mut Session) -> Result<String, String> {
        let port = self
            .launch
            .telnet_port
            .ok_or("the launch has no telnet port")?;
        let lib = s.command("crtlib LIB(UITEST) TEXT('Phase 8 displays')", CMD)?;
        if !lib.contains("UITEST") {
            return Err(format!("crtlib UITEST: {lib}"));
        }
        for file in ["QDDSSRC", "QCLSRC"] {
            let made = s.command(&format!("crtsrcpf FILE(QGPL/{file})"), CMD)?;
            if !made.contains("created") && !made.contains("already") {
                return Err(format!("crtsrcpf {file}: {made}"));
            }
        }
        crate::qir::qemu::push(s, "CUSTDSPSRC", CUSTDSP)?;
        let put = s.command(
            "svc.srcput QGPL/QDDSSRC CUSTDSP SRCTYPE(DSPF) FROMSTMF(CUSTDSPSRC)",
            CMD,
        )?;
        if !put.contains("written") {
            return Err(format!("srcput CUSTDSP: {put}"));
        }
        let created = s.command("crtdspf FILE(UITEST/CUSTDSP) SRCFILE(QGPL/QDDSSRC)", CMD)?;
        if !created.contains("created") {
            return Err(format!("crtdspf: {created}"));
        }
        let fd = s.command("dspfd FILE(UITEST/CUSTDSP)", CMD)?;
        if !fd.contains("*DSPF") || !fd.contains("PROMPT") {
            return Err(format!("dspfd of the display file:\n{fd}"));
        }
        crate::qir::qemu::push(s, "DSPPGMSRC", DSPPGM)?;
        let put = s.command(
            "svc.srcput QGPL/QCLSRC DSPPGM SRCTYPE(CLLE) FROMSTMF(DSPPGMSRC)",
            CMD,
        )?;
        if !put.contains("written") {
            return Err(format!("srcput DSPPGM: {put}"));
        }
        let compiled = s.command("crtclpgm PGM(UITEST/DSPPGM) SRCFILE(QGPL/QCLSRC)", CMD)?;
        if !compiled.contains("created") {
            return Err(format!("crtclpgm DSPPGM: {compiled}"));
        }
        let connect = Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
        let options = ScriptOptions::default();
        let mut ctx = Context::new(&connect, &options);
        let mut d = connect_display(port)?;
        secofr(&mut d, &mut ctx)?;
        run(
            &mut d,
            &mut ctx,
            "wait \"Main Menu\"\ntype call uitest/dsppgm\nkey enter\nwait \"Customer prompt\"\n",
        )?;
        let rows = d.term.text_rows();
        if !rows[4].contains("Quartz") {
            return Err(format!("the field the program filled is not shown:\n{}", rows.join("\n")));
        }
        if d.term.cursor() != (5, 20) {
            return Err(format!("the cursor is not in the input field: {:?}", d.term.cursor()));
        }
        run(
            &mut d,
            &mut ctx,
            "type Cody\nkey fieldexit\nkey enter\nwait \"You typed CODY\"\nkey f3\nwait \"Display file done\"\n",
        )?;
        run(&mut d, &mut ctx, "type signoff\nkey enter\nwait \"Sign On\"\n")?;
        d.disconnect();
        Ok("CRTDSPF compiled CUSTDSP; SNDRCVF showed it, read the name and echoed it; F3 set &IN03".into())
    }
}

/// The active virtual display of the one signed-on session.
fn device_of(s: &mut Session) -> Result<String, String> {
    let cfg = s.command("WRKCFGSTS CFGTYPE(*DEV)", CMD)?;
    cfg.lines()
        .find(|l| l.starts_with("QPADEV") && l.contains("ACTIVE"))
        .and_then(|l| l.split_whitespace().next())
        .map(str::to_string)
        .ok_or_else(|| format!("no active virtual display:\n{cfg}"))
}

fn connect_display(port: u16) -> Result<Display, String> {
    let mut d = connect(port, "QSECOFR", SECOFR_PASSWORD)?;
    let until = Instant::now() + Duration::from_secs(20);
    while Instant::now() < until && !d.term.contains("Sign On") {
        d.pump().map_err(|e| format!("{e:#}"))?;
    }
    Ok(d)
}
