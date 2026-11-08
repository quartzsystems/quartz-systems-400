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
    /// Unique for the service's life: connections come and go while a
    /// long statement waits its turn (`turns`).
    pub id: u64,
}

impl Conn {
    /// A new connection.
    pub fn new(chan: Chan, ctx: Option<JobCtx>) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self { chan, ctx, id }
    }
}

/// What the service keeps for one job.
pub struct JobCtx {
    /// Who.
    pub job: JobInfo,
    /// Its `QTEMP`: a temporary root in ASP 1, the one CL, record I/O
    /// and SQL share.
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
    /// Its programs' embedded SQL: cursors and prepared statements.
    pub sql: Option<sql::embed::Session>,
    /// For a `QSQSRVR` job serving a CLI connection: the client job
    /// (`QSYS2.SERVER_MODE_JOB_NAME`).
    pub sql_client: Option<String>,
    /// A secondary thread's link (`hello_thread`): closing it ends
    /// nothing.
    pub thread: bool,
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
            sql: None,
            sql_client: None,
            thread: false,
            job,
        }
    }
}

/// `hello_thread`: a secondary thread of active job `number` in
/// process `pid`, sharing the job's security session and `QTEMP`.
fn thread_of(system: &System, conns: &[Conn], number: u32, pid: u32) -> Result<JobCtx, String> {
    let main = conns
        .iter()
        .filter_map(|c| c.ctx.as_ref())
        .find(|c| !c.thread && c.job.number == number && c.job.pid == pid)
        .ok_or_else(|| format!("job number {number:06} is not active in process {pid}"))?;
    let mut ctx = JobCtx::new(system, main.job.clone());
    ctx.session = Rc::clone(&main.session);
    ctx.qtemp = main.qtemp;
    ctx.svc.qtemp = main.svc.qtemp.clone();
    ctx.thread = true;
    Ok(ctx)
}

/// Answer one request on connection `i`, then audit what it did.
pub fn handle(system: &mut System, conns: &mut [Conn], i: usize, req: Request) -> Vec<u8> {
    let reply = answer(system, conns, i, req);
    let who = conns.get(i).and_then(|c| c.ctx.as_ref()).map(charged);
    super::security::flush(system, who);
    reply
}

/// Who a job's audit entries are charged to.
pub fn charged(ctx: &JobCtx) -> super::security::Charged {
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
    if let Request::HelloThread { number, pid } = req {
        let r = if conns[i].ctx.is_some() {
            Err("this connection already said hello".into())
        } else {
            thread_of(system, conns, number, pid).map(|ctx| conns[i].ctx = Some(ctx))
        };
        return ipc::encode(&r);
    }
    if let Request::Close {} = req {
        return ipc::encode(&close(system, conns));
    }
    if let Request::JoblogOf { number } = req {
        // Another job's copy: it lives on that job's connection.
        return match conns[i].ctx.as_ref() {
            Some(me) => ipc::encode(&joblog_of(system, conns, &Rc::clone(&me.session), number)),
            None => Vec::new(),
        };
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
            | Request::WsEndRequests { .. }
            | Request::Breaks {}
            | Request::JoblogPut { .. }
            | Request::WtrNext { .. }
            | Request::ScdTick {}
            | Request::JobState { .. }
    );
    if !quiet {
        *system.stats.aux_io.entry(ctx.job.number).or_default() += 1;
        system.stats.request(ctx.job.number, &req);
        super::trace::request(ctx.job.number, &req);
    }
    let user = super::security::as_user(&req);
    let session = Rc::clone(&ctx.session);
    // Dates and times in the asking job's formats.
    let _formats = super::fmt::enter(
        system
            .attrs
            .get(&ctx.job.number)
            .map(wm::job::JobAttrs::formats),
    );
    super::security::freshen(system, &session);
    let mut served = Served { system, ctx };
    if user {
        sec::guard::as_job(&session, || stg::dispatch(&mut served, req))
    } else {
        stg::dispatch(&mut served, req)
    }
}

/// `JoblogOf`: the service's copy of job `number`'s log, for its own
/// user or one with `*JOBCTL`.
fn joblog_of(
    system: &mut System,
    conns: &[Conn],
    session: &Rc<RefCell<sec::auth::cache::Session>>,
    number: u32,
) -> Result<Vec<wm::joblog::Entry>, String> {
    let ctx = conns
        .iter()
        .filter_map(|c| c.ctx.as_ref())
        .find(|c| !c.thread && c.job.number == number)
        .ok_or_else(|| format!("CPF1070 Job {number:06} is not active."))?;
    super::security::freshen(system, session);
    let q = ctx.job.qualified();
    sec::guard::as_job(session, || super::security::may_display(&ctx.job.user, &q))?;
    // The log of a job with *ALLOBJ takes QIBM_ACCESS_ALLOBJ_JOBLOG
    // from a *JOBCTL user who is not its own.
    let target_allobj = ctx
        .session
        .borrow()
        .principal
        .has(sec::profile::SpecialAuth::ALLOBJ);
    let other = sec::guard::as_job(session, || {
        super::security::actor().is_some_and(|p| p.user != ctx.job.user)
    });
    if target_allobj && other {
        let entries = super::joblog::entries(ctx);
        let allowed = sec::guard::as_job(session, || {
            super::security::function_allowed(system, "QIBM_ACCESS_ALLOBJ_JOBLOG")
        });
        if !allowed {
            return Err(format!("CPF1343 {q} QIBM_ACCESS_ALLOBJ_JOBLOG"));
        }
        return Ok(entries);
    }
    Ok(super::joblog::entries(ctx))
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
    let (qtemp, made) = super::jobs::make_qtemp(system, &job.user)?;
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
    let mut attrs = attrs;
    // A job in a secondary language finds its text first.
    if let Some(lib) = super::jobs::language_library(system, &attrs.langid) {
        attrs.libl.set_language(Some(&lib));
    }
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
    let entered = queued.as_ref().map_or(0, |q| q.submitted / 1_000_000);
    super::acg::job_started(system, &job, entered);
    let jobq = system.queued.get(&job.number).cloned().unwrap_or_default();
    super::jobntfy::started(system, &job, entered, &jobq);
    super::liclog::write(
        0x0300,
        0x0001,
        job.number,
        &format!("{}/{}", job.user, job.name),
    );
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
    ctx.svc.qtemp.adopt(qtemp, made);
    if let Some(a) = system.attrs.get(&ctx.job.number) {
        ctx.joblog = wm::joblog::JobLog::new(a.jobmsgqmx, wm::joblog::Full::parse(&a.jobmsgqfl));
    }
    if let Some(q) = queued {
        ctx.completion = q.msgq;
    }
    Ok(ctx)
}

/// Open (`true`) or end a group commit. Ending a job answers no one,
/// so the job ends one pass of the serve loop finds commit together:
/// a few flushes for all of them instead of several for each.
pub fn group(system: &mut System, open: bool) {
    let Some(s) = system.storage.as_mut() else {
        return;
    };
    if open {
        s.store.begin_group();
    } else if let Err(e) = s.store.end_group() {
        eprintln!("qsstg: ending jobs: {e}");
    }
}

/// A job's connection closed: whatever it held goes.
pub fn vanished(system: &mut System, conn: Conn) {
    let Some(mut ctx) = conn.ctx else {
        return;
    };
    // A thread's link: the job goes on.
    if ctx.thread {
        return;
    }
    end_job(system, &mut ctx, true);
    let who = charged(&ctx);
    super::security::flush(system, Some(who));
}

/// End a job: its database use, its spooled files and job log, its
/// `QTEMP`, its table entry. `vanished` says what a job that reported
/// no end code did: vanish (abnormal) or end with the system.
fn end_job(system: &mut System, ctx: &mut JobCtx, vanished: bool) {
    super::ifs::release(system, ctx.job.handle);
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
    let minor = if abnormal { 0x0003 } else { 0x0002 };
    super::liclog::write(
        0x0300,
        minor,
        j.number,
        &format!("{}/{} end code {code}", j.user, j.name),
    );
    super::trace::job(j.number, "JOB", "ended");
    super::hist::system_message(system, "CPF1164", &data, "04", who.clone());
    for id in std::mem::take(&mut ctx.splfs) {
        let _ = super::spool::close(system, id, false);
    }
    let cpu = super::acg::cpu_ms(system, ctx.job.number);
    super::jobntfy::ended(system, &ctx.job, code, cpu);
    super::acg::job_ended(system, ctx.job.number, code);
    if ctx.end_code.is_none() {
        super::joblog::ended_here(system, ctx, &data);
        if vanished {
            software_error(system, &who);
        }
    }
    super::joblog::spool(system, ctx, abnormal);
    super::spool::job_ended(system, &who);
    super::wtr::job_ended(system, ctx.job.number);
    completion_message(system, ctx);
    // `end_db` dropped it.
    ctx.qtemp = None;
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
    system.stats.ended(ctx.job.number);
    super::sbs::monitor_ended(system, ctx.job.number);
}

/// A job vanished: when its process failed in the system's code (the
/// marker `stg::software_error_path` its panic hook left), a software
/// error, logged as a problem when `QSFWERRLOG` is `*LOG`.
fn software_error(system: &mut System, (name, user, number): &(String, String, u32)) {
    let path = stg::software_error_path(*number);
    let Ok(what) = std::fs::read_to_string(&path) else {
        return;
    };
    let _ = std::fs::remove_file(&path);
    if super::security::sysval_text(system, "QSFWERRLOG").trim() != "*LOG" {
        return;
    }
    let job = format!("{number:06}/{user}/{name}");
    let d = prb::analyze::from_software(&job, what.lines().next().unwrap_or_default());
    let _ = super::prblog::log(system, prb::PrbType::Machine, &d);
}

/// End a job's use of the database and the store.
fn end_db(system: &mut System, ctx: &mut JobCtx) {
    let lent = ile::lend(system).is_ok();
    ctx.io.end();
    ile::restore(system);
    if let Some(e) = ctx.sql.as_mut()
        && let Some(s) = system.storage.as_mut()
        && let Ok(mut txn) = s.store.begin()
    {
        e.ctx.session.end(&mut txn);
        if let Err(err) = txn.commit() {
            eprintln!("qsstg: ending {}: {err}", ctx.job.name);
        }
    }
    if let Some(sql) = ctx.svc.sql.as_mut()
        && let Some(s) = system.storage.as_mut()
        && let Ok(mut txn) = s.store.begin()
    {
        sql.session.end(&mut txn);
        if let Err(e) = txn.commit() {
            eprintln!("qsstg: ending {}: {e}", ctx.job.name);
        }
    }
    // The job's `QTEMP`, in ASP 1.
    if !ctx.thread
        && let Some(lib) = ctx.svc.qtemp.take().or(ctx.qtemp)
        && let Err(e) = super::jobs::drop_qtemp(system, JobHandle(ctx.job.handle), lib)
    {
        eprintln!("qsstg: QTEMP of {}: {e}", ctx.job.name);
    }
    let _ = lent;
}

/// Every job ends, then the store is checkpointed and closed.
fn close(system: &mut System, conns: &mut [Conn]) -> Result<String, String> {
    // The close checkpoint makes the job ends durable together.
    group(system, true);
    for c in conns.iter_mut() {
        if let Some(ctx) = c.ctx.as_mut().filter(|c| !c.thread) {
            end_job(system, ctx, false);
        }
    }
    // The IFS volume's filesystem first: it writes through the store.
    let ifs = super::ifsvol::close();
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
    let text = format!("storage service closed the store at generation {generation}");
    Ok(match ifs {
        Some(line) => format!("{text}; {line}"),
        None => text,
    })
}

/// A request being answered for a job.
struct Served<'a> {
    system: &'a mut System,
    ctx: &'a mut JobCtx,
}

impl Served<'_> {
    /// Run `f` with the store lent to the job's sources.
    fn lent<T>(&mut self, f: impl FnOnce(&mut JobCtx) -> T) -> T {
        let entered = enter(self.system, self.ctx);
        let r = f(self.ctx);
        leave(self.system, entered);
        r
    }

    /// Make the job's library the temporary ASP's `QTEMP`; the root it
    /// replaced.
    fn own_qtemp(&mut self) -> Option<ObjectId> {
        let t = self.system.temp.as_mut()?;
        let q = self.ctx.qtemp?;
        store::obj::ops::set_job_qtemp(Some(q));
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
        // A nested request's end leaves the outer job's in force.
        store::obj::ops::set_job_qtemp(root.filter(|r| r.asp != 0));
    }

    /// Run `f` as one of the job's commands: its `QTEMP`, library list
    /// and console state in place. The prompt to show next with it.
    fn as_command<T>(&mut self, f: impl FnOnce(&mut System) -> T) -> (T, String) {
        let root = self.own_qtemp();
        self.system.current_job = self.ctx.job.number;
        self.own_libl();
        std::mem::swap(&mut self.system.svc, &mut self.ctx.svc);
        let r = f(self.system);
        let prompt = svc::db::prompt(self.system).to_string();
        std::mem::swap(&mut self.system.svc, &mut self.ctx.svc);
        ile::set_libl(None);
        self.root_qtemp(root);
        (r, prompt)
    }

    /// What a command's reply carries at its end: the codes it raised,
    /// the system values when they changed, its parameter block.
    fn end_command(&mut self, reply: &mut stg::proto::CmdReply, segments: Vec<Option<Vec<u8>>>) {
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
    }
}

/// A `*CMD` found as the analyzer finds it, through the job's library
/// list, with the actor's `*OBJOPR` and `*USE` to it checked
/// (`cmd.run`); `None` when there is none. A refusal is its message
/// identifier.
pub fn cmd_lookup(
    system: &mut System,
    ctx: &JobCtx,
    name: &str,
    lib: Option<&str>,
) -> Result<Option<(Vec<u8>, String)>, String> {
    system.current_job = ctx.job.number;
    ile::set_libl(system.attrs.get(&ctx.job.number).map(|a| a.libl.clone()));
    let r = (|| {
        let s = system.storage.as_mut().ok_or("object store not open")?;
        let qsys = s.qsys.ok_or("object store not initialized")?;
        svc::db::with_txn(&mut s.store, |txn| {
            let names = ile::name_context(txn, qsys);
            let mut src = cmd::store::StoreSource { txn, qsys, names };
            match cmd::analyze::CmdSource::lookup(&mut src, name, lib) {
                Ok(Some((o, l))) => Ok(Some((o.encode(), l))),
                Ok(None) => Ok(None),
                Err(e) => Err(e.msgid().to_string()),
            }
        })
    })();
    ile::set_libl(None);
    r
}

/// A command's parameters from its block, or the escape that ends it.
fn command_parms(
    name: &str,
    segments: &[Option<Vec<u8>>],
    specified: &[bool],
) -> Result<cmd::block::Parms, Box<stg::proto::CmdReply>> {
    let def = match sys_cpps::install::object(name, 0) {
        Ok(o) => o.def,
        Err(_) => {
            return Err(Box::new(crate::cpp::escape(
                "CPF0006",
                name,
                &format!("{name} has no shipped definition"),
            )));
        }
    };
    cmd::block::read(&def, segments, specified)
        .map_err(|e| Box::new(crate::cpp::escape(e.msgid(), name, &e.to_string())))
}

/// A long command started (`stgsvc::turns`): its work, prepared as one
/// of the job's commands, with its parameter block for the reply; or
/// the reply when it ended (or ran) at once.
pub fn long_command(
    system: &mut System,
    ctx: &mut JobCtx,
    (name, segments, specified, typed): (&str, Vec<Option<Vec<u8>>>, &[bool], &str),
) -> Result<crate::cpp::longcmd::Started, Box<stg::proto::CmdReply>> {
    let parms = command_parms(name, &segments, specified)?;
    let job = JobHandle(ctx.job.handle);
    let mut served = Served { system, ctx };
    let (r, prompt) =
        served.as_command(|s| crate::cpp::longcmd::prepare(s, name, &parms, typed, job));
    match r {
        Ok(long) => Ok((long, segments)),
        Err(mut reply) => {
            reply.prompt = prompt;
            served.end_command(&mut reply, segments);
            Err(reply)
        }
    }
}

/// A long command's end: `finish` run as one of the job's commands.
pub fn long_finish(
    system: &mut System,
    ctx: &mut JobCtx,
    finish: crate::cpp::longcmd::Finish,
    segments: Vec<Option<Vec<u8>>>,
) -> stg::proto::CmdReply {
    let mut served = Served { system, ctx };
    let (mut reply, prompt) = served.as_command(finish);
    reply.prompt = prompt;
    served.end_command(&mut reply, segments);
    reply
}

/// What [`enter`] changed, for [`leave`].
#[derive(Clone, Copy)]
pub struct Entered {
    /// The temporary ASP's root `QTEMP` the job's replaced.
    root: Option<ObjectId>,
    /// Whether the store was lent.
    lent: bool,
}

/// Set the service up for one of `ctx`'s requests: its `QTEMP` as the
/// temporary ASP's, names through its library list, the store lent to
/// its sources.
pub fn enter(system: &mut System, ctx: &JobCtx) -> Entered {
    let root = match (system.temp.as_mut(), ctx.qtemp) {
        (Some(t), Some(q)) => Some(std::mem::replace(&mut t.qtemp, q)),
        _ => None,
    };
    store::obj::ops::set_job_qtemp(ctx.qtemp);
    ile::set_libl(system.attrs.get(&ctx.job.number).map(|a| a.libl.clone()));
    let lent = ile::lend(system).is_ok();
    Entered { root, lent }
}

/// The job's embedded SQL session, made at its first statement, with
/// names through its library list as it is now; with the store lent.
pub fn sql_session<'c>(
    shared: &ile::Shared,
    locks: db::odp::SharedLocks,
    c: &'c mut JobCtx,
) -> Result<&'c mut sql::embed::Session, Box<sql::embed::WireReply>> {
    use sql::embed::{Reply, Session};
    let Ok((names, qsys)) = with_main(shared, |txn, qsys| Ok((ile::name_context(txn, qsys), qsys)))
    else {
        let e = sql::SqlError::Database {
            what: "the object store is not available".into(),
            msgid: None,
        };
        return Err(Box::new(
            Reply {
                sqlca: sql::Sqlca::from_error(&e),
                rows: Vec::new(),
                call: None,
            }
            .to_wire(),
        ));
    };
    let handle = JobHandle(c.job.handle);
    let s = c.sql.get_or_insert_with(|| {
        let mut ctx = sql::SqlContext::with_locks(qsys, handle, names.clone(), locks);
        ctx.host = Some(Rc::new(super::sqlhost::SqlHost { job: c.job.number }));
        Session::with_ctx(Box::new(MainStore::new(Rc::clone(shared))), ctx)
    });
    s.ctx.names = names;
    if s.ctx.dbmon.ident.is_none() {
        s.ctx.dbmon.ident = Some(sql::dbmon::JobIdent {
            name: c.job.name.clone(),
            user: c.job.user.clone(),
            number: c.job.number,
        });
    }
    s.ctx.qtemp = c.svc.qtemp.clone();
    s.ctx.registers.job_name = Some(super::sqlsrv::qualified(&c.job));
    s.ctx
        .registers
        .server_mode_job_name
        .clone_from(&c.sql_client);
    // The job's own thread can be asked to end a statement.
    if !c.thread {
        let flag = stg::sql_cancel_path(c.job.number);
        s.ctx.interrupt = Some(Rc::new(move || std::path::Path::new(&flag).exists()));
    }
    Ok(s)
}

/// Undo [`enter`]: the service as it is between requests.
pub fn leave(system: &mut System, e: Entered) {
    if e.lent {
        ile::restore(system);
    }
    ile::set_libl(None);
    if let (Some(t), Some(r)) = (system.temp.as_mut(), e.root) {
        t.qtemp = r;
    }
    store::obj::ops::set_job_qtemp(e.root.filter(|r| r.asp != 0));
}

impl Service for Served<'_> {
    fn hello(&mut self, _job: JobInfo) -> Result<(), String> {
        Err("this connection already said hello".into())
    }

    fn hello_thread(&mut self, _number: u32, _pid: u32) -> Result<(), String> {
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

    fn power_down(
        &mut self,
        immediate: bool,
        delay: u32,
        restart: bool,
        iplsrc: u8,
    ) -> Result<(), String> {
        // slic reads the options and powers down on SIGUSR1.
        std::fs::write(
            crate::ipl::power::REQUEST_FILE,
            format!(
                "{} {delay} {} {iplsrc}\n",
                u8::from(immediate),
                u8::from(restart)
            ),
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

    fn ifs(&mut self, op: stg::ifs::IfsOp) -> stg::ifs::IfsReply {
        let principal = self.ctx.session.borrow().principal.clone();
        super::ifs::serve(
            self.system,
            self.ctx.job.handle,
            self.ctx.job.number,
            &principal,
            op,
        )
    }

    fn host_signon(
        &mut self,
        user: String,
        proof: sec::signon::Substitute,
        token: Vec<u8>,
        swap: bool,
    ) -> Result<stg::HostSignedOn, stg::HostRefused> {
        super::hostsvr::signon(self.system, self.ctx, &user, &proof, &token, swap)
    }

    fn qap_signon(
        &mut self,
        user: String,
        password: String,
    ) -> Result<stg::QapSession, stg::QapRefused> {
        super::qap::signon(self.system, self.ctx, &user, &password)
    }

    fn qap_chgpwd(
        &mut self,
        user: String,
        old: String,
        new: String,
    ) -> Result<stg::QapSession, stg::QapRefused> {
        super::qap::chgpwd(self.system, self.ctx, &user, &old, &new)
    }

    fn qap_resume(&mut self, token: Vec<u8>) -> Result<stg::QapSession, stg::QapRefused> {
        super::qap::resume(self.system, self.ctx, &token)
    }

    fn qap_attach(&mut self, token: Vec<u8>) -> Result<stg::QapSession, stg::QapRefused> {
        super::qap::attach(self.system, self.ctx, &token)
    }

    fn qap_signoff(&mut self, token: Vec<u8>, operator: bool) {
        super::qap::signoff(self.system, self.ctx, &token, operator);
    }

    fn qap_ended(&mut self) -> Option<bool> {
        super::qap::ended(self.ctx)
    }

    fn qap_detach(&mut self, token: Vec<u8>) {
        super::qap::detach(self.ctx, &token);
    }

    fn qap_end_all(&mut self) {
        super::qap::end_all(self.ctx);
    }

    fn qap_pwd_status(&mut self) -> (Option<i64>, u16) {
        super::qap::pwd_status(self.system, self.ctx)
    }

    fn qap_ticket(&mut self) -> Result<(Vec<u8>, u32), stg::QapRefused> {
        super::qap::ticket(self.system, self.ctx)
    }

    fn qap_splf_list(
        &mut self,
        req: qap_proto::ops::spool::ListSpooledFiles,
    ) -> Result<qap_proto::ops::spool::SpooledFiles, stg::QapRefused> {
        super::qap::spool::list(self.system, self.ctx, &req)
    }

    fn qap_splf_read(
        &mut self,
        req: qap_proto::ops::spool::ReadSpooledFile,
    ) -> Result<qap_proto::ops::spool::SpooledData, stg::QapRefused> {
        super::qap::spool::read(self.system, self.ctx, &req)
    }

    fn qap_splf_hold(
        &mut self,
        id: qap_proto::ops::spool::SpooledFileId,
        immediately: bool,
    ) -> Result<(), stg::QapRefused> {
        super::qap::spool::hold(self.system, self.ctx, &id, immediately)
    }

    fn qap_splf_release(
        &mut self,
        id: qap_proto::ops::spool::SpooledFileId,
    ) -> Result<(), stg::QapRefused> {
        super::qap::spool::release(self.system, self.ctx, &id)
    }

    fn qap_splf_delete(
        &mut self,
        id: qap_proto::ops::spool::SpooledFileId,
    ) -> Result<(), stg::QapRefused> {
        super::qap::spool::delete(self.system, self.ctx, &id)
    }

    fn qap_splf_move(
        &mut self,
        id: qap_proto::ops::spool::SpooledFileId,
        outq: qap_proto::types::ObjectName,
        first: bool,
    ) -> Result<(), stg::QapRefused> {
        super::qap::spool::move_to(self.system, self.ctx, &id, &outq, first)
    }

    fn qap_outq_list(
        &mut self,
        req: qap_proto::ops::spool::ListOutputQueues,
    ) -> Result<qap_proto::ops::spool::OutputQueues, stg::QapRefused> {
        super::qap::spool::queues(self.system, self.ctx, &req)
    }

    fn qap_msg_list(
        &mut self,
        queue: qap_proto::ops::messages::Queue,
        newest_first: bool,
        start: u32,
        max: u32,
    ) -> Result<(u32, Vec<QMsg>), stg::QapRefused> {
        self.own_libl();
        let r = super::qap::messages::list(self.system, self.ctx, &queue, newest_first, start, max);
        ile::set_libl(None);
        r
    }

    fn qap_msg_reply(
        &mut self,
        queue: qap_proto::ops::messages::Queue,
        key: u32,
        reply: String,
    ) -> Result<(), stg::QapRefused> {
        self.own_libl();
        let r = super::qap::messages::reply(self.system, self.ctx, &queue, key, &reply);
        ile::set_libl(None);
        r
    }

    fn qap_msg_remove(
        &mut self,
        queue: qap_proto::ops::messages::Queue,
        keys: Vec<u32>,
    ) -> Result<(), stg::QapRefused> {
        self.own_libl();
        let r = super::qap::messages::remove(self.system, self.ctx, &queue, &keys);
        ile::set_libl(None);
        r
    }

    fn qap_msg_send(
        &mut self,
        to: qap_proto::ops::messages::Recipient,
        text: String,
        inquiry: bool,
    ) -> Result<Option<u32>, stg::QapRefused> {
        self.own_libl();
        let r = super::qap::messages::send(self.system, self.ctx, &to, &text, inquiry);
        ile::set_libl(None);
        r
    }

    fn qap_mgmt_ptfs(
        &mut self,
        product: String,
    ) -> Result<Vec<qap_proto::ops::manage::Ptf>, stg::QapRefused> {
        super::qap::manage::ptfs(self.system, self.ctx, &product)
    }

    fn qap_mgmt_disks(&mut self) -> Result<qap_proto::ops::manage::DiskUnits, stg::QapRefused> {
        super::qap::manage::disks(self.system, self.ctx)
    }

    fn qap_mgmt_ipl(&mut self) -> Result<qap_proto::ops::manage::IplStatus, stg::QapRefused> {
        super::qap::manage::ipl(self.system, self.ctx)
    }

    fn qap_mgmt_power(
        &mut self,
        req: qap_proto::ops::manage::PowerDown,
    ) -> Result<(), stg::QapRefused> {
        super::qap::manage::power(self.system, self.ctx, &req)
    }

    fn qap_dev_read(
        &mut self,
        member: qap_proto::types::MemberName,
        lock: bool,
    ) -> Result<qap_proto::ops::develop::MemberData, stg::QapRefused> {
        self.own_libl();
        let r = super::qap::develop::read(self.system, self.ctx, &member, lock);
        ile::set_libl(None);
        r
    }

    fn qap_dev_write(
        &mut self,
        member: qap_proto::types::MemberName,
        records: Vec<qap_proto::types::SourceRecord>,
        renumber: bool,
        srctype: Option<String>,
        text: Option<String>,
    ) -> Result<u32, stg::QapRefused> {
        self.own_libl();
        let r = super::qap::develop::write(
            self.system,
            self.ctx,
            &member,
            records,
            renumber,
            (srctype.as_deref(), text.as_deref()),
        );
        ile::set_libl(None);
        r
    }

    fn qap_dev_unlock(
        &mut self,
        member: qap_proto::types::MemberName,
    ) -> Result<(), stg::QapRefused> {
        self.own_libl();
        let r = super::qap::develop::unlock(self.system, self.ctx, &member);
        ile::set_libl(None);
        r
    }

    fn telnet_ticket(
        &mut self,
        user: String,
        ticket: Vec<u8>,
        device: String,
        tls: bool,
    ) -> Result<sec::signon::SignedOn, sec::signon::Refused> {
        super::qap::telnet_ticket(self.system, self.ctx, &user, &ticket, &device, tls)
    }

    fn telnet_verify(&mut self, user: String, proof: sec::signon::Substitute) -> bool {
        super::qap::telnet_verify(self.system, self.ctx, &user, &proof)
    }

    fn telnet_exit_signon(
        &mut self,
        user: String,
        device: String,
    ) -> Result<sec::signon::SignedOn, sec::signon::Refused> {
        super::qap::telnet_exit_signon(self.system, self.ctx, &user, &device)
    }

    fn host_token(
        &mut self,
        user: String,
        proof: sec::signon::Substitute,
        token: Vec<u8>,
        kind: u8,
        timeout: u32,
    ) -> Result<Vec<u8>, stg::HostRefused> {
        super::hostsvr::token(self.system, &user, &proof, &token, kind, timeout)
    }

    fn host_chgpwd(
        &mut self,
        user: String,
        proof: sec::signon::Substitute,
        new_protected: Vec<u8>,
        new_len: u32,
    ) -> Result<(), stg::HostRefused> {
        super::hostsvr::chgpwd(self.system, &user, &proof, &new_protected, new_len)
    }

    fn host_reset(&mut self) {
        super::hostsvr::reset(self.system, self.ctx);
    }

    fn regfac(&mut self, q: regfac::Query) -> Result<regfac::Answer, regfac::RegFail> {
        let me = self.ctx.session.borrow().principal.clone();
        super::regfac::api(self.system, Some(&me), q)
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
        let dspw = wm::job::Status::Dspw.byte();
        let was = self.system.jobs.get(me).is_some_and(|j| j.status == dspw);
        self.system
            .stats
            .status(me, was, status == dspw, runnable, granted);
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
        let r = msgs::reply(self.system, &lib, &name, key, &reply, from).map_err(|e| e.to_string());
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

    fn arb_tick(&mut self) -> Vec<String> {
        super::arb::tick(self.system)
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
        // A server job no monitor started (QTFTP00001, QTVTELNET) is told
        // of its own end here.
        let me = self.ctx.job.number;
        if !self.system.sbs.values().any(|a| a.monitor == me) {
            self.system.self_ending.insert(me);
            if let Some(i) = self.system.end_requests.remove(&me) {
                s.end_jobs.push((me, i));
            }
            // Its subsystem ending (or ended) ends it too.
            if s.ending.is_none()
                && let Some(i) = super::sbs::ends_with_subsystem(self.system, me)
            {
                s.ending = Some((i, 0));
            }
        }
        s
    }

    fn ws_end_requests(&mut self, jobs: Vec<u32>) -> Vec<(u32, bool)> {
        jobs.into_iter()
            .filter_map(|n| {
                self.system
                    .end_requests
                    .remove(&n)
                    .or_else(|| super::sbs::ends_with_subsystem(self.system, n))
                    .map(|i| (n, i))
            })
            .collect()
    }

    fn sbmjob(&mut self, req: wm::obj::jobq::SbmJob) -> Result<(u32, String), String> {
        self.own_libl();
        // USER(*CURRENT) is the profile the job runs under now.
        let current = self.ctx.session.borrow().principal.user.clone();
        let r = super::jobqs::sbmjob_as(self.system, self.ctx.job.number, &req, Some(&current));
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
        let j = if q.is_empty() || q == "*" {
            self.system.jobs.get(self.ctx.job.number)
        } else {
            self.system.jobs.lookup(&q, &self.ctx.job.user)
        }
        .cloned()
        .ok_or_else(|| format!("CPF1070 Job {q} not found."))?;
        let number = j.number;
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

    fn joblog_of(&mut self, number: u32) -> Result<Vec<wm::joblog::Entry>, String> {
        // `answer` serves other jobs' logs; only the caller's reaches here.
        if number == self.ctx.job.number {
            Ok(super::joblog::entries(self.ctx))
        } else {
            Err(format!("CPF1070 Job {number:06} is not active."))
        }
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
                let status = super::wtr::status(self.system, w);
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
        let (id, prtf, missing) = r?;
        // The queue was not found: QGPL/QPRINT, and the job log says so.
        self.joblog_put(super::prtf::missing_outq_entries(&missing));
        self.ctx.splfs.push(id);
        Ok((id, prtf))
    }

    fn stats_tick(&mut self) -> Vec<String> {
        if super::security::sysval_text(self.system, "QDBFSTCCOL").trim() == "*NONE" {
            return Vec::new();
        }
        let Some(st) = self.system.storage.as_mut() else {
            return Vec::new();
        };
        let Ok(mut txn) = st.store.begin() else {
            return Vec::new();
        };
        let lines = sql::collect_statistics(&mut txn, crate::ipl::system::now_unix());
        txn.abort();
        lines
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

    fn wtr_target(&mut self, name: String) -> Result<wm::wtr::WtrDest, String> {
        super::wtr::target(self.system, &name)
    }

    fn wtr_failed(
        &mut self,
        name: String,
        id: u32,
        msgid: String,
        text: String,
        hold: bool,
    ) -> Result<(), String> {
        super::wtr::failed(self.system, &name, id, &msgid, &text, hold)
    }

    fn wtr_device_error(
        &mut self,
        name: String,
        id: u32,
        lost: bool,
        text: String,
        diag: String,
    ) -> Result<(), String> {
        super::wtr::device_error(self.system, &name, id, lost, &text, &diag)
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
        self.system.current_job = self.ctx.job.number;
        super::sbs::end(self.system, &name, immediate, delay, None).map(|_| ())
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
    ) -> Result<(Vec<u8>, String, String), String> {
        let kind = if kind == 1 {
            PgmKind::SrvPgm
        } else {
            PgmKind::Pgm
        };
        self.lent(|c| {
            c.objects
                .program(&name, &lib, kind)
                .ok_or_else(|| c.objects.refusal().unwrap_or(mi::Cond::Mch3401).msgid())
        })
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

    fn io_commit(&mut self, rollback: bool, actgrp: String) -> Option<Result<(), IoFail>> {
        self.lent(|c| c.io.commit(rollback, &actgrp))
    }

    fn sql_run(
        &mut self,
        scope: String,
        desc: String,
        inputs: Vec<(u8, String)>,
    ) -> sql::embed::WireReply {
        use sql::embed::{Desc, Embedded, Reply, Request, from_wire};
        let Some(desc) = Desc::decode(&desc) else {
            let e = sql::SqlError::Unsupported {
                what: "statement".into(),
            };
            return Reply {
                sqlca: sql::Sqlca::from_error(&e),
                rows: Vec::new(),
                call: None,
            }
            .to_wire();
        };
        let shared = Rc::clone(&self.system.lent);
        let locks = self.system.locks.clone();
        let sysname = sql::dbmon::watching(self.ctx.job.number).then(|| self.system.system_name());
        self.lent(|c| match sql_session(&shared, locks, c) {
            Ok(s) => {
                if let Some(n) = sysname {
                    s.ctx.dbmon.system = n;
                }
                let req = Request {
                    scope,
                    desc,
                    inputs: inputs.iter().map(from_wire).collect(),
                };
                s.run(&req).to_wire()
            }
            Err(w) => *w,
        })
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
        cmd_lookup(self.system, self.ctx, &name, lib.as_deref())
    }

    /// Run at once, not on a fiber: no call can be made in its middle.
    fn run_cmd_calling(
        &mut self,
        name: String,
        segments: Vec<Option<Vec<u8>>>,
        specified: Vec<bool>,
        typed: String,
    ) -> stg::proto::CmdReply {
        self.run_cmd(name, segments, specified, typed)
    }

    fn run_cmd(
        &mut self,
        name: String,
        segments: Vec<Option<Vec<u8>>>,
        specified: Vec<bool>,
        typed: String,
    ) -> stg::proto::CmdReply {
        let parms = match command_parms(&name, &segments, &specified) {
            Ok(p) => p,
            Err(r) => return *r,
        };
        let (mut reply, prompt) = self.as_command(|s| crate::cpp::run(s, &name, &parms, &typed));
        reply.prompt = prompt;
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
        self.end_command(&mut reply, segments);
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

    // --- panel data: system and security (180-199)
    fn sysval_panel(&mut self, name: String) -> Option<(String, String, Vec<String>)> {
        super::sysdata::sysval(self.system, &name)
    }

    fn sysval_panel_list(&mut self) -> Vec<(String, String, String)> {
        super::sysdata::sysvals(self.system)
    }

    fn neta_panel(&mut self) -> Result<Vec<(String, String)>, String> {
        super::sysdata::neta(self.system)
    }

    fn usrprf_panel(&mut self, name: String) -> Result<Vec<(String, String)>, String> {
        super::sysdata::usrprf(self.system, &name)
    }

    fn usrprf_panel_list(&mut self) -> Result<Vec<(String, String, String)>, String> {
        super::sysdata::usrprfs(self.system)
    }

    fn autusr_panel(&mut self, by_group: bool) -> Result<Vec<Vec<String>>, String> {
        super::sysdata::autusr(self.system, by_group)
    }

    fn sfwrsc_panel(&mut self) -> Vec<Vec<String>> {
        crate::cpp::lpcmd::sfwrsc_rows(self.system)
    }

    fn autl_panel(&mut self, name: String) -> Result<Vec<Vec<String>>, String> {
        super::sysdata::autl(self.system, &name)
    }

    fn autl_panel_list(&mut self) -> Result<Vec<(String, String)>, String> {
        super::sysdata::autls(self.system)
    }

    fn secaud_panel(&mut self) -> Result<Vec<(String, String)>, String> {
        super::sysdata::secaud(self.system)
    }

    fn cfgsts_panel(&mut self) -> Result<Vec<Vec<String>>, String> {
        super::sysdata::cfgsts(self.system)
    }

    fn devd_panel(&mut self, name: String) -> Result<Vec<(String, String)>, String> {
        super::sysdata::devd(self.system, &name)
    }

    // --- panel data: messages and objects (160-179)

    fn obj_detail(
        &mut self,
        lib: String,
        name: String,
        ty: String,
    ) -> Option<Vec<(String, String)>> {
        self.own_libl();
        let r = panel_objects::detail(self.system, &lib, &name, &ty);
        ile::set_libl(None);
        r
    }

    fn obj_sizes(
        &mut self,
        lib: String,
        name: String,
        ty: String,
    ) -> Vec<(String, String, String, u64)> {
        self.own_libl();
        let r = panel_objects::sizes(self.system, &lib, &name, &ty);
        ile::set_libl(None);
        r
    }

    fn rpyl_entries(&mut self) -> Result<Vec<stg::proto::RpyRow>, String> {
        let s = self
            .system
            .storage
            .as_mut()
            .ok_or("object store not open")?;
        let qsys = s.qsys.ok_or("object store not initialized")?;
        let mut txn = s.store.begin().map_err(|e| e.to_string())?;
        let list = wm::obj::rpyl::load(&mut txn, qsys).map_err(|e| e.to_string());
        txn.abort();
        Ok(list?
            .entries
            .into_iter()
            .map(|e| (e.seq, e.msgid, e.cmpdta, e.start, e.reply, e.dump))
            .collect())
    }

    fn hst_entries(&mut self) -> Result<Vec<Vec<String>>, String> {
        Ok(super::hist::read_all(self.system)?
            .into_iter()
            .map(|r| {
                vec![
                    r.date, r.time, r.msgid, r.severity, r.kind, r.job.0, r.job.1, r.job.2, r.text,
                ]
            })
            .collect())
    }

    fn msgf_list(&mut self, msgf: String, lib: String) -> Result<Vec<(String, String)>, String> {
        self.own_libl();
        let r = panel_objects::messages(self.system, &msgf, &lib);
        ile::set_libl(None);
        r
    }

    // --- system displays (200-219)

    fn grp_chg(&mut self, name: String, text: String) -> Result<u32, String> {
        super::sysdsp::grp_chg(self.ctx.job.number, &name, &text)
    }

    fn grp_info(&mut self) -> Option<stg::proto::GroupInfo> {
        super::sysdsp::grp_info(self.system, self.ctx.job.number)
    }

    fn grp_join(
        &mut self,
        group: u32,
        name: String,
        text: String,
        number: u32,
    ) -> Result<(), String> {
        super::sysdsp::grp_join(group, &name, &text, number)
    }

    fn grp_leave(&mut self, number: u32) {
        super::sysdsp::grp_leave(number);
    }

    fn gda_read(&mut self) -> Result<Vec<u8>, String> {
        super::sysdsp::gda_read(self.ctx.job.number)
    }

    fn gda_write(&mut self, start: u32, bytes: Vec<u8>) -> Result<(), String> {
        super::sysdsp::gda_write(self.ctx.job.number, start, &bytes)
    }

    fn ws_set(&mut self, device: String, jobs: Vec<stg::proto::WsJob>) {
        super::sysdsp::ws_set(&device, jobs);
    }

    fn ws_get(&mut self, device: String) -> Vec<stg::proto::WsJob> {
        super::sysdsp::ws_get(self.system, &device)
    }

    // --- UIM APIs (220-239)

    fn schidx_entries(
        &mut self,
        name: String,
        lib: String,
    ) -> Result<Vec<stg::proto::SchidxEntry>, String> {
        self.own_libl();
        let r = super::uiapi::schidx_entries(self.system, &name, &lib);
        ile::set_libl(None);
        r
    }

    fn schidx_create(
        &mut self,
        name: String,
        lib: String,
        text: String,
        replace: bool,
    ) -> Result<String, String> {
        self.own_libl();
        let owner = self.ctx.job.user.clone();
        let r = super::uiapi::schidx_create(self.system, (&name, &lib), &text, replace, &owner);
        ile::set_libl(None);
        r
    }

    fn schidx_add(
        &mut self,
        name: String,
        lib: String,
        pnlgrp: String,
        pnlgrp_lib: String,
    ) -> Result<String, String> {
        self.own_libl();
        let r = super::uiapi::schidx_add(self.system, (&name, &lib), (&pnlgrp, &pnlgrp_lib));
        ile::set_libl(None);
        r
    }

    fn schidx_remove(
        &mut self,
        name: String,
        lib: String,
        pnlgrp: String,
    ) -> Result<String, String> {
        self.own_libl();
        let r = super::uiapi::schidx_remove(self.system, (&name, &lib), &pnlgrp);
        ile::set_libl(None);
        r
    }

    // --- panels batch 2: work, spool, journals (240-269)

    fn panel2_data(
        &mut self,
        what: String,
        args: Vec<String>,
    ) -> Result<stg::proto::PanelData, String> {
        let root = self.own_qtemp();
        self.system.current_job = self.ctx.job.number;
        self.own_libl();
        std::mem::swap(&mut self.system.svc, &mut self.ctx.svc);
        let r = super::work2::data(self.system, &self.ctx.job, &what, &args);
        std::mem::swap(&mut self.system.svc, &mut self.ctx.svc);
        ile::set_libl(None);
        self.root_qtemp(root);
        r
    }

    // --- panels batch 2: programs, config, security, editors (270-299)
    fn sys2_panel(
        &mut self,
        what: String,
        args: Vec<String>,
    ) -> Result<stg::proto::Sys2Data, String> {
        self.own_libl();
        let r = super::system2::panel(self.system, &what, &args);
        svc::ile::set_libl(None);
        r
    }

    // --- SEU (300-319)

    fn seu_open(
        &mut self,
        lib: String,
        file: String,
        member: String,
        edit: bool,
    ) -> Result<stg::proto::SrcMember, String> {
        self.own_libl();
        let job = JobHandle(self.ctx.job.handle);
        let r = super::seu::open(self.system, job, (&lib, &file, &member), edit);
        ile::set_libl(None);
        r
    }

    fn seu_save(
        &mut self,
        lib: String,
        file: String,
        member: String,
        records: Vec<(u32, u32, String)>,
        srctype: String,
        text: String,
    ) -> Result<(), String> {
        self.own_libl();
        let job = JobHandle(self.ctx.job.handle);
        let r = super::seu::save(
            self.system,
            job,
            (&lib, &file, &member),
            &records,
            &srctype,
            &text,
        );
        ile::set_libl(None);
        r
    }

    fn seu_close(&mut self, lib: String, file: String, member: String) {
        self.own_libl();
        let job = JobHandle(self.ctx.job.handle);
        super::seu::close(self.system, job, (&lib, &file, &member));
        ile::set_libl(None);
    }

    // --- PDM (320-339)
    fn pdm_read(
        &mut self,
        lib: String,
        file: String,
        member: String,
    ) -> Result<Option<Vec<String>>, String> {
        self.own_libl();
        let r = super::pdm::read(self.system, &lib, &file, &member);
        ile::set_libl(None);
        r
    }

    fn pdm_write(
        &mut self,
        lib: String,
        file: String,
        member: String,
        lines: Vec<String>,
        srctype: String,
    ) -> Result<(), String> {
        self.own_libl();
        let r = super::pdm::write(self.system, &lib, &file, &member, &lines, &srctype);
        ile::set_libl(None);
        r
    }

    fn pdm_options(
        &mut self,
        lib: String,
        file: String,
        member: String,
    ) -> Result<Option<Vec<String>>, String> {
        self.own_libl();
        let r = super::pdm::read_options(self.system, &lib, &file, &member);
        ile::set_libl(None);
        r
    }

    fn pdm_options_put(
        &mut self,
        lib: String,
        file: String,
        member: String,
        lines: Vec<String>,
    ) -> Result<(), String> {
        self.own_libl();
        let r = super::pdm::write_options(self.system, &lib, &file, &member, &lines);
        ile::set_libl(None);
        r
    }

    fn pdm_defaults(&mut self) -> String {
        super::pdm::defaults(self.system)
    }

    fn pdm_defaults_put(&mut self, text: String) -> Result<(), String> {
        super::pdm::put_defaults(self.system, &text)
    }

    // --- SDA (340-359)

    // --- DFU and RLU (360-379)

    fn dfu_keys(
        &mut self,
        lib: String,
        file: String,
    ) -> Result<Vec<(String, Vec<String>)>, String> {
        super::dfu::keys(self.system, &lib, &file)
    }

    fn dfu_pgm(&mut self, lib: String, name: String) -> Result<Option<(String, String)>, String> {
        super::dfu::get(self.system, &lib, &name)
    }

    fn dfu_pgm_put(
        &mut self,
        lib: String,
        name: String,
        def: String,
        text: String,
    ) -> Result<String, String> {
        super::dfu::put(self.system, &lib, &name, &def, &text)
    }

    fn dfu_pgm_delete(&mut self, lib: String, name: String) -> Result<(), String> {
        super::dfu::delete(self.system, &lib, &name)
    }

    // --- STRDBG screens (380-399)
    fn dbg_program(&mut self, lib: String, name: String) -> Result<stg::proto::DbgProgram, String> {
        self.own_libl();
        let r = super::dbg::program(self.system, &lib, &name);
        ile::set_libl(None);
        r
    }

    fn srvjob_start(&mut self, job: String) -> Result<String, String> {
        let special = self.ctx.session.borrow().principal.special_adopted();
        super::dbg::start(self.system, &self.ctx.job, special, &job)
    }

    fn srvjob_end(&mut self) -> Result<String, String> {
        super::dbg::end(self.ctx.job.number)
    }

    fn srvjob_serving(&mut self) -> String {
        super::dbg::serving(self.system, self.ctx.job.number)
    }

    fn srvjob_order(&mut self, order: stg::proto::DbgOrder) -> Result<(), String> {
        super::dbg::order(self.ctx.job.number, order)
    }

    fn srvjob_orders(&mut self) -> Vec<stg::proto::DbgOrder> {
        super::dbg::orders(self.ctx.job.number)
    }

    fn srvjob_event(&mut self, event: stg::proto::DbgEvent) -> Result<(), String> {
        super::dbg::event(self.ctx.job.number, event)
    }

    fn srvjob_events(&mut self) -> Vec<stg::proto::DbgEvent> {
        super::dbg::events(self.ctx.job.number)
    }

    fn srvent_set(&mut self, lib: String, name: String, user: String) -> Result<String, String> {
        let (special, me_user) = {
            let s = self.ctx.session.borrow();
            (s.principal.special_adopted(), s.principal.user.clone())
        };
        self.own_libl();
        let r = super::dbg::entry_set(
            self.system,
            self.ctx.job.number,
            &me_user,
            special,
            &lib,
            &name,
            &user,
        );
        ile::set_libl(None);
        r
    }

    fn srvent_remove(&mut self, lib: String, name: String) -> Result<(), String> {
        self.own_libl();
        let r = super::dbg::entry_remove(self.system, self.ctx.job.number, &lib, &name);
        ile::set_libl(None);
        r
    }

    fn srvent_list(&mut self) -> Vec<String> {
        let user = self.ctx.session.borrow().principal.user.clone();
        super::dbg::entry_list(self.ctx.job.number, &user)
    }

    fn srvent_hit(&mut self, lib: String, name: String) -> String {
        let user = self.ctx.session.borrow().principal.user.clone();
        self.own_libl();
        let job = self.ctx.job.clone();
        let r = super::dbg::entry_hit(self.system, &job, &user, &lib, &name);
        ile::set_libl(None);
        r
    }

    // --- STRSQL (400-419)
    fn isql_start(&mut self, naming: String, commit: String) -> Result<bool, String> {
        let root = self.own_qtemp();
        self.own_libl();
        let user = self.ctx.job.user.clone();
        let r = super::isql::in_job(self.system, &mut self.ctx.svc, |s| {
            super::isql::start(s, &user, &naming, &commit)
        });
        ile::set_libl(None);
        self.root_qtemp(root);
        r
    }

    fn isql_run(
        &mut self,
        text: String,
        naming: String,
        check: bool,
        max_rows: u32,
    ) -> stg::proto::IsqlReply {
        let root = self.own_qtemp();
        self.system.current_job = self.ctx.job.number;
        self.own_libl();
        let r = super::isql::in_job(self.system, &mut self.ctx.svc, |s| {
            super::isql::run(s, &text, &naming, check, max_rows)
        });
        ile::set_libl(None);
        self.root_qtemp(root);
        r
    }

    fn isql_end(&mut self, commit: bool) -> Result<u64, String> {
        let root = self.own_qtemp();
        self.own_libl();
        let r = super::isql::in_job(self.system, &mut self.ctx.svc, |s| {
            super::isql::end(s, commit)
        });
        ile::set_libl(None);
        self.root_qtemp(root);
        r
    }

    fn isql_session(&mut self) -> Option<String> {
        let user = self.ctx.session.borrow().principal.user.clone();
        super::isql::session(self.system, &user)
    }

    fn isql_session_put(&mut self, text: Option<String>) -> Result<(), String> {
        let user = self.ctx.session.borrow().principal.user.clone();
        super::isql::session_put(self.system, &user, text.as_deref())
    }

    // --- STRSST (420-439)

    fn sst_sign_on(
        &mut self,
        user: String,
        password: String,
    ) -> Result<svctools::Privilege, svctools::users::SignOnError> {
        super::sst::sign_on(self.system, self.ctx.job.number, &user, &password)
    }

    fn sst_change_password(
        &mut self,
        user: String,
        old: String,
        new: String,
    ) -> Result<svctools::Privilege, String> {
        super::sst::change_password(self.system, self.ctx.job.number, &user, &old, &new)
    }

    fn sst_sign_off(&mut self) {
        super::sst::sign_off(self.ctx.job.number);
    }

    fn sst_disks(
        &mut self,
    ) -> Result<
        (
            Vec<svctools::env::DiskUnit>,
            Vec<svctools::env::NonConfigured>,
        ),
        String,
    > {
        super::sst::disks(self.system, self.ctx.job.number)
    }

    fn sst_disk_change(&mut self, op: String, unit: u16, device: String) -> Result<u16, String> {
        super::sst::disk_change(self.system, self.ctx.job.number, &op, unit, &device)
    }

    fn sst_parity(&mut self) -> Result<Vec<svctools::env::ParityUnit>, String> {
        super::sst::parity(self.system, self.ctx.job.number)
    }

    fn sst_parity_change(
        &mut self,
        op: String,
        level: u8,
        set: u8,
        units: Vec<u16>,
    ) -> Result<String, String> {
        super::sst::parity_change(self.system, self.ctx.job.number, &op, level, set, &units)
    }

    fn sst_disk_facts(&mut self) -> Result<(u8, Vec<String>), String> {
        super::sst::disk_facts(self.system, self.ctx.job.number)
    }

    fn sst_logs(&mut self) -> Result<(Vec<svctools::env::LogEntry>, Vec<String>), String> {
        super::sst::logs(self.system, self.ctx.job.number)
    }

    fn sst_resources(&mut self) -> Result<(Vec<svctools::env::ResourceRow>, u32), String> {
        super::sst::resources(self.system, self.ctx.job.number)
    }

    fn sst_remove_resource(&mut self, name: String) -> Result<(), String> {
        super::sst::remove_resource(self.system, self.ctx.job.number, &name)
    }

    fn sst_storage(&mut self, unit: u16, page: u64, id: String) -> Result<Vec<u8>, String> {
        super::sst::storage(self.system, self.ctx.job.number, unit, page, &id)
    }

    fn sst_security(&mut self) -> Result<u8, String> {
        super::sst::security(self.system, self.ctx.job.number)
    }

    fn sst_set_security(&mut self, flags: u8) -> Result<(), String> {
        super::sst::set_security(self.system, self.ctx.job.number, flags)
    }

    fn sst_users(&mut self) -> Result<Vec<u8>, String> {
        super::sst::users(self.system, self.ctx.job.number)
    }

    fn splf_fetch(
        &mut self,
        file: String,
        job: String,
        splnbr: String,
    ) -> Result<(wm::obj::splf::SplfAttrs, Vec<u8>), String> {
        let me = self.ctx.job.number;
        super::splcmd::fetch(self.system, me, &file, &job, &splnbr)
    }

    fn smtp_relay_auth(&mut self, host: String) -> Option<(String, String)> {
        crate::cpp::tcpcmd::relay_auth(self.system, &host)
    }

    fn tcp(&mut self, op: stg::tcp::TcpOp) -> stg::tcp::TcpReply {
        super::tcp::request(self.system, op)
    }

    fn qsh(&mut self, op: stg::qsh::QshOp) -> stg::qsh::QshReply {
        let me = self.ctx.session.borrow().principal.clone();
        super::qsh::request(self.system, &me, op)
    }

    fn licpgm_panel(
        &mut self,
        what: String,
        args: Vec<String>,
    ) -> Result<Vec<Vec<String>>, String> {
        crate::cpp::lpapi::panel(self.system, &what, &args)
    }

    fn licpgm_api(
        &mut self,
        api: String,
        parms: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, (String, Vec<u8>)> {
        crate::cpp::lpapi::call(self.system, &api, &parms)
    }

    fn service_op(&mut self, op: String, args: Vec<String>) -> Result<Vec<String>, String> {
        self.system.current_job = self.ctx.job.number;
        super::svcop::run(self.system, &self.ctx.job, &op, &args)
    }

    // --- hardware resources (1420-1429)
    fn hw_table(&mut self, refresh: bool) -> Result<hwrsrc::Table, String> {
        hwrsrc::client::table(refresh)
            .map_err(|e| format!("CPF9898 hardware resource service: {e}"))
    }

    fn hw_cfgds(&mut self, name: String) -> Vec<(String, String)> {
        self.system.current_job = self.ctx.job.number;
        super::system2::cfg_descriptions(self.system, &name)
    }

    fn ddm_signon(
        &mut self,
        user: String,
        password: String,
        swap: bool,
    ) -> Result<stg::HostSignedOn, stg::HostRefused> {
        super::ddm::signon(self.system, self.ctx, &user, &password, swap)
    }

    fn ddmf_put(
        &mut self,
        lib: String,
        name: String,
        payload: String,
        text: String,
        create: bool,
        replace: bool,
    ) -> Result<String, String> {
        let me = self.ctx.job.number;
        super::ddm::put(
            self.system,
            me,
            &lib,
            &name,
            &payload,
            &text,
            create,
            replace,
        )
    }

    fn ddmf_get(&mut self, lib: String, name: String) -> Result<(String, String), String> {
        super::ddm::get(self.system, &lib, &name)
    }

    fn svraute(
        &mut self,
        op: String,
        user: String,
        server: String,
        usrid: Option<String>,
        password: Option<String>,
    ) -> Result<String, String> {
        super::ddm::svraute(self.system, self.ctx, &op, &user, &server, usrid, password)
    }

    fn svraute_get(&mut self, server: String) -> Option<(String, String)> {
        super::ddm::svraute_get(self.system, self.ctx, &server)
    }

    fn acgcde(&mut self) -> String {
        super::acg::code(self.system, self.ctx.job.number)
    }

    fn sqlsrv_ticket(&mut self) -> Vec<u8> {
        super::sqlsrv::ticket(self.ctx)
    }

    fn sqlsrv_assume(
        &mut self,
        ticket: Vec<u8>,
        user: String,
        password: String,
    ) -> Result<String, stg::HostRefused> {
        super::sqlsrv::assume(self.system, self.ctx, &ticket, &user, &password)
    }

    fn sqlsrv_end(&mut self) {
        super::sqlsrv::end(self.system, self.ctx);
    }

    fn machine(&mut self) -> stg::proto::MachineInfo {
        let dmi = machine_dmi();
        stg::proto::MachineInfo {
            serial: super::security::sysval_text(self.system, "QSRLNBR")
                .trim()
                .to_string(),
            model: super::security::sysval_text(self.system, "QMODEL")
                .trim()
                .to_string(),
            hw_serial: dmi.serial().unwrap_or_default().to_string(),
            hw_model: dmi.model(),
        }
    }

    fn sst_user_change(
        &mut self,
        op: String,
        name: String,
        password: String,
        text: String,
        privileges: svctools::Privilege,
    ) -> Result<(), String> {
        super::sst::user_change(
            self.system,
            self.ctx.job.number,
            &op,
            &name,
            &password,
            &text,
            privileges,
        )
    }

    // --- certificates (1240-1249)

    fn cert_app(&mut self, app_id: String) -> Result<stg::proto::CertTrust, String> {
        let principal = self.ctx.session.borrow().principal.clone();
        super::certs::cert_app(self.system, &principal, &app_id)
    }

    fn cert_panel(
        &mut self,
        what: String,
        args: Vec<String>,
    ) -> Result<stg::proto::Sys2Data, String> {
        let principal = self.ctx.session.borrow().principal.clone();
        super::certs::panel(self.system, &principal, &what, &args)
    }

    fn cert_expiry(&mut self) -> Vec<String> {
        super::certs::expiry_scan(self.system)
    }

    fn upd_tick(&mut self) -> Vec<String> {
        crate::cpp::updcmd::tick(self.system, crate::ipl::system::now_unix())
    }

    fn web_audit(&mut self, kind: String, user: String, a: String, b: String) {
        super::webadm::audit(&self.ctx.job.user, &self.ctx.job.name, &kind, &user, &a, &b);
    }

    fn web_joblog(&mut self) -> Vec<String> {
        let mut all: Vec<&wm::joblog::Entry> = self.ctx.joblog_wrapped.iter().collect();
        all.extend(self.ctx.joblog.entries());
        wm::joblog::display(&all)
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn web_store(
        &mut self,
        index: String,
        who: String,
        put: Option<Vec<(String, String)>>,
    ) -> Result<Vec<(String, String)>, String> {
        let job = (self.ctx.job.name.clone(), self.ctx.job.user.clone());
        super::afwstore::store(self.system, (&job.0, &job.1), &index, &who, put)
    }

    fn web_certs(&mut self, call: Vec<u8>) -> Vec<u8> {
        let me = self.ctx.session.borrow().principal.clone();
        super::webadm::certs(self.system, &me, &call)
    }

    // --- communications objects (1180-1189)

    fn cmn_panel(
        &mut self,
        what: String,
        args: Vec<String>,
    ) -> Result<stg::proto::Sys2Data, String> {
        super::cmn::panel(self.system, &what, &args)
    }

    fn cfg_status(&mut self, name: String, ty: String) -> Option<(u16, String)> {
        super::cmn::cfg_status(self.system, &name, &ty)
    }
}

/// The store reads behind the message and object panels.
mod panel_objects {
    use store::obj::header::{Domain, text_of};
    use store::obj::{ObjectId, ops, types};
    use store::txn::Txn;

    use crate::ipl::system::System;
    use crate::svc::ile::name_context;

    /// Unix seconds as `CYYMMDDHHMMSS`, blank for none.
    fn stamp(unix: u64) -> String {
        if unix == 0 {
            return String::new();
        }
        sysapi::picture::render(sysapi::Civil::of_unix(unix), "YYYYMMDDHHMISS")
            .map(|s| {
                let c = if unix >= 946_684_800 { "1" } else { "0" };
                format!("{c}{}", &s[2..])
            })
            .unwrap_or_default()
    }

    /// Run `f` in a read-only transaction over the main store.
    fn read<T>(
        system: &mut System,
        f: impl FnOnce(&mut Txn<'_>, ObjectId) -> Option<T>,
    ) -> Option<T> {
        let s = system.storage.as_mut()?;
        let qsys = s.qsys?;
        let mut txn = s.store.begin().ok()?;
        let r = f(&mut txn, qsys);
        txn.abort();
        r
    }

    /// The libraries `lib` names: one, or the library list.
    fn libraries(txn: &mut Txn<'_>, qsys: ObjectId, lib: &str) -> Vec<(String, ObjectId)> {
        match lib {
            "*LIBL" | "*ALL" | "" => name_context(txn, qsys)
                .libl
                .iter()
                .filter_map(|id| {
                    ops::describe(txn, *id)
                        .ok()
                        .map(|h| (h.name.to_string(), *id))
                })
                .collect(),
            l => qsname::Name::parse(l)
                .ok()
                .and_then(|n| ops::find_library(txn, qsys, n).ok())
                .map(|f| vec![(l.to_string(), f.id)])
                .unwrap_or_default(),
        }
    }

    fn matches(name: &str, pattern: &str) -> bool {
        match pattern.strip_suffix('*') {
            _ if pattern == "*ALL" => true,
            Some(p) => name.starts_with(p),
            None => name == pattern,
        }
    }

    /// `obj_detail`.
    pub fn detail(
        system: &mut System,
        lib: &str,
        name: &str,
        ty: &str,
    ) -> Option<Vec<(String, String)>> {
        let code = types::type_by_name(ty)?.code;
        read(system, |txn, qsys| {
            let names = name_context(txn, qsys);
            let id = wm::obj::find(txn, qsys, &names, lib, name, code).ok()?;
            let h = ops::describe(txn, id).ok()?;
            let none = |s: String| if s.is_empty() { "*NONE".to_string() } else { s };
            let mut lib_attrs = Vec::new();
            if code == types::LIB
                && let Ok(l) = store::obj::library::Library::load(txn, id)
            {
                let kind = match l.attr.lib_type {
                    store::obj::LibType::Prod => "PROD",
                    store::obj::LibType::Test => "TEST",
                };
                lib_attrs = vec![
                    ("LIBTYPE".to_string(), kind.to_string()),
                    ("CRTAUT".to_string(), text_of(&l.attr.create_authority)),
                    ("ASPNUM".to_string(), l.attr.asp_number.to_string()),
                    ("OBJECTS".to_string(), l.attr.object_count.to_string()),
                ];
            }
            Some(
                lib_attrs
                    .into_iter()
                    .chain([
                        ("CREATOR".into(), text_of(&h.creator)),
                        ("SYSTEM".into(), text_of(&h.system)),
                        ("RELEASE".into(), h.release.to_string()),
                        (
                            "SIZE".into(),
                            (h.size_pages * store::page::PAGE_SIZE as u64).to_string(),
                        ),
                        (
                            "DOMAIN".into(),
                            match h.domain {
                                Domain::System => "*SYSTEM".into(),
                                Domain::User => "*USER".into(),
                            },
                        ),
                        (
                            "DAMAGED".into(),
                            if h.damaged { "YES" } else { "NO" }.into(),
                        ),
                        ("PGP".into(), none(text_of(&h.primary_group))),
                        ("AUDIT".into(), none(text_of(&h.audit))),
                        ("LASTUSED".into(), stamp(h.last_used)),
                        ("DAYSUSED".into(), h.days_used.to_string()),
                        ("RESET".into(), stamp(h.reset_date)),
                    ])
                    .collect(),
            )
        })
    }

    /// `obj_sizes`.
    pub fn sizes(
        system: &mut System,
        lib: &str,
        name: &str,
        ty: &str,
    ) -> Vec<(String, String, String, u64)> {
        let wanted = if ty == "*ALL" {
            None
        } else {
            types::type_by_name(ty).map(|t| t.code)
        };
        read(system, |txn, qsys| {
            let mut out = Vec::new();
            for (lname, lid) in libraries(txn, qsys, lib) {
                for (key, id) in ops::list(txn, lid, wanted).ok()? {
                    let n = key.name.to_string();
                    if !matches(&n, name) {
                        continue;
                    }
                    let size = ops::describe(txn, id)
                        .map_or(0, |h| h.size_pages * store::page::PAGE_SIZE as u64);
                    let t = types::type_by_code(key.type_code)
                        .map_or_else(String::new, |t| t.name.to_string());
                    out.push((lname.clone(), n, t, size));
                }
            }
            Some(out)
        })
        .unwrap_or_default()
    }

    /// `msgf_list`.
    pub fn messages(
        system: &mut System,
        msgf: &str,
        lib: &str,
    ) -> Result<Vec<(String, String)>, String> {
        read(system, |txn, qsys| {
            let names = name_context(txn, qsys);
            let id = wm::obj::find(txn, qsys, &names, lib, msgf, types::MSGF).ok()?;
            let f = msgf::store::load(txn, id).ok()?;
            Some(f.msgs.into_iter().map(|d| (d.id, d.text)).collect())
        })
        .ok_or_else(|| format!("CPF2407 message file {lib}/{msgf} not found"))
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

/// The firmware's identity, read once: it does not change while up.
fn machine_dmi() -> &'static hwrsrc::dmi::Dmi {
    static DMI: std::sync::OnceLock<hwrsrc::dmi::Dmi> = std::sync::OnceLock::new();
    DMI.get_or_init(hwrsrc::dmi::Dmi::read)
}
