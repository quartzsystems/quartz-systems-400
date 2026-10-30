// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `FTP` CL command's client: subcommands read from a [`Console`]
//! (the display, or the file `INPUT` in batch), the server's replies
//! and the client's messages written to it (the display, or the file
//! `OUTPUT`), local objects reached through the job's [`Host`] with the
//! server's own transfer rules ([`crate::store`]), and the request exit
//! point `QIBM_QTMF_CLIENT_REQ`. In batch the first line read after a
//! connection is the user and, after a blank, the password.

pub mod conn;
pub mod sub;

use std::io::Write as _;
use std::sync::Arc;

use ifs::vfs::qsys::QsysNode;
use regfac::formats::tcp::op;
use servers::attrs::ftp::CrtCcsid;
use tls::rustls::ClientConfig;

use crate::host::Host;
use crate::names;
use crate::objects;
use crate::reply::Reply;
use crate::store::{self, Form};
use crate::xlate::{Codes, Rep};

use conn::{Control, read_all};
use sub::Sub;

/// The exit point that validates the client's requests.
pub const CLIENT_REQ: &str = "QIBM_QTMF_CLIENT_REQ";

/// Where subcommands come from and messages go.
pub trait Console {
    /// The next line, `None` at the end of the input.
    fn read_line(&mut self, prompt: &str) -> Option<String>;
    /// A password (not shown where it is typed).
    fn read_password(&mut self, prompt: &str) -> Option<String> {
        self.read_line(prompt)
    }
    /// Write a line.
    fn write(&mut self, line: &str);
}

/// How the client connects (`SECCNN`, `DTAPROT`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Security {
    /// In the clear.
    None,
    /// `AUTH TLS` after connecting.
    Explicit,
    /// TLS from the first byte.
    Implicit,
}

/// What the `FTP` command gives the client.
#[derive(Clone)]
pub struct Options {
    /// `PORT`.
    pub port: u16,
    /// `SECCNN`.
    pub security: Security,
    /// `DTAPROT`: data connections under TLS.
    pub private: bool,
    /// `CCSID`: the ASCII CCSID.
    pub ccsid: u16,
    /// The TLS configuration, when TLS may be used.
    pub tls: Option<Arc<ClientConfig>>,
    /// Lines come from a file: echo them.
    pub batch: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            port: 21,
            security: Security::None,
            private: false,
            ccsid: 819,
            tls: None,
            batch: false,
        }
    }
}

/// The client.
#[allow(clippy::struct_excessive_bools)] // the subcommands' switches
pub struct Client<H: Host, C: Console> {
    /// The job's reach (local objects, commands, exit programs).
    pub host: H,
    /// The display or the files.
    pub console: C,
    opts: Options,
    conn: Option<Control>,
    remote: String,
    rep: Rep,
    namefmt: u8,
    lcwd: String,
    passive: bool,
    sunique: bool,
    block: bool,
    sendsite: bool,
    debug: bool,
    trim: bool,
    crtccsid: CrtCcsid,
    os400: bool,
    /// Every reply's code, in order (for tests and scripts).
    pub codes: Vec<u16>,
}

/// What a subcommand leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    /// Read the next one.
    More,
    /// The session is over.
    Quit,
}

impl<H: Host, C: Console> Client<H, C> {
    /// A client not yet connected; `lcwd` the local current directory
    /// (a path, or a library's `/QSYS.LIB` path for name format 0).
    pub fn new(host: H, console: C, opts: Options, lcwd: &str) -> Self {
        let namefmt = u8::from(names::library_of(lcwd).is_none());
        Self {
            host,
            console,
            opts,
            conn: None,
            remote: String::new(),
            rep: Rep::Ascii,
            namefmt,
            lcwd: lcwd.to_string(),
            passive: true,
            sunique: false,
            block: false,
            sendsite: true,
            debug: false,
            trim: true,
            crtccsid: CrtCcsid::Calc,
            os400: false,
            codes: Vec::new(),
        }
    }

    fn say(&mut self, text: &str) {
        self.console.write(text);
    }

    fn show(&mut self, r: &Reply) {
        self.codes.push(r.code);
        for l in r.render().lines() {
            self.console.write(l);
        }
    }

    fn form(&self) -> Form {
        Form {
            rep: self.rep,
            codes: Codes {
                ascii: self.opts.ccsid,
                ebcdic: self.host.job_ccsid(),
            },
            trim: self.trim,
            crtccsid: self.crtccsid,
            user_ccsid: 0,
        }
    }

    /// `QIBM_QTMF_CLIENT_REQ`: whether the operation may go ahead.
    fn allowed(&mut self, op_id: i32, info: &str) -> bool {
        let req = regfac::ExitRequest::Validate(regfac::formats::tcp::Validate {
            app: regfac::formats::tcp::app::FTP_CLIENT,
            op: op_id,
            user: String::new(),
            remote: self.conn.as_ref().map(|c| c.peer.ip().to_string()).unwrap_or_default(),
            info: ccsid::to_ebcdic(info).unwrap_or_default(),
        });
        let a = self.host.exit(CLIENT_REQ, &req);
        let ok = a.called.is_empty() || a.allow;
        if !ok {
            self.say(&format!("An exit program refused the request: {info}"));
        }
        ok
    }

    /// Run the client: connect to `rmtsys`, then subcommands until
    /// `QUIT` or the end of the input.
    pub fn run(&mut self, rmtsys: &str) {
        if !rmtsys.trim().is_empty() {
            self.open(rmtsys.trim(), self.opts.port);
        }
        while let Some(line) = self.console.read_line("Enter an FTP subcommand.") {
            if self.opts.batch {
                self.echo(&line);
            }
            if self.subcommand(&line) == Next::Quit {
                break;
            }
        }
        if let Some(mut c) = self.conn.take() {
            if let Ok(r) = c.command("QUIT") {
                self.show(&r);
            }
            c.close();
        }
    }

    /// Echo a line read from the input, the password masked.
    fn echo(&mut self, line: &str) {
        let shown = match Sub::parse(line) {
            Some((Sub::User, args)) if args.split_whitespace().count() > 1 => {
                format!("> {} {} ********", line.split_whitespace().next().unwrap_or(""), args.split_whitespace().next().unwrap_or(""))
            }
            Some((Sub::Pass, _)) => "> PASS ********".to_string(),
            _ => format!("> {line}"),
        };
        self.say(&shown);
    }

    /// Connect and sign on.
    fn open(&mut self, host: &str, port: u16) {
        if self.conn.is_some() {
            self.say("A connection is already open; CLOSE it first.");
            return;
        }
        if !self.allowed(op::SESSION, host) {
            return;
        }
        self.say(&format!("Connecting to {host} port {port}."));
        let implicit = self.opts.security == Security::Implicit;
        let mut c = match Control::connect(host, port, self.opts.tls.clone(), implicit) {
            Ok(c) => c,
            Err(e) => {
                self.say(&format!("No connection to {host}: {e}."));
                return;
            }
        };
        match c.reply() {
            Ok(r) => self.show(&r),
            Err(e) => {
                self.say(&format!("No greeting from {host}: {e}."));
                return;
            }
        }
        self.remote = host.to_string();
        if self.opts.security == Security::Explicit {
            match c.command("AUTH TLS") {
                Ok(r) if r.code == 234 => {
                    self.show(&r);
                    if let Err(e) = c.start_tls() {
                        self.say(&format!("TLS failed: {e}."));
                        return;
                    }
                }
                Ok(r) => {
                    self.show(&r);
                    self.say("The server does not take TLS; the connection is closed.");
                    c.close();
                    return;
                }
                Err(e) => {
                    self.say(&format!("TLS failed: {e}."));
                    return;
                }
            }
        }
        if self.opts.security != Security::None {
            for cmd in ["PBSZ 0", if self.opts.private { "PROT P" } else { "PROT C" }] {
                if let Ok(r) = c.command(cmd) {
                    self.show(&r);
                }
            }
            c.private = self.opts.private;
        }
        self.conn = Some(c);
        self.login();
    }

    /// Sign on: a line with the user (and the password after a blank),
    /// the password asked for when it is not there.
    fn login(&mut self) {
        let Some(line) = self.console.read_line("Enter the user name (and password).") else {
            return;
        };
        let mut words = line.split_whitespace();
        let user = words.next().unwrap_or("").to_string();
        let pass = words.next().map(str::to_string);
        if self.opts.batch {
            self.say(&format!("> {user}{}", if pass.is_some() { " ********" } else { "" }));
        }
        self.user(&user, pass);
    }

    fn user(&mut self, user: &str, pass: Option<String>) {
        let Some(c) = self.conn.as_mut() else {
            self.say("Not connected; OPEN a connection first.");
            return;
        };
        let r = match c.command(&format!("USER {user}")) {
            Ok(r) => r,
            Err(e) => return self.lost(&e),
        };
        self.show(&r);
        if r.code != 331 {
            return;
        }
        let pass = match pass {
            Some(p) => Some(p),
            None => self.console.read_password("Enter the password."),
        };
        self.pass(&pass.unwrap_or_default());
    }

    fn pass(&mut self, pass: &str) {
        let Some(c) = self.conn.as_mut() else {
            return;
        };
        let r = match c.command(&format!("PASS {pass}")) {
            Ok(r) => r,
            Err(e) => return self.lost(&e),
        };
        self.show(&r);
        if r.code == 230 {
            self.after_login();
        }
    }

    /// After sign-on: what the server is; an IBM i server takes the
    /// client's naming format.
    fn after_login(&mut self) {
        if let Some(r) = self.raw("SYST") {
            self.os400 = r.lines.join(" ").contains(crate::SYST_SYSTEM);
        }
        // Paths work wherever the server stands; library names only
        // in a library, so only format 1 is sent at sign-on.
        if self.os400 && self.sendsite && self.namefmt == 1 {
            self.raw("SITE NAMEFMT 1");
        }
    }

    fn lost(&mut self, e: &std::io::Error) {
        self.say(&format!("The connection to {} ended: {e}.", self.remote));
        if let Some(mut c) = self.conn.take() {
            c.close();
        }
    }

    /// Send a command and show its reply.
    fn raw(&mut self, cmd: &str) -> Option<Reply> {
        if self.debug {
            let shown = if cmd.starts_with("PASS ") { "PASS ********".to_string() } else { cmd.to_string() };
            self.say(&format!(">>> {shown}"));
        }
        let Some(c) = self.conn.as_mut() else {
            self.say("Not connected; OPEN a connection first.");
            return None;
        };
        match c.command(cmd) {
            Ok(r) => {
                self.show(&r);
                Some(r)
            }
            Err(e) => {
                self.lost(&e);
                None
            }
        }
    }

    /// Run a transfer command; the data the server sent, or what went
    /// wrong.
    fn transfer(&mut self, cmd: &str, upload: Option<&[u8]>) -> Option<Vec<u8>> {
        let passive = self.passive;
        let debug = self.debug;
        let block = self.block;
        let mut log: Vec<Reply> = Vec::new();
        let Some(c) = self.conn.as_mut() else {
            self.say("Not connected; OPEN a connection first.");
            return None;
        };
        let result = (|| -> std::io::Result<Option<Vec<u8>>> {
            let setup = c.setup(passive, &mut |r| log.push(r.clone()))?;
            c.send(cmd)?;
            let first = c.reply()?;
            log.push(first.clone());
            if first.code >= 300 {
                return Ok(None);
            }
            let mut d = c.data(setup)?;
            let data = if let Some(bytes) = upload {
                {
                    if block {
                        d.write_all(&crate::block::encode(bytes))?;
                    } else {
                        d.write_all(bytes)?;
                    }
                    d.flush()?;
                    drop(d);
                    Vec::new()
                }
            } else {
                let got = read_all(d.as_mut())?;
                drop(d);
                if block { crate::block::decode(&got) } else { got }
            };
            if first.code < 200 {
                log.push(c.reply()?);
            }
            Ok(Some(data))
        })();
        if debug {
            self.say(&format!(">>> {cmd}"));
        }
        for r in &log {
            self.show(r);
        }
        match result {
            Ok(Some(d)) if log.last().is_some_and(|r| r.code < 400) => Some(d),
            Ok(_) => None,
            Err(e) => {
                self.lost(&e);
                None
            }
        }
    }

    /// A local name's path, in the local naming format.
    fn local_path(&mut self, name: &str) -> String {
        if self.namefmt == 1 || name.starts_with('/') {
            return names::join(&self.lcwd, name, "/");
        }
        match names::parse_lib_name(name) {
            Some(n) => {
                let lib = n.lib.clone().or_else(|| names::library_of(&self.lcwd)).unwrap_or_else(|| "QGPL".into());
                if let Some(m) = &n.mbr {
                    objects::member_path(&lib, &n.file, m)
                } else {
                    let savf = objects::savf_path(&lib, &n.file);
                    if objects::exists(self.host.ifs(), &savf) {
                        savf
                    } else {
                        objects::member_path(&lib, &n.file, &n.file)
                    }
                }
            }
            None => names::join(&self.lcwd, name, "/"),
        }
    }

    /// The local name a remote name gets when none is given: its last
    /// part.
    fn default_local(remote: &str) -> String {
        remote.rsplit('/').next().unwrap_or(remote).to_string()
    }

    fn get(&mut self, args: &str, append: bool) {
        let replace = args.to_ascii_uppercase().contains("(REPLACE");
        let words: Vec<&str> = args.split_whitespace().filter(|w| !w.starts_with('(')).collect();
        let Some(remote) = words.first().map(|s| (*s).to_string()) else {
            self.say("GET needs a remote name.");
            return;
        };
        let local = words.get(1).map_or_else(|| Self::default_local(&remote), |s| (*s).to_string());
        let path = self.local_path(&local);
        if !append && !replace && objects::exists(self.host.ifs(), &path) {
            self.say(&format!("{} exists; add (REPLACE to replace it.", store::describe(&path)));
            return;
        }
        if !self.allowed(op::RECEIVE_FILE, &path) {
            return;
        }
        let Some(data) = self.transfer(&format!("RETR {remote}"), None) else {
            return;
        };
        let form = self.form();
        match store::put(&mut self.host, &form, &path, &data, append, 0) {
            Ok(cut) => {
                let n = data.len();
                self.say(&format!("{n} bytes written to {}.", store::describe(&path)));
                if cut {
                    self.say("Lines longer than the records were cut to fit.");
                }
            }
            Err(e) => self.say(&format!("{} not written: {e}.", store::describe(&path))),
        }
    }

    fn put(&mut self, args: &str, verb: &str) {
        let words: Vec<&str> = args.split_whitespace().collect();
        let Some(local) = words.first().map(|s| (*s).to_string()) else {
            self.say(&format!("{verb} needs a local name."));
            return;
        };
        let path = self.local_path(&local);
        let remote = words.get(1).map_or_else(|| Self::default_local(&local), |s| (*s).to_string());
        if !self.allowed(op::SEND_FILE, &path) {
            return;
        }
        let form = self.form();
        let data = match store::fetch(&mut self.host, &form, &path) {
            Ok(d) => d,
            Err(r) => {
                self.say(&format!("{} cannot be sent: {}", store::describe(&path), r.lines.join(" ")));
                return;
            }
        };
        let cmd = if verb == "APPE" {
            "APPE"
        } else if self.sunique {
            "STOU"
        } else {
            "STOR"
        };
        if self.transfer(&format!("{cmd} {remote}"), Some(&data)).is_some() {
            self.say(&format!("{} bytes sent from {}.", data.len(), store::describe(&path)));
        }
    }

    /// Names a remote pattern lists.
    fn remote_names(&mut self, pattern: &str) -> Vec<String> {
        self.transfer(&format!("NLST {pattern}"), None)
            .map(|d| {
                String::from_utf8_lossy(&d)
                    .lines()
                    .map(|l| l.trim_end_matches('\r').to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Local names a pattern matches: in a directory, or the members of
    /// a file (`FILE.*`) in name format 0.
    fn local_names(&mut self, pattern: &str) -> Vec<String> {
        if self.namefmt == 0
            && let Some(n) = names::parse_lib_name(pattern)
            && let Some(m) = &n.mbr
        {
            let lib = n.lib.clone().or_else(|| names::library_of(&self.lcwd)).unwrap_or_else(|| "QGPL".into());
            let dir = objects::file_path(&lib, &n.file);
            let ents = self.host.ifs().read_dir(&dir).unwrap_or_default();
            return ents
                .into_iter()
                .filter_map(|e| ifs::vfs::qsys::parse_component(&e.name).ok())
                .filter(|(name, _)| names::matches(m, name))
                .map(|(name, _)| format!("{}.{name}", n.file))
                .collect();
        }
        let full = names::join(&self.lcwd, pattern, "/");
        let (dir, pat) = full.rsplit_once('/').map_or(("/".to_string(), full.clone()), |(d, p)| {
            (if d.is_empty() { "/".into() } else { d.to_string() }, p.to_string())
        });
        let ents = self.host.ifs().read_dir(&dir).unwrap_or_default();
        ents.into_iter()
            .filter(|e| names::matches(&pat, &e.name))
            .map(|e| if dir == self.lcwd { e.name } else { format!("{dir}/{}", e.name) })
            .collect()
    }

    fn list(&mut self, verb: &str, args: &str) {
        let cmd = if args.trim().is_empty() { verb.to_string() } else { format!("{verb} {}", args.trim()) };
        if let Some(d) = self.transfer(&cmd, None) {
            let text = String::from_utf8_lossy(&d).to_string();
            for l in text.lines() {
                let l = l.trim_end_matches('\r').to_string();
                self.say(&l);
            }
        }
    }

    fn lcd(&mut self, args: &str) {
        let a = args.trim();
        if a.is_empty() {
            let shown = self.local_place();
            self.say(&shown);
            return;
        }
        let path = if self.namefmt == 0 && !a.starts_with('/') {
            names::library_dir(&a.to_ascii_uppercase())
        } else {
            names::join(&self.lcwd, a, "/")
        };
        match objects::look(self.host.ifs(), &path) {
            Ok((k, _)) if k.is_dir() => {
                self.lcwd = path;
                let shown = self.local_place();
                self.say(&shown);
            }
            _ => self.say(&format!("{a} is not a local library or directory.")),
        }
    }

    fn local_place(&self) -> String {
        match names::library_of(&self.lcwd) {
            Some(l) if self.namefmt == 0 => format!("Local current library is {l}."),
            _ => format!("Local current directory is {}.", self.lcwd),
        }
    }

    /// `LOCSITE`: the client's own settings.
    fn locsite(&mut self, args: &str) {
        let mut w = args.split_whitespace();
        let sub = w.next().unwrap_or("").to_ascii_uppercase();
        let v = w.next().map(str::to_ascii_uppercase);
        match (sub.as_str(), v.as_deref()) {
            ("NAMEFMT", Some("0" | "1")) => self.namefmt = u8::from(v.as_deref() == Some("1")),
            ("TRIM", Some("0" | "1")) => self.trim = v.as_deref() == Some("1"),
            ("CRTCCSID", Some(c)) => match CrtCcsid::parse(c) {
                Ok(x) => self.crtccsid = x,
                Err(e) => return self.say(&format!("LOCSITE CRTCCSID: {e}.")),
            },
            _ => return self.say("LOCSITE takes NAMEFMT 0|1, TRIM 0|1 or CRTCCSID value."),
        }
        self.say(&format!("Local {sub} set."));
    }

    fn toggle(&mut self, name: &str, args: &str, flag: bool) -> bool {
        let v = match args.trim().to_ascii_uppercase().as_str() {
            "ON" | "1" => true,
            "OFF" | "0" => false,
            _ => !flag,
        };
        self.say(&format!("{name} is {}.", if v { "on" } else { "off" }));
        v
    }

    fn status(&mut self) {
        let lines = vec![
            format!(
                "Connected to {}.",
                if self.conn.is_some() { self.remote.as_str() } else { "no system" }
            ),
            format!("Representation type {:?}; naming format {}.", self.rep, self.namefmt),
            self.local_place(),
            format!(
                "Passive {}; unique names {}; SITE sent {}; debug {}.",
                on(self.passive),
                on(self.sunique),
                on(self.sendsite),
                on(self.debug)
            ),
        ];
        for l in lines {
            self.say(&l);
        }
    }

    /// Run one subcommand.
    #[allow(clippy::too_many_lines)] // one arm per subcommand
    pub fn subcommand(&mut self, line: &str) -> Next {
        let line = line.trim();
        if line.is_empty() {
            return Next::More;
        }
        let Some((sub, args)) = Sub::parse(line) else {
            let w = line.split_whitespace().next().unwrap_or("");
            self.say(&format!("{w} is not an FTP subcommand; HELP lists them."));
            return Next::More;
        };
        match sub {
            Sub::Open => {
                let mut w = args.split_whitespace();
                let host = w.next().unwrap_or("").to_string();
                let port = w.next().and_then(|p| p.parse().ok()).unwrap_or(self.opts.port);
                if host.is_empty() {
                    self.say("OPEN needs a system name or address.");
                } else {
                    self.open(&host, port);
                }
            }
            Sub::User => {
                let mut w = args.split_whitespace();
                let u = w.next().unwrap_or("").to_string();
                let p = w.next().map(str::to_string);
                self.user(&u, p);
            }
            Sub::Pass => self.pass(args.trim()),
            Sub::Close => {
                if let Some(mut c) = self.conn.take() {
                    if let Ok(r) = c.command("QUIT") {
                        self.show(&r);
                    }
                    c.close();
                }
            }
            Sub::Quit => return Next::Quit,
            Sub::Cd => {
                if self.allowed(op::SET_DIR, args) {
                    self.raw(&format!("CWD {}", args.trim()));
                }
            }
            Sub::Cdup => {
                self.raw("CDUP");
            }
            Sub::Pwd => {
                self.raw("PWD");
            }
            Sub::Lcd => self.lcd(args),
            Sub::Lpwd => {
                let shown = self.local_place();
                self.say(&shown);
            }
            Sub::Ls => self.list("NLST", args),
            Sub::Dir => self.list("LIST", args),
            Sub::Get => self.get(args, false),
            Sub::Put => self.put(args, "STOR"),
            Sub::Append => self.put(args, "APPE"),
            Sub::Mget => {
                let replace = args.to_ascii_uppercase().contains("(REPLACE");
                let pats: Vec<String> = args.split_whitespace().filter(|w| !w.starts_with('(')).map(str::to_string).collect();
                for p in pats {
                    for n in self.remote_names(&p) {
                        let local = Self::default_local(&n);
                        self.get(&format!("{n} {local}{}", if replace { " (REPLACE" } else { "" }), false);
                    }
                }
            }
            Sub::Mput => {
                let pats: Vec<String> = args.split_whitespace().map(str::to_string).collect();
                for p in pats {
                    for n in self.local_names(&p) {
                        self.put(&n, "STOR");
                    }
                }
            }
            Sub::Delete => {
                if self.allowed(op::DELETE_FILE, args) {
                    self.raw(&format!("DELE {}", args.trim()));
                }
            }
            Sub::Rename => {
                let w: Vec<&str> = args.split_whitespace().collect();
                if w.len() != 2 {
                    self.say("RENAME needs the old and the new name.");
                } else if self.allowed(op::RENAME, args)
                    && self.raw(&format!("RNFR {}", w[0])).is_some_and(|r| r.code == 350)
                {
                    self.raw(&format!("RNTO {}", w[1]));
                }
            }
            Sub::Mkdir => {
                if self.allowed(op::CREATE_DIR, args) {
                    self.raw(&format!("MKD {}", args.trim()));
                }
            }
            Sub::Rmdir => {
                if self.allowed(op::DELETE_DIR, args) {
                    self.raw(&format!("RMD {}", args.trim()));
                }
            }
            Sub::Binary => self.set_type(Rep::Image, "TYPE I"),
            Sub::Ascii => self.set_type(Rep::Ascii, "TYPE A"),
            Sub::Ebcdic => self.set_type(Rep::Ebcdic, "TYPE E"),
            Sub::Type => {
                let t = args.trim().to_ascii_uppercase();
                let rep = match t.split_whitespace().collect::<Vec<_>>().as_slice() {
                    ["A", ..] => Some(Rep::Ascii),
                    ["E", ..] => Some(Rep::Ebcdic),
                    ["I"] | ["L", "8"] => Some(Rep::Image),
                    ["C", n] => n.parse().ok().map(Rep::Ccsid),
                    _ => None,
                };
                match rep {
                    Some(r) => self.set_type(r, &format!("TYPE {t}")),
                    None => self.say("TYPE takes A, E, I or C and a CCSID."),
                }
            }
            Sub::Mode => {
                let m = args.trim().to_ascii_uppercase();
                if self.raw(&format!("MODE {m}")).is_some_and(|r| r.code == 200) {
                    self.block = m == "B";
                }
            }
            Sub::Struct => {
                self.raw(&format!("STRU {}", args.trim()));
            }
            Sub::Namefmt => {
                let v = args.trim();
                if v.is_empty() {
                    let n = self.namefmt;
                    self.say(&format!("Naming format {n}."));
                } else if v == "0" || v == "1" {
                    self.namefmt = u8::from(v == "1");
                    if self.conn.is_some() && self.sendsite {
                        self.raw(&format!("SITE NAMEFMT {v}"));
                    }
                } else {
                    self.say("NAMEFMT takes 0 or 1.");
                }
            }
            Sub::Quote => {
                self.raw(args.trim());
            }
            Sub::Site => {
                self.raw(&format!("SITE {}", args.trim()));
            }
            Sub::Syscmd => {
                if self.allowed(op::COMMAND, args) {
                    match self.host.command(args.trim()) {
                        Ok(()) => self.say(&format!("Command {} ran.", args.trim())),
                        Err(e) => self.say(&format!("Command {} failed: {} {}", args.trim(), e.msgid, e.text)),
                    }
                }
            }
            Sub::Locsite => self.locsite(args),
            Sub::Sendsite => self.sendsite = self.toggle("SENDSITE", args, self.sendsite),
            Sub::Sendpasv => self.passive = self.toggle("SENDPASV", args, self.passive),
            Sub::Sunique => self.sunique = self.toggle("SUNIQUE", args, self.sunique),
            Sub::Debug => self.debug = self.toggle("DEBUG", args, self.debug),
            Sub::Status => self.status(),
            Sub::System => {
                self.raw("SYST");
            }
            Sub::Help => {
                let all: Vec<&str> = sub::TABLE.iter().map(|(n, _, _)| *n).collect();
                for chunk in all.chunks(10) {
                    let l = chunk.join(" ");
                    self.say(&l);
                }
            }
            Sub::Noop => {
                self.raw("NOOP");
            }
        }
        Next::More
    }

    fn set_type(&mut self, rep: Rep, cmd: &str) {
        self.rep = rep;
        if self.conn.is_some() {
            self.raw(cmd);
        }
    }

    /// The local current directory or library.
    pub fn local_dir(&self) -> &str {
        &self.lcwd
    }

    /// Whether a library path is the local current one.
    pub fn in_library(&self) -> bool {
        matches!(names::qsys_node(&self.lcwd), Some(QsysNode::Lib(_) | QsysNode::Root))
    }
}

fn on(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}
