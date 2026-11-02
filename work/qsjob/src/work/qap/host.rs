// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The connection job's reach. [`JobHost`] is the job's own thread:
//! commands in its runtime (`commands.rs`), and the end of the job: its
//! parent's `End` (the server ending), `ENDJOB` (an operator), or its
//! session ended on another connection.
//! [`JobThread`] is a secondary thread of the job on a thread link of
//! its own (`hello_thread`), sharing the job's security session: the
//! system's description and messages, sign-on, sessions and passwords
//! through the storage service (`stgsvc/qap.rs`). Passwords and tokens
//! go to the service and nowhere else.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ipc::WireError;
use ipc::chan::Chan;
use qap_proto::ops::command::CommandDefinition;
use qap_proto::ops::signon::{PasswordStatus, SignOnReply, TelnetTicketReply};
use qap_proto::ops::spool::{
    ListOutputQueues, ListSpooledFiles, OutputQueues, ReadSpooledFile, SpooledData, SpooledFileId, SpooledFiles,
};
use qap_proto::ops::system::{Product, SystemInfo};
use qap_proto::types::ObjectName;
use qap_proto::{Secret, ServerMessage, Status, Token};
use qapd::sessions::EndedBy;
use qapd::{Canceller, Ending, Host, Ran, Refusal, SystemId, ThreadHost, Threads};
use stg::{Client, Link, QapRefused, QapSession, Shared};
use wm::ctl::{Start, ToJob};

use crate::console::Console;

/// How often the job asks whether an operator ended it.
const CHECK: Duration = Duration::from_millis(500);

/// A system value's text, trimmed.
fn sysval_text(link: &mut Link, name: &str) -> String {
    link.with(|l| l.sysval(name.to_string()))
        .map(|v| match v.kind {
            sysapi::SysValKind::Bin => v
                .data
                .get(..4)
                .and_then(|b| b.try_into().ok())
                .map(i32::from_be_bytes)
                .unwrap_or_default()
                .to_string(),
            sysapi::SysValKind::Char => ccsid::from_ebcdic(&v.data).trim().to_string(),
        })
        .unwrap_or_default()
}

/// Message `id`, looked up in this job, so in its user's language once
/// the job runs as the signed-on profile (its library list finds the
/// message files).
fn message(link: &mut Link, id: &str, data: &[&str]) -> ServerMessage {
    let file = sysapi::param::file_of(id);
    let desc = link.with(|l| l.describe(id.to_string(), file.into(), "*LIBL".into()));
    match desc {
        Some(d) => {
            let bytes = d.data_of(data);
            ServerMessage {
                id: id.to_string(),
                severity: u8::try_from(d.severity).unwrap_or(99),
                text: d.format(&bytes),
                help: d.format_help(&bytes),
                data: bytes,
            }
        }
        None => ServerMessage {
            id: id.to_string(),
            severity: 40,
            data: Vec::new(),
            text: format!("{id} {}", data.join(" ")).trim().to_string(),
            help: String::new(),
        },
    }
}

/// The job cannot go on (its link to the service is gone): the close
/// is logged with why, then the process ends.
fn ending(why: &str) -> ! {
    qapd::exiting(&qapd::Closed::Exit(why.to_string()));
    eprintln!("qsjob: QAPCNN: {why}");
    std::process::exit(70)
}

/// A lost link to the storage service, on any of the job's threads.
pub fn lost(e: &WireError) -> ! {
    ending(&format!("storage service lost: {e}"))
}

/// The connection job's own thread.
pub struct JobHost {
    pub link: Shared,
    /// The job's runtime: commands, its job log, its control channel.
    pub console: Console,
    me: u32,
    checked: Instant,
}

impl JobHost {
    /// The job `start` describes, over `link`; its console reads `ctl`.
    pub fn new(link: &Shared, start: &Start, ctl: Chan) -> Self {
        Self {
            link: std::rc::Rc::clone(link),
            console: Console::new(ctl, link, start),
            me: start.number,
            checked: Instant::now(),
        }
    }

    /// Whether the parent sent `End`: the console's inbox and what it
    /// put aside.
    fn told_to_end(&self) -> bool {
        let h = &self.console.host;
        while let Ok(m) = h.inbox.try_recv() {
            h.pending.borrow_mut().push_back(m);
        }
        h.pending.borrow().iter().any(|m| matches!(m, ToJob::End { .. }))
    }
}

impl Host for JobHost {
    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage {
        message(&mut self.link.borrow_mut(), id, data)
    }

    fn ending(&mut self) -> Option<Ending> {
        if self.told_to_end() {
            return Some(Ending::Server);
        }
        if self.checked.elapsed() < CHECK {
            return None;
        }
        self.checked = Instant::now();
        let state = self.link.borrow_mut().with(Client::sbs_state);
        if state.ending.is_some() {
            Some(Ending::Server)
        } else if state.end_jobs.iter().any(|(n, _)| *n == self.me) {
            Some(Ending::Operator)
        } else {
            let ended = self.link.borrow_mut().with(Client::qap_ended);
            ended.map(|op| Ending::Session(if op { EndedBy::Operator } else { EndedBy::SignOff }))
        }
    }

    fn sign_off(&mut self, token: &Token) {
        self.link.borrow_mut().with(|l| l.qap_signoff(token.0.to_vec(), false));
        self.console.swapped();
    }

    fn end_session(&mut self, token: &Token) {
        self.link.borrow_mut().with(|l| l.qap_signoff(token.0.to_vec(), true));
        self.console.swapped();
    }

    fn detach(&mut self, token: &Token) {
        self.link.borrow_mut().with(|l| l.qap_detach(token.0.to_vec()));
    }

    /// A secondary thread swapped the job's profile: the runtime
    /// follows (its attributes and library list).
    fn swapped(&mut self) {
        self.console.swapped();
    }

    fn run_command(&mut self, command: &str, cancelled: &dyn Fn() -> bool) -> Ran {
        super::commands::run(&mut self.console, command, cancelled)
    }

    fn check_command(&mut self, command: &str) -> (Status, Vec<ServerMessage>) {
        super::commands::check(&mut self.console, command)
    }

    fn describe_command(&mut self, command: &ObjectName) -> Result<CommandDefinition, Refusal> {
        match super::commands::describe(&mut self.console, command) {
            Ok(d) => Ok(d),
            Err((status, id, data)) => {
                let data: Vec<&str> = data.iter().map(String::as_str).collect();
                let message = self.message(&id, &data);
                Err(Refusal { status, message })
            }
        }
    }

    /// `Cancel` of a running command ends its request, as System
    /// Request option 2 does.
    fn canceller(&mut self) -> Option<Canceller> {
        Some(Arc::new(crate::sysdsp::keys::end_from_outside))
    }

    fn threads(&mut self) -> Threads {
        let (number, pid) = (self.me, std::process::id());
        Arc::new(move || Box::new(JobThread::open(number, pid)))
    }
}

/// A secondary thread of the connection job.
pub struct JobThread {
    link: Link,
}

impl JobThread {
    /// A thread of job `number` in process `pid`, on a link of its own.
    /// Without the service the job can do nothing: it ends.
    fn open(number: u32, pid: u32) -> Self {
        let chan = match Chan::connect(Path::new(stg::SOCKET)) {
            Ok(c) => c,
            Err(e) => ending(&format!("a thread's link to the storage service: {e}")),
        };
        let mut link = Link::over(chan);
        link.on_lost(lost);
        if let Err(e) = link.with(|l| l.hello_thread(number, pid)) {
            ending(&format!("a thread's hello to the storage service: {e}"));
        }
        Self { link }
    }

    /// A refusal from the service: its status, and the message as a 5250
    /// display shows it (the service's text, no more) or from the file.
    fn refusal(&mut self, r: &QapRefused) -> Refusal {
        let status = ipc::decode::<Status>(&r.status.to_le_bytes()).unwrap_or(Status::Error);
        let data: Vec<&str> = r.data.iter().map(String::as_str).collect();
        let mut message = self.message(&r.msgid, &data);
        if !r.text.is_empty() {
            message.text.clone_from(&r.text);
            message.help.clear();
        }
        Refusal { status, message }
    }

    fn session(&mut self, r: Result<QapSession, QapRefused>) -> Result<SignOnReply, Refusal> {
        r.map(|s| reply_of(&s)).map_err(|e| self.refusal(&e))
    }
}

/// A session from the service as the client sees it.
fn reply_of(s: &QapSession) -> SignOnReply {
    SignOnReply {
        token: Token(s.token.as_slice().try_into().unwrap_or([0; 32])),
        idle_timeout_secs: s.idle_secs,
        profile: s.profile.clone(),
        text: s.text.clone(),
        ccsid: s.ccsid,
        date_format: s.date_format.clone(),
        date_separator: s.date_separator.clone(),
        time_separator: s.time_separator.clone(),
        home_directory: s.home_directory.clone(),
        special_authorities: s.special_authorities.clone(),
    }
}

impl ThreadHost for JobThread {
    fn system_id(&mut self) -> SystemId {
        SystemId {
            system_name: sysval_text(&mut self.link, "QSYSNAME"),
            release: release::RELEASE.to_string(),
            time_zone: sysval_text(&mut self.link, "QTIMZON"),
            ccsid: sysval_text(&mut self.link, "QCCSID").parse().unwrap_or(65535),
        }
    }

    fn now_ms(&mut self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
    }

    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage {
        message(&mut self.link, id, data)
    }

    /// `DSPSFWRSC`'s rows give the products, the system job `SCPF`'s
    /// start the IPL, the firmware the serial (`QSRLNBR`) and model.
    fn system_info(&mut self) -> SystemInfo {
        let rows = self.link.with(Client::sfwrsc_panel);
        let col = |r: &[String], i: usize| r.get(i).map_or_else(String::new, |v| v.trim().to_string());
        let release = rows
            .iter()
            .find(|r| r.first().is_some_and(|id| id == release::PRODUCT_ID))
            .map_or_else(|| release::RELEASE.to_string(), |r| col(r, 6));
        let jobs = self.link.with(Client::active_jobs);
        let ipl = jobs.iter().find(|j| j.kind == 'X').map_or(0, |j| j.started);
        let machine = self.link.with(Client::machine);
        SystemInfo {
            system_name: sysval_text(&mut self.link, "QSYSNAME"),
            release,
            serial: if machine.serial.is_empty() {
                sysval_text(&mut self.link, "QSRLNBR")
            } else {
                machine.serial
            },
            // The hardware's model as the firmware names it, empty when
            // it names none; QMODEL is for programs, not clients.
            model: machine.hw_model,
            ipl_time: i64::try_from(ipl).unwrap_or(0).saturating_mul(1000),
            products: rows
                .iter()
                .map(|r| Product {
                    id: col(r, 0),
                    option: col(r, 1),
                    release: col(r, 6),
                })
                .collect(),
        }
    }

    fn sign_on(&mut self, user: &str, password: &Secret) -> Result<SignOnReply, Refusal> {
        let r = self
            .link
            .with(|l| l.qap_signon(user.to_string(), password.expose().to_string()));
        self.session(r)
    }

    fn change_password(&mut self, user: &str, old: &Secret, new: &Secret) -> Result<SignOnReply, Refusal> {
        let r = self.link.with(|l| {
            l.qap_chgpwd(user.to_string(), old.expose().to_string(), new.expose().to_string())
        });
        self.session(r)
    }

    fn resume(&mut self, token: &Token) -> Result<SignOnReply, Refusal> {
        let r = self.link.with(|l| l.qap_resume(token.0.to_vec()));
        self.session(r)
    }

    fn attach(&mut self, token: &Token) -> Result<SignOnReply, Refusal> {
        let r = self.link.with(|l| l.qap_attach(token.0.to_vec()));
        self.session(r)
    }

    fn sign_off(&mut self, token: &Token) {
        self.link.with(|l| l.qap_signoff(token.0.to_vec(), false));
    }

    fn password_status(&mut self) -> PasswordStatus {
        let (expires, warning_days) = self.link.with(Client::qap_pwd_status);
        PasswordStatus { expires, warning_days }
    }

    fn telnet_ticket(&mut self) -> Result<TelnetTicketReply, Refusal> {
        let r = self.link.with(Client::qap_ticket);
        match r {
            Ok((ticket, expires_secs)) => Ok(TelnetTicketReply {
                ticket: Token(ticket.as_slice().try_into().unwrap_or([0; 32])),
                expires_secs,
            }),
            Err(e) => Err(self.refusal(&e)),
        }
    }

    fn list_spooled_files(&mut self, req: &ListSpooledFiles) -> Result<SpooledFiles, Refusal> {
        let r = self.link.with(|l| l.qap_splf_list(req.clone()));
        r.map_err(|e| self.refusal(&e))
    }

    fn read_spooled_file(&mut self, req: &ReadSpooledFile) -> Result<SpooledData, Refusal> {
        let r = self.link.with(|l| l.qap_splf_read(req.clone()));
        r.map_err(|e| self.refusal(&e))
    }

    fn hold_spooled_file(&mut self, id: &SpooledFileId, immediately: bool) -> Result<(), Refusal> {
        let r = self.link.with(|l| l.qap_splf_hold(id.clone(), immediately));
        r.map_err(|e| self.refusal(&e))
    }

    fn release_spooled_file(&mut self, id: &SpooledFileId) -> Result<(), Refusal> {
        let r = self.link.with(|l| l.qap_splf_release(id.clone()));
        r.map_err(|e| self.refusal(&e))
    }

    fn delete_spooled_file(&mut self, id: &SpooledFileId) -> Result<(), Refusal> {
        let r = self.link.with(|l| l.qap_splf_delete(id.clone()));
        r.map_err(|e| self.refusal(&e))
    }

    fn move_spooled_file(&mut self, id: &SpooledFileId, outq: &ObjectName, first: bool) -> Result<(), Refusal> {
        let r = self.link.with(|l| l.qap_splf_move(id.clone(), outq.clone(), first));
        r.map_err(|e| self.refusal(&e))
    }

    fn list_output_queues(&mut self, req: &ListOutputQueues) -> Result<OutputQueues, Refusal> {
        let r = self.link.with(|l| l.qap_outq_list(req.clone()));
        r.map_err(|e| self.refusal(&e))
    }
}
