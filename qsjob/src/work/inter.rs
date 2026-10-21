// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An interactive job at the console: signed on, routed to `QCMD`,
//! which shows Command Entry because there are no menus yet
//! (`MIGRATE(phase8)`: `INLMNU` and the main menu). Commands typed on
//! its line run as the service console's do; their output and the
//! previous commands scroll above it. `SIGNOFF` or F3 ends the job.

use ipc::chan::Chan;
use stg::remote::status;
use stg::{Client, Shared};
use wm::ctl::{FromJob, Panel, Start};
use wm::job::Status;

use crate::console::Console;

/// Lines of previous commands and messages the panel shows.
const SHOWN: usize = 14;
/// The width a line of output wraps at.
const WIDTH: usize = 76;

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
    let mut history: Vec<String> = Vec::new();
    let mut commands: Vec<String> = Vec::new();
    // The program named at sign-on runs first.
    if !start.rqsdta.trim().is_empty() {
        run_command(
            &mut c,
            link,
            &format!("call {}", start.rqsdta.trim()),
            &mut history,
        );
    }
    let mut field = String::new();
    let mut message = String::new();
    loop {
        c.show(panel(&system, &history, &field, &message));
        status(link, Status::Dspw, "PGM-QCMD");
        let Some((key, fields)) = c.wait_input() else {
            break;
        };
        message.clear();
        field.clear();
        match key {
            3 => break,
            9 => {
                field = commands.last().cloned().unwrap_or_default();
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
        commands.push(cmd.clone());
        history.push(format!(" > {cmd}"));
        let word = cmd
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if word == "signoff" {
            break;
        }
        run_command(&mut c, link, &cmd, &mut history);
    }
    c.ended(0);
    let _ = ending.send(&FromJob::Ending {
        reason: "signed off".into(),
    });
    std::process::exit(0)
}

/// Run one command and add its output to the history.
fn run_command(c: &mut Console, link: &Shared, cmd: &str, history: &mut Vec<String>) {
    let word = cmd.split_whitespace().next().unwrap_or("");
    status(
        link,
        Status::Run,
        &format!("CMD-{}", word.to_ascii_uppercase()),
    );
    c.job().log_request(cmd, ("QCMD", "QSYS"));
    let out = c.line(cmd);
    for l in out.lines() {
        let mut rest = l;
        loop {
            let cut = rest
                .char_indices()
                .nth(WIDTH)
                .map_or(rest.len(), |(i, _)| i);
            history.push(format!("   {}", &rest[..cut]));
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
