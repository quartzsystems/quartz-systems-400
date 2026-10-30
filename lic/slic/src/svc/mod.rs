// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The serial service console: a line prompt on the serial port, enabled
//! by `qs400.svc=1`. No shell, no filesystem.

mod authority;
pub(crate) mod db;
pub(crate) mod ile;
pub(crate) mod ifs;
mod jobq;
pub mod neta;
mod msgq;
pub(crate) mod objects;
mod sbs;
pub(crate) mod security;
mod spool;
mod storage;
pub mod ui;
pub(crate) mod work;

use std::fmt::Write as _;

use licpart::IplMode;

use crate::ipl::lic;
use crate::ipl::power;
use crate::ipl::system::System;
use crate::src;

/// The prompt.
pub const PROMPT: &str = wm::ctl::PROMPT;

const HELP: &str = "\
help                 this list
srcs [table]         SRC log of this IPL, or the whole SRC table
hist                 IPL history from the LIC partition
hw                   hardware resource table
lic                  LIC record header
sysval NAME [VALUE]  show or set a system value
mode manual|normal   keylock for the next IPL
screen               dump the console screen as text
key NAME             press a key on the console: enter f1..f24 tab backtab fieldexit home end left right up down backspace delete insert reset attn
type TEXT            type text into the current console field
pwrdwn [restart]     power down (or restart)
release              version, release, modification and build
asp                  ASP units, free space, WAL, checkpoint generation
unit add DEV | rmv N          add a block device as an unprotected unit, or remove the last unit
mirror start N DEV | stop N | suspend N | resume N | replace N DEV | status
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
ifs                  the IFS mount, its capacity and free space
";

/// Help for the database commands, after the general list.
const HELP_DB: &str = db::HELP;

/// Announce the console after IPL complete.
pub fn banner(system: &mut System) {
    system.out.serial("Service console ready. Type help.\n");
    system.out.serial(PROMPT);
}

/// Execute one command line and reprint the prompt.
pub fn handle(system: &mut System, line: &str) {
    if let Some(reply) = db::intercept(system, line) {
        system.out.serial(&reply);
        system.out.serial(db::prompt(system));
        return;
    }
    let reply = reply(system, line);
    system.out.serial(&reply);
    system.out.serial(db::prompt(system));
}

/// `dbgkill JOB(name|number)`: the debug hook that kills a job's
/// process as a crash would (`SIGKILL`), for the harness to see the
/// storage service clean up after it.
fn dbgkill(system: &mut System, line: &str) -> String {
    let p = wm::cmd::Params::parse(line);
    let Ok(want) = p.pos_or(0, "JOB") else {
        return "dbgkill JOB(name|number)\n".into();
    };
    let want = want.to_ascii_uppercase();
    let Some(stg) = system.procs.stg.as_mut() else {
        return "the storage service is not running\n".into();
    };
    let jobs = stg::Client::active_jobs(stg).unwrap_or_default();
    let Some(j) = jobs
        .iter()
        .find(|j| j.name == want || format!("{:06}", j.number) == want)
    else {
        return format!("no active job {want}\n");
    };
    let r = rustix::process::Pid::from_raw(i32::try_from(j.pid).unwrap_or(0))
        .ok_or_else(|| "no process".to_string())
        .and_then(|pid| {
            rustix::process::kill_process(pid, rustix::process::Signal::KILL)
                .map_err(|e| e.to_string())
        });
    match r {
        Ok(()) => format!("Job {:06}/{}/{} killed.\n", j.number, j.user, j.name),
        Err(e) => format!("dbgkill: {e}\n"),
    }
}

/// Execute one command line; its reply.
pub fn reply(system: &mut System, line: &str) -> String {
    let mut words = line.split_whitespace();
    let cmd = words.next().unwrap_or("");
    let args: Vec<&str> = words.collect();
    match (cmd, args.as_slice()) {
        ("", _) => String::new(),
        ("help", _) => format!(
            "{HELP}{}{HELP_DB}{}{}{}{}{}{}{}{}{}",
            ifs::HELP,
            ile::HELP,
            work::HELP,
            msgq::HELP,
            msgq::HELP_RPYL,
            sbs::HELP,
            jobq::HELP,
            spool::HELP,
            security::HELP_AUDIT,
            authority::HELP
        ),
        ("release", _) => format!("{}\n", release::full()),
        ("srcs", []) => system.log.dump(),
        ("srcs", ["table"]) => src_table(),
        ("hist", _) => hist(system),
        ("hw", _) => {
            system.hw.refresh_ports(std::path::Path::new("/sys"));
            format!("{}\n{}", system.hw.dump(), crate::net::dump(&system.links))
        }
        ("lic", _) => lic_dump(system),
        ("sysval", rest) => sysval(system, rest),
        ("mode", [m]) => mode(system, m),
        ("screen", _) => crate::ipl::display::dump(system),
        ("tn5250", rest) => crate::ipl::link::command(system, rest),
        ("key", [name]) => key(system, name),
        ("type", _) => type_text(system, line),
        ("dbgkill", _) => dbgkill(system, line),
        ("pwrdwn", []) => power::down(system, false),
        ("pwrdwn", ["restart"]) => power::down(system, true),
        ("asp", _) => storage::asp(system),
        ("unit", rest) => storage::unit(system, rest),
        ("mirror", rest) => storage::mirror(system, rest),
        ("lib", ["ls"]) => objects::lib_ls(system),
        ("obj", rest) => objects::obj(system, rest),
        ("dtaara", rest) => objects::dtaara_cmd(system, rest),
        ("dtaq", rest) => objects::dtaq_cmd(system, rest),
        ("usrspc", rest) => objects::usrspc_cmd(system, rest),
        ("rclstg", rest) => storage::rclstg(system, rest),
        ("chkstg", _) => storage::chkstg(system),
        ("txn", ["stats"]) => storage::txn_stats(system),
        ("ifs", rest) => ifs::command(system, rest, line),
        _ => match db::command(system, cmd, &args, line) {
            Some(reply) => reply,
            None => match ile::command(system, cmd, &args, line)
                .or_else(|| work::command(system, cmd, line))
                .or_else(|| work::libl_command(system, cmd, line))
                .or_else(|| msgq::command(system, cmd, line))
                .or_else(|| msgq::rpyl_command(system, cmd, line))
                .or_else(|| sbs::command(system, cmd, line))
                .or_else(|| jobq::command(system, cmd, line))
                .or_else(|| spool::command(system, cmd, line))
                .or_else(|| security::audit_command(system, cmd, line))
                .or_else(|| authority::command(system, cmd, line))
            {
                Some(reply) => reply,
                None => format!(
                    "Unknown command '{cmd}'. Type help.
"
                ),
            },
        },
    }
}

/// Commands that work while a menu or DST owns the serial line; any
/// other line is typed into the current field. Returns whether the
/// line was a command.
pub fn during_menu(system: &mut System, line: &str) -> bool {
    let word = line.split_whitespace().next().unwrap_or("");
    let is_command = matches!(
        word,
        "screen"
            | "key"
            | "type"
            | "srcs"
            | "hw"
            | "lic"
            | "hist"
            | "help"
            | "release"
            | "asp"
            | "unit"
            | "mirror"
    );
    if is_command && system.boot.svc {
        handle(system, line);
    }
    is_command && system.boot.svc
}

/// `key NAME`: a 5250 key on the console.
fn key(system: &mut System, name: &str) -> String {
    use console::Key;
    let lower = name.to_ascii_lowercase();
    let key = match lower.as_str() {
        "enter" => Key::Enter,
        "tab" => Key::FieldAdvance,
        "backtab" => Key::FieldBackspace,
        "fieldexit" => Key::FieldExit,
        "home" => Key::Home,
        "left" => Key::Left,
        "right" => Key::Right,
        "up" => Key::Up,
        "down" => Key::Down,
        "backspace" => Key::Backspace,
        "delete" => Key::Delete,
        "insert" => Key::Insert,
        "reset" => Key::Reset,
        "attn" => Key::Attention,
        f if f.starts_with('f') => match f[1..].parse::<u8>() {
            Ok(n) if (1..=24).contains(&n) => Key::Function(n),
            _ => return format!("Unknown key '{name}'\n"),
        },
        _ => return format!("Unknown key '{name}'\n"),
    };
    system.show(console::Command::Virtual(key));
    String::new()
}

/// `type TEXT`: the rest of the line into the current field.
fn type_text(system: &mut System, line: &str) -> String {
    let text = line
        .trim_start()
        .strip_prefix("type")
        .unwrap_or("")
        .trim_start();
    for c in text.trim_end().chars() {
        system.show(console::Command::Virtual(console::Key::Char(c)));
    }
    String::new()
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
            let security = vals.catalog().get(&name).is_some_and(|e| e.group == sysval::Group::Sec);
            let actor = crate::stgsvc::security::actor();
            let dst = crate::stgsvc::security::dst_allows_sysval_changes(system.lic.as_ref().map(|(_, r)| r));
            if let Err(e) = sec::sysval::require_change(actor.as_ref(), &name, security, dst) {
                return format!("{e}\n");
            }
            let journal = crate::stgsvc::security::audit_journal_exists(system);
            if let Err(e) = sec::sysval::validate(&name, &rest.join(" "), journal) {
                return format!("{e}\n");
            }
            let Some(vals) = system.sysvals.as_mut() else {
                return "System values not loaded\n".to_string();
            };
            let before = vals.get(&name).map(|v| v.text()).unwrap_or_default();
            match vals.set_text(&name, &rest.join(" ")) {
                Ok(effect) => {
                    let now = vals.get(&name).map(|v| v.text()).unwrap_or_default();
                    sec::audit::record(
                        sec::audit::Event::new(sec::audit::Kind::Sv)
                            .set("ETYP", "A")
                            .set("SYSV", name.as_str())
                            .set("NVAL", now.as_str())
                            .set("OVAL", before.as_str()),
                    );
                    let saved = crate::ipl::sysvals::save(system);
                    if saved.is_ok() && system.role == crate::ipl::system::Role::Storage {
                        let data = wm::joblog::data(&[(&name, 10), (&before, 80), (&now, 80)]);
                        let who = crate::stgsvc::msgs::sender(system, system.current_job);
                        crate::stgsvc::hist::system_message(system, "CPF1806", &data, "04", who);
                    }
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
