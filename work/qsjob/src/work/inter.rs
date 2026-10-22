// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An interactive job at the console. Signing on may first need a new
//! password (Change Password) and may show Sign-on Information; then
//! the initial program runs, and the initial menu shows: the interim
//! MAIN menu for `MAIN`, sign-off for `*SIGNOFF`, otherwise Command
//! Entry (`MIGRATE(phase8)`: `*MENU` objects). Commands typed on either
//! line run as the service console's do; `LMTCPB(*YES)` allows only the
//! limited commands. `SIGNOFF`, option 90 or F3 at Command Entry ends
//! the job.

use ipc::chan::Chan;
use sec::signon::{Next, SignedOn};
use stg::remote::status;
use stg::{Client, Shared};
use wm::ctl::{FromJob, Panel, Start};
use wm::job::Status;
use wm::panels;

use crate::console::Console;

/// Lines of previous commands and messages the panel shows.
const SHOWN: usize = 14;
/// The width a line of output wraps at.
const WIDTH: usize = 76;
/// The message for menu options that wait for menus.
const NO_MENU: &str = "Menu not available in this release.";

/// What a job's session keeps between its panels.
struct Session<'a> {
    c: Console,
    link: &'a Shared,
    system: String,
    signon: SignedOn,
    /// Commands typed, for F9.
    commands: Vec<String>,
    history: Vec<String>,
}

/// Run the job. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let mut ending = match ctl.try_clone() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("qsjob: {}: {e}", start.name);
            std::process::exit(1);
        }
    };
    let mut c = Console::new(ctl, link, start);
    c.started();
    let system = link
        .borrow_mut()
        .with(|l| l.sysval("QSYSNAME".into()))
        .map_or_else(
            || "QS400".to_string(),
            |v| ccsid::from_ebcdic(&v.data).trim().to_string(),
        );
    let mut s = Session {
        c,
        link,
        system,
        signon: start.signon.clone().unwrap_or_else(|| SignedOn {
            user: start.user.clone(),
            inlmnu: "*NONE".into(),
            ..SignedOn::default()
        }),
        commands: Vec::new(),
        history: Vec::new(),
    };
    if s.signon.next == Next::ChangePassword && !change_password(&mut s) {
        end(s, &mut ending);
    }
    if s.signon.show_info && !signon_info(&mut s) {
        end(s, &mut ending);
    }
    prepare(&mut s);
    // The program named at sign-on, or the profile's, runs first.
    if !start.rqsdta.trim().is_empty() {
        let cmd = format!("call {}", start.rqsdta.trim());
        run_command(&mut s, &cmd);
    }
    match s.signon.inlmnu.as_str() {
        "*SIGNOFF" => {}
        "MAIN" => main_menu(&mut s),
        _ => {
            command_entry(&mut s);
        }
    }
    end(s, &mut ending)
}

/// The job ends: signed off.
fn end(mut s: Session<'_>, ending: &mut Chan) -> ! {
    s.c.ended(0);
    let _ = ending.send(&FromJob::Ending {
        reason: "signed off".into(),
    });
    std::process::exit(0)
}

/// The current library and break delivery the profile asks for.
fn prepare(s: &mut Session<'_>) {
    let curlib = s.signon.curlib.clone();
    if !curlib.is_empty() && !curlib.starts_with('*') {
        let _ = s.c.line(&format!("chgcurlib CURLIB({curlib})"));
    }
    if s.signon.dlvry == "*BREAK" {
        let q = s.signon.msgq.clone();
        let _ = s.c.line(&format!("chgmsgq {q} DLVRY(*BREAK)"));
    }
}

/// Change Password until it succeeds; false when the user leaves it.
fn change_password(s: &mut Session<'_>) -> bool {
    let mut message = "Your password has expired and must be changed.".to_string();
    loop {
        s.c.show(panels::change_password(&s.system, 0, &message));
        status(s.link, Status::Dspw, "PGM-QSYCHGPW");
        let Some((key, fields)) = s.c.wait_input() else {
            return false;
        };
        if key == 3 || key == 12 {
            return false;
        }
        if key != 0 {
            message = "Function key not allowed.".into();
            continue;
        }
        let field = |i: usize| fields.get(i).map_or("", |f| f.trim()).to_string();
        let (current, new, confirm) = (field(0), field(1), field(2));
        if new != confirm {
            message = "The new password and its confirmation differ.".into();
            continue;
        }
        let user = s.signon.user.clone();
        match super::pwdpgm::validate(&mut s.c, s.link, &user, &new, &current) {
            Ok(true) => {}
            Ok(false) => {
                message = "The password validation program did not accept the new password.".into();
                continue;
            }
            Err(e) => {
                message = e;
                continue;
            }
        }
        match s.link.borrow_mut().with(|l| l.chg_pwd(current, new, true)) {
            Ok(()) => return true,
            Err(e) => message = e,
        }
    }
}

/// Sign-on Information; false when the user leaves with F3.
fn signon_info(s: &mut Session<'_>) -> bool {
    let p = panels::signon_info(
        &s.system,
        s.signon.previous,
        s.signon.invalid,
        s.signon.expires_in,
    );
    loop {
        s.c.show(p.clone());
        let Some((key, _)) = s.c.wait_input() else {
            return false;
        };
        match key {
            0 | 12 => return true,
            3 => return false,
            _ => {}
        }
    }
}

/// Whether the profile may run `word` from a command line.
fn allowed(s: &Session<'_>, word: &str) -> bool {
    s.signon.lmtcpb != "*YES"
        || sec::profile::limits::LIMITED_COMMANDS
            .iter()
            .any(|c| c.eq_ignore_ascii_case(word))
}

/// The interim MAIN menu until the user signs off. F3 and F12 do
/// nothing at the top; `CALL QCMD` opens Command Entry, whose F3 comes
/// back here.
fn main_menu(s: &mut Session<'_>) {
    let mut field = String::new();
    let mut message = String::new();
    loop {
        s.c.show(panels::main_menu(&s.system, &field, &message));
        status(s.link, Status::Dspw, "MNU-MAIN");
        let Some((key, fields)) = s.c.wait_input() else {
            return;
        };
        message.clear();
        field.clear();
        match key {
            0 => {}
            3 | 12 => continue,
            4 => {
                message = "Prompting not available in this release.".into();
                continue;
            }
            9 => {
                field = s.commands.last().cloned().unwrap_or_default();
                continue;
            }
            13 | 23 => {
                message = NO_MENU.into();
                continue;
            }
            _ => {
                message = "Function key not allowed.".into();
                continue;
            }
        }
        let typed = fields.first().map_or("", |f| f.trim()).to_string();
        if typed.is_empty() {
            continue;
        }
        if let Ok(n) = typed.parse::<u32>() {
            match n {
                90 => return,
                1..=11 => message = NO_MENU.into(),
                _ => message = format!("Selection {n} is not valid."),
            }
            continue;
        }
        s.commands.push(typed.clone());
        let word = typed
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if word == "signoff" {
            signoff(s, &typed);
            return;
        }
        if word == "call" && typed.to_ascii_uppercase().trim_end().ends_with("QCMD") {
            if command_entry(s) {
                return;
            }
            continue;
        }
        if !allowed(s, &word) {
            message = format!(
                "{} is limited and may not run {}.",
                s.signon.user,
                word.to_ascii_uppercase()
            );
            continue;
        }
        let out = command_output(s, &typed);
        message = out
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim()
            .to_string();
    }
}

/// Command Entry until F3 (false) or sign-off (true).
fn command_entry(s: &mut Session<'_>) -> bool {
    let mut field = String::new();
    let mut message = String::new();
    loop {
        s.c.show(panel(&s.system, &s.history, &field, &message));
        status(s.link, Status::Dspw, "PGM-QCMD");
        let Some((key, fields)) = s.c.wait_input() else {
            return true;
        };
        message.clear();
        field.clear();
        match key {
            3 => return false,
            9 => {
                field = s.commands.last().cloned().unwrap_or_default();
                continue;
            }
            12 => continue,
            4 | 10 | 11 | 13 | 16 | 24 => {
                message = "That function key is not available yet.".into();
                continue;
            }
            0 => {}
            _ => {
                message = "Function key not allowed.".into();
                continue;
            }
        }
        let cmd = fields.first().map_or("", |f| f.trim()).to_string();
        if cmd.is_empty() {
            continue;
        }
        s.commands.push(cmd.clone());
        s.history.push(format!(" > {cmd}"));
        let word = cmd
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if word == "signoff" {
            signoff(s, &cmd);
            return true;
        }
        if !allowed(s, &word) {
            message = format!(
                "{} is limited and may not run {}.",
                s.signon.user,
                word.to_ascii_uppercase()
            );
            continue;
        }
        run_command(s, &cmd);
    }
}

/// `SIGNOFF LOG(*LIST)` keeps the job log; `DROP` and `ENDCNN` have no
/// connection to drop at the console.
fn signoff(s: &mut Session<'_>, cmd: &str) {
    let p = wm::cmd::Params::parse(cmd.trim_start().get(7..).unwrap_or(""));
    if p.upper("LOG").as_deref() == Some("*LIST") {
        let _ = s.c.line("chgjob LOG(4 00 *SECLVL)");
    }
}

/// Run a command; its output.
fn command_output(s: &mut Session<'_>, cmd: &str) -> String {
    let word = cmd.split_whitespace().next().unwrap_or("");
    status(
        s.link,
        Status::Run,
        &format!("CMD-{}", word.to_ascii_uppercase()),
    );
    s.c.job().log_request(cmd, ("QCMD", "QSYS"));
    s.c.line(cmd)
}

/// Run one command and add its output to the history.
fn run_command(s: &mut Session<'_>, cmd: &str) {
    let out = command_output(s, cmd);
    for l in out.lines() {
        let mut rest = l;
        loop {
            let cut = rest
                .char_indices()
                .nth(WIDTH)
                .map_or(rest.len(), |(i, _)| i);
            s.history.push(format!("   {}", &rest[..cut]));
            rest = &rest[cut..];
            if rest.is_empty() {
                break;
            }
        }
    }
}

/// The Command Entry panel.
fn panel(system: &str, history: &[String], field: &str, message: &str) -> Panel {
    let mut p = Panel::default();
    p.text(1, 34, 1, "Command Entry");
    p.text(1, 72, 0, system);
    p.text(2, 59, 0, "Request level:   1");
    p.text(4, 2, 0, "Previous commands and messages:");
    let from = history.len().saturating_sub(SHOWN);
    for (i, l) in history[from..].iter().enumerate() {
        let row = u8::try_from(5 + i).unwrap_or(18);
        let attr = u8::from(l.starts_with(" >"));
        p.text(row, 2, attr, l.clone());
    }
    p.text(19, 2, 0, "Type command, press Enter.");
    p.text(20, 2, 1, "===>");
    p.field(20, 7, 73, field);
    p.text(
        21,
        2,
        2,
        "F3=Exit   F4=Prompt   F9=Retrieve   F10=Detailed messages",
    );
    p.text(
        22,
        2,
        2,
        "F11=Full display   F12=Cancel   F13=Assistant   F24=More keys",
    );
    p.message = message.to_string();
    p.error = !message.is_empty();
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout() {
        let h: Vec<String> = (0..20).map(|i| format!(" > cmd{i}")).collect();
        let lines = panel("QS400", &h, "wrk", "").lines();
        assert_eq!(&lines[0][33..46], "Command Entry");
        assert_eq!(&lines[0][71..76], "QS400");
        assert!(lines[4].contains("cmd6"), "{}", lines[4]);
        assert!(lines[17].contains("cmd19"));
        assert!(lines[19].starts_with(" ===> wrk"));
    }
}
