// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One connection and the job behind it: the job's sources over the
//! lent store, its open files and console state, and the answers to its
//! requests.

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
use store::obj::{JobHandle, ObjectId, ops, types};
use sysapi::{
    ApiFail, CommitInfo, DataArea, DepInfo, DtaqEntry, DtaqInfo, FileInfo, FormatInfo, IdxAttr,
    LocalNow, MemberInfo, ObjInfo, Provider, SysVal, UserInfo,
};

use crate::ipl::system::System;
use crate::svc;
use crate::svc::ile::{
    self, MainStore, StoreData, StoreMsgs, StoreObjects, StoreProvider, StoreUserSpaces,
    with_main,
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
    objects: StoreObjects,
    spaces: StoreUserSpaces,
    msgs: StoreMsgs,
    data: StoreData,
    io: StoreIo,
    provider: StoreProvider,
    /// Console state: modes, transfer buffers, the SQL context.
    pub svc: svc::db::State,
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
            job,
        }
    }
}

/// Answer one request on connection `i`.
pub fn handle(system: &mut System, conns: &mut [Conn], i: usize, req: Request) -> Vec<u8> {
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
    let mut served = Served { system, ctx };
    stg::dispatch(&mut served, req)
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
    let attrs = super::jobs::resolve_attrs(system, &job.jobd, &job.class);
    system.attrs.insert(job.number, attrs);
    system.jobs.add(job.clone());
    let mut ctx = JobCtx::new(system, job);
    ctx.qtemp = Some(qtemp);
    Ok(ctx)
}

/// A job's connection closed: whatever it held goes.
pub fn vanished(system: &mut System, conn: Conn) {
    let Some(mut ctx) = conn.ctx else {
        return;
    };
    end_job(system, &mut ctx);
}

/// End a job: its database use, its `QTEMP`, its table entry.
fn end_job(system: &mut System, ctx: &mut JobCtx) {
    end_db(system, ctx);
    if let Some(q) = ctx.qtemp.take() {
        super::jobs::drop_qtemp(system, ctx.job.number, q);
    }
    system.jobs.remove(ctx.job.number);
    system.attrs.remove(&ctx.job.number);
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
            end_job(system, ctx);
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
        ile::set_libl(None);
        self.root_qtemp(root);
        let srcs = std::mem::take(&mut self.system.raised)
            .into_iter()
            .map(|s| s.code.to_string())
            .collect();
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

    fn job_number(&mut self) -> Result<u32, String> {
        super::jobs::next(self.system)
    }

    fn set_status(&mut self, status: u8, function: String) {
        if let Some(j) = self.system.jobs.get_mut(self.ctx.job.number) {
            j.status = status;
            j.function = function;
        }
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

    fn program(&mut self, name: String, lib: String, kind: u8) -> Option<(Vec<u8>, String, String)> {
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
