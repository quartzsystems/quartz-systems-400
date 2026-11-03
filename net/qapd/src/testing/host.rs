// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One connection job of the test [`System`]: [`MemHost`] its own
//! thread (commands), [`MemThread`] a secondary thread (the rest).

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use qap_proto::ops::command::{CommandDefinition, CommandReply};
use qap_proto::ops::signon::{PasswordStatus, SignOnReply, TelnetTicketReply};
use qap_proto::ops::spool::{
    ListOutputQueues, ListSpooledFiles, OutputQueues, ReadSpooledFile, SpooledData, SpooledFileId, SpooledFiles,
};
use qap_proto::ops::sql::{
    Columns, Execute, Executed, Fetch, Fetched, ListColumns, ListSchemas, ListTables, Prepared, Schemas, SetOptions,
    Tables,
};
use qap_proto::ops::system::{Product, SystemInfo};
use qap_proto::types::{JobId, ObjectName};
use qap_proto::{Secret, ServerMessage, Status, Token};

use ifs::client::IfsClient;
use ifscmd::cmds::{Subtree, auth, yes};
use ifscmd::{Msg, Reply};

use super::commands::{self, definition, find, parse};
use super::{Crash, System, lock};
use crate::host::{Ending, Host, Ran, Refusal, SystemId, ThreadHost, Threads};
use crate::sessions::{EndedBy, Gone};
use crate::sql::SqlResult;
use crate::tickets::TICKET_SECS;

/// Message `id` as job `job` of `sys` looks it up: its text says whose
/// language it was looked up in.
fn message(sys: &System, job: u32, id: &str, data: &[&str]) -> ServerMessage {
    let lang = me(sys, job).map_or_else(|| "system".to_string(), |u| format!("lang of {u}"));
    ServerMessage {
        id: id.into(),
        severity: commands::severity(id),
        data: data.join(" ").into_bytes(),
        text: format!("{id} {} ({lang})", data.join(" ")),
        help: format!("Help for {id}."),
    }
}

/// A value with its apostrophes taken off (`'/a b'` is `/a b`).
fn unquote(v: &str) -> String {
    let v = v.trim();
    match v.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        Some(inner) => inner.replace("''", "'"),
        None => v.to_string(),
    }
}

/// Who job `job` runs as, once signed on.
fn me(sys: &System, job: u32) -> Option<String> {
    lock(&sys.running_as).get(&job).cloned()
}

/// A connection job's own thread.
pub struct MemHost {
    /// The system.
    pub sys: Arc<System>,
    /// The job's number.
    pub job: u32,
    /// The connection's statements, cursors and transaction.
    pub sql: super::sql::Conn,
}

impl MemHost {
    /// Job `job` of `sys`.
    #[must_use]
    pub fn new(sys: Arc<System>, job: u32) -> Self {
        sys.opened(job);
        Self {
            sys,
            job,
            sql: super::sql::Conn::default(),
        }
    }

    /// What looks messages up for this job, apart from `self`.
    fn msgs(&self) -> impl Fn(&str, &[&str]) -> ServerMessage + use<> {
        let (sys, job) = (Arc::clone(&self.sys), self.job);
        move |id: &str, data: &[&str]| message(&sys, job, id, data)
    }

    fn refuse(&mut self, status: Status, id: &str, data: &[&str]) -> Refusal {
        Refusal {
            status,
            message: self.message(id, data),
        }
    }

    fn job_id(&self) -> JobId {
        JobId {
            name: "QAPCNN".into(),
            user: "QUSER".into(),
            number: format!("{:06}", self.job),
        }
    }

    /// An `ifscmd` command over the file system as the signed-on
    /// profile: its messages in order, the escape last; `Error` when
    /// there is one, as the job's commands answer.
    fn ifs_command(&mut self, f: impl FnOnce(&dyn IfsClient) -> Reply) -> Ran {
        let user = me(&self.sys, self.job).unwrap_or_else(|| "QUSER".into());
        let r = self.sys.files.as_user(&user, u64::from(self.job), f);
        let msg = |m: Msg| {
            let mut s = message(&self.sys, self.job, &m.id, &[]);
            s.data = m.data;
            s
        };
        let failed = r.escape.is_some();
        let messages: Vec<ServerMessage> = r.messages.into_iter().map(|(_, m)| m).chain(r.escape).map(msg).collect();
        Ran {
            status: if failed { Status::Error } else { Status::Ok },
            escape: messages.last().filter(|_| failed).cloned(),
            reply: CommandReply {
                job: self.job_id(),
                messages,
            },
        }
    }

    fn ran(&mut self, status: Status, ids: &[(&str, &[&str])]) -> Ran {
        let messages: Vec<ServerMessage> = ids.iter().map(|(id, data)| self.message(id, data)).collect();
        let escape = (status != Status::Ok).then(|| messages.last().cloned()).flatten();
        Ran {
            status,
            escape,
            reply: CommandReply {
                job: self.job_id(),
                messages,
            },
        }
    }
}

impl Host for MemHost {
    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage {
        message(&self.sys, self.job, id, data)
    }

    fn ending(&mut self) -> Option<Ending> {
        if self.sys.server_ended_for(self.job) {
            return Some(Ending::Server);
        }
        let e = lock(&self.sys.ending).take();
        e.or_else(|| lock(&self.sys.sessions).take_ended(self.job).map(Ending::Session))
    }

    fn sign_off(&mut self, token: &Token) {
        lock(&self.sys.sessions).end(token, self.job, EndedBy::SignOff);
        lock(&self.sys.running_as).remove(&self.job);
    }

    fn end_session(&mut self, token: &Token) {
        lock(&self.sys.sessions).end(token, self.job, EndedBy::Operator);
        lock(&self.sys.running_as).remove(&self.job);
    }

    fn detach(&mut self, token: &Token) {
        let now = self.sys.now();
        lock(&self.sys.sessions).detach(token, self.job, now);
        lock(&self.sys.running_as).remove(&self.job);
    }

    fn run_command(&mut self, command: &str, cancelled: &dyn Fn() -> bool) -> Ran {
        self.sys.crash_at(Crash::Command);
        let (name, parms) = parse(command);
        let _running = self.sys.command_runs(self.job, &name);
        let Some(c) = find(&name) else {
            return self.ran(Status::NotFound, &[("CPD0030", &[&name, "*LIBL"]), ("CPF0006", &[])]);
        };
        if !c.authorized {
            return self.ran(Status::NotAuthorized, &[("CPF9802", &["*CMD", c.name])]);
        }
        if c.interactive_only {
            return self.ran(Status::Error, &[("CPD0031", &[c.name]), ("CPF0006", &[])]);
        }
        let get = |k: &str| parms.iter().find(|(p, _)| p == k).map(|(_, v)| v.clone());
        if let Some(missing) = c.required.iter().find(|k| get(k).is_none()) {
            return self.ran(Status::Error, &[("CPD0071", &[missing]), ("CPF0006", &[])]);
        }
        match c.name {
            "CRTLIB" => {
                let lib = get("LIB").unwrap_or_default().to_ascii_uppercase();
                let mut libs = lock(&self.sys.libraries);
                if libs.contains(&lib) {
                    drop(libs);
                    return self.ran(Status::Error, &[("CPF2111", &[&lib])]);
                }
                libs.push(lib.clone());
                drop(libs);
                self.ran(Status::Ok, &[("CPC2102", &[&lib])])
            }
            "DSPOBJD" if get("OBJ").is_some_and(|o| o.eq_ignore_ascii_case("QSYS/SECRET")) => {
                self.ran(Status::NotAuthorized, &[("CPF9802", &["*FILE", "SECRET"])])
            }
            "DLYJOB" => {
                let secs = get("DLY").and_then(|d| d.trim().parse::<u64>().ok()).unwrap_or(1);
                let until = Instant::now() + Duration::from_secs(secs);
                while Instant::now() < until {
                    if cancelled() {
                        return self.ran(Status::Cancelled, &[("CPF1907", &[])]);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                self.ran(Status::Ok, &[])
            }
            "CPYFRMSTMF" => {
                let from = get("FROMSTMF").unwrap_or_default();
                let to = get("TOMBR").unwrap_or_default();
                if from.to_ascii_lowercase().contains("long") {
                    self.ran(Status::Error, &[("CPFA096", &[&from, &to, "3", "80"])])
                } else {
                    self.ran(Status::Ok, &[("CPCA081", &[&from, &to, "10"])])
                }
            }
            "CHGCURLIB" => {
                let lib = get("CURLIB").unwrap_or_default().to_ascii_uppercase();
                if lib != "*CRTDFT" && !lock(&self.sys.libraries).contains(&lib) {
                    return self.ran(Status::Error, &[("CPF2110", &[&lib])]);
                }
                lock(&self.sys.curlib).insert(self.job, lib);
                self.ran(Status::Ok, &[])
            }
            "CHGOWN" => {
                let (obj, own) = (unquote(&get("OBJ").unwrap_or_default()), get("NEWOWN").unwrap_or_default());
                let rvk = get("RVKOLDAUT").is_none_or(|v| yes(&v));
                let sym = get("SYMLNK").is_some_and(|v| yes(&v));
                let sub = Subtree::of(&get("SUBTREE").unwrap_or_default());
                self.ifs_command(|c| auth::chgown(c, &obj, &own, rvk, sub, sym))
            }
            "CHGPGP" => {
                let a = auth::Chgpgp {
                    obj: unquote(&get("OBJ").unwrap_or_default()),
                    newpgp: get("NEWPGP").unwrap_or_default(),
                    dtaaut: get("DTAAUT").unwrap_or_else(|| "*OLDPGP".into()),
                    objaut: get("OBJAUT").map_or_else(Vec::new, |v| v.split_whitespace().map(String::from).collect()),
                    rvkoldaut: get("RVKOLDAUT").is_none_or(|v| yes(&v)),
                    subtree: Subtree::of(&get("SUBTREE").unwrap_or_default()),
                    symlnk: get("SYMLNK").is_some_and(|v| yes(&v)),
                };
                self.ifs_command(|c| auth::chgpgp(c, &a))
            }
            "CHGAUT" => {
                let dta = get("DTAAUT").unwrap_or_else(|| "*SAME".into()).to_ascii_uppercase();
                if matches!(dta.as_str(), "*SAME" | "*RWX" | "*RW" | "*RX" | "*R" | "*WX" | "*W" | "*X" | "*EXCLUDE" | "*NONE") {
                    self.ran(Status::Ok, &[])
                } else {
                    self.ran(Status::Error, &[("CPDA091", &[])])
                }
            }
            _ => self.ran(Status::Ok, &[]),
        }
    }

    fn check_command(&mut self, command: &str) -> (Status, Vec<ServerMessage>) {
        let (name, parms) = parse(command);
        let Some(c) = commands::find(&name) else {
            return (Status::NotFound, vec![self.message("CPD0030", &[&name, "*LIBL"])]);
        };
        match c.required.iter().find(|k| !parms.iter().any(|(p, _)| p == *k)) {
            Some(missing) => (
                Status::Error,
                vec![self.message("CPD0071", &[missing]), self.message("CPF0006", &[])],
            ),
            None => (Status::Ok, Vec::new()),
        }
    }

    fn describe_command(&mut self, command: &ObjectName) -> Result<CommandDefinition, Refusal> {
        let lib = command.library.trim().to_ascii_uppercase();
        let found = find(&command.name).filter(|_| matches!(lib.as_str(), "" | "*LIBL" | "QSYS"));
        match found {
            None => Err(self.refuse(Status::NotFound, "CPD0030", &[&command.name, &lib])),
            Some(c) if !c.authorized => Err(self.refuse(Status::NotAuthorized, "CPF9802", &["*CMD", c.name])),
            Some(c) => Ok(definition(c)),
        }
    }

    fn sql_set_options(&mut self, options: &SetOptions) -> Result<(), Refusal> {
        let msg = self.msgs();
        self.sql.set_options(options, &msg)
    }

    fn sql_prepare(&mut self, sql: &str) -> SqlResult<Prepared> {
        let (msg, me) = (self.msgs(), me(&self.sys, self.job).unwrap_or_default());
        self.sql.prepare(&self.sys.sql, &me, sql, &msg)
    }

    fn sql_execute(&mut self, req: &Execute, cancelled: &dyn Fn() -> bool) -> SqlResult<Executed> {
        let msg = self.msgs();
        self.sql.execute(&self.sys.sql, req, cancelled, &msg)
    }

    fn sql_fetch(&mut self, req: &Fetch, cancelled: &dyn Fn() -> bool) -> SqlResult<Fetched> {
        let msg = self.msgs();
        self.sql.fetch(req, cancelled, &msg)
    }

    fn sql_close_cursor(&mut self, cursor: u32) -> Result<(), Refusal> {
        let msg = self.msgs();
        self.sql.close_cursor(cursor, &msg)
    }

    fn sql_release(&mut self, statement: u32) -> Result<(), Refusal> {
        let msg = self.msgs();
        self.sql.release(statement, &msg)
    }

    fn sql_commit(&mut self) -> SqlResult<()> {
        self.sql.commit();
        Ok(())
    }

    fn sql_rollback(&mut self) -> SqlResult<()> {
        self.sql.rollback(&self.sys.sql);
        Ok(())
    }

    fn sql_end(&mut self) {
        self.sql.end(&self.sys.sql);
    }

    fn threads(&mut self) -> Threads {
        let (sys, job) = (Arc::clone(&self.sys), self.job);
        Arc::new(move || {
            Box::new(MemThread {
                sys: Arc::clone(&sys),
                job,
            })
        })
    }
}

/// A secondary thread of a connection job.
pub struct MemThread {
    /// The system.
    pub sys: Arc<System>,
    /// The job's number.
    pub job: u32,
}

impl MemThread {
    fn me(&self) -> Option<String> {
        me(&self.sys, self.job)
    }

    fn refuse(&mut self, status: Status, id: &str, data: &[&str]) -> Refusal {
        Refusal {
            status,
            message: self.message(id, data),
        }
    }

    /// A spool refusal with its message.
    fn no(&mut self, (status, id, data): &super::spool::No) -> Refusal {
        let data: Vec<&str> = data.iter().map(String::as_str).collect();
        self.refuse(*status, id, &data)
    }

    fn reply(&self, token: Token, user: &str) -> SignOnReply {
        SignOnReply {
            token,
            idle_timeout_secs: self.sys.idle_secs,
            profile: user.into(),
            text: format!("{user} profile"),
            ccsid: 37,
            date_format: "*MDY".into(),
            date_separator: "/".into(),
            time_separator: ":".into(),
            home_directory: format!("/home/{user}"),
            special_authorities: Vec::new(),
        }
    }

    /// The display's checks; the profile and whether it expired.
    fn check(&mut self, user: &str, password: &str) -> Result<(String, bool), Refusal> {
        let user = user.trim().to_ascii_uppercase();
        let audit = |s: &System, e: &str| lock(&s.audit).push(format!("PW {e} {user}"));
        let mut users = lock(&self.sys.users);
        let Some(u) = users.get_mut(&user) else {
            drop(users);
            audit(&self.sys, "U");
            return Err(self.refuse(Status::SignOnFailed, "CPF1107", &[]));
        };
        if u.disabled {
            drop(users);
            audit(&self.sys, "Q");
            return Err(self.refuse(Status::SignOnFailed, "CPF1394", &[&user]));
        }
        if u.none {
            drop(users);
            return Err(self.refuse(Status::SignOnFailed, "CPF1118", &[&user]));
        }
        if u.password != password {
            u.invalid += 1;
            let n = u.invalid;
            if n >= 3 {
                u.disabled = true;
            }
            drop(users);
            audit(&self.sys, "P");
            return Err(match n {
                3.. => self.refuse(Status::SignOnFailed, "CPF1393", &[&user]),
                2 => self.refuse(Status::SignOnFailed, "CPF1392", &[]),
                _ => self.refuse(Status::SignOnFailed, "CPF1107", &[]),
            });
        }
        u.invalid = 0;
        Ok((user, u.expired))
    }

    /// A resume or attach: the job runs as the session's profile, its
    /// job state its own.
    fn joined(&mut self, token: &Token, r: Result<crate::sessions::Session, Gone>) -> Result<SignOnReply, Refusal> {
        match r {
            Ok(s) => {
                lock(&self.sys.running_as).insert(self.job, s.user.clone());
                Ok(self.reply(token.clone(), &s.user))
            }
            Err(Gone::Full) => {
                let user = lock(&self.sys.sessions).get(token).map(|s| s.user.clone()).unwrap_or_default();
                Err(self.refuse(Status::Busy, "TCP1A59", &[&user]))
            }
            Err(_) => Err(self.refuse(Status::SessionExpired, "TCP1A53", &[])),
        }
    }

    fn open(&mut self, user: &str) -> SignOnReply {
        let token = self.sys.next_token(0x5A);
        lock(&self.sys.sessions).create(&token, user, self.sys.idle_secs, self.job);
        lock(&self.sys.running_as).insert(self.job, user.into());
        self.reply(token, user)
    }
}

impl ThreadHost for MemThread {
    fn system_id(&mut self) -> SystemId {
        SystemId {
            system_name: "QSHQ400".into(),
            release: "V1R0M0".into(),
            time_zone: "QN0000UTC".into(),
            ccsid: 37,
        }
    }

    fn now_ms(&mut self) -> i64 {
        self.sys.crash_at(Crash::Ping);
        i64::try_from(self.sys.now()).unwrap_or(0) * 1000
    }

    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage {
        message(&self.sys, self.job, id, data)
    }

    /// The command's message: its identifier and data as the command
    /// lays it out, the text saying whose language.
    fn message_data(&mut self, id: &str, data: &[u8]) -> ServerMessage {
        let mut m = message(&self.sys, self.job, id, &[]);
        m.data = data.to_vec();
        m
    }

    /// The test system's IFS, as the signed-on profile, the job one
    /// owner of descriptors and locks for all its threads.
    fn with_ifs(&mut self, f: &mut dyn FnMut(&dyn ifs::client::IfsClient)) -> Result<(), Refusal> {
        let user = self.me().unwrap_or_else(|| "QUSER".into());
        self.sys.files.as_user(&user, u64::from(self.job), |c| f(c));
        Ok(())
    }

    fn system_info(&mut self) -> SystemInfo {
        self.sys.gate.pass();
        self.sys.crash_at(Crash::SystemInfo);
        let n = *lock(&self.sys.products);
        SystemInfo {
            system_name: "QSHQ400".into(),
            release: "V1R0M0".into(),
            serial: "1234567".into(),
            model: "Test Machine 1".into(),
            ipl_time: 1_792_000_000_000,
            products: (0..n)
                .map(|i| Product {
                    id: format!("QS{i:05}"),
                    option: "0000".into(),
                    release: "V1R0M0".into(),
                })
                .collect(),
        }
    }

    fn sign_on(&mut self, user: &str, password: &Secret) -> Result<SignOnReply, Refusal> {
        let (user, expired) = self.check(user, password.expose())?;
        if expired {
            return Err(self.refuse(Status::PasswordExpired, "CPD9BAE", &[]));
        }
        Ok(self.open(&user))
    }

    fn change_password(&mut self, user: &str, old: &Secret, new: &Secret) -> Result<SignOnReply, Refusal> {
        let me = self.me();
        let user = user.trim().to_ascii_uppercase();
        match &me {
            Some(m) if *m != user => return Err(self.refuse(Status::NotAuthorized, "CPF22E9", &[&user])),
            Some(_) => {
                if self.sys.get(&user).password != old.expose() {
                    return Err(self.refuse(Status::Error, "CPF22E2", &[&user]));
                }
            }
            None => {
                self.check(&user, old.expose())?;
            }
        }
        if new.expose().len() < 6 || new.expose() == old.expose() {
            return Err(self.refuse(Status::Error, "CPF2362", &[]));
        }
        self.sys.edit(&user, |u| {
            u.password = new.expose().into();
            u.expired = false;
        });
        Ok(match me {
            Some(_) => self.reply(Token([0; 32]), &user),
            None => self.open(&user),
        })
    }

    fn resume(&mut self, token: &Token) -> Result<SignOnReply, Refusal> {
        let now = self.sys.now();
        let r = lock(&self.sys.sessions).resume(token, self.job, now, &|_| true);
        self.joined(token, r)
    }

    fn attach(&mut self, token: &Token) -> Result<SignOnReply, Refusal> {
        let now = self.sys.now();
        let r = lock(&self.sys.sessions).attach(token, self.job, now, &|_| true);
        self.joined(token, r)
    }

    fn sign_off(&mut self, token: &Token) {
        lock(&self.sys.sessions).end(token, self.job, EndedBy::SignOff);
        lock(&self.sys.running_as).remove(&self.job);
    }

    fn password_status(&mut self) -> PasswordStatus {
        let me = self.me().unwrap_or_default();
        let u = self.sys.get(&me);
        let ms = |s: u64| i64::try_from(s).unwrap_or(i64::MAX) * 1000;
        PasswordStatus {
            expires: if u.expired { Some(ms(self.sys.now())) } else { u.expires.map(ms) },
            warning_days: self.sys.warning_days,
        }
    }

    fn telnet_ticket(&mut self) -> Result<TelnetTicketReply, Refusal> {
        let me = self.me().unwrap_or_default();
        let u = self.sys.get(&me);
        if !self.sys.auto_signon.load(Ordering::SeqCst) || u.disabled || u.none || u.expired {
            return Err(self.refuse(Status::NotAuthorized, "TCP1A55", &[&me]));
        }
        let t = self.sys.next_token(0x7E);
        lock(&self.sys.tickets).issue(&t, &me, self.sys.now());
        lock(&self.sys.audit).push(format!("PS P {me}"));
        Ok(TelnetTicketReply {
            ticket: t,
            expires_secs: TICKET_SECS,
        })
    }

    fn list_spooled_files(&mut self, req: &ListSpooledFiles) -> Result<SpooledFiles, Refusal> {
        let me = self.me().unwrap_or_default();
        let r = lock(&self.sys.spool).list(&me, req);
        r.map_err(|n| self.no(&n))
    }

    fn read_spooled_file(&mut self, req: &ReadSpooledFile) -> Result<SpooledData, Refusal> {
        self.sys.gate.pass();
        let me = self.me().unwrap_or_default();
        let r = super::spool::Spool::read(&self.sys.spool, &me, req);
        r.map_err(|n| self.no(&n))
    }

    fn hold_spooled_file(&mut self, id: &SpooledFileId, _immediately: bool) -> Result<(), Refusal> {
        let me = self.me().unwrap_or_default();
        let r = lock(&self.sys.spool).hold(&me, id);
        r.map_err(|n| self.no(&n))
    }

    fn release_spooled_file(&mut self, id: &SpooledFileId) -> Result<(), Refusal> {
        let me = self.me().unwrap_or_default();
        let r = lock(&self.sys.spool).release(&me, id);
        r.map_err(|n| self.no(&n))
    }

    fn delete_spooled_file(&mut self, id: &SpooledFileId) -> Result<(), Refusal> {
        let me = self.me().unwrap_or_default();
        let r = lock(&self.sys.spool).delete(&me, id);
        r.map_err(|n| self.no(&n))
    }

    fn move_spooled_file(&mut self, id: &SpooledFileId, outq: &ObjectName, first: bool) -> Result<(), Refusal> {
        let me = self.me().unwrap_or_default();
        let r = lock(&self.sys.spool).move_to(&me, id, outq, first);
        r.map_err(|n| self.no(&n))
    }

    fn list_output_queues(&mut self, req: &ListOutputQueues) -> Result<OutputQueues, Refusal> {
        let me = self.me().unwrap_or_default();
        let r = lock(&self.sys.spool).queues(&me, req);
        r.map_err(|n| self.no(&n))
    }

    fn list_schemas(&mut self, req: &ListSchemas) -> Result<Schemas, Refusal> {
        Ok(super::sql::list_schemas(&lock(&self.sys.sql), req))
    }

    fn list_tables(&mut self, req: &ListTables) -> Result<Tables, Refusal> {
        let (sys, job) = (Arc::clone(&self.sys), self.job);
        let msg = move |id: &str, data: &[&str]| message(&sys, job, id, data);
        super::sql::list_tables(&lock(&self.sys.sql), req, &msg)
    }

    fn list_columns(&mut self, req: &ListColumns) -> Result<Columns, Refusal> {
        let me = self.me().unwrap_or_default();
        let (sys, job) = (Arc::clone(&self.sys), self.job);
        let msg = move |id: &str, data: &[&str]| message(&sys, job, id, data);
        super::sql::list_columns(&lock(&self.sys.sql), &me, req, &msg)
    }
}
