// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SITE` (IBM's subcommands: `NAMEFMT`, `LISTFMT`, `TRIM`,
//! `NULLFLDS`, `CRTCCSID`), `RCMD` (a CL command in the session's job)
//! and IBM's object commands (`ADDM`, `ADDV`, `CRTL`, `CRTP`, `CRTS`,
//! `DLTF`, `DLTL`), which run the CL command they stand for.

use regfac::formats::tcp::op;
use servers::attrs::ftp::CrtCcsid;

use super::Session;
use crate::host::Host;
use crate::names;
use crate::net::Net;
use crate::reply::Reply;

impl<H: Host> Session<H> {
    pub(super) fn site(&mut self, arg: &str) -> Reply {
        let mut words = arg.split_whitespace();
        let sub = words.next().unwrap_or("").to_ascii_uppercase();
        // Words after the value are not looked at.
        let value = words.next().map(str::to_ascii_uppercase);
        let flag = |v: &str, max: u8| v.parse::<u8>().ok().filter(|n| *n <= max);
        match (sub.as_str(), value.as_deref()) {
            ("NAMEFMT", None) => Reply::new(250, format!("Naming format \"{}\" is in effect.", self.namefmt)),
            ("NAMEFMT", Some(v)) => match flag(v, 1) {
                Some(0) if names::library_of(&self.cwd).is_none() => {
                    Reply::new(550, "Name format 0 needs a library as the current directory.")
                }
                Some(n) => {
                    self.namefmt = n;
                    self.lib_mode = false;
                    Reply::new(250, format!("Naming format \"{n}\" is in effect."))
                }
                None => Reply::new(501, "SITE NAMEFMT takes 0 or 1."),
            },
            ("LISTFMT", None) => Reply::new(250, format!("Listing format (LISTFMT) is {}.", self.listfmt)),
            ("LISTFMT", Some(v)) => match flag(v, 1) {
                Some(n) => {
                    self.listfmt = n;
                    Reply::new(250, format!("Listing format (LISTFMT) is {n}."))
                }
                None => Reply::new(501, "SITE LISTFMT takes 0 or 1."),
            },
            ("TRIM", None) => Reply::new(250, format!("Trailing blanks dropped from records sent (TRIM): {}.", self.trim)),
            ("TRIM", Some(v)) => match flag(v, 2) {
                Some(n) => {
                    self.trim = n;
                    Reply::new(250, format!("Trailing blanks dropped from records sent (TRIM): {n}."))
                }
                None => Reply::new(501, "SITE TRIM takes 0, 1 or 2."),
            },
            ("NULLFLDS", None) => Reply::new(250, format!("Files with null-capable fields (NULLFLDS): {}.", self.nullflds)),
            ("NULLFLDS", Some(v)) => match flag(v, 1) {
                Some(n) => {
                    self.nullflds = n;
                    Reply::new(250, format!("Files with null-capable fields (NULLFLDS): {n}."))
                }
                None => Reply::new(501, "SITE NULLFLDS takes 0 or 1."),
            },
            ("CRTCCSID", None) => Reply::new(250, format!("CCSID of files STOR creates (CRTCCSID): {}.", self.crtccsid.text())),
            ("CRTCCSID", Some(v)) => match CrtCcsid::parse(v) {
                Ok(c) if v != "0" && v != "-2" && v != "-3" => {
                    self.crtccsid = c;
                    Reply::new(250, format!("CCSID of files STOR creates (CRTCCSID): {}.", c.text()))
                }
                _ => Reply::new(501, "SITE CRTCCSID takes *CALC, *USER, *SYSVAL or a CCSID from 1 to 65533."),
            },
            _ => Reply::new(550, format!("SITE {} is not a subcommand of this server.", sub.trim())),
        }
    }

    pub(super) fn rcmd(&mut self, arg: &str, net: &mut dyn Net) -> Reply {
        let cmd = arg.trim();
        if cmd.is_empty() {
            return Reply::new(501, "RCMD needs a CL command.");
        }
        if !self.allowed(net, op::COMMAND, cmd) {
            return Reply::new(550, format!("An exit program refused command {cmd}."));
        }
        match self.host.command(cmd) {
            Ok(()) => {
                self.after_command(cmd);
                Reply::new(250, format!("Command {cmd} ran."))
            }
            Err(e) => {
                let text = if e.text.trim().is_empty() { e.msgid.clone() } else { e.text.trim().to_string() };
                Reply::lines(550, vec![format!("Command {cmd} failed."), text])
            }
        }
    }

    /// What a command run by `RCMD` changes in the session.
    fn after_command(&mut self, cmd: &str) {
        let upper = cmd.to_ascii_uppercase();
        if upper.starts_with("CHGCURLIB")
            && let Some(lib) = servers::attrs::keyword(cmd, "CURLIB")
        {
            let lib = lib.trim().to_ascii_uppercase();
            if let Some(p) = &mut self.user {
                p.curlib.clone_from(&lib);
            }
            // The current library is the session's in a library.
            if self.curlib().is_some() && !lib.starts_with('*') {
                self.cwd = names::library_dir(&lib);
            }
        }
    }

    /// IBM's object commands, as the CL commands they stand for.
    pub(super) fn object_cmd(&mut self, verb: &str, arg: &str, net: &mut dyn Net) -> Reply {
        let words: Vec<&str> = arg.split_whitespace().collect();
        let cur = self.curlib().unwrap_or_else(|| "QGPL".into());
        let qual = |name: &str| -> String {
            match names::parse_lib_name(name) {
                Some(n) => format!("{}/{}", n.lib.unwrap_or_else(|| cur.clone()), n.file),
                None => name.to_ascii_uppercase(),
            }
        };
        let cmd = match (verb, words.as_slice()) {
            ("ADDM" | "ADDV", [file, mbr, ..]) => format!("ADDPFM FILE({}) MBR({})", qual(file), mbr.to_ascii_uppercase()),
            ("CRTL", [lib, ..]) => format!("CRTLIB LIB({})", lib.to_ascii_uppercase()),
            ("CRTP", [file]) => format!("CRTPF FILE({}) RCDLEN(80)", qual(file)),
            ("CRTP", [file, len, ..]) => format!("CRTPF FILE({}) RCDLEN({len})", qual(file)),
            ("CRTS", [file]) => format!("CRTSRCPF FILE({})", qual(file)),
            ("CRTS", [file, len, ..]) => format!("CRTSRCPF FILE({}) RCDLEN({len})", qual(file)),
            ("DLTF", [file, ..]) => format!("DLTF FILE({})", qual(file)),
            ("DLTL", [lib, ..]) => format!("DLTLIB LIB({})", lib.to_ascii_uppercase()),
            _ => return Reply::new(501, format!("{verb} needs its names; HELP {verb} says which.")),
        };
        let op = match verb {
            "CRTL" => op::CREATE_DIR,
            "DLTL" => op::DELETE_DIR,
            "DLTF" => op::DELETE_FILE,
            _ => op::COMMAND,
        };
        if !self.allowed(net, op, &cmd) {
            return Reply::new(550, format!("An exit program refused {verb}."));
        }
        match self.host.command(&cmd) {
            Ok(()) => Reply::new(250, format!("{verb} done: {cmd}.")),
            Err(e) => Reply::lines(550, vec![format!("{verb} failed: {cmd}."), e.text.trim().to_string()]),
        }
    }
}
