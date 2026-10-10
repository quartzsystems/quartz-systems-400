// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The serial service console: a line prompt on the serial port, enabled
//! by `qs400.svc=1`. No shell, no filesystem.

mod objects;
mod storage;

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
release              version, release, modification and build
asp                  ASP units, free space, WAL, checkpoint generation
lib ls               every library, EBCDIC order
obj ls LIB [TYPE]    objects of a library
obj crt TYPE LIB/NAME [TEXT]   create (defaults for *DTAARA, *DTAQ, *USRSPC, *USRIDX)
obj dlt TYPE LIB/NAME          delete
obj rnm TYPE LIB/NAME NEWNAME  rename
obj mov TYPE LIB/NAME TOLIB    move
obj dsp LIB/NAME     object description, every type of that name
obj bench N          create and delete N data areas over ten libraries
dtaara get|set LIB/NAME [VALUE]
dtaq snd|rcv LIB/NAME [DATA]
usrspc rd LIB/NAME OFFSET [LEN] | wr LIB/NAME OFFSET TEXT
rclstg [*CHECK]      reclaim storage (*ALL default), or verify only
chkstg               checksum sweep of every allocated page
txn stats            transaction, WAL and cache counters
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
        ("release", _) => format!("{}\n", release::full()),
        ("srcs", []) => system.log.dump(),
        ("srcs", ["table"]) => src_table(),
        ("hist", _) => hist(system),
        ("hw", _) => format!("{}\n{}", system.hw.dump(), crate::net::dump(&system.links)),
        ("lic", _) => lic_dump(system),
        ("sysval", rest) => sysval(system, rest),
        ("mode", [m]) => mode(system, m),
        ("screen", _) => crate::ipl::display::dump(system),
        ("pwrdwn", []) => power::down(system, false),
        ("pwrdwn", ["restart"]) => power::down(system, true),
        ("asp", _) => storage::asp(system),
        ("lib", ["ls"]) => objects::lib_ls(system),
        ("obj", rest) => objects::obj(system, rest),
        ("dtaara", rest) => objects::dtaara_cmd(system, rest),
        ("dtaq", rest) => objects::dtaq_cmd(system, rest),
        ("usrspc", rest) => objects::usrspc_cmd(system, rest),
        ("rclstg", rest) => storage::rclstg(system, rest),
        ("chkstg", _) => storage::chkstg(system),
        ("txn", ["stats"]) => storage::txn_stats(system),
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
    let mut out = format!(
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
    );
    for t in &record.tlvs {
        let _ = writeln!(out, "  tlv {:#06x} ({} bytes)", t.tag, t.data.len());
    }
    out
}

fn sysval(system: &mut System, args: &[&str]) -> String {
    let Some(vals) = system.sysvals.as_mut() else {
        return "System values not loaded\n".to_string();
    };
    match args {
        [] => "Usage: sysval NAME [VALUE]\n".to_string(),
        [name] => {
            let name = name.to_ascii_uppercase();
            match vals.get(&name) {
                Ok(v) => {
                    let text = vals.catalog().get(&name).map_or("", |e| e.text.as_str());
                    format!("{name}: {v}\n  {text}\n")
                }
                Err(e) => format!("{e}\n"),
            }
        }
        [name, rest @ ..] => {
            let name = name.to_ascii_uppercase();
            match vals.set_text(&name, &rest.join(" ")) {
                Ok(effect) => {
                    let now = vals.get(&name).map(|v| v.text()).unwrap_or_default();
                    let saved = crate::ipl::sysvals::save(system);
                    let when = match effect {
                        sysval::Effect::Immediate => "",
                        sysval::Effect::NextIpl => " (takes effect at the next IPL)",
                    };
                    match saved {
                        Ok(()) => format!("{name} set to {now}{when}\n"),
                        Err(e) => format!("{name} set to {now}{when}, but not saved: {e}\n"),
                    }
                }
                Err(e) => format!("{e}\n"),
            }
        }
    }
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
