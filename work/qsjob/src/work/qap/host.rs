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
//! go to the service and nowhere else. `Sql` runs in the lane over the
//! job's own link and its catalog lists on a thread's (`sql.rs`,
//! `crate::qapsql`). `Ifs` runs on a thread's link through the IFS
//! requests every job uses (`stg::remote::RemoteIfs`), so its authority
//! checks and audit entries are the file-system commands' (`qapd::files`).
//! `Messages` runs on a thread's link through the service's message
//! queue functions, the message commands' (`messages.rs`). `Manage`
//! runs on a thread's link through the commands it stands for, the
//! analyzer first (`manage.rs`).

use std::cell::{RefCell, RefMut};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use ifs::client::IfsClient;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use stg::remote::RemoteIfs;

use ipc::WireError;
use ipc::chan::Chan;
use qap_proto::ops::command::CommandDefinition;
use qap_proto::ops::develop::{
    ChangeMember, CopyMember, CreateMember, DescribeObject, Library, LibraryList, ListLibraries,
    ListMembers, ListObjects, MemberData, MemberEntry, ObjectDescription, ObjectEntry,
    RenameMember, SetLibraryList,
};
use qap_proto::ops::jobs::{EndJob, JobEntry, JobLogEntry, ListJobs};
use qap_proto::ops::manage::{
    DiskUnits, EndSubsystem, HardwareResources, IplStatus, PowerDown, Ptf, ResourceType,
    SetSystemValue, Subsystem, SystemValue, SystemValues,
};
use qap_proto::ops::messages::{
    ListMessages, Messages, RemoveMessages, ReplyMessage, SendMessage, Sent,
};
use qap_proto::ops::signon::{PasswordStatus, SignOnReply, TelnetTicketReply};
use qap_proto::ops::spool::{
    ListOutputQueues, ListSpooledFiles, OutputQueues, ReadSpooledFile, SpooledData, SpooledFileId,
    SpooledFiles,
};
use qap_proto::ops::sql::{
    Columns, Execute, Executed, Fetch, Fetched, Indexes, ListColumns, ListIndexes, ListRoutines,
    ListSchemas, ListTables, Prepared, Routine, Schemas, SetOptions, Tables,
};
use qap_proto::ops::system::{Product, SystemInfo};
use qap_proto::types::{JobId, MemberName, ObjectName, SourceRecord};
use qap_proto::{Secret, ServerMessage, Status, Token};
use qapd::manage::ExitRequest;
use qapd::sessions::EndedBy;
use qapd::sql::SqlResult;
use qapd::{Canceller, Compiled, Ending, Host, Ran, Refusal, SystemId, ThreadHost, Threads};
use stg::{Client, Link, QapRefused, QapSession, Shared};
use wm::ctl::{Start, ToJob};

use super::sql::{COMMAND, IDLE, LaneDb, Running, SQL, ThreadDb};
use super::{develop, jobs, manage};
use crate::console::Console;
use crate::qapsql::{Lane, catalog};

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
pub(super) fn message(link: &mut Link, id: &str, data: &[&str]) -> ServerMessage {
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

/// Message `id` with substitution data already laid out (a command's),
/// looked up as [`message`] looks it up.
pub(super) fn message_bytes(link: &mut Link, id: &str, data: &[u8]) -> ServerMessage {
    let file = sysapi::param::file_of(id);
    let desc = link.with(|l| l.describe(id.to_string(), file.into(), "*LIBL".into()));
    match desc {
        Some(d) => ServerMessage {
            id: id.to_string(),
            severity: u8::try_from(d.severity).unwrap_or(99),
            text: d.format(data),
            help: d.format_help(data),
            data: data.to_vec(),
        },
        None => ServerMessage {
            id: id.to_string(),
            severity: 40,
            data: data.to_vec(),
            text: id.to_string(),
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
    /// The connection's statements, cursors and transaction.
    sql: Lane,
    /// What runs in the lane, for the canceller.
    running: Running,
}

impl JobHost {
    /// The job `start` describes, over `link`; its console reads `ctl`.
    pub fn new(link: &Shared, start: &Start, ctl: Chan) -> Self {
        Self {
            link: std::rc::Rc::clone(link),
            console: Console::new(ctl, link, start),
            me: start.number,
            checked: Instant::now(),
            sql: Lane::default(),
            running: Arc::new(AtomicU8::new(IDLE)),
        }
    }

    /// Whether the parent sent `End`: the console's inbox and what it
    /// put aside.
    fn told_to_end(&self) -> bool {
        let h = &self.console.host;
        while let Ok(m) = h.inbox.try_recv() {
            h.pending.borrow_mut().push_back(m);
        }
        h.pending
            .borrow()
            .iter()
            .any(|m| matches!(m, ToJob::End { .. }))
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
            ended.map(|op| {
                Ending::Session(if op {
                    EndedBy::Operator
                } else {
                    EndedBy::SignOff
                })
            })
        }
    }

    fn sign_off(&mut self, token: &Token) {
        self.link
            .borrow_mut()
            .with(|l| l.qap_signoff(token.0.to_vec(), false));
        self.console.swapped();
    }

    fn end_session(&mut self, token: &Token) {
        self.link
            .borrow_mut()
            .with(|l| l.qap_signoff(token.0.to_vec(), true));
        self.console.swapped();
    }

    fn detach(&mut self, token: &Token) {
        self.link
            .borrow_mut()
            .with(|l| l.qap_detach(token.0.to_vec()));
    }

    /// A secondary thread swapped the job's profile: the runtime
    /// follows (its attributes and library list).
    fn swapped(&mut self) {
        self.console.swapped();
    }

    fn run_command(&mut self, command: &str, cancelled: &dyn Fn() -> bool) -> Ran {
        self.running.store(COMMAND, Ordering::SeqCst);
        let r = super::commands::run(&mut self.console, command, cancelled);
        self.running.store(IDLE, Ordering::SeqCst);
        r
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
    /// Request option 2 does; of a running statement, asks the engine to
    /// end it.
    fn canceller(&mut self) -> Option<Canceller> {
        let (running, number) = (Arc::clone(&self.running), self.me);
        Some(Arc::new(move || match running.load(Ordering::SeqCst) {
            COMMAND => crate::sysdsp::keys::end_from_outside(),
            SQL => super::sql::ask_cancel(number),
            _ => {}
        }))
    }

    fn sql_set_options(&mut self, options: &SetOptions) -> Result<(), Refusal> {
        self.sql
            .set_options(&mut LaneDb(&self.link, &mut self.console), options)
    }

    fn sql_prepare(&mut self, sql: &str) -> SqlResult<Prepared> {
        self.sql
            .prepare(&mut LaneDb(&self.link, &mut self.console), sql)
    }

    fn sql_execute(&mut self, req: &Execute, cancelled: &dyn Fn() -> bool) -> SqlResult<Executed> {
        let (lane, link, console) = (&mut self.sql, &self.link, &mut self.console);
        super::sql::running(&self.running, self.me, || {
            lane.execute(&mut LaneDb(link, console), req, cancelled)
        })
    }

    fn sql_fetch(&mut self, req: &Fetch, cancelled: &dyn Fn() -> bool) -> SqlResult<Fetched> {
        let (lane, link, console) = (&mut self.sql, &self.link, &mut self.console);
        super::sql::running(&self.running, self.me, || {
            lane.fetch(&mut LaneDb(link, console), req, cancelled)
        })
    }

    fn sql_close_cursor(&mut self, cursor: u32) -> Result<(), Refusal> {
        self.sql
            .close_cursor(&mut LaneDb(&self.link, &mut self.console), cursor)
    }

    fn sql_release(&mut self, statement: u32) -> Result<(), Refusal> {
        self.sql
            .release(&mut LaneDb(&self.link, &mut self.console), statement)
    }

    fn sql_commit(&mut self) -> SqlResult<()> {
        self.sql
            .end_unit(&mut LaneDb(&self.link, &mut self.console), true)
    }

    fn sql_rollback(&mut self) -> SqlResult<()> {
        self.sql
            .end_unit(&mut LaneDb(&self.link, &mut self.console), false)
    }

    fn sql_end(&mut self) {
        self.sql.end(&mut LaneDb(&self.link, &mut self.console));
    }

    // `Develop` 1, 2 and 15 (version 8): the job's library list and
    // compiles, in the lane.

    fn library_list(&mut self) -> Result<LibraryList, Refusal> {
        Ok(develop::library_list(&mut self.link.borrow_mut()))
    }

    fn set_library_list(&mut self, req: &SetLibraryList) -> Result<LibraryList, Refusal> {
        develop::set_library_list(&mut self.console, req)?;
        Ok(develop::library_list(&mut self.link.borrow_mut()))
    }

    fn compile(
        &mut self,
        command: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Compiled, Refusal> {
        self.running.store(COMMAND, Ordering::SeqCst);
        let r = super::compile::run(&self.link, &mut self.console, command, cancelled);
        self.running.store(IDLE, Ordering::SeqCst);
        r
    }

    fn threads(&mut self) -> Threads {
        let (number, pid) = (self.me, std::process::id());
        Arc::new(move || Box::new(JobThread::open(number, pid)))
    }
}

/// A secondary thread of the connection job.
pub struct JobThread {
    shared: Shared,
    /// The IFS over the thread's link, as the job's profile.
    ifs: RemoteIfs,
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
        let shared = Rc::new(RefCell::new(link));
        Self {
            ifs: RemoteIfs::new(Rc::clone(&shared)),
            shared,
        }
    }

    /// The thread's link, for one call.
    fn link(&self) -> RefMut<'_, Link> {
        self.shared.borrow_mut()
    }

    /// What the `Manage` operations reach.
    fn cx(&self) -> manage::Ctx<'_> {
        manage::Ctx {
            shared: &self.shared,
        }
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
        let system_name = sysval_text(&mut self.link(), "QSYSNAME");
        let time_zone = sysval_text(&mut self.link(), "QTIMZON");
        let ccsid = sysval_text(&mut self.link(), "QCCSID")
            .parse()
            .unwrap_or(65535);
        SystemId {
            system_name,
            release: release::RELEASE.to_string(),
            time_zone,
            ccsid,
        }
    }

    fn now_ms(&mut self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
    }

    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage {
        message(&mut self.link(), id, data)
    }

    fn message_data(&mut self, id: &str, data: &[u8]) -> ServerMessage {
        message_bytes(&mut self.link(), id, data)
    }

    /// The IFS as the job's file-system commands reach it: the service's
    /// one `Ifs`, as the job's profile, its audit entries in `QAUDJRN`.
    fn with_ifs(&mut self, f: &mut dyn FnMut(&dyn IfsClient)) -> Result<(), Refusal> {
        f(&self.ifs);
        Ok(())
    }

    /// `DSPSFWRSC`'s rows give the products, the system job `SCPF`'s
    /// start the IPL, the firmware the serial (`QSRLNBR`) and model.
    fn system_info(&mut self) -> SystemInfo {
        let rows = self.link().with(Client::sfwrsc_panel);
        let col =
            |r: &[String], i: usize| r.get(i).map_or_else(String::new, |v| v.trim().to_string());
        let release = rows
            .iter()
            .find(|r| r.first().is_some_and(|id| id == release::PRODUCT_ID))
            .map_or_else(|| release::RELEASE.to_string(), |r| col(r, 6));
        let jobs = self.link().with(Client::active_jobs);
        let ipl = jobs.iter().find(|j| j.kind == 'X').map_or(0, |j| j.started);
        let machine = self.link().with(Client::machine);
        // DSPSFWRSC has a row per feature (code and language): one
        // product option each here.
        let mut products: Vec<Product> = Vec::new();
        for r in &rows {
            let p = Product {
                id: col(r, 0),
                option: col(r, 1),
                release: col(r, 6),
            };
            if !products
                .iter()
                .any(|q| q.id == p.id && q.option == p.option)
            {
                products.push(p);
            }
        }
        let system_name = sysval_text(&mut self.link(), "QSYSNAME");
        let serial = if machine.serial.is_empty() {
            sysval_text(&mut self.link(), "QSRLNBR")
        } else {
            machine.serial
        };
        SystemInfo {
            system_name,
            release,
            serial,
            // The hardware's model as the firmware names it, empty when
            // it names none; QMODEL is for programs, not clients.
            model: machine.hw_model,
            ipl_time: i64::try_from(ipl).unwrap_or(0).saturating_mul(1000),
            products,
        }
    }

    fn sign_on(&mut self, user: &str, password: &Secret) -> Result<SignOnReply, Refusal> {
        let r = self
            .link()
            .with(|l| l.qap_signon(user.to_string(), password.expose().to_string()));
        self.session(r)
    }

    fn change_password(
        &mut self,
        user: &str,
        old: &Secret,
        new: &Secret,
    ) -> Result<SignOnReply, Refusal> {
        let r = self.link().with(|l| {
            l.qap_chgpwd(
                user.to_string(),
                old.expose().to_string(),
                new.expose().to_string(),
            )
        });
        self.session(r)
    }

    fn resume(&mut self, token: &Token) -> Result<SignOnReply, Refusal> {
        let r = self.link().with(|l| l.qap_resume(token.0.to_vec()));
        self.session(r)
    }

    fn attach(&mut self, token: &Token) -> Result<SignOnReply, Refusal> {
        let r = self.link().with(|l| l.qap_attach(token.0.to_vec()));
        self.session(r)
    }

    fn sign_off(&mut self, token: &Token) {
        self.link().with(|l| l.qap_signoff(token.0.to_vec(), false));
    }

    fn password_status(&mut self) -> PasswordStatus {
        let (expires, warning_days) = self.link().with(Client::qap_pwd_status);
        PasswordStatus {
            expires,
            warning_days,
        }
    }

    fn telnet_ticket(&mut self) -> Result<TelnetTicketReply, Refusal> {
        let r = self.link().with(Client::qap_ticket);
        match r {
            Ok((ticket, expires_secs)) => Ok(TelnetTicketReply {
                ticket: Token(ticket.as_slice().try_into().unwrap_or([0; 32])),
                expires_secs,
            }),
            Err(e) => Err(self.refusal(&e)),
        }
    }

    fn list_spooled_files(&mut self, req: &ListSpooledFiles) -> Result<SpooledFiles, Refusal> {
        let r = self.link().with(|l| l.qap_splf_list(req.clone()));
        r.map_err(|e| self.refusal(&e))
    }

    fn read_spooled_file(&mut self, req: &ReadSpooledFile) -> Result<SpooledData, Refusal> {
        let r = self.link().with(|l| l.qap_splf_read(req.clone()));
        r.map_err(|e| self.refusal(&e))
    }

    fn hold_spooled_file(&mut self, id: &SpooledFileId, immediately: bool) -> Result<(), Refusal> {
        let r = self
            .link()
            .with(|l| l.qap_splf_hold(id.clone(), immediately));
        r.map_err(|e| self.refusal(&e))
    }

    fn release_spooled_file(&mut self, id: &SpooledFileId) -> Result<(), Refusal> {
        let r = self.link().with(|l| l.qap_splf_release(id.clone()));
        r.map_err(|e| self.refusal(&e))
    }

    fn delete_spooled_file(&mut self, id: &SpooledFileId) -> Result<(), Refusal> {
        let r = self.link().with(|l| l.qap_splf_delete(id.clone()));
        r.map_err(|e| self.refusal(&e))
    }

    fn move_spooled_file(
        &mut self,
        id: &SpooledFileId,
        outq: &ObjectName,
        first: bool,
    ) -> Result<(), Refusal> {
        let r = self
            .link()
            .with(|l| l.qap_splf_move(id.clone(), outq.clone(), first));
        r.map_err(|e| self.refusal(&e))
    }

    fn list_output_queues(&mut self, req: &ListOutputQueues) -> Result<OutputQueues, Refusal> {
        let r = self.link().with(|l| l.qap_outq_list(req.clone()));
        r.map_err(|e| self.refusal(&e))
    }

    fn list_schemas(&mut self, req: &ListSchemas) -> Result<Schemas, Refusal> {
        catalog::list_schemas(&mut ThreadDb(&mut self.link()), req)
    }

    fn list_tables(&mut self, req: &ListTables) -> Result<Tables, Refusal> {
        catalog::list_tables(&mut ThreadDb(&mut self.link()), req)
    }

    fn list_columns(&mut self, req: &ListColumns) -> Result<Columns, Refusal> {
        catalog::list_columns(&mut ThreadDb(&mut self.link()), req)
    }

    fn list_messages(&mut self, req: &ListMessages) -> Result<Messages, Refusal> {
        let r = self
            .link()
            .with(|l| l.qap_msg_list(req.queue.clone(), req.newest_first, req.start, req.max));
        let (total, msgs) = r.map_err(|e| self.refusal(&e))?;
        let mut texts = super::messages::Texts::default();
        let mut link = self.link();
        let messages = msgs
            .iter()
            .map(|m| super::messages::queued(&mut link, &mut texts, m))
            .collect();
        Ok(Messages { total, messages })
    }

    fn reply_message(&mut self, req: &ReplyMessage) -> Result<(), Refusal> {
        let key = u32::from_be_bytes(req.key);
        let r = self
            .link()
            .with(|l| l.qap_msg_reply(req.queue.clone(), key, req.reply.clone()));
        r.map_err(|e| self.refusal(&e))
    }

    fn remove_messages(&mut self, req: &RemoveMessages) -> Result<(), Refusal> {
        let keys = req.keys.iter().map(|k| u32::from_be_bytes(*k)).collect();
        let r = self
            .link()
            .with(|l| l.qap_msg_remove(req.queue.clone(), keys));
        r.map_err(|e| self.refusal(&e))
    }

    fn send_message(&mut self, req: &SendMessage) -> Result<Sent, Refusal> {
        let r = self
            .link()
            .with(|l| l.qap_msg_send(req.to.clone(), req.text.clone(), req.inquiry));
        r.map(|k| Sent {
            key: k.map(u32::to_be_bytes),
        })
        .map_err(|e| self.refusal(&e))
    }

    // `Manage` (version 7): `manage.rs`, and the service's parts.

    fn exit_allows(&mut self, point: &str, format: &str, req: &ExitRequest) -> Result<(), Refusal> {
        manage::exit_allows(&self.cx(), point, format, req)
    }

    fn subsystems(&mut self, library: &str) -> Result<Vec<Subsystem>, Refusal> {
        manage::may_use(&self.cx(), "WRKSBS")?;
        manage::subsystems(&self.cx(), library)
    }

    fn start_subsystem(&mut self, sbsd: &ObjectName) -> Result<(), Refusal> {
        manage::start_subsystem(&self.cx(), sbsd)
    }

    fn end_subsystem(&mut self, req: &EndSubsystem) -> Result<(), Refusal> {
        manage::end_subsystem(&self.cx(), req)
    }

    fn system_values(&mut self, names: &[String]) -> Result<SystemValues, Refusal> {
        manage::may_use(&self.cx(), "DSPSYSVAL")?;
        manage::system_values(&self.cx(), names)
    }

    fn set_system_value(&mut self, req: &SetSystemValue) -> Result<SystemValue, Refusal> {
        manage::set_system_value(&self.cx(), req)
    }

    fn ptfs(&mut self, product: &str) -> Result<Vec<Ptf>, Refusal> {
        manage::may_use(&self.cx(), "DSPPTF")?;
        let r = self.link().with(|l| l.qap_mgmt_ptfs(product.to_string()));
        r.map_err(|e| self.refusal(&e))
    }

    fn disk_units(&mut self) -> Result<DiskUnits, Refusal> {
        manage::may_use(&self.cx(), "WRKDSKSTS")?;
        let r = self.link().with(Client::qap_mgmt_disks);
        r.map_err(|e| self.refusal(&e))
    }

    fn hardware_resources(&mut self, kind: ResourceType) -> Result<HardwareResources, Refusal> {
        manage::may_use(&self.cx(), "WRKHDWRSC")?;
        manage::hardware_resources(&self.cx(), kind)
    }

    /// The service's answer, the IPL's time the system job `SCPF`'s
    /// start, as `SystemInfo` gives it.
    fn ipl_status(&mut self) -> Result<IplStatus, Refusal> {
        let r = self.link().with(Client::qap_mgmt_ipl);
        let mut st = r.map_err(|e| self.refusal(&e))?;
        let jobs = self.link().with(Client::active_jobs);
        if let Some(j) = jobs.iter().find(|j| j.kind == 'X') {
            st.ipl_time = i64::try_from(j.started).unwrap_or(0).saturating_mul(1000);
        }
        Ok(st)
    }

    fn power_down(&mut self, req: &PowerDown) -> Result<(), Refusal> {
        manage::power_down_allowed(&self.cx(), req)?;
        let r = self.link().with(|l| l.qap_mgmt_power(req.clone()));
        r.map_err(|e| self.refusal(&e))
    }

    // `Develop` 3 to 14 (version 8): `develop.rs`, and the service's
    // member requests.

    fn list_libraries(&mut self, req: &ListLibraries) -> Result<Vec<Library>, Refusal> {
        Ok(develop::list_libraries(&self.cx(), req))
    }

    fn list_objects(&mut self, req: &ListObjects) -> Result<Vec<ObjectEntry>, Refusal> {
        develop::list_objects(&self.cx(), req)
    }

    fn describe_object(&mut self, req: &DescribeObject) -> Result<ObjectDescription, Refusal> {
        develop::describe_object(&self.cx(), req)
    }

    fn list_members(&mut self, req: &ListMembers) -> Result<Vec<MemberEntry>, Refusal> {
        develop::list_members(&self.cx(), req)
    }

    fn read_member(&mut self, member: &MemberName, lock: bool) -> Result<MemberData, Refusal> {
        let r = self.link().with(|l| l.qap_dev_read(member.clone(), lock));
        r.map_err(|e| self.refusal(&e))
    }

    fn write_member(
        &mut self,
        member: &MemberName,
        records: Vec<SourceRecord>,
        renumber: bool,
        srctype: Option<&str>,
        text: Option<&str>,
    ) -> Result<u32, Refusal> {
        let (st, tx) = (srctype.map(String::from), text.map(String::from));
        let r = self
            .link()
            .with(|l| l.qap_dev_write(member.clone(), records, renumber, st, tx));
        r.map_err(|e| self.refusal(&e))
    }

    fn unlock_member(&mut self, member: &MemberName) -> Result<(), Refusal> {
        let r = self.link().with(|l| l.qap_dev_unlock(member.clone()));
        r.map_err(|e| self.refusal(&e))
    }

    fn create_member(&mut self, req: &CreateMember) -> Result<(), Refusal> {
        develop::create_member(&self.cx(), req)
    }

    fn copy_member(&mut self, req: &CopyMember) -> Result<(), Refusal> {
        develop::copy_member(&self.cx(), req)
    }

    fn rename_member(&mut self, req: &RenameMember) -> Result<(), Refusal> {
        develop::rename_member(&self.cx(), req)
    }

    fn delete_member(&mut self, member: &MemberName) -> Result<(), Refusal> {
        develop::delete_member(&self.cx(), member)
    }

    fn change_member(&mut self, req: &ChangeMember) -> Result<(), Refusal> {
        develop::change_member(&self.cx(), req)
    }

    // `Jobs` (version 8): `jobs.rs`.

    fn list_jobs(&mut self, req: &ListJobs) -> Result<Vec<JobEntry>, Refusal> {
        jobs::list_jobs(&self.cx(), req)
    }

    fn job_log(&mut self, job: &JobId) -> Result<Vec<JobLogEntry>, Refusal> {
        jobs::job_log(&self.cx(), job)
    }

    fn hold_job(&mut self, job: &JobId) -> Result<(), Refusal> {
        jobs::hold_job(&self.cx(), job, true)
    }

    fn release_job(&mut self, job: &JobId) -> Result<(), Refusal> {
        jobs::hold_job(&self.cx(), job, false)
    }

    fn end_job(&mut self, req: &EndJob) -> Result<(), Refusal> {
        jobs::end_job(&self.cx(), req)
    }

    // `Sql` 12 and 13 (version 8): the catalog, as 9 to 11.

    fn list_routines(&mut self, req: &ListRoutines) -> Result<Vec<Routine>, Refusal> {
        catalog::list_routines(&mut ThreadDb(&mut self.link()), req)
    }

    fn list_indexes(&mut self, req: &ListIndexes) -> Result<Indexes, Refusal> {
        catalog::list_indexes(&mut ThreadDb(&mut self.link()), req)
    }
}
