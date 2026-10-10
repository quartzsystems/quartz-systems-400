// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The serial service console: a line prompt on the serial port, enabled
//! by `qs400.svc=1`. No shell, no filesystem.

use std::fmt::Write as _;

use licpart::IplMode;

use crate::ipl::lic;
use crate::ipl::power;
use crate::ipl::system::System;
use crate::src;

/// The prompt.
pub const PROMPT: &str = "QS400-SVC> ";

const HELP: &str = "\
help                 this list
srcs [table]         SRC log of this IPL, or the whole SRC table
hist                 IPL history from the LIC partition
hw                   hardware resource table
lic                  LIC record header
sysval NAME [VALUE]  show or set a system value
mode manual|normal   keylock for the next IPL
screen               dump the console screen as text
pwrdwn [restart]     power down (or restart)
";

/// Announce the console after IPL complete.
pub fn banner(system: &mut System) {
    system.out.serial("Service console ready. Type help.\n");
    system.out.serial(PROMPT);
}

/// Execute one command line and reprint the prompt.
pub fn handle(system: &mut System, line: &str) {
    let mut words = line.split_whitespace();
    let cmd = words.next().unwrap_or("");
    let args: Vec<&str> = words.collect();
    let reply = match (cmd, args.as_slice()) {
        ("", _) => String::new(),
        ("help", _) => HELP.to_string(),
        ("srcs", []) => system.log.dump(),
        ("srcs", ["table"]) => src_table(),
        ("hist", _) => hist(system),
        ("hw", _) => "Hardware resource table not available yet\n".to_string(),
        ("lic", _) => lic_dump(system),
        ("sysval", _) => "System values not available yet\n".to_string(),
        ("mode", [m]) => mode(system, m),
        ("screen", _) => "Console screen not available yet\n".to_string(),
        ("pwrdwn", []) => power::down(system, false),
        ("pwrdwn", ["restart"]) => power::down(system, true),
        _ => format!("Unknown command '{cmd}'. Type help.\n"),
    };
    system.out.serial(&reply);
    system.out.serial(PROMPT);
}

fn hist(system: &mut System) -> String {
    let Some((_, record)) = system.lic.as_ref() else {
        return "LIC partition not attached\n".to_string();
    };
    if record.history.is_empty() {
        return "No IPL history\n".to_string();
    }
    let mut s =
        String::from("  #  start       end         type mode    slot final     outcome   text\n");
    for (i, h) in record.history.iter().enumerate() {
        let text = src::lookup(h.final_src_str()).map_or("", |s| s.text);
        let _ = writeln!(
            s,
            "{:3}  {:<11} {:<11} {:<4} {:<7} {:<4} {:<9} {:<10} {text}",
            i + 1,
            h.start_time,
            h.end_time,
            format!("{:?}", h.ipl_type),
            format!("{:?}", h.mode),
            format!("{:?}", h.slot),
            h.final_src_str(),
            format!("{:?}", h.outcome),
        );
    }
    s
}

fn src_table() -> String {
    let mut s = String::new();
    for e in src::TABLE {
        let _ = writeln!(s, "{}  {:<10} {}", e.code, format!("{:?}", e.phase), e.text);
    }
    s
}

fn lic_dump(system: &mut System) -> String {
    let Some((_, record)) = system.lic.as_ref() else {
        return "LIC partition not attached\n".to_string();
    };
    let h = &record.header;
    format!(
        "sequence        {}\n\
         ipl_in_progress {}\n\
         last_ipl_type   {:?}\n\
         last_ipl_mode   {:?}\n\
         last_slot       {:?}\n\
         last_good_slot  {:?}\n\
         next_mode       {:?}\n\
         ipl_count       {}\n\
         abnormal_count  {}\n\
         install_time    {}\n\
         history         {} record(s)\n\
         tlvs            {} entr(ies)\n",
        record.sequence,
        h.ipl_in_progress,
        h.last_ipl_type,
        h.last_ipl_mode,
        h.last_slot,
        h.last_good_slot,
        h.next_mode,
        h.ipl_count,
        h.abnormal_count,
        h.install_time,
        record.history.len(),
        record.tlvs.len()
    )
}

fn mode(system: &mut System, m: &str) -> String {
    let mode = match m {
        "manual" => IplMode::Manual,
        "normal" => IplMode::Normal,
        _ => return "Usage: mode manual|normal\n".to_string(),
    };
    match lic::set_next_mode(system, mode) {
        Ok(()) => format!("Next IPL will be {mode:?}\n"),
        Err(e) => format!("Cannot set mode: {e}\n"),
    }
}
