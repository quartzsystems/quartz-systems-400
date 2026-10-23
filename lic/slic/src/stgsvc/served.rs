// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One connection and the job behind it: the job's sources over the
//! lent store, its open files and console state, and the answers to its
//! requests.

use std::cell::RefCell;
use std::rc::Rc;

use ::ile::PgmKind;
use ipc::chan::Chan;
use mi::io::store::StoreIo;
use mi::io::{
    DataAreaValue, DataSource, FileInfo as IoFileInfo, IoFail, IoResult, IoSource, OpenSpec,
    OwnedRequest,
};
use mi::{ObjectSource, UserSpaceSource};
use msgf::{MsgDesc, MsgLookup};
use stg::proto::IdxAttrs;
use stg::{ExecReply, JobInfo, Request, Service};
use wm::job::JobAttrs;
use wm::obj::msgq::{MsgKind, QMsg};

use super::msgs;
use store::obj::{JobHandle, ObjectId, ops, types};
use sysapi::{
    ApiFail, CommitInfo, DataArea, DepInfo, DtaqEntry, DtaqInfo, FileInfo, FormatInfo, IdxAttr,
    LocalNow, MemberInfo, ObjInfo, Provider, SysVal, UserInfo,
};

use crate::ipl::system::System;
use crate::svc;
use crate::svc::ile::{
    self, MainStore, StoreData, StoreMsgs, StoreObjects, StoreProvider, StoreUserSpaces, with_main,
};

/// A connection: `slic`'s, or a job's once it said hello.
pub struct Conn {
    /// The socket.
    pub chan: Chan,
    /// The job.
    pub ctx: Option<JobCtx>,
}

impl Conn {
    /// A new connection.
    pub fn new(chan: Chan, ctx: Option<JobCtx>) -> Self {
        Self { chan, ctx }
    }
}

/// What the service keeps for one job.
pub struct JobCtx {
    /// Who.
    pub job: JobInfo,
    /// Its `QTEMP` in ASP 0.
    pub qtemp: Option<ObjectId>,
    /// Where its completion message goes, `lib/name`; empty for none.
    pub completion: String,
    /// The end code it reported; none means it vanished.
    pub end_code: Option<u16>,
    /// The service's copy of its job log, kept current by the job.
    pub joblog: wm::joblog::JobLog,
    /// Entries the copy wrapped out, for `*PRTWRAP`.
    pub joblog_wrapped: Vec<wm::joblog::Entry>,
    /// Spooled files it has open.
    pub splfs: Vec<u32>,
    objects: StoreObjects,
    spaces: StoreUserSpaces,
    msgs: StoreMsgs,
    data: StoreData,
    io: StoreIo,
    provider: StoreProvider,
    /// Console state: modes, transfer buffers, the SQL context.
    pub svc: svc::db::State,
    /// Its security session: who it is and the decisions made.
    pub session: Rc<RefCell<sec::auth::cache::Session>>,
    /// Profile handles `QSYGETPH` gave it, with their users.
    pub handles: std::collections::HashMap<Vec<u8>, String>,
}

impl JobCtx {
    /// The sources for `job` over the system's lent store.
    pub fn new(system: &System, job: JobInfo) -> Self {
        let shared = &system.lent;
        let resolver = Rc::clone(shared);
        let resolve = move |fr: &dds::FileRef| -> db::Result<ObjectId> {
            with_main(&resolver, |txn, qsys| {
                let names = ile::name_context(txn, qsys);
                ops::resolve(txn, qsys, &names, fr.library, fr.file, types::FILE)
                    .map(|f| f.id)
                    .map_err(|e| e.to_string())
            })
            .map_err(|what| db::DbError::Definition { what })
        };
        Self {
            objects: StoreObjects::new(Rc::clone(shared)),
            spaces: StoreUserSpaces::new(Rc::clone(shared)),
            msgs: StoreMsgs::new(Rc::clone(shared)),
            data: StoreData::new(Rc::clone(shared)),
            io: StoreIo::with_locks(
                Box::new(MainStore::new(Rc::clone(shared))),
                JobHandle(job.handle),
                Box::new(resolve),
                system.locks.clone(),
            ),
            provider: StoreProvider::new(Rc::clone(shared)),
            svc: svc::db::State::default(),
            qtemp: None,
            completion: String::new(),
            end_code: None,
            joblog: wm::joblog::JobLog::default(),
            joblog_wrapped: Vec::new(),
            splfs: Vec::new(),
            session: Rc::new(RefCell::new(sec::auth::cache::Session::default())),
            handles: std::collections::HashMap::new(),
            job,
        }
    }
}

/// Answer one request on connection `i`, then audit what it did.
pub fn handle(system: &mut System, conns: &mut [Conn], i: usize, req: Request) -> Vec<u8> {
    let reply = answer(system, conns, i, req);
    let who = conns.get(i).and_then(|c| c.ctx.as_ref()).map(charged);
    super::security::flush(system, who);
    reply
}

/// Who a job's audit entries are charged to.
fn charged(ctx: &JobCtx) -> super::security::Charged {
    super::security::Charged {
        job: (ctx.job.name.clone(), ctx.job.user.clone(), ctx.job.number),
        audlvl: ctx.session.borrow().principal.audlvl.clone(),
    }
}

fn answer(system: &mut System, conns: &mut [Conn], i: usize, req: Request) -> Vec<u8> {
    let conn = &mut conns[i];
    if let Request::Hello { job } = req {
        let r = if conn.ctx.is_some() {
            Err("this connection already said hello".into())
        } else {
            begin_job(system, job).map(|ctx| conn.ctx = Some(ctx))
        };
        return ipc::encode(&r);
    }
    if let Request::Close {} = req {
        return ipc::encode(&close(system, conns));
    }
    let Some(ctx) = conn.ctx.as_mut() else {
        // Nothing but hello before hello.
        return Vec::new();
    };
    let quiet = matches!(
        req,
        Request::Ping {}
            | Request::SetStatus { .. }
            | Request::SbsState {}
            | Request::Breaks {}
            | Request::JoblogPut { .. }
            | Request::WtrNext { .. }
            | Request::ScdTick {}
            | Request::JobState { .. }
    );
    if !quiet {
        *system.stats.aux_io.entry(ctx.job.number).or_default() += 1;
    }
    let user = super::security::as_user(&req);
    let session = Rc::clone(&ctx.session);
    super::security::freshen(system, &session);
    let mut served = Served { system, ctx };
    if user {
        sec::guard::as_job(&session, || stg::dispatch(&mut served, req))
    } else {
        stg::dispatch(&mut served, req)
    }
}

/// A job said hello: into the active table, with a `QTEMP` of its own.
pub fn begin_job(system: &mut System, mut job: JobInfo) -> Result<JobCtx, String> {
    if system.jobs.in_use(job.number) {
        return Err(format!("job number {:06} is in use", job.number));
    }
    job.handle = u64::from(job.number);
    job.status = super::jobs::initial_status();
    if job.started == 0 {
        job.started = crate::ipl::system::now_unix();
    }
    if job.jobd.is_empty() {
        job.jobd = "QGPL/QDFTJOBD".into();
    }
    if job.class.is_empty() {
        job.class = "QGPL/QBATCH".into();
    }
    let qtemp = super::jobs::make_qtemp(system, job.number)?;
    // A submitted job starts as SBMJOB resolved it, with its class's
    // run attributes; any other from its job description.
    let queued = system.pending.remove(&job.number);
    system.stats.started(job.number);
    let attrs = match &queued {
        Some(q) => {
            let mut a = q.attrs.clone();
            super::jobs::apply_class(system, &mut a, &job.class);
            a
        }
        None => super::jobs::resolve_attrs(system, &job.jobd, &job.class, &job.user.clone()),
    };
    system.attrs.insert(job.number, attrs);
    system.job_seen.insert(job.name.clone(), 1);
    jrn::entry::name_job(
        store::obj::JobHandle(job.handle),
        jrn::entry::JobInfo {
            name: job.name.clone(),
            user: job.user.clone(),
            number: job.number,
        },
    );
    system.jobs.add(job.clone());
    super::security::js("S", (&job.name, &job.user, job.number), job.kind, &job.jobd);
    let who = (job.name.clone(), job.user.clone(), job.number);
    let data = wm::joblog::started_data(
        (&job.name, &job.user, job.number),
        &job.subsystem,
        job.started,
        job.started,
    );
    super::hist::system_message(system, "CPF1124", &data, "04", who);
    let mut ctx = JobCtx::new(system, job);
    ctx.session = super::security::session(system, &ctx.job.user);
    ctx.qtemp = Some(qtemp);
    if let Some(a) = system.attrs.get(&ctx.job.number) {
        ctx.joblog = wm::joblog::JobLog::new(a.jobmsgqmx, wm::joblog::Full::parse(&a.jobmsgqfl));
    }
    if let Some(q) = queued {
        ctx.completion = q.msgq;
    }
    Ok(ctx)
}

/// A job's connection closed: whatever it held goes.
pub fn vanished(system: &mut System, conn: Conn) {
    let Some(mut ctx) = conn.ctx else {
        return;
    };
    end_job(system, &mut ctx, true);
    let who = charged(&ctx);
    super::security::flush(system, Some(who));
}

/// End a job: its database use, its spooled files and job log, its
/// `QTEMP`, its table entry. `vanished` says what a job that reported
/// no end code did: vanish (abnormal) or end with the system.
fn end_job(system: &mut System, ctx: &mut JobCtx, vanished: bool) {
    end_db(system, ctx);
    let abnormal = ctx.end_code.map_or(vanished, |c| c >= 20);
    let code = ctx.end_code.unwrap_or(if vanished { 70 } else { 0 });
    let j = &ctx.job;
    let data = wm::joblog::ended_data(
        (&j.name, &j.user, j.number),
        crate::ipl::system::now_unix(),
        0,
        code,
    );
    let who = (j.name.clone(), j.user.clone(), j.number);
    super::hist::system_message(system, "CPF1164", &data, "04", who.clone());
    for id in std::mem::take(&mut ctx.splfs) {
        let _ = super::spool::close(system, id, false);
    }
    if ctx.end_code.is_none() {
        super::joblog::ended_here(system, ctx, &data);
    }
    super::joblog::spool(system, ctx, abnormal);
    super::spool::job_ended(system, &who);
    super::wtr::job_ended(system, ctx.job.number);
    completion_message(system, ctx);
    if let Some(q) = ctx.qtemp.take() {
        super::jobs::drop_qtemp(system, ctx.job.number, q);
    }
    super::security::js(
        "E",
        (&ctx.job.name, &ctx.job.user, ctx.job.number),
        ctx.job.kind,
        &ctx.job.jobd,
    );
    system.jobs.remove(ctx.job.number);
    system.job_seen.insert(ctx.job.name.clone(), 2);
    jrn::entry::forget_job(JobHandle(ctx.job.handle));
    system.attrs.remove(&ctx.job.number);
    super::msgs::release(system, ctx.job.number);
    system.pools.remove(ctx.job.number);
    system.stats.aux_io.remove(&ctx.job.number);
    super::sbs::monitor_ended(system, ctx.job.number);
}

/// End a job's use of the database and the store.
fn end_db(system: &mut System, ctx: &mut JobCtx) {
    let lent = ile::lend(system).is_ok();
    ctx.io.end();
    ile::restore(system);
    if let Some(sql) = ctx.svc.sql.as_mut()
        && let Some(s) = system.storage.as_mut()
        && let Ok(mut txn) = s.store.begin()
    {
        sql.session.end(&mut txn);
        if let Err(e) = txn.commit() {
            eprintln!("qsstg: ending {}: {e}", ctx.job.name);
        }
    }
    let _ = lent;
}

/// Every job ends, then the store is checkpointed and closed.
fn close(system: &mut System, conns: &mut [Conn]) -> Result<String, String> {
    for c in conns.iter_mut() {
        if let Some(ctx) = c.ctx.as_mut() {
            end_job(system, ctx, false);
        }
    }
    let storage = system
        .storage
        .take()
        .ok_or("the object store is not open")?;
    let generation = storage.store.asp().superblock().generation;
    storage
        .store
        .close()
        .map_err(|e| format!("close store: {e}"))?;
    system.temp = None;
    Ok(format!(
        "storage service closed the store at generation {generation}"
    ))
}

/// A request being answered for a job.
struct Served<'a> {
    system: &'a mut System,
    ctx: &'a mut JobCtx,
}

impl Served<'_> {
    /// Run `f` with the store lent to the job's sources.
    fn lent<T>(&mut self, f: impl FnOnce(&mut JobCtx) -> T) -> T {
        let root = self.own_qtemp();
        self.own_libl();
        let lent = ile::lend(self.system).is_ok();
        let r = f(self.ctx);
        if lent {
            ile::restore(self.system);
        }
        ile::set_libl(None);
        self.root_qtemp(root);
        r
    }

    /// Make the job's library the temporary ASP's `QTEMP`; the root it
    /// replaced.
    fn own_qtemp(&mut self) -> Option<ObjectId> {
        let t = self.system.temp.as_mut()?;
        let q = self.ctx.qtemp?;
        Some(std::mem::replace(&mut t.qtemp, q))
    }

    /// Resolve names through the job's library list.
    fn own_libl(&mut self) {
        ile::set_libl(
            self.system
                .attrs
                .get(&self.ctx.job.number)
                .map(|a| a.libl.clone()),
        );
    }

    /// Put the root back.
    fn root_qtemp(&mut self, root: Option<ObjectId>) {
        if let (Some(t), Some(r)) = (self.system.temp.as_mut(), root) {
            t.qtemp = r;
        }
    }
}

impl Service for Served<'_> {
    fn hello(&mut self, _job: JobInfo) -> Result<(), String> {
        Err("this connection already said hello".into())
    }

    fn exec(&mut self, line: String) -> ExecReply {
        let root = self.own_qtemp();
        self.system.current_job = self.ctx.job.number;
        self.own_libl();
        std::mem::swap(&mut self.system.svc, &mut self.ctx.svc);
        let text = super::exec::run(self.system, &line);
        let prompt = svc::db::prompt(self.system).to_string();
        std::mem::swap(&mut self.system.svc, &mut self.ctx.svc);
        // One override table per job: what command level changed reaches
        // the programs' opens.
        if matches!(
            line.split_whitespace().next(),
            Some("ovrdbf" | "dltovr" | "ovrmsgf" | "agend")
        ) {
            if let Some(sql) = self.ctx.svc.sql.as_ref() {
                let table = sql.session.overrides().to_vec();
                self.ctx.io.session().set_overrides(table);
            }
            self.ctx
                .msgs
                .overrides
                .clone_from(&self.ctx.svc.msgf_overrides);
        }
        ile::set_libl(None);
        self.root_qtemp(root);
        let srcs = std::mem::take(&mut self.system.raised)
            .into_iter()
            .map(|s| s.code.to_string())
            .collect();
        if self.system.sysvals_changed {
            super::security::refresh(self.system);
        }
        let sysvals = if std::mem::take(&mut self.system.sysvals_changed) {
            self.system.sysvals.as_ref().map(sysval::SysVals::to_blob)
        } else {
            None
        };
        ExecReply {
            text,
            prompt,
            srcs,
            sysvals,
        }
    }

    fn close(&mut self) -> Result<String, String> {
        Err("only slic closes the store".into())
    }

    fn ping(&mut self) {}

    fn adopt(&mut self, programs: Vec<String>) -> Result<Vec<String>, String> {
        let adopted = super::security::adopted(self.system, &programs);
        let owners = adopted.iter().map(|a| a.owner.clone()).collect();
        self.ctx.session.borrow_mut().principal.adopted = adopted;
        Ok(owners)
    }

    fn sign_on(
        &mut self,
        user: String,
        password: String,
        device: String,
        console: bool,
    ) -> Result<sec::signon::SignedOn, sec::signon::Refused> {
        super::security::sign_on(self.system, &user, &password, &device, console, None)
    }

    fn sign_on_substitute(
        &mut self,
        user: String,
        device: String,
        proof: sec::signon::Substitute,
    ) -> Result<sec::signon::SignedOn, sec::signon::Refused> {
        super::security::sign_on(self.system, &user, "", &device, false, Some(proof))
    }

    fn device_allocate(
        &mut self,
        requested: String,
        devtype: String,
        model: String,
        kbdtype: String,
    ) -> Result<String, String> {
        super::devices::allocate(self.system, &requested, &devtype, &model, &kbdtype)
    }

    fn device_status(&mut self, device: String, status: u8) -> Result<(), String> {
        super::devices::set_status(self.system, &device, status)
    }

    fn power_down(&mut self, immediate: bool, delay: u32, restart: bool) -> Result<(), String> {
        // slic reads the options and powers down on SIGUSR1.
        std::fs::write(
            crate::ipl::power::REQUEST_FILE,
            format!("{} {delay} {}\n", u8::from(immediate), u8::from(restart)),
        )
        .map_err(|e| e.to_string())?;
        rustix::process::kill_process(rustix::process::Pid::INIT, rustix::process::Signal::USR1)
            .map_err(|e| e.to_string())
    }

    fn chg_pwd(&mut self, current: String, new: String, validated: bool) -> Result<(), String> {
        let me = self.ctx.session.borrow().principal.clone();
        super::security::chg_pwd(self.system, &me, &current, &new, validated)
    }

    fn audit_lic(&mut self, events: Vec<(String, String, String)>) {
        super::security::audit_lic(&events);
    }

    fn sec_api(&mut self, q: sec::api::Query) -> Result<sec::api::Answer, ApiFail> {
        super::security::api(self.system, self.ctx, q)
    }

    fn whoami(&mut self) -> sec::auth::principal::Principal {
        self.ctx.session.borrow().principal.clone()
    }

    fn job_number(&mut self) -> Result<u32, String> {
        super::jobs::next(self.system)
    }

    fn set_status(&mut self, status: u8, function: String) -> bool {
        let me = self.ctx.job.number;
        let runnable = wm::job::Status::from_byte(status).runnable();
        let granted = self.system.pools.status(&self.ctx.job.pool, me, runnable);
        if let Some(j) = self.system.jobs.get_mut(me) {
            j.status = if granted {
                status
            } else {
                wm::job::Status::Inel.byte()
            };
            j.function = function;
        }
        granted
    }

    fn active_jobs(&mut self) -> Vec<JobInfo> {
        self.system.jobs.all()
    }

    fn rtvjoba(&mut self) -> JobAttrs {
        self.system
            .attrs
            .get(&self.ctx.job.number)
            .cloned()
            .unwrap_or_default()
    }

    fn chgjob(&mut self, job: String, changes: Vec<(String, String)>) -> Result<(), String> {
        super::jobs::chgjob(self.system, self.ctx.job.number, &job, &changes)
    }

    fn msgq_send(&mut self, queue: String, mut msg: QMsg) -> Result<u32, String> {
        let me = self.ctx.job.number;
        msg.job = msgs::sender(self.system, me);
        let (lib, name) = msgs::queue_name(&queue);
        self.own_libl();
        let r = msgs::send(self.system, &lib, &name, msg);
        ile::set_libl(None);
        r
    }

    fn msgq_receive(
        &mut self,
        queue: String,
        kind: Option<MsgKind>,
        key: Option<u32>,
        remove: bool,
    ) -> Result<Option<QMsg>, String> {
        let (lib, name) = msgs::queue_name(&queue);
        self.own_libl();
        let r = msgs::with_queue(
            self.system,
            if remove {
                "msgq.remove"
            } else {
                "msgq.receive"
            },
            &lib,
            &name,
            |q, _| {
                let m = q.receive(kind, key, remove);
                let changed = m.is_some();
                Ok((m, changed))
            },
        );
        ile::set_libl(None);
        r
    }

    fn msgq_reply(&mut self, queue: String, key: u32, reply: String) -> Result<u32, String> {
        let from = msgs::sender(self.system, self.ctx.job.number);
        let (lib, name) = msgs::queue_name(&queue);
        self.own_libl();
        let r = msgs::with_queue(self.system, "msgq.reply", &lib, &name, |q, _| {
            let k = q
                .reply(key, &reply, from, msgs::now_micros())
                .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF2422")))?;
            Ok((k, true))
        });
        ile::set_libl(None);
        r
    }

    fn msgq_list(&mut self, queue: String) -> Result<Vec<QMsg>, String> {
        let (lib, name) = msgs::queue_name(&queue);
        self.own_libl();
        let r = msgs::with_queue(self.system, "msgq.receive", &lib, &name, |q, _| {
            Ok((q.msgs.clone(), false))
        });
        ile::set_libl(None);
        r
    }

    fn breaks(&mut self) -> Vec<msgs::Break> {
        msgs::take_breaks(self.system, self.ctx.job.number)
    }

    fn sbs_check(&mut self, name: String) -> Result<(String, Vec<u8>), String> {
        super::sbs::check(self.system, &name)
    }

    fn sbs_register(&mut self, name: String, lib: String) -> Result<Vec<u8>, String> {
        super::sbs::register(self.system, self.ctx.job.number, &name, &lib)
    }

    fn sbs_state(&mut self) -> wm::sbs::SbsState {
        let mut s = super::sbs::state(self.system, self.ctx.job.number);
        s.end_jobs = super::jobqs::take_end_requests(self.system, self.ctx.job.number);
        s
    }

    fn sbmjob(&mut self, req: wm::obj::jobq::SbmJob) -> Result<(u32, String), String> {
        self.own_libl();
        let r = super::jobqs::sbmjob(self.system, self.ctx.job.number, &req);
        ile::set_libl(None);
        r
    }

    fn jobq_take(&mut self, qual: String, allowed: Vec<bool>) -> Option<wm::obj::jobq::QJob> {
        super::jobqs::take(self.system, self.ctx.job.number, &qual, &allowed)
    }

    fn job_ending(&mut self, code: u16) {
        self.ctx.end_code = Some(code);
    }

    fn scd_tick(&mut self) -> Vec<String> {
        super::jobscd::tick(self.system, self.ctx.job.number)
    }

    fn joblog_put(&mut self, entries: Vec<wm::joblog::Entry>) {
        for e in entries {
            if let wm::joblog::Appended::Wrapped(gone) = self.ctx.joblog.append(e) {
                self.ctx.joblog_wrapped.extend(gone);
            }
        }
    }

    fn splf_open(&mut self, attrs: wm::obj::splf::SplfAttrs) -> Result<u32, String> {
        let j = &self.ctx.job;
        let attrs = wm::obj::splf::SplfAttrs {
            job: (j.name.clone(), j.user.clone(), j.number),
            ..attrs
        };
        self.own_libl();
        let r = super::spool::create(self.system, attrs);
        ile::set_libl(None);
        let id = r?;
        self.ctx.splfs.push(id);
        Ok(id)
    }

    fn menu_open(&mut self, name: String, lib: String) -> Result<(Vec<u8>, String), String> {
        self.own_libl();
        let r = crate::svc::ui::open_menu(self.system, &name, &lib);
        ile::set_libl(None);
        r
    }

    fn pnlgrp_open(&mut self, name: String, lib: String) -> Result<(Vec<u8>, String), String> {
        self.own_libl();
        let r = crate::svc::ui::open_pnlgrp(self.system, &name, &lib);
        ile::set_libl(None);
        r
    }

    // --- panel data: work management (140-159)

    fn actjob_data(&mut self, advance: bool) -> stg::proto::ActJobs {
        super::stats::actjob_data(self.system, advance)
    }

    fn job_detail(&mut self, job: String) -> Result<stg::proto::JobDetail, String> {
        let q = job.trim().to_ascii_uppercase();
        let number = if q.is_empty() || q == "*" {
            self.ctx.job.number
        } else if let Some(id) = wm::job::JobId::parse(&q) {
            id.number
        } else {
            // A bare name: the caller's own job of that name first.
            let all = self.system.jobs.all();
            all.iter()
                .find(|j| j.name == q && j.user == self.ctx.job.user)
                .or_else(|| all.iter().find(|j| j.name == q))
                .map(|j| j.number)
                .ok_or_else(|| format!("CPF1070 Job {q} not found."))?
        };
        let j = self
            .system
            .jobs
            .get(number)
            .cloned()
            .ok_or_else(|| format!("CPF1070 Job {q} not found."))?;
        let attrs = self.system.attrs.get(&number).cloned().unwrap_or_default();
        let (cpu_ms, threads, tmp_mb, aux_io) = super::stats::job_figures(self.system, number);
        let sbs_pool = super::sbs::list(self.system)
            .into_iter()
            .find(|s| s.name == j.subsystem)
            .and_then(|s| s.pools.into_iter().find(|(_, p)| *p == j.pool))
            .map_or(0, |(id, _)| id);
        Ok(stg::proto::JobDetail {
            job: j,
            attrs,
            cpu_ms,
            threads,
            tmp_mb,
            aux_io,
            sbs_pool,
        })
    }

    fn jobq_attach(&mut self) -> Vec<(String, String)> {
        self.system
            .jobq_attach
            .iter()
            .map(|(q, s)| (q.clone(), s.clone()))
            .collect()
    }

    fn wtr_list(&mut self) -> Vec<(String, String, String, String, String, u32)> {
        self.system
            .writers
            .iter()
            .map(|(name, w)| {
                let status = if w.ending {
                    "END"
                } else if w.held {
                    "HLD"
                } else if w.current.is_some() {
                    "WTR"
                } else if self.system.jobs.get(w.job).is_some() {
                    "STR"
                } else {
                    "JOBQ"
                };
                (
                    name.clone(),
                    w.dev.clone(),
                    status.to_string(),
                    w.formtype.clone(),
                    w.outq.clone(),
                    w.printed,
                )
            })
            .collect()
    }

    fn splf_list(&mut self, user: String, job: String) -> Vec<wm::obj::splf::SplfAttrs> {
        let user = user.trim().to_ascii_uppercase();
        let job = job.trim().to_ascii_uppercase();
        let mut files: Vec<wm::obj::splf::SplfAttrs> = self
            .system
            .spool
            .files
            .values()
            .map(|e| e.attrs.clone())
            .filter(|a| user.is_empty() || a.job.1 == user)
            .filter(|a| job.is_empty() || super::spool::job_matches(&job, &a.job))
            .collect();
        files.sort_by_key(|a| (a.created, a.id));
        files
    }

    fn dsp_open(
        &mut self,
        file: String,
        lib: String,
        actgrp: String,
    ) -> Result<(Vec<u8>, String), String> {
        self.own_libl();
        let ovr = self.ctx.svc.dsp_overrides.clone();
        let r = crate::svc::db::dspf::open(self.system, &ovr, (&file, &lib), &actgrp);
        ile::set_libl(None);
        r
    }

    fn prt_open(
        &mut self,
        file: String,
        lib: String,
        actgrp: String,
    ) -> Result<(u32, wm::obj::prtf::Prtf), String> {
        self.own_libl();
        let ovr = self.ctx.svc.prt_overrides.clone();
        let r = super::prtf::open(self.system, &ovr, &self.ctx.job, (&file, &lib), &actgrp);
        ile::set_libl(None);
        let (id, prtf) = r?;
        self.ctx.splfs.push(id);
        Ok((id, prtf))
    }

    fn spool_maint(&mut self) -> String {
        super::spool::maintain(self.system)
    }

    fn sys_status(&mut self, format: String, reset: bool) -> Result<Vec<u8>, String> {
        super::stats::qwcrssts(self.system, &format, reset)
    }

    fn next_iplsts(&mut self, value: u8) -> Result<(), String> {
        super::jobs::set_next_iplsts(self.system, value)
    }

    fn job_state(&mut self, name: String) -> u8 {
        if self.system.pending.values().any(|q| q.name == name) {
            return 1;
        }
        self.system.job_seen.get(&name).copied().unwrap_or(0)
    }

    fn wtr_next(&mut self, name: String) -> wm::wtr::WtrStep {
        super::wtr::next(self.system, &name)
    }

    fn wtr_printed(
        &mut self,
        name: String,
        id: u32,
        text: String,
        pdf: Vec<u8>,
    ) -> Result<(), String> {
        super::wtr::printed(self.system, &name, id, &text, &pdf)
    }

    fn splf_put(&mut self, id: u32, data: Vec<u8>, pages: u32, records: u32) -> Result<(), String> {
        if !self.ctx.splfs.contains(&id) {
            return Err(format!("CPF3309 spooled file {id} is not open in this job"));
        }
        self.system.stats.spooled.0 += 1;
        self.system.stats.spooled.1 += data.len() as u64;
        super::spool::append(self.system, id, &data, pages, records)
    }

    fn splf_close(&mut self, id: u32, hold: bool) -> Result<(), String> {
        if !self.ctx.splfs.contains(&id) {
            return Err(format!("CPF3309 spooled file {id} is not open in this job"));
        }
        self.ctx.splfs.retain(|x| *x != id);
        super::spool::close(self.system, id, hold)
    }

    fn sbs_list(&mut self) -> Vec<wm::sbs::SbsInfo> {
        super::sbs::list(self.system)
    }

    fn pools(&mut self) -> Vec<wm::sbs::PoolInfo> {
        self.system.pools.info()
    }

    fn sbs_end(&mut self, name: String, immediate: bool, delay: u32) -> Result<(), String> {
        super::sbs::end(self.system, &name, immediate, delay).map(|_| ())
    }

    fn wm_object(&mut self, qual: String, ty: String) -> Result<Vec<u8>, String> {
        self.own_libl();
        let r = super::sbs::wm_object(self.system, &qual, &ty);
        ile::set_libl(None);
        r
    }

    fn define_pool(&mut self, name: String, size_kb: u64, actlvl: u32) {
        self.system.pools.define(&name, size_kb, actlvl);
    }

    fn sysrpyl(&mut self, msgid: String, data: Vec<u8>) -> Option<(String, bool)> {
        let s = self.system.storage.as_mut()?;
        let qsys = s.qsys?;
        let mut txn = s.store.begin().ok()?;
        let list = wm::obj::rpyl::load(&mut txn, qsys).ok();
        txn.abort();
        let text = ccsid::from_ebcdic(&data);
        list?.find(&msgid, &text).map(|e| (e.reply.clone(), e.dump))
    }

    fn program(
        &mut self,
        name: String,
        lib: String,
        kind: u8,
    ) -> Option<(Vec<u8>, String, String)> {
        let kind = if kind == 1 {
            PgmKind::SrvPgm
        } else {
            PgmKind::Pgm
        };
        self.lent(|c| c.objects.program(&name, &lib, kind))
            .map(|(p, l)| (p.encode(), p.name.clone(), l))
    }

    fn resolve(&mut self, name: String, lib: String, ty: String) -> Option<u64> {
        self.lent(|c| c.objects.resolve(&name, &lib, &ty))
    }

    fn by_id(&mut self, id: u64) -> Option<(String, String)> {
        self.lent(|c| c.objects.by_id(id))
    }

    fn us_load(&mut self, name: String, lib: String) -> Option<Vec<u8>> {
        self.lent(|c| c.spaces.load(&name, &lib))
    }

    fn us_store(&mut self, name: String, lib: String, data: Vec<u8>) -> bool {
        self.lent(|c| c.spaces.store(&name, &lib, &data))
    }

    fn us_create(
        &mut self,
        name: String,
        lib: String,
        size: u64,
        init: u8,
        text: String,
    ) -> Result<(), String> {
        let size = usize::try_from(size).map_err(|_| "CPF9810".to_string())?;
        self.lent(|c| c.spaces.create(&name, &lib, size, init, &text))
            .map_err(str::to_string)
    }

    fn us_delete(&mut self, name: String, lib: String) -> bool {
        self.lent(|c| c.spaces.delete(&name, &lib))
    }

    fn us_library_of(&mut self, name: String, lib: String) -> Option<String> {
        self.lent(|c| c.spaces.library_of(&name, &lib))
    }

    fn us_domain(&mut self, name: String, lib: String) -> bool {
        self.lent(|c| c.spaces.system_domain(&name, &lib))
    }

    fn describe(&mut self, msgid: String, msgf: String, lib: String) -> Option<MsgDesc> {
        self.lent(|c| c.msgs.describe(&msgid, &msgf, &lib))
    }

    fn da_lock(&mut self, name: String, lib: String) -> Result<(), IoFail> {
        self.lent(|c| c.data.dtaara_lock(&name, &lib))
    }

    fn da_unlock(&mut self, name: String, lib: String) -> Result<(), IoFail> {
        self.lent(|c| c.data.dtaara_unlock(&name, &lib))
    }

    fn da_read(&mut self, name: String, lib: String) -> Result<DataAreaValue, IoFail> {
        self.lent(|c| c.data.dtaara_read(&name, &lib))
    }

    fn da_write(&mut self, name: String, lib: String, value: DataAreaValue) -> Result<(), IoFail> {
        self.lent(|c| c.data.dtaara_write(&name, &lib, &value))
    }

    fn dq_send(&mut self, name: String, lib: String, data: Vec<u8>) -> Result<(), IoFail> {
        self.lent(|c| c.data.dtaq_send(&name, &lib, &data))
    }

    fn dq_receive(&mut self, name: String, lib: String) -> Result<Option<Vec<u8>>, IoFail> {
        self.lent(|c| c.data.dtaq_receive(&name, &lib, 0))
    }

    fn open(&mut self, spec: OpenSpec) -> Result<(u32, IoFileInfo), IoFail> {
        self.lent(|c| c.io.open(&spec))
    }

    fn io(&mut self, handle: u32, req: OwnedRequest) -> IoResult {
        self.lent(|c| c.io.io(handle, &req.as_request()))
    }

    fn close_file(&mut self, handle: u32) -> IoResult {
        self.lent(|c| c.io.close(handle))
    }

    fn list_objects(&mut self, lib: String, name: String, ty: String) -> Vec<ObjInfo> {
        self.lent(|c| c.provider.list_objects(&lib, &name, &ty))
    }

    fn object(&mut self, lib: String, name: String, ty: String) -> Option<ObjInfo> {
        self.lent(|c| c.provider.object(&lib, &name, &ty))
    }

    fn change_object(
        &mut self,
        lib: String,
        name: String,
        ty: String,
        text: Option<String>,
        user_attr: Option<String>,
    ) -> Result<String, ApiFail> {
        self.lent(|c| c.provider.change_object(&lib, &name, &ty, text, user_attr))
    }

    fn rename_object(
        &mut self,
        lib: String,
        name: String,
        ty: String,
        new_lib: String,
        new_name: String,
        replace: bool,
    ) -> Result<String, ApiFail> {
        self.lent(|c| {
            c.provider
                .rename_object(&lib, &name, &ty, &new_lib, &new_name, replace)
        })
    }

    fn usridx_create(&mut self, lib: String, name: String, attr: IdxAttr) -> Result<(), ApiFail> {
        self.lent(|c| c.provider.usridx_create(&lib, &name, attr))
    }

    fn usridx_delete(&mut self, lib: String, name: String) -> Result<(), ApiFail> {
        self.lent(|c| c.provider.usridx_delete(&lib, &name))
    }

    fn usridx_add(
        &mut self,
        lib: String,
        name: String,
        entries: Vec<Vec<u8>>,
        replace: bool,
    ) -> Result<usize, ApiFail> {
        self.lent(|c| c.provider.usridx_add(&lib, &name, entries, replace))
    }

    fn usridx_find(
        &mut self,
        lib: String,
        name: String,
        rule: u32,
        key: Vec<u8>,
        key2: Vec<u8>,
        max: usize,
        remove: bool,
    ) -> Result<Vec<Vec<u8>>, ApiFail> {
        self.lent(|c| {
            c.provider
                .usridx_find(&lib, &name, rule, &key, &key2, max, remove)
        })
    }

    fn usridx_attrs(&mut self, lib: String, name: String) -> Result<IdxAttrs, ApiFail> {
        self.lent(|c| c.provider.usridx_attrs(&lib, &name))
    }

    fn api_dtaq_send(
        &mut self,
        lib: String,
        name: String,
        key: Vec<u8>,
        data: Vec<u8>,
        sender: String,
    ) -> Result<(), ApiFail> {
        self.lent(|c| c.provider.dtaq_send(&lib, &name, &key, &data, &sender))
    }

    fn api_dtaq_receive(
        &mut self,
        lib: String,
        name: String,
        order: String,
        key: Vec<u8>,
        remove: bool,
    ) -> Result<Option<DtaqEntry>, ApiFail> {
        self.lent(|c| c.provider.dtaq_receive(&lib, &name, &order, &key, remove))
    }

    fn api_dtaq_clear(
        &mut self,
        lib: String,
        name: String,
        order: String,
        key: Vec<u8>,
    ) -> Result<(), ApiFail> {
        self.lent(|c| c.provider.dtaq_clear(&lib, &name, &order, &key))
    }

    fn api_dtaq_describe(&mut self, lib: String, name: String) -> Result<DtaqInfo, ApiFail> {
        self.lent(|c| c.provider.dtaq_describe(&lib, &name))
    }

    fn api_dtaq_entries(&mut self, lib: String, name: String) -> Result<Vec<DtaqEntry>, ApiFail> {
        self.lent(|c| c.provider.dtaq_entries(&lib, &name))
    }

    fn api_dtaara_read(&mut self, lib: String, name: String) -> Result<DataArea, ApiFail> {
        self.lent(|c| c.provider.dtaara_read(&lib, &name))
    }

    fn api_dtaara_write(
        &mut self,
        lib: String,
        name: String,
        start: usize,
        bytes: Vec<u8>,
    ) -> Result<(), ApiFail> {
        self.lent(|c| c.provider.dtaara_write(&lib, &name, start, &bytes))
    }

    fn sysval(&mut self, name: String) -> Option<SysVal> {
        self.lent(|c| c.provider.sysval(&name))
    }

    fn netattr(&mut self, name: String) -> Option<SysVal> {
        self.lent(|c| c.provider.netattr(&name))
    }

    fn now(&mut self) -> LocalNow {
        self.lent(|c| c.provider.now())
    }

    fn check_command(&mut self, text: String) -> Result<(), ApiFail> {
        self.lent(|c| c.provider.check_command(&text))
    }

    fn cmd_lookup(
        &mut self,
        name: String,
        lib: Option<String>,
    ) -> Result<Option<(Vec<u8>, String)>, String> {
        self.system.current_job = self.ctx.job.number;
        self.own_libl();
        let r = (|| {
            let s = self
                .system
                .storage
                .as_mut()
                .ok_or("object store not open")?;
            let qsys = s.qsys.ok_or("object store not initialized")?;
            svc::db::with_txn(&mut s.store, |txn| {
                let names = ile::name_context(txn, qsys);
                let mut src = cmd::store::StoreSource { txn, qsys, names };
                match cmd::analyze::CmdSource::lookup(&mut src, &name, lib.as_deref()) {
                    Ok(Some((o, l))) => Ok(Some((o.encode(), l))),
                    Ok(None) => Ok(None),
                    Err(e) => Err(e.msgid().to_string()),
                }
            })
        })();
        ile::set_libl(None);
        r
    }

    fn run_cmd(
        &mut self,
        name: String,
        segments: Vec<Option<Vec<u8>>>,
        specified: Vec<bool>,
        typed: String,
    ) -> stg::proto::CmdReply {
        let def = match sys_cpps::install::object(&name, 0) {
            Ok(o) => o.def,
            Err(_) => {
                return crate::cpp::escape(
                    "CPF0006",
                    &name,
                    &format!("{name} has no shipped definition"),
                );
            }
        };
        let parms = match cmd::block::read(&def, &segments, &specified) {
            Ok(p) => p,
            Err(e) => return crate::cpp::escape(e.msgid(), &name, &e.to_string()),
        };
        let root = self.own_qtemp();
        self.system.current_job = self.ctx.job.number;
        self.own_libl();
        std::mem::swap(&mut self.system.svc, &mut self.ctx.svc);
        let mut reply = crate::cpp::run(self.system, &name, &parms, &typed);
        reply.prompt = svc::db::prompt(self.system).to_string();
        std::mem::swap(&mut self.system.svc, &mut self.ctx.svc);
        if matches!(name.as_str(), "OVRDBF" | "DLTOVR" | "OVRMSGF") {
            if let Some(sql) = self.ctx.svc.sql.as_ref() {
                let table = sql.session.overrides().to_vec();
                self.ctx.io.session().set_overrides(table);
            }
            self.ctx
                .msgs
                .overrides
                .clone_from(&self.ctx.svc.msgf_overrides);
        }
        ile::set_libl(None);
        self.root_qtemp(root);
        reply.srcs = std::mem::take(&mut self.system.raised)
            .into_iter()
            .map(|s| s.code.to_string())
            .collect();
        if self.system.sysvals_changed {
            super::security::refresh(self.system);
        }
        reply.sysvals = if std::mem::take(&mut self.system.sysvals_changed) {
            self.system.sysvals.as_ref().map(sysval::SysVals::to_blob)
        } else {
            None
        };
        reply.segments = segments;
        reply
    }

    fn file(&mut self, lib: String, name: String) -> Option<FileInfo> {
        self.lent(|c| c.provider.file(&lib, &name))
    }

    fn members(&mut self, lib: String, name: String) -> Vec<MemberInfo> {
        self.lent(|c| c.provider.members(&lib, &name))
    }

    fn formats(&mut self, lib: String, name: String) -> Vec<FormatInfo> {
        self.lent(|c| c.provider.formats(&lib, &name))
    }

    fn dependents(&mut self, lib: String, name: String, member: String) -> Vec<DepInfo> {
        self.lent(|c| c.provider.dependents(&lib, &name, &member))
    }

    fn commit_info(&mut self) -> Option<CommitInfo> {
        // The job's files and their commit cycle live in its session here.
        let cmt = self.ctx.io.session().commit_def().cloned();
        cmt.map(|c| {
            let c = c.borrow();
            CommitInfo {
                lock_level: c.lcklvl().name().to_string(),
                pending: c.changes() > 0,
                journal: None,
            }
        })
        .or_else(|| self.lent(|c| c.provider.commit_info()))
    }

    fn user_info(&mut self, name: String) -> Option<UserInfo> {
        self.lent(|c| c.provider.user_info(&name))
    }
}

/// `CPF1241` (normal) or `CPF1240` (abnormal: end code 20 and up, or no
/// end code because the job vanished) to the queue `SBMJOB` named.
fn completion_message(system: &mut System, ctx: &JobCtx) {
    if ctx.completion.is_empty() {
        return;
    }
    let code = ctx.end_code.unwrap_or(30);
    let j = &ctx.job;
    let num = format!("{:06}", j.number);
    let data = wm::joblog::data(&[
        (&j.name, 10),
        (&j.user, 10),
        (&num, 6),
        (&code.to_string(), 3),
    ]);
    let (msgid, kind) = if code >= 20 {
        ("CPF1240", wm::obj::msgq::MsgKind::Escape)
    } else {
        ("CPF1241", wm::obj::msgq::MsgKind::Comp)
    };
    let text = super::hist::message_text(msgid, &data);
    let m = QMsg {
        kind,
        msgid: msgid.into(),
        msgf: ("QCPFMSG".into(), "QSYS".into()),
        data,
        text,
        severity: if code >= 20 { 50 } else { 0 },
        job: (j.name.clone(), j.user.clone(), j.number),
        ..QMsg::default()
    };
    let (lib, name) = wm::obj::params::qualified(&ctx.completion);
    if let Err(e) = msgs::send(system, &lib, &name, m) {
        eprintln!("qsstg: completion message to {}: {e}", ctx.completion);
    }
}
