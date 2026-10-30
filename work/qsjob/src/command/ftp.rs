// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `FTP` and `STRTCPFTP`: `net/ftp`'s client in this job. Subcommands
//! come from the member the file `INPUT` is overridden to, as IBM's
//! client reads them in batch (the first line the user and password);
//! what the session prints goes to the member `OUTPUT` is overridden to
//! (added to it), or is the command's output when there is no override.
//! Local objects are reached as the job reaches them; `SYSCMD` runs in
//! this job.
// MIGRATE(phase11): the client's own display (the FTP session screen)
// for an interactive job without an INPUT override.

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
            msgid: "CPF9898".into(),
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
        let mut r = ccsid::conv::from_str(desc.ccsid, l).unwrap_or_default();
        r.truncate(desc.rec_len);
        r.resize(desc.rec_len, 0x40);
        recs.push(r);
    }
    ftp::objects::write_member(side, lib, file, mbr, &recs).map_err(|e| format!("{e:?}"))
}

/// The TLS configuration the client uses: server certificates are taken
/// as they come until the job can read the system's trust store.
// BLOCKED(phase10): server authentication with the *SYSTEM store and
// the application ID QIBM_QTMF_FTP_CLIENT needs the storage service to
// answer them.
fn client_tls() -> Option<Arc<tls::rustls::ClientConfig>> {
    tls::config::client_config(
        &tls::policy::TlsPolicy::default(),
        tls::rustls::RootCertStore::empty(),
        None,
        tls::config::ServerAuthMode::Passthru,
    )
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
        "*KERBEROS" => return Err(Fail::new("CPF9898", "Kerberos connections are not supported.")),
        _ => Security::None,
    };
    let private = match parms.name("DTAPROT").as_str() {
        "*CLEAR" => false,
        "*SAFE" => return Err(Fail::new("CPF9898", "Data protection *SAFE needs Kerberos.")),
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
    let lines = match &input {
        Some((l, f, m)) => member_lines(&mut side, l, f, m).map_err(|e| Fail::new("CPF9898", format!("INPUT: {e}")))?,
        None => {
            return Err(Fail::new(
                "CPF9898",
                "FTP reads its subcommands from the file INPUT: override it to a member (OVRDBF FILE(INPUT)).",
            ));
        }
    };
    let script = Script {
        lines: lines.into(),
        out: vec!["Subcommands are read from the file INPUT.".into()],
    };
    let opts = Options {
        port,
        security,
        private,
        ccsid,
        tls: if security == Security::None { None } else { client_tls() },
        batch: true,
    };
    let lcwd = ftp::names::library_dir(&curlib);
    let mut client = Client::new(side, script, opts, &lcwd);
    client.run(&rmtsys);
    let Client { mut host, console, .. } = client;
    let text = console.out;
    match &output {
        Some((l, f, m)) => {
            add_lines(&mut host, l, f, m, &text).map_err(|e| Fail::new("CPF9898", format!("OUTPUT: {e}")))?;
            Ok(format!("FTP session with {rmtsys} ended; its log is in {l}/{f}({m}).\n"))
        }
        None => Ok(text.join("\n") + "\n"),
    }
}
