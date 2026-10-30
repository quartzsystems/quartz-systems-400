// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One FTP session: the state a control connection carries (who signed
//! on, the naming and listing formats, the current directory or
//! library, the representation, mode and structure, `RNFR`, `REST`,
//! the TLS state) and the answer to each command line. The session
//! knows nothing of sockets: it speaks through [`Net`] and reaches the
//! system through [`Host`].

mod dirs;
mod info;
mod listing;
mod login;
mod site;
mod xfer;

use servers::attrs::ftp::{CrtCcsid, FtpAttrs};

use crate::host::{Host, Profile};
use crate::names;
use crate::net::Net;
use crate::reply::Reply;
use crate::xlate::Rep;

/// What the server gives every session.
#[derive(Clone, Debug)]
pub struct Config {
    /// `CHGFTPA`'s attributes.
    pub attrs: FtpAttrs,
    /// The system's name (the banner, `STAT`).
    pub system: String,
    /// The release (`SYST`), `VxRyMz`.
    pub release: String,
    /// The connection came in on the implicit TLS port.
    pub implicit: bool,
}

impl Config {
    /// With the attributes and the system's name.
    pub fn new(attrs: FtpAttrs, system: &str) -> Self {
        Self {
            attrs,
            system: system.to_string(),
            release: release::RELEASE.to_string(),
            implicit: false,
        }
    }
}

/// What comes after a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    /// Read the next command.
    Continue,
    /// Close the connection.
    Close,
}

/// The exit point that validates requests.
pub const SERVER_REQ: &str = "QIBM_QTMF_SERVER_REQ";
/// The exit point that signs sessions on.
pub const SVR_LOGON: &str = "QIBM_QTMF_SVR_LOGON";

/// One session.
#[allow(clippy::struct_excessive_bools)] // the protocol's own switches
pub struct Session<H: Host> {
    /// The job's reach.
    pub host: H,
    cfg: Config,
    pending_user: Option<String>,
    user: Option<Profile>,
    namefmt: u8,
    listfmt: u8,
    trim: u8,
    nullflds: u8,
    crtccsid: CrtCcsid,
    cwd: String,
    rep: Rep,
    block: bool,
    record: bool,
    rnfr: Option<String>,
    /// The current library was set by a `NAMEFMT 0` name.
    lib_mode: bool,
    rest: u64,
    pbsz: bool,
    private: bool,
    ctl_bytes: u64,
    /// The last command's verb (for `STAT`).
    last: String,
}

impl<H: Host> Session<H> {
    /// A session not yet signed on.
    pub fn new(host: H, cfg: Config) -> Self {
        let namefmt = cfg.attrs.namefmt_number();
        let listfmt = cfg.attrs.listfmt_number();
        let crtccsid = cfg.attrs.crtccsid;
        Self {
            host,
            cfg,
            pending_user: None,
            user: None,
            namefmt,
            listfmt,
            trim: 1,
            nullflds: 0,
            crtccsid,
            cwd: "/".into(),
            rep: Rep::Ascii,
            block: false,
            record: false,
            rnfr: None,
            lib_mode: false,
            rest: 0,
            pbsz: false,
            private: false,
            ctl_bytes: 0,
            last: String::new(),
        }
    }

    /// The server's configuration.
    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// The signed-on profile.
    pub fn user(&self) -> Option<&str> {
        self.user.as_ref().map(|p| p.user.as_str())
    }

    /// The current directory, as an IFS path.
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// The naming format.
    pub fn namefmt(&self) -> u8 {
        self.namefmt
    }

    /// The inactivity timeout, seconds (0 for none).
    pub fn idle_secs(&self) -> u32 {
        self.cfg.attrs.inacttimo
    }

    fn ascii(&self) -> u16 {
        self.cfg.attrs.ccsid
    }

    /// The CCSIDs text travels in.
    fn codes(&self) -> crate::xlate::Codes {
        crate::xlate::Codes {
            ascii: self.cfg.attrs.ccsid,
            ebcdic: self.host.job_ccsid(),
        }
    }

    fn home(&self) -> String {
        self.user.as_ref().map_or_else(|| "/".into(), |p| p.home.clone())
    }

    /// The banner, or the refusal when the request exit says no.
    pub fn greet(&mut self, net: &mut dyn Net) -> Flow {
        if !self.validate(net, regfac::formats::tcp::op::SESSION, "*N", b"") {
            let _ = net.send(&Reply::new(421, "The server is not taking this connection."));
            return Flow::Close;
        }
        let minutes = self.cfg.attrs.inacttimo / 60;
        let r = Reply::lines(
            220,
            vec![
                format!("Quartz FTP server at {}.", self.cfg.system),
                if self.cfg.attrs.inacttimo == 0 {
                    "Idle connections stay open.".to_string()
                } else {
                    format!("A connection idle for more than {minutes} minutes is closed.")
                },
            ],
        );
        self.say(net, &r);
        Flow::Continue
    }

    /// Send a reply, counting it.
    fn say(&mut self, net: &mut dyn Net, r: &Reply) {
        self.ctl_bytes += r.render().len() as u64;
        if let Err(e) = net.send(r) {
            tracing::debug!("FTP reply not sent: {e}");
        }
    }

    /// Answer one command line.
    #[allow(clippy::too_many_lines)] // one arm per command
    pub fn handle(&mut self, line: &str, net: &mut dyn Net) -> Flow {
        self.ctl_bytes += line.len() as u64 + 2;
        let line = line.trim_end_matches(['\r', '\n']);
        let (verb, arg) = match line.trim_start().split_once(' ') {
            Some((v, a)) => (v.to_ascii_uppercase(), a.trim().to_string()),
            None => (line.trim().to_ascii_uppercase(), String::new()),
        };
        // A rename is RNFR followed at once by RNTO.
        let from = if verb == "RNTO" { self.rnfr.take() } else { None };
        if verb != "RNFR" && verb != "RNTO" {
            self.rnfr = None;
        }
        // REST counts for the next transfer only.
        let rest = std::mem::take(&mut self.rest);
        let signed = self.user.is_some();
        if !signed && !login::before_sign_on(&verb) {
            if known(&verb) {
                self.say(net, &Reply::new(530, "Sign on with USER and PASS first."));
            } else {
                self.say(net, &unknown(&verb));
            }
            return Flow::Continue;
        }
        self.last.clone_from(&verb);
        let r = match verb.as_str() {
            "USER" => self.user_cmd(&arg),
            "PASS" => self.pass_cmd(&arg, net),
            "ACCT" => Reply::new(503, "No account is needed."),
            "REIN" => self.rein(),
            "QUIT" => {
                self.say(net, &Reply::new(221, "Session ended."));
                return Flow::Close;
            }
            "NOOP" => Reply::new(200, "Nothing to do."),
            "SYST" => self.syst(),
            "FEAT" => info::feat(),
            "HELP" => info::help(&arg),
            "STAT" => self.stat(&arg),
            "OPTS" => info::opts(&arg),
            "TYPE" => self.type_cmd(&arg),
            "MODE" => self.mode_cmd(&arg),
            "STRU" => self.stru_cmd(&arg),
            "ALLO" => Reply::new(202, "No space needs to be set aside."),
            "PASV" => self.pasv(net),
            "EPSV" => self.epsv(&arg, net),
            "PORT" => self.port(&arg, net),
            "EPRT" => self.eprt(&arg, net),
            "REST" => self.rest_cmd(&arg),
            "AUTH" => return self.auth(&arg, net),
            "PBSZ" => self.pbsz_cmd(&arg, net),
            "PROT" => self.prot(&arg, net),
            "CCC" => return self.ccc(net),
            "PWD" | "XPWD" => self.pwd(),
            "CWD" | "XCWD" => self.cwd_cmd(&arg, net),
            "CDUP" | "XCUP" => self.cdup(net),
            "MKD" | "XMKD" => self.mkd(&arg, net),
            "RMD" | "XRMD" => self.rmd(&arg, net),
            "DELE" => self.dele(&arg, net),
            "RNFR" => self.rnfr_cmd(&arg, net),
            "RNTO" => self.rnto(&arg, from, net),
            "RETR" => self.retr(&arg, rest, net),
            "STOR" => self.stor(&arg, rest, xfer::Store::Replace, net),
            "APPE" => self.stor(&arg, 0, xfer::Store::Append, net),
            "STOU" => self.stor(&arg, 0, xfer::Store::Unique, net),
            "LIST" => self.list(&arg, false, net),
            "NLST" => self.list(&arg, true, net),
            "MLSD" => self.mlsd(&arg, net),
            "MLST" => self.mlst(&arg),
            "SIZE" => self.size(&arg),
            "MDTM" => self.mdtm(&arg),
            "SITE" => self.site(&arg),
            "RCMD" => self.rcmd(&arg, net),
            "ADDM" | "ADDV" | "CRTL" | "CRTP" | "CRTS" | "DLTF" | "DLTL" => self.object_cmd(&verb, &arg, net),
            "ABOR" => Reply::new(226, "No transfer is running; nothing to abort."),
            _ => unknown(&verb),
        };
        self.say(net, &r);
        Flow::Continue
    }

    /// The session ends: the profile swap goes with the connection.
    pub fn ended(&mut self) {
        self.user = None;
    }

    /// `QIBM_QTMF_SERVER_REQ`: whether the operation may go ahead.
    fn validate(&mut self, net: &dyn Net, op: i32, user: &str, info: &[u8]) -> bool {
        let req = regfac::ExitRequest::Validate(regfac::formats::tcp::Validate {
            app: regfac::formats::tcp::app::FTP_SERVER,
            op,
            user: user.to_string(),
            remote: net.peer().ip().to_string(),
            info: ccsid::to_ebcdic(&String::from_utf8_lossy(info)).unwrap_or_else(|_| info.to_vec()),
        });
        let a = self.host.exit(SERVER_REQ, &req);
        a.called.is_empty() || a.allow
    }

    /// The request exit for an operation on a name.
    fn allowed(&mut self, net: &dyn Net, op: i32, info: &str) -> bool {
        let user = self.user().unwrap_or("*N").to_string();
        self.validate(net, op, &user, info.as_bytes())
    }

    /// The current library for `NAMEFMT 0`.
    fn curlib(&self) -> Option<String> {
        names::library_of(&self.cwd)
    }
}

/// Whether a verb is one the server knows.
fn known(verb: &str) -> bool {
    info::COMMANDS.iter().any(|(v, _)| *v == verb)
}

/// The answer to a verb the server does not know.
fn unknown(verb: &str) -> Reply {
    let shown: String = verb.chars().take(4).collect();
    Reply::new(500, format!("{shown} is not an FTP subcommand here."))
}
