// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The connection job's [`qapd::Host`]: the system's description and
//! messages through its link, sign-on, sessions and passwords through
//! the storage service (`stgsvc/qap.rs`), commands in the job's own
//! runtime (`commands.rs`), and the end of the job: its parent's `End`
//! (the server ending) or `ENDJOB` (an operator). Passwords and tokens
//! go to the service and nowhere else.

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ipc::chan::Chan;
use qap_proto::ops::command::CommandDefinition;
use qap_proto::ops::signon::{PasswordStatus, SignOnReply, TelnetTicketReply};
use qap_proto::types::ObjectName;
use qap_proto::ops::system::{Product, SystemInfo};
use qap_proto::{Secret, ServerMessage, Status, Token};
use qapd::{Canceller, Ending, Host, Ran, Refusal, SystemId};
use stg::{Client, QapRefused, QapSession, Shared};
use wm::ctl::{Start, ToJob};

use crate::console::Console;

/// How often the job asks whether an operator ended it.
const CHECK: Duration = Duration::from_millis(500);

/// The connection job's reach.
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

    /// The job runs as another profile: its runtime follows.
    fn swapped<T>(&mut self, r: Result<T, Refusal>) -> Result<T, Refusal> {
        if r.is_ok() {
            self.console.swapped();
        }
        r
    }

    /// A system value's text, trimmed.
    pub fn sysval_text(&self, name: &str) -> String {
        self.link
            .borrow_mut()
            .with(|l| l.sysval(name.to_string()))
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

impl JobHost {
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

impl Host for JobHost {
    fn system_id(&mut self) -> SystemId {
        SystemId {
            system_name: self.sysval_text("QSYSNAME"),
            release: release::RELEASE.to_string(),
            time_zone: self.sysval_text("QTIMZON"),
            ccsid: self.sysval_text("QCCSID").parse().unwrap_or(65535),
        }
    }

    fn now_ms(&mut self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
    }

    /// Looked up in this job, so in its user's language once the job
    /// runs as the signed-on profile (its library list finds the
    /// message files).
    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage {
        let file = sysapi::param::file_of(id);
        let desc = self
            .link
            .borrow_mut()
            .with(|l| l.describe(id.to_string(), file.into(), "*LIBL".into()));
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

    /// `DSPSFWRSC`'s rows give the products, the system job `SCPF`'s
    /// start the IPL, the firmware the serial (`QSRLNBR`) and model.
    fn system_info(&mut self) -> SystemInfo {
        let rows = self.link.borrow_mut().with(Client::sfwrsc_panel);
        let col = |r: &[String], i: usize| r.get(i).map_or_else(String::new, |v| v.trim().to_string());
        let release = rows
            .iter()
            .find(|r| r.first().is_some_and(|id| id == release::PRODUCT_ID))
            .map_or_else(|| release::RELEASE.to_string(), |r| col(r, 6));
        let jobs = self.link.borrow_mut().with(Client::active_jobs);
        let ipl = jobs.iter().find(|j| j.kind == 'X').map_or(0, |j| j.started);
        let machine = self.link.borrow_mut().with(Client::machine);
        SystemInfo {
            system_name: self.sysval_text("QSYSNAME"),
            release,
            serial: if machine.serial.is_empty() { self.sysval_text("QSRLNBR") } else { machine.serial },
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
            .borrow_mut()
            .with(|l| l.qap_signon(user.to_string(), password.expose().to_string()));
        let r = self.session(r);
        self.swapped(r)
    }

    fn change_password(&mut self, user: &str, old: &Secret, new: &Secret) -> Result<SignOnReply, Refusal> {
        let r = self.link.borrow_mut().with(|l| {
            l.qap_chgpwd(user.to_string(), old.expose().to_string(), new.expose().to_string())
        });
        let r = self.session(r);
        self.swapped(r)
    }

    fn resume(&mut self, token: &Token) -> Result<SignOnReply, Refusal> {
        let r = self.link.borrow_mut().with(|l| l.qap_resume(token.0.to_vec()));
        let r = self.session(r);
        self.swapped(r)
    }

    fn sign_off(&mut self, token: &Token) {
        self.link.borrow_mut().with(|l| l.qap_signoff(token.0.to_vec()));
        self.console.swapped();
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

    fn password_status(&mut self) -> PasswordStatus {
        let (expires, warning_days) = self.link.borrow_mut().with(Client::qap_pwd_status);
        PasswordStatus { expires, warning_days }
    }

    fn telnet_ticket(&mut self) -> Result<TelnetTicketReply, Refusal> {
        let r = self.link.borrow_mut().with(Client::qap_ticket);
        match r {
            Ok((ticket, expires_secs)) => Ok(TelnetTicketReply {
                ticket: Token(ticket.as_slice().try_into().unwrap_or([0; 32])),
                expires_secs,
            }),
            Err(e) => Err(self.refusal(&e)),
        }
    }

    fn run_command(&mut self, command: &str) -> Ran {
        super::commands::run(&mut self.console, command)
    }

    fn check_command(&mut self, command: &str) -> (Status, Vec<ServerMessage>) {
        super::commands::check(&mut self.console, command)
    }

    /// `Cancel` of a running command ends its request, as System
    /// Request option 2 does.
    fn canceller(&mut self) -> Option<Canceller> {
        Some(Arc::new(crate::sysdsp::keys::end_from_outside))
    }

    fn detach(&mut self, token: &Token) {
        self.link.borrow_mut().with(|l| l.qap_detach(token.0.to_vec()));
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
            None
        }
    }
}
