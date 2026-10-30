// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The commands about the session itself: `SYST`, `FEAT`, `HELP`,
//! `STAT`, `OPTS`, `TYPE`, `MODE`, `STRU`, the data connection's
//! `PASV`, `EPSV`, `PORT`, `EPRT`, `REST`, and TLS's `AUTH`, `PBSZ`,
//! `PROT`, `CCC`.

use std::net::{IpAddr, SocketAddr};

use super::{Flow, Session};
use crate::host::Host;
use crate::net::Net;
use crate::net::tcp::pasv_text;
use crate::reply::Reply;
use crate::xlate::Rep;

/// Every command, with its line for `HELP`.
pub const COMMANDS: &[(&str, &str)] = &[
    ("ABOR", "ABOR: stop the transfer that is running."),
    ("ACCT", "ACCT <account>: no account is needed on this server."),
    ("ADDM", "ADDM <file> <member>: add a member to a physical file."),
    ("ADDV", "ADDV <file> <member>: add a member to a physical file."),
    ("ALLO", "ALLO <bytes>: accepted; no space needs to be set aside."),
    ("APPE", "APPE <name>: add the data sent to the end of a file or member."),
    ("AUTH", "AUTH TLS: protect the control connection with TLS."),
    ("CCC", "CCC: carry on with the control connection in the clear."),
    ("CDUP", "CDUP: go to the parent directory or to /QSYS.LIB."),
    ("CRTL", "CRTL <library>: create a library."),
    ("CRTP", "CRTP <file> [<record length>]: create a physical file."),
    ("CRTS", "CRTS <file> [<record length>]: create a source physical file."),
    ("CWD", "CWD <directory or library>: change the current directory or library."),
    ("DELE", "DELE <name>: delete a file or member."),
    ("DLTF", "DLTF <file>: delete a database file."),
    ("DLTL", "DLTL <library>: delete a library."),
    ("EPRT", "EPRT |<proto>|<address>|<port>|: make the next data connection to this address."),
    ("EPSV", "EPSV: wait for the next data connection on a port this reply names."),
    ("FEAT", "FEAT: list the extensions this server has."),
    ("HELP", "HELP [<command>]: show the commands, or what one does."),
    ("LIST", "LIST [<name>]: list a directory, library, file or member in full."),
    ("MDTM", "MDTM <name>: the time a file last changed."),
    ("MKD", "MKD <name>: create a directory or library."),
    ("MLSD", "MLSD [<directory>]: list a directory in the machine form of RFC 3659."),
    ("MLST", "MLST [<name>]: describe one object in the machine form of RFC 3659."),
    ("MODE", "MODE <S or B>: stream or block transfer mode."),
    ("NLST", "NLST [<name>]: list names only."),
    ("NOOP", "NOOP: do nothing."),
    ("OPTS", "OPTS <command> <options>: set a command's options."),
    ("PASS", "PASS <password>: the password of the USER."),
    ("PASV", "PASV: wait for the next data connection on the address this reply names."),
    ("PBSZ", "PBSZ 0: the protection buffer size, after AUTH TLS."),
    ("PORT", "PORT <h1,h2,h3,h4,p1,p2>: make the next data connection to this address."),
    ("PROT", "PROT <C or P>: data connections in the clear or under TLS."),
    ("PWD", "PWD: show the current directory or library."),
    ("QUIT", "QUIT: end the session."),
    ("RCMD", "RCMD <command>: run a CL command in this session's job."),
    ("REIN", "REIN: sign off and start the session again."),
    ("REST", "REST <offset>: start the next transfer at this byte."),
    ("RETR", "RETR <name>: send a file or member to the client."),
    ("RMD", "RMD <name>: delete a directory or library."),
    ("RNFR", "RNFR <name>: the object to rename; RNTO follows."),
    ("RNTO", "RNTO <name>: the new name of the object RNFR named."),
    ("SITE", "SITE <subcommand>: NAMEFMT, LISTFMT, TRIM, NULLFLDS, CRTCCSID."),
    ("SIZE", "SIZE <name>: how many bytes RETR would send."),
    ("STAT", "STAT [<name>]: the session's state, or a listing on the control connection."),
    ("STOR", "STOR <name>: store the data sent, replacing what was there."),
    ("STOU", "STOU <name>: store the data sent under a name not yet used."),
    ("STRU", "STRU <F or R>: file or record structure."),
    ("SYST", "SYST: the operating system's name."),
    ("TYPE", "TYPE <A, E, I, L 8, B, F or C n>: the data representation."),
    ("USER", "USER <name>: the user profile to sign on as."),
    ("XCUP", "XCUP: as CDUP."),
    ("XCWD", "XCWD <name>: as CWD."),
    ("XMKD", "XMKD <name>: as MKD."),
    ("XPWD", "XPWD: as PWD."),
    ("XRMD", "XRMD <name>: as RMD."),
];

/// `FEAT`.
pub fn feat() -> Reply {
    let mut lines = vec!["Extensions this server has:".to_string()];
    for f in [
        "AUTH TLS",
        "CCC",
        "EPRT",
        "EPSV",
        "MDTM",
        "MLST type*;size*;modify*;perm*;",
        "PBSZ",
        "PROT",
        "REST STREAM",
        "SIZE",
    ] {
        lines.push(format!(" {f}"));
    }
    lines.push("End of the list.".into());
    Reply::bare(211, lines)
}

/// `HELP` and `HELP cmd`.
pub fn help(arg: &str) -> Reply {
    let arg = arg.trim().to_ascii_uppercase();
    if arg.is_empty() {
        let mut lines = vec!["The commands of this server:".to_string()];
        let names: Vec<&str> = COMMANDS.iter().map(|(v, _)| *v).collect();
        for chunk in names.chunks(12) {
            lines.push(chunk.join(" "));
        }
        lines.push("Name format 0: LIBRARY/FILE.MEMBER, for files in libraries.".into());
        lines.push("Name format 1: paths, as /QSYS.LIB/LIBRARY.LIB/FILE.FILE/MEMBER.MBR or /home/user/file.".into());
        lines.push("HELP <command> says what one command does.".into());
        return Reply::lines(214, lines);
    }
    if arg == "SITE" {
        return Reply::lines(
            214,
            vec![
                "SITE NAMEFMT <0 or 1>: library names (0) or paths (1).".into(),
                "SITE LISTFMT <0 or 1>: listings in the IBM i layout (0) or the Unix one (1).".into(),
                "SITE TRIM <0 or 1>: drop trailing blanks of records sent (1).".into(),
                "SITE NULLFLDS <0 or 1>: allow files with null-capable fields (1).".into(),
                "SITE CRTCCSID <*CALC, *USER, *SYSVAL or 1-65533>: the CCSID of files STOR creates.".into(),
            ],
        );
    }
    match COMMANDS.iter().find(|(v, _)| *v == arg) {
        Some((_, text)) => Reply::new(214, *text),
        None => help(""),
    }
}

/// `OPTS`.
pub fn opts(arg: &str) -> Reply {
    let mut words = arg.split_whitespace();
    match words.next().map(str::to_ascii_uppercase).as_deref() {
        Some("MLST") => {
            let facts: Vec<String> = words
                .next()
                .unwrap_or("")
                .split(';')
                .filter(|f| !f.is_empty())
                .map(str::to_ascii_lowercase)
                .filter(|f| ["type", "size", "modify", "perm"].contains(&f.as_str()))
                .map(|f| format!("{f};"))
                .collect();
            Reply::new(200, format!("MLST OPTS {}", facts.concat()))
        }
        _ => Reply::new(501, "OPTS takes MLST only on this server."),
    }
}

/// A `PORT` argument as an address.
fn port_addr(arg: &str) -> Option<SocketAddr> {
    let n: Vec<u8> = arg.split(',').map(|p| p.trim().parse::<u8>().ok()).collect::<Option<_>>()?;
    if n.len() != 6 {
        return None;
    }
    let ip = IpAddr::from([n[0], n[1], n[2], n[3]]);
    Some(SocketAddr::new(ip, u16::from(n[4]) << 8 | u16::from(n[5])))
}

/// An `EPRT` argument as an address.
fn eprt_addr(arg: &str) -> Option<SocketAddr> {
    let d = arg.chars().next()?;
    let parts: Vec<&str> = arg.split(d).collect();
    if parts.len() != 5 {
        return None;
    }
    let ip: IpAddr = parts[2].parse().ok()?;
    match (parts[1], ip) {
        ("1", IpAddr::V4(_)) | ("2", IpAddr::V6(_)) => {}
        _ => return None,
    }
    Some(SocketAddr::new(ip, parts[3].parse().ok()?))
}

#[allow(clippy::unused_self)] // every command is the session's
impl<H: Host> Session<H> {
    pub(super) fn syst(&self) -> Reply {
        Reply::new(
            215,
            format!(
                " OS/400 is the remote operating system. The TCP/IP version is \"{}\".",
                self.cfg.release
            ),
        )
    }

    pub(super) fn stat(&mut self, arg: &str) -> Reply {
        if !arg.trim().is_empty() {
            return Reply::new(504, "STAT takes no name on this server; use LIST.");
        }
        let rep = match self.rep {
            Rep::Ascii => format!("ASCII, CCSID {}", self.ascii()),
            Rep::Ebcdic => "EBCDIC".into(),
            Rep::Image => "binary image".into(),
            Rep::Ccsid(c) => format!("CCSID {c}"),
        };
        let user = self.user().unwrap_or("not signed on").to_string();
        let lines = vec![
            format!("Quartz FTP server at {}, session state:", self.cfg.system),
            format!("Naming format (NAMEFMT) {}.", self.namefmt),
            format!("Listing format (LISTFMT) {}.", self.listfmt),
            format!("Trailing blanks dropped from records sent (TRIM) {}.", self.trim),
            format!("Files with null-capable fields allowed (NULLFLDS) {}.", self.nullflds),
            format!("CCSID of database files created (CRTCCSID) {}.", self.crtccsid.text()),
            format!("Signed on as {user}; current directory {}.", self.cwd),
            format!("Data representation {rep}; mode {}; structure {}.",
                if self.block { "block" } else { "stream" },
                if self.record { "record" } else { "file" }),
            format!("{} bytes of control data so far.", self.ctl_bytes),
            if self.cfg.attrs.inacttimo == 0 {
                "No inactivity limit.".into()
            } else {
                format!("Inactivity limit {} seconds.", self.cfg.attrs.inacttimo)
            },
            "End of status.".into(),
        ];
        Reply::lines(211, lines)
    }

    pub(super) fn type_cmd(&mut self, arg: &str) -> Reply {
        let words: Vec<String> = arg.split_whitespace().map(str::to_ascii_uppercase).collect();
        let first = words.first().map_or("", String::as_str);
        let second = words.get(1).map(String::as_str);
        let stays = |code: u16, why: String| Reply::lines(code, vec![why, "The representation type stays as it was.".into()]);
        let (rep, text) = match (first, second) {
            ("A", None | Some("N")) => (Rep::Ascii, "ASCII".to_string()),
            ("E", None | Some("N")) => (Rep::Ebcdic, "EBCDIC".to_string()),
            ("I", None) | ("L", Some("8")) => (Rep::Image, "binary image".to_string()),
            ("A" | "E" | "I", Some(f)) => return stays(504, format!("Format {first} {f} is not supported.")),
            ("L", _) => return Reply::new(504, "Only a byte size of 8 is supported."),
            ("B", _) => (Rep::Ascii, "ASCII (double-byte)".to_string()),
            ("F", _) => (Rep::Ebcdic, "EBCDIC (double-byte)".to_string()),
            ("C", Some(n)) => match n.parse::<u16>() {
                Ok(c) if crate::xlate::is_text(c) => (Rep::Ccsid(c), format!("CCSID {c}")),
                _ => return stays(504, format!("CCSID {n} cannot be used for transfers.")),
            },
            _ => {
                return stays(501, format!("Type {} is not valid; use A, E, I, L 8, B, F or C.", arg.trim()));
            }
        };
        self.rep = rep;
        Reply::new(200, format!("Representation type is now {text}."))
    }

    pub(super) fn mode_cmd(&mut self, arg: &str) -> Reply {
        let stays = |code: u16, why: &str, block: bool| {
            Reply::lines(
                code,
                vec![
                    why.to_string(),
                    format!("Transfer mode stays {}.", if block { "block" } else { "stream" }),
                ],
            )
        };
        match arg.trim().to_ascii_uppercase().as_str() {
            "S" => {
                self.block = false;
                Reply::new(200, "Transfer mode is now stream.")
            }
            "B" => {
                self.block = true;
                Reply::new(200, "Transfer mode is now block.")
            }
            "C" => stays(504, "Compressed mode is not supported; use S or B.", self.block),
            m => stays(501, &format!("Transfer mode {m} is not valid."), self.block),
        }
    }

    pub(super) fn stru_cmd(&mut self, arg: &str) -> Reply {
        let stays = |code: u16, why: &str, record: bool| {
            Reply::lines(
                code,
                vec![
                    why.to_string(),
                    format!("Data structure stays {}.", if record { "record" } else { "file" }),
                ],
            )
        };
        match arg.trim().to_ascii_uppercase().as_str() {
            "F" => {
                self.record = false;
                Reply::new(250, "Data structure is now file.")
            }
            "R" => {
                self.record = true;
                Reply::new(250, "Data structure is now record.")
            }
            "P" => stays(504, "Page structure is not supported.", self.record),
            s => stays(501, &format!("Data structure {s} is not valid."), self.record),
        }
    }

    pub(super) fn pasv(&mut self, net: &mut dyn Net) -> Reply {
        match net.passive(false) {
            Ok(a) => match pasv_text(a) {
                Some(t) => Reply::new(227, format!("Entering Passive Mode ({t}).")),
                None => Reply::new(425, "PASV needs IPv4; use EPSV."),
            },
            Err(e) => Reply::new(425, format!("No data port could be opened: {e}.")),
        }
    }

    pub(super) fn epsv(&mut self, _arg: &str, net: &mut dyn Net) -> Reply {
        match net.passive(true) {
            Ok(a) => Reply::new(229, format!("Entering Extended Passive Mode (|||{}|)", a.port())),
            Err(e) => Reply::new(425, format!("No data port could be opened: {e}.")),
        }
    }

    pub(super) fn port(&mut self, arg: &str, net: &mut dyn Net) -> Reply {
        let Some(a) = port_addr(arg) else {
            return Reply::new(501, "PORT needs h1,h2,h3,h4,p1,p2.");
        };
        self.active_to(a, net)
    }

    pub(super) fn eprt(&mut self, arg: &str, net: &mut dyn Net) -> Reply {
        let Some(a) = eprt_addr(arg.trim()) else {
            return Reply::new(501, "EPRT needs |1|address|port| or |2|address|port|.");
        };
        self.active_to(a, net)
    }

    /// The address the next data connection goes to; the connection
    /// itself goes only to the client's own address.
    fn active_to(&mut self, a: SocketAddr, net: &mut dyn Net) -> Reply {
        match net.active(a) {
            Ok(()) => Reply::new(200, "Data connection address accepted."),
            Err(e) => Reply::new(425, format!("That address cannot be used: {e}.")),
        }
    }

    pub(super) fn rest_cmd(&mut self, arg: &str) -> Reply {
        match arg.trim().parse::<u64>() {
            Ok(n) => {
                self.rest = n;
                Reply::new(350, format!("The next transfer starts at byte {n}."))
            }
            Err(_) => Reply::new(501, "REST needs a byte offset."),
        }
    }

    pub(super) fn auth(&mut self, arg: &str, net: &mut dyn Net) -> Flow {
        let mech = arg.trim().to_ascii_uppercase();
        let r = if !matches!(mech.as_str(), "TLS" | "TLS-C" | "SSL" | "TLS-P") {
            Reply::new(504, format!("Security mechanism {mech} is not supported."))
        } else if net.secure() {
            Reply::new(503, "The control connection is already protected.")
        } else if !net.tls_available() {
            Reply::new(534, "TLS is not available on this server.")
        } else {
            self.say(net, &Reply::new(234, "Starting TLS on the control connection."));
            if let Err(e) = net.start_tls() {
                tracing::info!("FTP AUTH TLS: {e}");
                return Flow::Close;
            }
            self.pbsz = false;
            if mech == "SSL" || mech == "TLS-P" {
                self.private = true;
                net.set_private(true);
            }
            return Flow::Continue;
        };
        self.say(net, &r);
        Flow::Continue
    }

    pub(super) fn pbsz_cmd(&mut self, arg: &str, net: &mut dyn Net) -> Reply {
        if !net.secure() {
            return Reply::new(421, "PBSZ needs a protected control connection; send AUTH TLS first.");
        }
        if arg.trim().parse::<u64>().is_err() {
            return Reply::new(501, "PBSZ needs a number.");
        }
        self.pbsz = true;
        Reply::new(200, "PBSZ=0")
    }

    pub(super) fn prot(&mut self, arg: &str, net: &mut dyn Net) -> Reply {
        if !self.pbsz {
            return Reply::new(503, "PROT needs PBSZ first.");
        }
        match arg.trim().to_ascii_uppercase().as_str() {
            "C" => {
                self.private = false;
                net.set_private(false);
                Reply::new(200, "Data connections are now in the clear.")
            }
            "P" => {
                self.private = true;
                net.set_private(true);
                Reply::new(200, "Data connections are now protected.")
            }
            "S" | "E" => Reply::new(536, "Only protection levels C and P are supported."),
            _ => Reply::new(504, "PROT takes C or P."),
        }
    }

    pub(super) fn ccc(&mut self, net: &mut dyn Net) -> Flow {
        if !net.secure() {
            self.say(net, &Reply::new(533, "The control connection is not protected."));
            return Flow::Continue;
        }
        self.say(net, &Reply::new(200, "The control connection is now in the clear."));
        if let Err(e) = net.end_tls() {
            tracing::info!("FTP CCC: {e}");
            return Flow::Close;
        }
        Flow::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses() {
        assert_eq!(port_addr("10,0,0,1,4,1"), Some(SocketAddr::from(([10, 0, 0, 1], 1025))));
        assert_eq!(port_addr("1,2,3"), None);
        assert_eq!(port_addr("300,1,1,1,1,1"), None);
        assert_eq!(eprt_addr("|1|10.0.0.1|1025|"), Some(SocketAddr::from(([10, 0, 0, 1], 1025))));
        assert!(eprt_addr("|2|::1|1025|").is_some());
        assert!(eprt_addr("|1|::1|1025|").is_none());
        assert!(eprt_addr("bad").is_none());
    }

    #[test]
    fn help_knows_every_command() {
        for (v, _) in COMMANDS {
            assert_eq!(help(v).code, 214, "{v}");
        }
        assert_eq!(help("XYZ").code, 214);
        assert_eq!(feat().render().lines().count(), 12);
    }
}
