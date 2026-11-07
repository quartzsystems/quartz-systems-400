// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL steps, 130 on: what the SQL work could only check in memory,
//! run on the booted system. STRSQL and Command Entry over a TN5250
//! session, `JTOpen` against the host servers, the CLI in server mode,
//! the DRDA client, QTEMP and triggers from RPG, external procedures,
//! a saved STRSQL session across sign-ons, and DRDA between two guests.
//! Each step makes what it needs in library `SQLQ`.

mod cli;
mod drda;
mod extproc;
mod jtopen;
mod qtemp;
mod runsql;
mod session;
mod strsql;
mod trigger;
#[cfg(unix)]
mod twoguest;

use std::time::Duration;

use tn5250_driver::script::Context;
use tn5250_driver::session::Session as Display;
use tn5250_driver::{Connect, ScriptOptions};

use crate::phase5::SECOFR_PASSWORD;
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The steps' library.
const LIB: &str = "SQLQ";

/// A TN5250 session signed on as `QSECOFR`, at Command Entry.
pub(crate) struct Tn {
    pub d: Display,
    pub ctx: Context,
}

impl Tn {
    /// Connect, sign on and reach Command Entry with `SQLQ` on the
    /// library list.
    pub fn open(suite: &Suite) -> Result<Self, String> {
        Self::open_at(suite.telnet_port().ok_or("the launch has no telnet port")?)
    }

    /// As [`Self::open`], through the telnet forward on host `port`.
    pub fn open_at(port: u16) -> Result<Self, String> {
        let connect = Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
        let options = ScriptOptions::default();
        let mut ctx = Context::new(&connect, &options);
        ctx.timeout = Duration::from_secs(90);
        let mut d = crate::phase8::connect(port, "QSECOFR", SECOFR_PASSWORD)?;
        let until = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < until && !d.term.contains("Sign On") {
            d.pump().map_err(|e| format!("{e:#}"))?;
        }
        crate::phase8::secofr(&mut d, &mut ctx)?;
        let mut tn = Self { d, ctx };
        tn.run("wait \"Main Menu\"\ntype call qcmd\nkey enter\nwait \"Command Entry\"\n")?;
        tn.cmd(&format!("addlible {LIB}"), "")?;
        Ok(tn)
    }

    /// Run script text.
    pub fn run(&mut self, text: &str) -> Result<(), String> {
        crate::phase8::run(&mut self.d, &mut self.ctx, text)
    }

    /// The screen as text.
    pub fn screen(&self) -> String {
        self.d.term.text_rows().join("\n")
    }

    /// A command at Command Entry; waits for `want` (any screen once
    /// the keyboard unlocks when empty). The screen after.
    pub fn cmd(&mut self, line: &str, want: &str) -> Result<String, String> {
        let until = if want.is_empty() {
            "settle 800\nunlock\n".to_string()
        } else {
            format!("wait \"{want}\"\n")
        };
        self.run(&format!("key eraseinput\ntype {line}\nkey enter\n{until}"))
            .map_err(|e| format!("{line}: {e}"))?;
        Ok(self.screen())
    }

    /// The message ID behind the cursor at `row`, `col` (1-based): F1,
    /// read from Additional Message Information, F12 back.
    pub fn msgid_at(&mut self, row: usize, col: usize) -> Result<String, String> {
        self.d.term.set_cursor(row, col);
        self.run("key help\nwait \"Additional Message Information\"\n")?;
        let screen = self.screen();
        let id = screen
            .lines()
            .find_map(|l| {
                let i = l.find("Message ID")?;
                l[i..]
                    .split(':')
                    .nth(1)?
                    .split_whitespace()
                    .next()
                    .map(str::to_string)
            })
            .ok_or_else(|| format!("no message ID shown:\n{screen}"))?;
        self.run("key f12\nsettle 800\nunlock\n")?;
        Ok(id)
    }

    /// Sign off and disconnect.
    pub fn close(mut self) -> Result<(), String> {
        let r = self.run("key eraseinput\ntype signoff\nkey enter\nwait \"Sign On\"\n");
        self.d.disconnect();
        r
    }
}

/// The library and source files, the password the sessions sign on
/// with, made once whichever step runs first.
pub(crate) fn setup(s: &mut Session) -> Result<(), String> {
    let set = s.command(
        &format!("CHGUSRPRF USRPRF(QSECOFR) PASSWORD('{SECOFR_PASSWORD}') PWDEXP(*NO)"),
        CMD,
    )?;
    if set.trim_start().starts_with("CPF") {
        return Err(format!("CHGUSRPRF QSECOFR: {set}"));
    }
    let _ = s.command(&format!("CRTLIB LIB({LIB}) TEXT('SQL steps')"), CMD)?;
    for (file, len) in [("QSQLSRC", 92), ("QRPGLESRC", 112), ("QCLSRC", 92)] {
        let _ = s.command(&format!("CRTSRCPF FILE({LIB}/{file}) RCDLEN({len})"), CMD)?;
    }
    Ok(())
}

/// A source member into `SQLQ/file` over the service console.
pub(crate) fn put(
    s: &mut Session,
    file: &str,
    name: &str,
    kind: &str,
    text: &str,
) -> Result<(), String> {
    let buf = format!("Q{name}");
    crate::qir::qemu::push(s, &buf, text)?;
    let r = s.command(
        &format!("svc.srcput {LIB}/{file} {name} SRCTYPE({kind}) FROMSTMF({buf})"),
        CMD,
    )?;
    if !r.contains("written") {
        return Err(format!("srcput {name}: {r}"));
    }
    Ok(())
}

/// A statement at the service console's SQL line; its output.
pub(crate) fn sql(s: &mut Session, stmt: &str) -> Result<String, String> {
    s.command(&format!("svc.sql {stmt};"), CMD)
}

/// A query at the service console that must show every word.
pub(crate) fn sql_has(s: &mut Session, stmt: &str, words: &[&str]) -> Result<String, String> {
    let out = sql(s, stmt)?;
    for w in words {
        if !out.contains(w) {
            return Err(format!("{stmt}: no {w:?} in:\n{out}"));
        }
    }
    Ok(out)
}

/// The text of the newest spooled file named `file`, any user's.
pub(crate) fn spooled(s: &mut Session, file: &str) -> Result<String, String> {
    let list = s.command("svc.wrksplf SELECT(*ALL) VIEW(3)", CMD)?;
    let row: Vec<&str> = list
        .lines()
        .rfind(|l| l.split_whitespace().next() == Some(file))
        .ok_or_else(|| format!("no spooled file {file}:\n{list}"))?
        .split_whitespace()
        .collect();
    let [_, splnbr, job, user, number, ..] = row[..] else {
        return Err(format!("a short row for {file}: {row:?}"));
    };
    s.command(
        &format!("svc.dspsplf {file} JOB({number}/{user}/{job}) SPLNBR({splnbr})"),
        CMD,
    )
}

/// The DDM server up and `LOOP`, an *RDB entry aliasing this system,
/// with the server authority entry its connections sign on with.
pub(crate) fn loop_rdb(s: &mut Session) -> Result<(), String> {
    let _ = s.command("STRSBS SBSD(QUSRWRK)", CMD);
    let _ = s.command("STRTCPSVR SERVER(*DDM)", CMD)?;
    let until = std::time::Instant::now() + Duration::from_secs(90);
    loop {
        let out = s.command("WRKACTJOB", CMD)?;
        if out.contains("QRWTLSTN") && out.contains("QRWTSRVR") {
            break;
        }
        if std::time::Instant::now() >= until {
            return Err(format!("QRWTLSTN or QRWTSRVR is not active:\n{out}"));
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    let _ = s.command("RMVRDBDIRE RDB(LOOP)", CMD);
    // This system's database again, by the alias LOOP, as IBM i reaches itself.
    let local = local_rdb(s)?;
    let add = s.command(
        &format!(
            "ADDRDBDIRE RDB({local} LOOP) RMTLOCNAME('127.0.0.1' *IP) TEXT('This system again')"
        ),
        CMD,
    )?;
    if add.trim_start().starts_with("CPF") {
        return Err(format!("ADDRDBDIRE: {add}"));
    }
    let _ = s.command("CHGSYSVAL SYSVAL(QRETSVRSEC) VALUE('1')", CMD);
    let _ = s.command("RMVSVRAUTE USRPRF(QSECOFR) SERVER(LOOP)", CMD);
    let aute = s.command(
        &format!(
            "ADDSVRAUTE USRPRF(QSECOFR) SERVER(LOOP) USRID(QSECOFR) PASSWORD('{SECOFR_PASSWORD}')"
        ),
        CMD,
    )?;
    if aute.trim_start().starts_with("CPF") {
        return Err(format!("ADDSVRAUTE: {aute}"));
    }
    Ok(())
}

/// The local RDB name: the directory's `*LOCAL` entry.
pub(crate) fn local_rdb(s: &mut Session) -> Result<String, String> {
    let out = s.command("WRKRDBDIRE", CMD)?;
    out.lines()
        .find(|l| l.contains("*LOCAL"))
        .and_then(|l| l.split_whitespace().next())
        .map(str::to_string)
        .ok_or_else(|| format!("no *LOCAL entry:\n{out}"))
}
