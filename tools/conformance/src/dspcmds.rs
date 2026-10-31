// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 82: every display command of `QSYS` from the main menu over
//! TN5250. The commands are the shipped definitions IBM shows a display
//! for (`sys_cpps::inventory::is_display`: `WRK*`, `DSP*`, `EDT*`, the
//! tools' `STR*`, `GO`). Each must open a panel whose title line is not
//! the menu's, and leave nothing on the menu's message line when it
//! ends; one that stays on the menu, prompts for a parameter, or leaves
//! text behind is a shim and is listed by name. `docs/CPP_AUDIT.md`
//! has the table behind it.

use std::fmt::Write as _;

use tn5250_driver::script::Context;
use tn5250_driver::{Connect, ScriptOptions};

use crate::phase5::SECOFR_PASSWORD;
use crate::phase8::{connect, run, secofr};
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// What a command needs to open its display on a fresh system: its
/// required parameters, naming the objects [`SETUP`] makes or the
/// system ships.
const PARMS: &[(&str, &str)] = &[
    ("DSPAUDJRNE", "ENTTYP(AF)"),
    ("DSPAUT", "OBJ('/QSYS.LIB')"),
    ("DSPAUTL", "AUTL(Q82AUTL)"),
    ("DSPAUTLOBJ", "AUTL(Q82AUTL)"),
    ("DSPBCKUP", "BCKUPOPT(*DAILY)"),
    ("DSPBKP", ""),
    ("DSPBNDDIR", "BNDDIR(Q82LIB/Q82BND)"),
    ("DSPCLS", "CLS(QGPL/QINTER)"),
    ("DSPCMD", "CMD(QSYS/DSPLIB)"),
    ("DSPDBR", "FILE(Q82LIB/Q82T)"),
    ("DSPDEVD", "DEVD(Q82DSP)"),
    ("DSPDTAARA", "DTAARA(Q82LIB/Q82DA)"),
    ("DSPF", "STMF('/q82.txt')"),
    ("DSPFD", "FILE(Q82LIB/Q82T)"),
    ("DSPFFD", "FILE(Q82LIB/Q82T)"),
    ("DSPHDWRSC", "TYPE(*CMN)"),
    ("DSPJOBD", "JOBD(QGPL/QDFTJOBD)"),
    ("DSPJRN", "JRN(Q82LIB/Q82JRN)"),
    ("DSPJRNRCVA", "JRNRCV(Q82LIB/Q82RCV)"),
    ("DSPLIB", "LIB(QGPL)"),
    ("DSPLIND", "LIND(Q82ETH)"),
    ("DSPMOD", "MODULE(Q82LIB/Q82MOD)"),
    ("DSPOBJAUT", "OBJ(QGPL) OBJTYPE(*LIB)"),
    ("DSPOBJD", "OBJ(QGPL) OBJTYPE(*LIB)"),
    ("DSPOPT", "VOL(Q82O) DEV(Q82OPT)"),
    ("DSPPFM", "FILE(Q82LIB/Q82T)"),
    ("DSPPGM", "PGM(Q82LIB/Q82PGM)"),
    ("DSPPGMREF", "PGM(Q82LIB/Q82PGM)"),
    ("DSPSAVF", "FILE(Q82LIB/Q82SAVF)"),
    ("DSPSBSD", "SBSD(QSYS/QINTER)"),
    ("DSPSPLF", "FILE(QPSRVDMP) SPLNBR(*LAST)"),
    ("DSPSRVPGM", "SRVPGM(Q82LIB/Q82SRV)"),
    ("DSPSYSVAL", "SYSVAL(QDATE)"),
    ("DSPTAP", "DEV(Q82TAP)"),
    ("DSPTAPCTG", "DEV(Q82MLB)"),
    ("DSPUDFS", "UDFS('/dev/QASP01/q82.udfs')"),
    ("DSPUSRPRF", "USRPRF(QSECOFR)"),
    ("EDTAUTL", "AUTL(Q82AUTL)"),
    ("EDTF", "STMF('/q82.txt')"),
    ("EDTOBJAUT", "OBJ(Q82LIB/Q82DA) OBJTYPE(*DTAARA)"),
    ("GO", "USER"),
    ("STRDBG", "PGM(Q82LIB/Q82PGM) UPDPROD(*YES)"),
    ("STRRLU", "SRCFILE(Q82LIB/QDDSSRC) SRCMBR(Q82RLU) OPTION(5)"),
    ("STRSEU", "SRCFILE(Q82LIB/QCLSRC) SRCMBR(Q82MOD) OPTION(5)"),
    ("WRKMBRPDM", "FILE(Q82LIB/QCLSRC)"),
    ("WRKAUT", "OBJ('/QSYS.LIB')"),
    ("WRKCFGSTS", "CFGTYPE(*DEV)"),
    ("WRKHDWRSC", "TYPE(*CMN)"),
    ("WRKIMGCLGE", "IMGCLG(Q82TAP)"),
    ("WRKJRNA", "JRN(Q82LIB/Q82JRN)"),
    ("WRKMSGD", "MSGF(QSYS/QCPFMSG)"),
    ("WRKOBJ", "OBJ(QGPL/*ALL)"),
    ("WRKOBJLCK", "OBJ(QGPL) OBJTYPE(*LIB)"),
    ("WRKOBJPGP", "PGP(Q82GRP)"),
    ("WRKOUTQD", "OUTQ(QGPL/QPRINT)"),
    ("WRKTAPCTG", "DEV(Q82MLB)"),
];

/// Commands that open their display only in a state the step does not
/// put the job in, with why.
const NOT_HERE: &[(&str, &str)] = &[
    ("DSPDTA", "runs a DFU program; step 55 makes one"),
    ("DSPMODSRC", "shows a program being debugged; it follows STRDBG here"),
];

/// Commands run in the debug session `STRDBG` starts, before `ENDDBG`.
const IN_DEBUG: &[&str] = &["DSPMODSRC", "DSPBKP"];

/// The service console lines that make the objects [`PARMS`] names.
/// A line that fails leaves its commands to fail by name below.
const SETUP: &[&str] = &[
    "crtlib LIB(Q82LIB) TEXT('Step 82 objects')",
    "crtautl AUTL(Q82AUTL) TEXT('Step 82 list')",
    "crtbnddir BNDDIR(Q82LIB/Q82BND)",
    "crtdtaara DTAARA(Q82LIB/Q82DA) TYPE(*CHAR) LEN(10) VALUE('step 82')",
    "runsql SQL('CREATE TABLE Q82LIB.Q82T (C CHAR(10))') COMMIT(*NONE)",
    "runsql SQL('INSERT INTO Q82LIB.Q82T VALUES(''one'')') COMMIT(*NONE)",
    "crtjrnrcv JRNRCV(Q82LIB/Q82RCV)",
    "crtjrn JRN(Q82LIB/Q82JRN) JRNRCV(Q82LIB/Q82RCV)",
    "crtsavf FILE(Q82LIB/Q82SAVF)",
    "crtsrcpf FILE(Q82LIB/QCLSRC)",
    "crtsrcpf FILE(Q82LIB/QDDSSRC)",
    "crtdevdsp DEVD(Q82DSP) DEVCLS(*VRT) TYPE(3179) MODEL(2)",
    "chgdevd DEVD(Q82DSP) TEXT('Step 82 display')",
    "chgneta MAXHOP(16)",
    "crtusrprf USRPRF(Q82GRP) PASSWORD(*NONE) TEXT('Step 82 group')",
    "crtudfs UDFS('/dev/QASP01/q82.udfs')",
    "crtjrnrcv JRNRCV(QSYS/AUDRCV0)",
    "crtjrn JRN(QSYS/QAUDJRN) JRNRCV(QSYS/AUDRCV0)",
    "QSCRTCRTS KEYSTORE(*SYSTEM) PASSWORD('step82pw')",
    // A tape and its catalog.
    "crtdevtap DEVD(Q82TAP) RSRCNAME(*VRT)",
    "crtimgclg IMGCLG(Q82TAP) DIR('/q82/tap') TYPE(*TAP)",
    "addimgclge IMGCLG(Q82TAP) FROMFILE(*NEW) TOFILE(VOL1) VOLNAM(Q82V1) IMGSIZ(8)",
    "lodimgclg IMGCLG(Q82TAP) DEV(Q82TAP)",
    "vrycfg CFGOBJ(Q82TAP) CFGTYPE(*DEV) STATUS(*ON)",
    "inztap DEV(Q82TAP) NEWVOL(Q82V1) CHECK(*NO)",
    // An optical volume.
    "crtdevopt DEVD(Q82OPT) RSRCNAME(*VRT)",
    "crtimgclg IMGCLG(Q82OPT) DIR('/q82/opt') TYPE(*OPT)",
    "addimgclge IMGCLG(Q82OPT) FROMFILE(*NEW) TOFILE(OPT1.ISO) VOLNAM(Q82O) IMGSIZ(8)",
    "lodimgclg IMGCLG(Q82OPT) DEV(Q82OPT)",
    "vrycfg CFGOBJ(Q82OPT) CFGTYPE(*DEV) STATUS(*ON)",
    "inzopt VOL(Q82O) DEV(Q82OPT) CHECK(*NO)",
    // A tape media library with a cartridge.
    "crtdevmlb DEVD(Q82MLB) DEVCLS(*TAP) RSRCNAME(*VRT)",
    "crtimgclg IMGCLG(Q82LIB) DIR('/q82/lib') TYPE(*TAP)",
    "addimgclge IMGCLG(Q82LIB) FROMFILE(*NEW) TOFILE(Q82A) VOLNAM(Q82A) IMGSIZ(2)",
    "lodimgclg IMGCLG(Q82LIB) DEV(Q82MLB)",
    "vrycfg CFGOBJ(Q82MLB) CFGTYPE(*DEV) STATUS(*ON)",
    "addtapctg DEV(Q82MLB) CTG(Q82A) CGY(*SHARE400)",
];

/// A CL module for `DSPMOD`, `DSPPGM`, `DSPSRVPGM` and the debugger;
/// its program runs `RTVCFGSTS`.
const MODULE_SRC: &str = "PGM\n  DCL VAR(&N) TYPE(*DEC) LEN(5 0) VALUE(82)\n  DCL VAR(&S) TYPE(*DEC) LEN(5 0)\n  RTVCFGSTS CFGD(Q82DSP) CFGTYPE(*DEV) STSCDE(&S)\n  CHGVAR VAR(&N) VALUE(&N + &S)\nENDPGM\n";

/// The screen as text, one row a line.
fn screen(d: &tn5250_driver::session::Session) -> String {
    let mut out = String::new();
    for row in d.term.text_rows() {
        out.push('|');
        out.push_str(&row);
        out.push_str("|\n");
    }
    out
}

/// Row `n` (1-based), trimmed.
fn row(d: &tn5250_driver::session::Session, n: usize) -> String {
    d.term
        .text_rows()
        .get(n - 1)
        .map(|r| r.trim().to_string())
        .unwrap_or_default()
}

/// Whether the main menu shows.
fn at_main(d: &tn5250_driver::session::Session) -> bool {
    d.term.contains("Main Menu") && d.term.contains("Selection or command")
}

/// Whether the job ended and the device shows the sign-on display.
fn signed_off(d: &tn5250_driver::session::Session) -> bool {
    row(d, 1) == "Sign On"
}

/// A command's prompt: the title ends with `(NAME)`.
fn prompted(title: &str, name: &str) -> bool {
    title.ends_with(&format!("({name})"))
}

/// The objects the commands need, through the service console.
fn setup(s: &mut Session) -> String {
    let mut failed = String::new();
    for line in SETUP {
        match s.command(line, CMD) {
            Ok(out) if out.lines().any(|l| l.trim_start().starts_with("CPF")) => {
                let _ = writeln!(failed, "  {line}: {}", out.lines().next().unwrap_or("").trim());
            }
            Ok(_) => {}
            Err(e) => {
                let _ = writeln!(failed, "  {line}: {e}");
            }
        }
    }
    // An Ethernet line over the virtio port.
    if let Ok(hw) = s.command("hw", CMD)
        && let Some(port) = hw
            .lines()
            .find(|l| l.starts_with("CMN") && l.contains("Ethernet port"))
            .and_then(|l| l.split_whitespace().next())
    {
        let _ = s.command(&format!("CRTLINETH LIND(Q82ETH) RSRCNAME({port}) ONLINE(*NO)"), CMD);
    }
    // A printer file source for RLU.
    if crate::qir::qemu::push(s, "Q82RLUSRC", "     A          R LINE1\n     A                                  2'Step 82'\n").is_ok() {
        let _ = s.command("svc.srcput Q82LIB/QDDSSRC Q82RLU SRCTYPE(PRTF) FROMSTMF(Q82RLUSRC)", CMD);
    }
    // A stream file, and the module and programs.
    let _ = s.command("svc.ifs put /q82.txt Step 82 stream file", CMD);
    if crate::qir::qemu::push(s, "Q82SRC", MODULE_SRC).is_ok() {
        let _ = s.command("svc.srcput Q82LIB/QCLSRC Q82MOD SRCTYPE(CLLE) FROMSTMF(Q82SRC)", CMD);
        for line in [
            "crtclmod MODULE(Q82LIB/Q82MOD) SRCFILE(Q82LIB/QCLSRC) DBGVIEW(*SOURCE)",
            "crtpgm PGM(Q82LIB/Q82PGM) MODULE(Q82LIB/Q82MOD)",
            "crtsrvpgm SRVPGM(Q82LIB/Q82SRV) MODULE(Q82LIB/Q82MOD) EXPORT(*ALL)",
            "call PGM(Q82LIB/Q82PGM)",
        ] {
            if let Ok(out) = s.command(line, CMD)
                && out.contains("CPF")
            {
                let _ = writeln!(failed, "  {line}: {}", out.lines().next().unwrap_or("").trim());
            }
        }
    }
    failed
}

/// Send `keys`, then wait (up to ten seconds) for `done`
/// (given the screen before): a slow display is not taken for no display.
fn press(
    d: &mut tn5250_driver::session::Session,
    ctx: &mut Context,
    keys: &str,
    done: &dyn Fn(&tn5250_driver::session::Session, &str) -> bool,
) -> Result<(), String> {
    let before = screen(d);
    run(d, ctx, keys)?;
    for _ in 0..20 {
        if done(d, &before) {
            break;
        }
        run(d, ctx, "settle 500\n")?;
    }
    run(d, ctx, "settle 500\n")
}

/// Back to the main menu: F3, F12, and the tools' own ways out.
fn to_main(d: &mut tn5250_driver::session::Session, ctx: &mut Context) -> Result<bool, String> {
    for i in 0..8 {
        if at_main(d) {
            return Ok(true);
        }
        if signed_off(d) {
            return Ok(false);
        }
        // F3 first; a tool's exit display takes Enter (Interactive SQL:
        // leave without saving); F12 once F3 has not done it.
        let keys = if d.term.contains("Exit Interactive SQL") {
            "key eraseeof\ntype 2\nkey enter\n"
        } else if row(d, 1).contains("Exit") {
            "key enter\n"
        } else if i >= 4 && i % 2 == 1 {
            "key f12\n"
        } else {
            "key f3\n"
        };
        // Done when the menu or the sign-on shows, or another display.
        press(d, ctx, keys, &|d, before| at_main(d) || signed_off(d) || screen(d) != before)?;
    }
    Ok(at_main(d))
}

impl Suite {
    /// Step 82: every display command of `QSYS` from the main menu.
    #[allow(clippy::too_many_lines)] // the setup, the walk and the report
    pub(crate) fn display_commands(&self, s: &mut Session) -> Result<String, String> {
        let port = self.telnet_port().ok_or("the launch has no telnet port")?;
        let setup_failed = setup(s);
        let c = Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
        let options = ScriptOptions {
            golden: None,
            update: false,
            release: String::new(),
            timeout_ms: 20000,
        };
        let mut ctx = Context::new(&c, &options);
        let mut d = connect(port, "QSECOFR", SECOFR_PASSWORD)?;
        secofr(&mut d, &mut ctx)?;
        run(&mut d, &mut ctx, "wait \"Main Menu\"\n")?;
        let main_title = row(&d, 1);
        let main_23 = row(&d, 23);
        let main_24 = row(&d, 24);
        // A spooled file for DSPSPLF: the job's dump.
        run(&mut d, &mut ctx, "type DMPJOB\nkey enter\nsettle 1500\n")?;
        to_main(&mut d, &mut ctx)?;

        let names: Vec<&str> = sys_cpps::all()
            .iter()
            .map(|e| e.name.as_str())
            .filter(|n| sys_cpps::inventory::is_display(n))
            .filter(|n| !NOT_HERE.iter().any(|(x, _)| x == n) && !IN_DEBUG.contains(n))
            .collect();
        let mut failures: Vec<String> = Vec::new();
        let mut report = String::new();
        let mut try_one = |d: &mut tn5250_driver::session::Session,
                           ctx: &mut Context,
                           name: &str,
                           failures: &mut Vec<String>|
         -> Result<(), String> {
            let parms = PARMS.iter().find(|(n, _)| *n == name).map_or("", |(_, p)| *p);
            let line = format!("{name} {parms}");
            run(d, ctx, &format!("type \"{}\"\n", line.trim()))?;
            // Done when the menu is gone or its message line changed.
            press(d, ctx, "key enter\n", &|d, _| row(d, 1) != main_title || row(d, 24) != main_24)?;
            let title = row(d, 1);
            let shown = screen(d);
            let fail = if at_main(d) || title == main_title {
                Some(format!("stayed on the menu: {}", row(d, 24)))
            } else if prompted(&title, name) {
                Some(format!("prompted: {title}"))
            } else {
                None
            };
            let back = to_main(d, ctx)?;
            let ended = signed_off(d);
            let fail = fail.or_else(|| {
                let (r23, r24) = (row(d, 23), row(d, 24));
                if ended {
                    Some(format!("the job ended: {r24}"))
                } else if !back {
                    Some("could not get back to the menu".into())
                } else if r23 != main_23 || r24 != main_24 {
                    Some(format!("left text on the menu: {r24}"))
                } else {
                    None
                }
            });
            if let Some(why) = fail {
                failures.push(format!("{name}: {why}"));
                let _ = write!(report, "--- {line}\n{shown}");
                // What the job logged for it, when the menu shows.
                if at_main(d) {
                    run(d, ctx, "type DSPJOBLOG\n")?;
                    press(d, ctx, "key enter\n", &|d, _| !at_main(d))?;
                    run(d, ctx, "key pgdn\nsettle 800\nkey pgdn\nsettle 800\n")?;
                    let _ = write!(report, "--- job log after {name}\n{}", screen(d));
                    to_main(d, ctx)?;
                }
            }
            if ended {
                run(d, ctx, &format!("signon QSECOFR {SECOFR_PASSWORD}\nwait \"Main Menu\"\n"))?;
            }
            if !at_main(d) {
                // A display with no way out: a new session goes on.
                failures.push(format!("{name}: no way back to the menu"));
                let _ = write!(report, "--- {line} (stuck)\n{}", screen(d));
                d.disconnect();
                *d = connect(port, "QSECOFR", SECOFR_PASSWORD)?;
                // Device recovery may hand back the job as it was.
                run(d, ctx, "settle 2000\n")?;
                if signed_off(d) {
                    secofr(d, ctx)?;
                }
                to_main(d, ctx)?;
                run(d, ctx, "wait \"Main Menu\"\n")?;
            }
            Ok(())
        };
        for name in &names {
            try_one(&mut d, &mut ctx, name, &mut failures)?;
        }
        // The debugger's displays, inside a debug session.
        run(&mut d, &mut ctx, "type \"STRDBG PGM(Q82LIB/Q82PGM) UPDPROD(*YES)\"\nkey enter\nsettle 2000\n")?;
        to_main(&mut d, &mut ctx)?;
        for name in IN_DEBUG {
            try_one(&mut d, &mut ctx, name, &mut failures)?;
        }
        run(&mut d, &mut ctx, "type ENDDBG\nkey enter\nsettle 1000\n")?;
        d.disconnect();

        let mut detail = format!("{} display commands from MAIN", names.len() + IN_DEBUG.len());
        for (n, why) in NOT_HERE {
            let _ = write!(detail, "; {n} not run ({why})");
        }
        if failures.is_empty() {
            return Ok(detail);
        }
        Err(format!(
            "{} display commands are shims:\n  {}\n{}setup lines that failed:\n{setup_failed}\n{report}",
            failures.len(),
            failures.join("\n  "),
            if setup_failed.is_empty() { "" } else { "\n" },
        ))
    }
}
