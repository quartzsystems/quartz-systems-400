// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `FTP` and `STRTCPFTP`: `net/ftp`'s client in this job. Subcommands
//! come from the member the file `INPUT` is overridden to, as IBM's
//! client reads them in batch (the first line the user and password);
//! what the session prints goes to the member `OUTPUT` is overridden to
//! (added to it), or is the command's output when there is no override.
//! Local objects are reached as the job reaches them; `SYSCMD` runs in
//! this job. An interactive job without an `INPUT` override gets the
//! client's own display, File Transfer Protocol, laid out as the
//! primary oracle's (`tools/conformance/golden/phase12/ftpclient/`).

use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use cmd::Resolved;
use ftp::client::{Client, Console, Options, Security};
use ftp::host::{CmdFail, DbFile, Host as FtpHost, Profile, Refused};
use ifs::client::IfsClient;
use mi::Job;
use mi::io::{IoSource, OpenSpec};
use regfac::{ExitAnswer, ExitRegistry, ExitRequest};
use stg::Client as _;
use stg::remote::{RemoteIfs, RemoteIo};

use super::{Fail, Outcome, parms_of};
use crate::console::host::Host;

/// Lines in, lines out.
struct Script {
    lines: VecDeque<String>,
    out: Vec<String>,
}

impl Console for Script {
    fn read_line(&mut self, _prompt: &str) -> Option<String> {
        self.lines.pop_front()
    }

    fn write(&mut self, line: &str) {
        self.out.push(line.to_string());
    }
}

/// The File Transfer Protocol display: the previous subcommands and
/// messages, the prompt, a command line of four rows. F3 ends the
/// session, F9 retrieves, F17 and F18 and the roll keys page.
struct Screen {
    host: Host,
    lines: Vec<String>,
    commands: Vec<String>,
    /// The first line shown; `None` follows the end.
    top: Option<usize>,
}

/// Rows of the display.
mod rows {
    /// The first of the previous lines.
    pub const FIRST: usize = 4;
    /// How many show.
    pub const SHOWN: usize = 13;
    /// The prompt.
    pub const PROMPT: usize = 17;
    /// The command line.
    pub const LINE: usize = 18;
}

/// The command line's length: row 18 column 7 to row 21 column 80.
const LINE_LEN: usize = 314;

impl Screen {
    fn new(host: &Host) -> Self {
        Self {
            host: host.clone(),
            lines: Vec::new(),
            commands: Vec::new(),
            top: None,
        }
    }

    /// The prompt as IBM's client words it.
    fn prompt_of(&self, prompt: &str) -> String {
        if prompt.starts_with("Enter the user name") {
            format!("Enter login ID ({}):", self.host.start.user.to_ascii_lowercase())
        } else if prompt.starts_with("Enter the password") {
            "Enter password:".into()
        } else {
            prompt.to_string()
        }
    }

    fn draw(&self, m: &mut tn5250::screen::ScreenModel, prompt: &str, hidden: bool, value: &str, message: &str) -> Option<usize> {
        use tn5250::ds::{attr, field::Ffw};
        m.clear();
        let title = "File Transfer Protocol";
        m.put(1, 1 + (m.cols() - title.len()) / 2, title, attr::WHITE);
        m.put(3, 2, "Previous FTP subcommands and messages:", attr::BLUE);
        let last = self.lines.len().saturating_sub(rows::SHOWN);
        let from = self.top.unwrap_or(last).min(last);
        for (i, l) in self.lines.iter().skip(from).take(rows::SHOWN).enumerate() {
            let shown: String = l.chars().take(m.cols() - 3).collect();
            m.put(rows::FIRST + i, 2, &shown, attr::GREEN);
        }
        m.put(rows::PROMPT, 2, prompt, attr::GREEN);
        m.put(rows::LINE, 2, "===>", attr::GREEN);
        let a = if hidden { attr::NONDISPLAY } else { attr::GREEN_UL };
        m.field(rows::LINE, 7, LINE_LEN, a, value, Ffw::default(), &[]);
        m.put(22, 2, "F3=Exit     F6=Print       F9=Retrieve", attr::BLUE);
        m.put(23, 2, "F17=Top    F18=Bottom    F21=CL command line", attr::BLUE);
        m.set_cursor(rows::LINE, 7);
        m.set_error_row(m.rows());
        if !message.is_empty() {
            m.error_line(message, attr::WHITE);
            m.alarm();
        }
        m.field_index_at(rows::LINE, 7)
    }

    fn ask(&mut self, prompt: &str, hidden: bool) -> Option<String> {
        use tn5250::ds::aid;
        use tn5250::screen::Read;
        let prompt = self.prompt_of(prompt);
        let mut surface = crate::display::surface::Surface::new(self.host.clone());
        let mut value = String::new();
        let mut message = String::new();
        let mut back = 0;
        loop {
            let field = surface.with_model(|m| self.draw(m, &prompt, hidden, &value, &message));
            surface.flush(Read::MdtFields);
            let (a, _) = surface.wait()?;
            let typed = surface.with_model(|m| {
                field
                    .and_then(|i| m.fields().get(i).map(tn5250::screen::Field::text))
                    .unwrap_or_default()
            });
            message.clear();
            value.clear();
            let last = self.lines.len().saturating_sub(rows::SHOWN);
            match a {
                aid::ENTER => {
                    if !hidden {
                        let t = typed.trim().to_string();
                        self.lines.push(format!(">    {t}"));
                        if !t.is_empty() {
                            self.commands.push(t);
                        }
                    }
                    self.top = None;
                    return Some(typed.trim().to_string());
                }
                aid::ROLL_UP => {
                    let at = self.top.unwrap_or(last);
                    self.top = Some((at + rows::SHOWN).min(last));
                }
                aid::ROLL_DOWN => {
                    let at = self.top.unwrap_or(last);
                    self.top = Some(at.saturating_sub(rows::SHOWN));
                }
                a => match aid::function_of(a) {
                    Some(3) => return None,
                    Some(9) if !hidden => {
                        back += 1;
                        if let Some(c) = self.commands.iter().rev().nth(back - 1) {
                            value.clone_from(c);
                        } else {
                            back = 0;
                        }
                    }
                    Some(17) => self.top = Some(0),
                    Some(18) => self.top = None,
                    _ => {
                        value = typed;
                        message = crate::display::msgline::text(&self.host, "CPD6A60", &[]);
                    }
                },
            }
        }
    }
}

impl Console for Screen {
    fn read_line(&mut self, prompt: &str) -> Option<String> {
        self.ask(prompt, false)
    }

    fn read_password(&mut self, prompt: &str) -> Option<String> {
        self.ask(prompt, true)
    }

    fn write(&mut self, line: &str) {
        self.lines.push(format!("  {line}"));
    }
}

/// The job, as the client reaches it.
struct JobSide<'a> {
    job: &'a mut Job,
    host: &'a Host,
    ifs: RemoteIfs,
    ccsid: u16,
}

impl FtpHost for JobSide<'_> {
    fn ifs(&self) -> &dyn IfsClient {
        &self.ifs
    }

    fn sign_on(&mut self, _user: &str, _password: &str) -> Result<String, Refused> {
        Err(Refused {
            msgid: sys_cpps::check::LATER.into(),
            text: "The client does not sign on locally.".into(),
        })
    }

    fn profile(&mut self, _user: &str) -> Option<Profile> {
        None
    }

    fn become_user(&mut self, _user: &str, _password: Option<&str>) -> Result<(), String> {
        Err("the client keeps its job's user".into())
    }

    fn command(&mut self, line: &str) -> Result<(), CmdFail> {
        let o = super::run(self.job, self.host, line);
        match o.escape {
            Some(f) => Err(CmdFail {
                text: if f.text.is_empty() { f.msgid.clone() } else { f.text },
                msgid: f.msgid,
            }),
            None => Ok(()),
        }
    }

    fn exit(&mut self, point: &str, req: &ExitRequest) -> ExitAnswer {
        let mut reg = stg::remote::RemoteRegistry::new(Rc::clone(&self.host.link));
        match reg.programs(point, req.format()) {
            Ok(p) if p.is_empty() => ExitAnswer {
                allow: true,
                outputs: req.parms(),
                called: Vec::new(),
                error: None,
            },
            _ => regfac::call_exit(&mut reg, self.job, point, req),
        }
    }

    fn file(&mut self, lib: &str, file: &str) -> Option<DbFile> {
        let f = self.host.link.borrow_mut().with(|l| l.file(lib.to_string(), file.to_string()))?;
        let ccsid = self
            .ifs
            .stat(&ftp::objects::file_path(lib, file), true)
            .map_or(self.ccsid, |s| s.ccsid);
        let len = usize::try_from(f.rec_len).unwrap_or(0);
        Some(DbFile {
            rec_len: if f.source { len.saturating_sub(12) } else { len },
            source: f.source,
            ccsid,
            logical: f.attr == "LF",
        })
    }

    fn sysval(&mut self, name: &str) -> Option<String> {
        let v = self.host.link.borrow_mut().with(|l| l.sysval(name.to_string()))?;
        Some(ccsid::from_ebcdic(&v.data).trim().to_string())
    }

    fn job_ccsid(&self) -> u16 {
        self.ccsid
    }

    fn now(&self) -> i64 {
        let n = self.host.link.borrow_mut().with(stg::Client::now);
        i64::try_from(n.unix).unwrap_or(0)
    }

    fn utc_offset(&self) -> i64 {
        i64::from(self.host.link.borrow_mut().with(stg::Client::now).offset_min) * 60
    }
}

/// The library, file and member an override sends `file` to, when it
/// is overridden.
fn overridden(host: &Host, file: &str, usage: qir::FileUsage) -> Option<(String, String, String)> {
    let mut io = RemoteIo::new(host.link.clone());
    let spec = OpenSpec {
        // BLOCKED(sql): no trigger programs called here; a file with
        // triggers refuses the change.
        triggers: false,
        file: file.into(),
        lib: "*LIBL".into(),
        usage,
        keyed: false,
        lvlchk: None,
        member: None,
        actgrp: "*DFTACTGRP".into(),
        commit: false,
    };
    let (h, info) = io.open(&spec).ok()?;
    let _ = io.close(h);
    // INPUT and OUTPUT are not files of their own: only an override
    // names one.
    if info.file.eq_ignore_ascii_case(file) {
        return None;
    }
    Some((info.lib, info.file, info.member))
}

/// The text of a member's records, trailing blanks off.
fn member_lines(side: &mut JobSide<'_>, lib: &str, file: &str, mbr: &str) -> Result<Vec<String>, String> {
    let (recs, desc) =
        ftp::objects::read_member(side, lib, file, mbr).map_err(|e| format!("{lib}/{file}({mbr}): {e:?}"))?;
    Ok(recs
        .iter()
        .map(|r| {
            ccsid::conv::to_string(desc.ccsid, r)
                .unwrap_or_default()
                .trim_end()
                .to_string()
        })
        .collect())
}

/// Add lines to a member as records.
fn add_lines(side: &mut JobSide<'_>, lib: &str, file: &str, mbr: &str, lines: &[String]) -> Result<(), String> {
    let desc = side.file(lib, file).ok_or_else(|| format!("{lib}/{file} cannot be used"))?;
    let mut recs = ftp::objects::read_member(side, lib, file, mbr).map(|r| r.0).unwrap_or_default();
    for l in lines {
        let b = ccsid::conv::from_str(desc.ccsid, l).unwrap_or_default();
        recs.push(ccsid::conv::fit(desc.ccsid, &b, desc.rec_len, 0x40).bytes);
    }
    ftp::objects::write_member(side, lib, file, mbr, &recs).map_err(|e| format!("{e:?}"))
}

/// The TLS configuration the client uses: the server's certificate must
/// chain to the shipped roots or a trusted CA of `*SYSTEM` and name the
/// host. The client presents no certificate: `QS400_FTP_CLIENT`'s
/// key goes only to server profiles (docs/CERTS.md).
fn client_tls(host: &Host) -> Option<Arc<tls::rustls::ClientConfig>> {
    crate::work::tls::client_config(&mut host.link.borrow_mut())
        .map_err(|e| eprintln!("qsjob: FTP client TLS: {e}"))
        .ok()
}

/// `FTP RMTSYS INTNETADR CCSID PORT SECCNN DTAPROT`.
pub fn run(job: &mut Job, host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    let _ = out;
    let parms = parms_of(r);
    let mut rmtsys = parms.text("RMTSYS");
    if rmtsys.eq_ignore_ascii_case("*INTNETADR") {
        rmtsys = parms.text("INTNETADR");
    }
    let port = parms.int("PORT").and_then(|p| u16::try_from(p).ok()).unwrap_or(21);
    let security = match parms.name("SECCNN").as_str() {
        "*SSL" => Security::Explicit,
        "*IMPLICIT" => Security::Implicit,
        "*DFT" if port == 990 => Security::Implicit,
        "*KERBEROS" => return Err(Fail::later("FTP")),
        _ => Security::None,
    };
    let private = match parms.name("DTAPROT").as_str() {
        "*CLEAR" => false,
        "*SAFE" => return Err(Fail::later("FTP")),
        _ => security != Security::None,
    };
    let ccsid = parms.int("CCSID").and_then(|c| u16::try_from(c).ok()).unwrap_or(819);
    let attrs = host.attrs();
    let job_ccsid = match u16::try_from(attrs.ccsid) {
        Ok(65535) | Err(_) => u16::try_from(attrs.dftccsid).unwrap_or(37),
        Ok(c) => c,
    };
    let input = overridden(host, "INPUT", qir::FileUsage::Input);
    let output = overridden(host, "OUTPUT", qir::FileUsage::Output);
    let curlib = attrs.libl.cur.clone().filter(|c| !c.is_empty()).unwrap_or_else(|| "QGPL".to_string());
    let mut side = JobSide {
        job,
        host,
        ifs: RemoteIfs::new(host.link.clone()),
        ccsid: job_ccsid,
    };
    let opts = |batch: bool| Options {
        port,
        security,
        private,
        ccsid,
        tls: if security == Security::None { None } else { client_tls(host) },
        batch,
    };
    let lcwd = ftp::names::library_dir(&curlib);
    let lines = match &input {
        Some((l, f, m)) => member_lines(&mut side, l, f, m).map_err(|e| Fail::check("", format!("INPUT: {e}")))?,
        None if host.display.borrow().is_some() => {
            // At a display: the client's own screen.
            let mut client = Client::new(side, Screen::new(host), opts(false), &lcwd);
            client.run(&rmtsys);
            return Ok(format!("FTP session with {rmtsys} ended.\n"));
        }
        None => {
            return Err(Fail::no_display("FTP"));
        }
    };
    let script = Script {
        lines: lines.into(),
        out: vec!["Subcommands are read from the file INPUT.".into()],
    };
    let mut client = Client::new(side, script, opts(true), &lcwd);
    client.run(&rmtsys);
    let Client { mut host, console, .. } = client;
    let text = console.out;
    match &output {
        Some((l, f, m)) => {
            add_lines(&mut host, l, f, m, &text).map_err(|e| Fail::check("", format!("OUTPUT: {e}")))?;
            Ok(format!("FTP session with {rmtsys} ended; its log is in {l}/{f}({m}).\n"))
        }
        None => Ok(text.join("\n") + "\n"),
    }
}
