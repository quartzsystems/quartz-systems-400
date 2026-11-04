// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An interactive job at a display. Signing on may first need a new
//! password (Change Password) and may show Sign-on Information; then
//! the initial program runs, and the initial menu shows: the `MAIN`
//! menu object through UIM (the interim MAIN panel when the object is
//! missing), sign-off for `*SIGNOFF`, otherwise Command Entry, from
//! which `GO` shows any `*MENU`. Command Entry is `QCMD`:
//! a line of up to 512 characters runs through the command analyzer,
//! `F4` prompts it, `F9` brings back the previous commands, `F10` adds
//! the second-level text of the messages, `F11` the full width of long
//! lines, `F24` the other keys. `LMTCPB(*YES)` allows only the limited
//! commands. `SIGNOFF`, option 90 or F3 at Command Entry ends the job.

use ipc::chan::Chan;
use sec::signon::{Next, SignedOn};
use stg::remote::status;
use stg::{Client, Shared};
use wm::ctl::{FromJob, Panel, Start};
use wm::job::Status;
use wm::panels;

use crate::command::RunMode;
use crate::console::Console;
use crate::display::uim::LineMsg;

/// Lines of previous commands and messages the panel shows.
const SHOWN: usize = 12;
/// The width a line of output wraps at.
const WIDTH: usize = 76;
/// The most characters a command line holds (`CPF0005` beyond).
const LINE_MAX: usize = 512;
/// Commands kept for `F9`.
const RETRIEVED: usize = 25;
/// The message for menu options that wait for menus.
const NO_MENU: &str = "CPD9B81";

/// A line of the previous commands and messages.
#[derive(Clone, Debug)]
struct Line {
    /// The text, with its leading marker.
    text: String,
    /// A command the user typed.
    command: bool,
    /// The message's second-level text, when `F10` is to show it.
    help: Option<String>,
}

/// What a job's session keeps between its panels.
pub(super) struct Session<'a> {
    pub(super) c: Console,
    pub(super) link: &'a Shared,
    pub(super) system: String,
    pub(super) signon: SignedOn,
    /// Commands typed, oldest first, for F9.
    commands: Vec<String>,
    /// Where F9 is in `commands`; `commands.len()` after a command.
    retrieve: usize,
    history: Vec<Line>,
    /// A message for the next Command Entry display.
    pending: Option<LineMsg>,
    /// `F10`: second-level text shown.
    detailed: bool,
    /// `F11`: lines shown at their full width.
    full: bool,
    /// `F24`: the second set of keys shown.
    more_keys: bool,
    /// The first history line shown, after a roll; `None` at the end.
    top: Option<usize>,
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
    // Every interactive job has a display device, the console's too
    // (`QSCONSOLE`, held by `QCONSOLE`).
    if start.device.is_empty() {
        crate::start::end(&mut ending, "an interactive job needs a display device");
    }
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
        retrieve: 0,
        history: Vec::new(),
        detailed: false,
        full: false,
        more_keys: false,
        top: None,
        pending: None,
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
        run_command(&mut s, &cmd, RunMode::Run);
    }
    match s.signon.inlmnu.as_str() {
        "*SIGNOFF" => {}
        // The MAIN menu object through the UIM engine; the interim
        // panel when the object is missing.
        "MAIN" => {
            if super::menu::go(&mut s, "MAIN", true) == super::menu::Flow::Back {
                main_menu(&mut s);
            }
        }
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
    let message = s.msg("CPD9BAE", &[]);
    let host = s.c.host.clone();
    let user = s.signon.user.clone();
    super::chgpwd::show(s.c.job(), &host, &user, message)
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

/// The interim MAIN menu until the user signs off. F3 and F12 do
/// nothing at the top; `CALL QCMD` opens Command Entry, whose F3 comes
/// back here.
impl Session<'_> {
    /// A message for the next display.
    pub(super) fn say(&mut self, m: LineMsg) {
        self.pending = Some(m);
    }

    /// The message waiting for the next display, taken.
    pub(super) fn take_message(&mut self) -> Option<LineMsg> {
        self.pending.take()
    }

    /// A message's text from the job's message files.
    pub(super) fn text(&self, m: &LineMsg) -> String {
        crate::display::msgline::text_of(&self.c.host, m)
    }

    /// The text of message `id` with `data`.
    pub(super) fn msg(&self, id: &str, data: &[&str]) -> String {
        crate::display::msgline::text(&self.c.host, id, data)
    }
}

/// The job's status for the service, with its function.
pub(super) fn status_of(s: &Session<'_>, st: Status, function: &str) {
    status(s.link, st, function);
}

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
                let typed = fields.first().map_or("", |f| f.trim()).to_string();
                if typed.is_empty() {
                    message = s.msg("CPD9B86", &[]);
                } else {
                    let m = command_msg(s, &typed, RunMode::PromptRun);
                    message = m.map(|m| s.text(&m)).unwrap_or_default();
                }
                continue;
            }
            9 => {
                field = retrieve(s);
                continue;
            }
            13 | 23 => {
                message = s.msg(NO_MENU, &[]);
                continue;
            }
            _ => {
                message = s.msg("CPD6A60", &[]);
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
                1..=11 => message = s.msg(NO_MENU, &[]),
                _ => message = s.msg("CPD6A64", &[]),
            }
            continue;
        }
        remember(s, &typed);
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
        let m = command_msg(s, &typed, RunMode::Run);
        message = m.map(|m| s.text(&m)).unwrap_or_default();
    }
}


/// Keep a command for F9: the last 25, the newest last.
pub(super) fn remember(s: &mut Session<'_>, cmd: &str) {
    if s.commands.last().is_none_or(|l| l != cmd) {
        s.commands.push(cmd.to_string());
    }
    if s.commands.len() > RETRIEVED {
        s.commands.remove(0);
    }
    s.retrieve = s.commands.len();
}

/// F9: the previous command, older at each press.
pub(super) fn retrieve(s: &mut Session<'_>) -> String {
    if s.commands.is_empty() {
        return String::new();
    }
    s.retrieve = s.retrieve.saturating_sub(1);
    s.commands[s.retrieve].clone()
}

/// What a function key at Command Entry leads to.
enum Act {
    /// Show the panel again.
    Show,
    /// Leave Command Entry.
    Exit,
}

/// A function key at Command Entry; the field keeps what was typed
/// unless the key replaces it.
fn function_key(
    s: &mut Session<'_>,
    key: u8,
    typed: &str,
    field: &mut String,
    message: &mut String,
) -> Act {
    *field = typed.to_string();
    match key {
        3 | 16 => return Act::Exit,
        4 => {
            if typed.is_empty() {
                *message = s.msg("CPD9B86", &[]);
            } else {
                s.top = None;
                field.clear();
                run_command(s, typed, RunMode::PromptRun);
            }
        }
        7 => {
            let shown = visible(s).0;
            s.top = Some(shown.saturating_sub(SHOWN));
        }
        8 => {
            let (from, _) = visible(s);
            let next = from + SHOWN;
            s.top = (next + SHOWN < s.history.len()).then_some(next);
        }
        9 => *field = retrieve(s),
        10 => s.detailed = !s.detailed,
        11 => s.full = !s.full,
        12 => field.clear(),
        13 => *message = s.msg("CPD9B81", &[]),
        17 => s.top = Some(0),
        18 => s.top = None,
        24 => s.more_keys = !s.more_keys,
        _ => *message = s.msg("CPD6A60", &[]),
    }
    Act::Show
}

/// Command Entry until F3 (false) or sign-off (true).
pub(super) fn command_entry(s: &mut Session<'_>) -> bool {
    let mut field = String::new();
    let mut message = String::new();
    loop {
        if message.is_empty()
            && let Some(m) = s.pending.take()
        {
            message = s.text(&m);
        }
        s.c.show(panel(s, &field, &message));
        status(s.link, Status::Dspw, "PGM-QCMD");
        let Some((key, fields)) = s.c.wait_input() else {
            return true;
        };
        message.clear();
        field.clear();
        let typed = {
            let mut t = String::new();
            for f in &fields {
                t.push_str(f);
            }
            t.trim().to_string()
        };
        if key != 0 {
            match function_key(s, key, &typed, &mut field, &mut message) {
                Act::Show => continue,
                Act::Exit => return false,
            }
        }
        if typed.is_empty() {
            continue;
        }
        if typed.chars().count() > LINE_MAX {
            message = s.msg("CPF0005", &[]);
            field = typed;
            continue;
        }
        s.top = None;
        let word = typed
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if word == "signoff" {
            remember(s, &typed);
            signoff(s, &typed);
            return true;
        }
        if word == "go" {
            remember(s, &typed);
            let rest = typed.split_once(char::is_whitespace).map_or("", |(_, r)| r);
            let _request = crate::sysdsp::keys::Request::enter();
            match super::menu::go(s, rest, false) {
                super::menu::Flow::Back => {}
                super::menu::Flow::SignOff | super::menu::Flow::End => return true,
            }
            continue;
        }
        run_command(s, &typed, RunMode::Run);
    }
}

/// `SIGNOFF LOG(*LIST)` keeps the job log; `DROP` and `ENDCNN` have no
/// connection to drop at the console.
pub(super) fn signoff(s: &mut Session<'_>, cmd: &str) {
    s.c.job().log_request(cmd, ("QCMD", "QSYS"));
    let p = wm::cmd::Params::parse(cmd.trim_start().get(7..).unwrap_or(""));
    if p.upper("LOG").as_deref() == Some("*LIST") {
        let _ = s.c.line("chgjob LOG(4 00 *SECLVL)");
    }
}

/// Run a command as a request of `QCMD`; what it leaves on the message
/// line ([`crate::display::msgline::of_outcome`]).
pub(super) fn command_msg(s: &mut Session<'_>, cmd: &str, mode: RunMode) -> Option<LineMsg> {
    let since = crate::display::msgline::newest(s.c.job());
    let _request = crate::sysdsp::keys::Request::enter();
    let word = cmd.split_whitespace().next().unwrap_or("");
    status(
        s.link,
        Status::Run,
        &format!("CMD-{}", word.trim_start_matches('?').to_ascii_uppercase()),
    );
    s.c.job().log_request(cmd, ("QCMD", "QSYS"));
    let o = s.c.run(cmd, mode);
    let host = s.c.host.clone();
    crate::display::msgline::of_outcome(s.c.job(), &host, since, &o)
}

/// Run one command and add it, its output and the messages' help to
/// the previous commands and messages.
fn run_command(s: &mut Session<'_>, cmd: &str, mode: RunMode) {
    let _request = crate::sysdsp::keys::Request::enter();
    remember(s, cmd);
    let before = s.c.job().joblog.len();
    let word = cmd.split_whitespace().next().unwrap_or("");
    status(
        s.link,
        Status::Run,
        &format!("CMD-{}", word.trim_start_matches('?').to_ascii_uppercase()),
    );
    s.c.job().log_request(cmd, ("QCMD", "QSYS"));
    let o = s.c.run(cmd, mode);
    // The command as typed, in the user's case; after a prompt, the
    // command the prompter built.
    let prompted = mode != RunMode::Run || cmd.trim_start().starts_with('?');
    let shown = if !prompted || o.changed.is_empty() || o.escape.as_ref().is_some_and(|f| f.msgid == "CPF6801") {
        cmd.to_string()
    } else {
        o.changed.clone()
    };
    s.history.push(Line {
        text: format!(" > {shown}"),
        command: true,
        help: None,
    });
    let helps: Vec<(String, String)> =
        s.c.job()
            .joblog
            .entries()
            .skip(before)
            .filter(|e| !e.help.trim().is_empty())
            .map(|e| (e.msgid.clone(), e.help.clone()))
            .collect();
    for l in o.text.lines() {
        let help = l
            .get(..7)
            .and_then(|id| helps.iter().find(|(m, _)| m == id))
            .map(|(_, h)| h.clone());
        s.history.push(Line {
            text: format!("   {l}"),
            command: false,
            help,
        });
    }
}

/// The rows the panel can show, each at most one row wide: the text
/// and whether it is a command the user typed.
fn rows(s: &Session<'_>) -> Vec<(String, bool)> {
    rows_of(&s.history, s.full, s.detailed)
}

fn rows_of(history: &[Line], full: bool, detailed: bool) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for l in history {
        push_wrapped(&mut out, &l.text, l.command, full);
        if detailed && let Some(h) = &l.help {
            for para in h.lines() {
                push_wrapped(&mut out, &format!("     {para}"), false, full);
            }
        }
    }
    out
}

fn push_wrapped(out: &mut Vec<(String, bool)>, text: &str, command: bool, full: bool) {
    if full {
        let mut rest = text;
        loop {
            let cut = rest
                .char_indices()
                .nth(WIDTH)
                .map_or(rest.len(), |(i, _)| i);
            out.push((rest[..cut].to_string(), command));
            rest = &rest[cut..];
            if rest.is_empty() {
                break;
            }
        }
    } else {
        out.push((text.chars().take(WIDTH).collect(), command));
    }
}

/// The rows shown: the first row's index and the rows.
fn visible(s: &Session<'_>) -> (usize, Vec<(String, bool)>) {
    let all = rows(s);
    let from = match s.top {
        Some(t) => t.min(all.len().saturating_sub(SHOWN)),
        None => all.len().saturating_sub(SHOWN),
    };
    let to = (from + SHOWN).min(all.len());
    (from, all[from..to].to_vec())
}

/// The Command Entry panel.
fn panel(s: &Session<'_>, field: &str, message: &str) -> Panel {
    // The layout of IBM's Command Entry display, measured on the oracle
    // (tools/conformance/golden/verified/pub400/command-entry.txt).
    let mut p = Panel::default();
    p.text(1, 34, 1, "Command Entry");
    p.text(1, 72, 0, s.system.clone());
    p.text(2, 60, 0, "Request level:   1");
    p.text(
        3,
        2,
        2,
        if s.detailed {
            "All previous commands and messages:"
        } else {
            "Previous commands and messages:"
        },
    );
    let (from, shown) = visible(s);
    let total = rows(s).len();
    if total == 0 {
        p.text(5, 4, 0, "(No previous commands or messages)");
    }
    for (i, (l, command)) in shown.iter().enumerate() {
        let row = u8::try_from(4 + i).unwrap_or(15);
        p.text(row, 2, u8::from(*command), l.clone());
    }
    let at_end = from + shown.len() >= total;
    if at_end {
        p.text(16, 74, 1, "Bottom");
    } else {
        p.text(16, 73, 1, "More...");
    }
    p.text(17, 2, 0, "Type command, press Enter.");
    p.text(18, 2, 1, "===>");
    // One field of 313 cells over rows 18 to 21, as the oracle lays it
    // out; the framebuffer console shows its first row until it becomes
    // a 5250 client (section 7).
    p.field(18, 7, 313, field);
    if s.more_keys {
        p.text(22, 2, 2, "F16=System main menu   F24=More keys");
    } else {
        p.text(
            22,
            2,
            2,
            if s.detailed {
                "F3=Exit   F4=Prompt   F9=Retrieve   F10=Exclude detailed messages"
            } else {
                "F3=Exit   F4=Prompt   F9=Retrieve   F10=Include detailed messages"
            },
        );
        p.text(
            23,
            2,
            2,
            "F11=Display full      F12=Cancel    F13=Information Assistant   F24=More keys",
        );
    }
    p.message = message.to_string();
    p.error = !message.is_empty();
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(history: &[Line], full: bool, detailed: bool) -> Vec<String> {
        rows_of(history, full, detailed)
            .into_iter()
            .map(|(t, _)| t)
            .collect()
    }

    #[test]
    fn wrapping_and_help() {
        let h = vec![
            Line {
                text: " > crtlib x".into(),
                command: true,
                help: None,
            },
            Line {
                text: format!("   CPC2102 {}", "x".repeat(100)),
                command: false,
                help: Some("Library X was created.".into()),
            },
        ];
        assert_eq!(lines(&h, false, false).len(), 2);
        assert_eq!(lines(&h, true, false).len(), 3);
        let d = lines(&h, false, true);
        assert_eq!(d.len(), 3);
        assert!(d[2].starts_with("     Library X"));
        assert!(h[0].command);
    }
}
