// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Requests 1560-1567 (docs/DIRSRV.md §4): the instance job's
//! connections as sessions, each with its bound identity, its search
//! cursors and its persistent searches.
//!
//! Only the instance's job asks: user `QDIRSRV`, job name the instance's
//! (`CPF2189` otherwise), and a session answers only the job that opened
//! it. Each request is one store transaction: a bind's is committed
//! whatever its result, an operation's when it succeeded (then its
//! changes reach the persistent searches), a search's always (it only
//! reads). The instance stays open while its job runs; when the job
//! ends its sessions and the instance go, so the next start reads the
//! configuration again.

use std::cell::RefCell;
use std::collections::HashMap;

use dirsrv::config::InstanceList;
use dirsrv::ops::{Budget, Conn, Cursors, PageState, Reply, Session, Started};
use ldap::msg::LdapMessage;
use stg::JobInfo;
use stg::dir::{
    DirAnswer, DirBudget, DirChanges, DirClose, DirConn, DirConns, DirError, DirOpened,
    DirSettings, DirState,
};
use store::obj::ObjectId;
use store::txn::Txn;

use super::hooks::BindCtx;
use super::instance::{Host, Instance, USER, dir_msg, failed, no_store};
use crate::ipl::system::{System, now_unix};

/// The certificate store's application ID of the server.
pub const APP_ID: &str = "QS400_DIRECTORY_SERVER";

/// The most entries one answer carries.
const MAX_ENTRIES: u32 = 1000;
/// The most bytes of entries one answer carries.
const MAX_BYTES: u32 = 4 << 20;
/// The largest request the job takes.
const MAX_MESSAGE: u32 = 16 << 20;

/// One connection.
struct Sess {
    instance: String,
    job: u32,
    conn: DirConn,
    session: Session,
    cursors: Cursors,
    /// Each open cursor's search message ID.
    cursor_msg: HashMap<u64, i32>,
    /// Each persistent search's message ID.
    watches: HashMap<u64, i32>,
}

/// The instances and sessions.
#[derive(Default)]
pub struct State {
    /// Open instances and the job that opened each.
    instances: HashMap<String, (u32, Instance)>,
    sessions: HashMap<u64, Sess>,
    next: u64,
}

thread_local! {
    /// The service is one thread; the directory lives here between
    /// requests.
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Only an instance's job asks: the user it runs as.
pub const NOT_ALLOWED: &str = "GLD0244";
/// No such session: its number.
pub const NO_SESSION: &str = "GLD014B";
/// The request does not decode: why.
pub const NOT_LDAP: &str = "GLD0259";
/// The instance is not active (not open in the service): its name.
pub const NOT_ACTIVE: &str = "GLD0122";

fn not_allowed() -> DirError {
    dir_msg(NOT_ALLOWED, vec![USER.into()])
}

fn no_session(id: u64) -> DirError {
    dir_msg(NO_SESSION, vec![id.to_string()])
}

/// `e` with its text from the message file when it has none yet.
fn worded(system: &mut System, mut e: DirError) -> DirError {
    if e.text.is_empty() {
        let vars: Vec<&str> = e.vars.iter().map(String::as_str).collect();
        e.text = crate::stgsvc::msgs::text(system, &e.msgid, &vars);
    }
    e
}

fn only_instance_job(job: &JobInfo, instance: &str) -> Result<(), DirError> {
    if job.user.trim().eq_ignore_ascii_case(USER)
        && job.name.trim().eq_ignore_ascii_case(instance.trim())
    {
        Ok(())
    } else {
        Err(not_allowed())
    }
}

fn decode(msg: &[u8]) -> Result<LdapMessage, DirError> {
    LdapMessage::decode_with_limit(msg, MAX_MESSAGE as usize)
        .map_err(|e| dir_msg(NOT_LDAP, vec![e.to_string()]))
}

fn encode(id: i32, r: Reply) -> Vec<u8> {
    LdapMessage {
        id,
        op: r.op,
        controls: r.controls,
    }
    .encode()
}

fn budget(b: DirBudget) -> Budget {
    Budget {
        entries: b.entries.clamp(1, MAX_ENTRIES) as usize,
        bytes: b.bytes.clamp(1, MAX_BYTES) as usize,
    }
}

impl State {
    fn sess(&mut self, job: &JobInfo, id: u64) -> Result<(&mut Sess, &mut Instance), DirError> {
        let s = self
            .sessions
            .get_mut(&id)
            .filter(|s| s.job == job.number)
            .ok_or_else(|| no_session(id))?;
        let (_, inst) = self
            .instances
            .get_mut(&s.instance)
            .ok_or_else(|| no_session(id))?;
        Ok((s, inst))
    }

    /// 1560.
    pub fn open(
        &mut self,
        host: &mut impl Host,
        job: &JobInfo,
        instance: &str,
        conn: DirConn,
    ) -> Result<DirOpened, DirError> {
        only_instance_job(job, instance)?;
        let key = instance.trim().to_ascii_uppercase();
        let fresh = self
            .instances
            .get(&key)
            .is_none_or(|(j, _)| *j != job.number);
        if fresh {
            let inst = Instance::open(host, &key)?;
            self.drop_instance(&key);
            self.instances.insert(key.clone(), (job.number, inst));
        }
        let (_, inst) = &self.instances[&key];
        let rt = &inst.runtime;
        let settings = DirSettings {
            instance: inst.name.clone(),
            port: rt.settings.port,
            secure_port: rt.secure_port,
            security: rt.settings.security.clone(),
            size_limit: rt.settings.size_limit,
            time_limit: rt.settings.time_limit,
            idle_timeout: rt.idle_timeout,
            write_timeout: rt.write_timeout,
            app_id: APP_ID.into(),
            max_message: MAX_MESSAGE,
        };
        self.next += 1;
        let id = self.next;
        self.sessions.insert(
            id,
            Sess {
                instance: key,
                job: job.number,
                conn,
                session: Session::anonymous(),
                cursors: Cursors::default(),
                cursor_msg: HashMap::new(),
                watches: HashMap::new(),
            },
        );
        Ok(DirOpened {
            session: id,
            settings,
        })
    }

    /// 1561.
    pub fn bind(
        &mut self,
        host: &mut impl Host,
        job: &JobInfo,
        id: u64,
        msg: &[u8],
        conn: DirConn,
    ) -> Result<DirAnswer, DirError> {
        let m = decode(msg)?;
        let (s, inst) = self.sess(job, id)?;
        s.conn = conn;
        inst.prepare(host, id, &s.conn.client, s.conn.tls);
        let c = Conn {
            tls: s.conn.tls,
            client_subject: s.conn.client_subject.clone(),
        };
        let o = host
            .with_store(|mut t, _| {
                let o = inst.dir.bind(&mut t, &m, &c);
                // Committed whatever the result: failures are recorded.
                if let Err(e) = t.commit() {
                    eprintln!("qsstg: directory bind: {e}");
                }
                o
            })
            .ok_or_else(no_store)?;
        if let Some(n) = o.session {
            s.session = n;
        }
        Ok(DirAnswer {
            replies: o.replies.into_iter().map(|r| encode(m.id, r)).collect(),
            state: DirState::Done,
            start_tls: false,
            bound: s.session.dn.clone(),
        })
    }

    /// 1562.
    pub fn op(
        &mut self,
        host: &mut impl Host,
        job: &JobInfo,
        id: u64,
        msg: &[u8],
    ) -> Result<DirAnswer, DirError> {
        let m = decode(msg)?;
        let (s, inst) = self.sess(job, id)?;
        inst.prepare(host, id, &s.conn.client, s.conn.tls);
        let session = s.session.clone();
        let o = host
            .with_store(|mut t, _| {
                let o = inst.dir.op(&mut t, &session, &m);
                if !o.succeeded() {
                    t.abort();
                    return o;
                }
                if let Err(e) = t.commit() {
                    eprintln!("qsstg: directory operation: {e}");
                }
                o
            })
            .ok_or_else(no_store)?;
        if o.succeeded() && !o.changes.is_empty() {
            host.with_store(|mut t, _| {
                inst.dir.publish(&mut t, &o.changes);
                t.abort();
            });
        }
        if o.schema_changed {
            let text = inst.dir.schema.modified_text();
            host.write(&inst.schema_path, &text).map_err(failed)?;
        }
        inst.flush(host)?;
        Ok(DirAnswer {
            replies: o.replies.into_iter().map(|r| encode(m.id, r)).collect(),
            state: DirState::Done,
            start_tls: o.start_tls,
            bound: s.session.dn.clone(),
        })
    }

    /// 1563.
    pub fn search(
        &mut self,
        host: &mut impl Host,
        job: &JobInfo,
        id: u64,
        msg: &[u8],
        b: DirBudget,
    ) -> Result<DirAnswer, DirError> {
        let m = decode(msg)?;
        let (s, inst) = self.sess(job, id)?;
        inst.prepare(host, id, &s.conn.client, s.conn.tls);
        let session = s.session.clone();
        let r = host
            .with_store(|mut t, _| {
                let a = match inst.dir.search(&mut t, &session, &m, &mut s.cursors) {
                    Started::Done(replies) => DirAnswer {
                        replies: replies.into_iter().map(|r| encode(m.id, r)).collect(),
                        ..DirAnswer::default()
                    },
                    Started::Cursor(c) => {
                        s.cursor_msg.insert(c, m.id);
                        page(&mut t, inst, s, c, budget(b))
                    }
                };
                let _ = t.commit();
                a
            })
            .ok_or_else(no_store)?;
        Ok(r)
    }

    /// 1564.
    pub fn next(
        &mut self,
        host: &mut impl Host,
        job: &JobInfo,
        id: u64,
        cursor: u64,
        b: DirBudget,
    ) -> Result<DirAnswer, DirError> {
        let (s, inst) = self.sess(job, id)?;
        if !s.cursor_msg.contains_key(&cursor) {
            return Ok(DirAnswer::default());
        }
        host.with_store(|mut t, _| {
            let a = page(&mut t, inst, s, cursor, budget(b));
            let _ = t.commit();
            a
        })
        .ok_or_else(no_store)
    }

    /// 1565.
    pub fn close(&mut self, job: &JobInfo, id: u64, what: DirClose) -> Result<(), DirError> {
        match what {
            DirClose::Message { id: msgid } => {
                let (s, inst) = self.sess(job, id)?;
                inst.dir.counters.abandons_requested += 1;
                let cursors: Vec<u64> = s
                    .cursor_msg
                    .iter()
                    .filter(|(_, m)| **m == msgid)
                    .map(|(c, _)| *c)
                    .collect();
                for c in cursors {
                    s.cursors.close(c);
                    s.cursor_msg.remove(&c);
                }
                let watches: Vec<u64> = s
                    .watches
                    .iter()
                    .filter(|(_, m)| **m == msgid)
                    .map(|(w, _)| *w)
                    .collect();
                for w in watches {
                    inst.dir.close_watch(w);
                    s.watches.remove(&w);
                }
                inst.dir.counters.abandons_completed += 1;
                Ok(())
            }
            DirClose::Session => {
                let (s, inst) = self.sess(job, id)?;
                inst.dir.counters.unbinds_requested += 1;
                inst.unbind(s.session.dn.as_deref());
                inst.dir.counters.unbinds_completed += 1;
                self.end_session(id);
                Ok(())
            }
        }
    }

    /// 1566.
    pub fn conns(&mut self, job: &JobInfo, instance: &str, c: DirConns) -> Result<(), DirError> {
        only_instance_job(job, instance)?;
        let key = instance.trim().to_ascii_uppercase();
        let (owner, inst) = self
            .instances
            .get_mut(&key)
            .ok_or_else(|| dir_msg(NOT_ACTIVE, vec![key.clone()]))?;
        if *owner != job.number {
            return Err(not_allowed());
        }
        inst.dir
            .counters
            .connections(c.current, c.total, c.tls, c.ssl);
        Ok(())
    }

    /// 1567.
    pub fn changes(&mut self, job: &JobInfo, id: u64, max: u32) -> Result<DirChanges, DirError> {
        let (s, inst) = self.sess(job, id)?;
        let mut left = max.clamp(1, MAX_ENTRIES) as usize;
        let mut out = DirChanges::default();
        let mut watches: Vec<(u64, i32)> = s.watches.iter().map(|(w, m)| (*w, *m)).collect();
        watches.sort_unstable();
        for (w, msgid) in watches {
            match inst.dir.changes(w, left) {
                None => {
                    s.watches.remove(&w);
                    out.ended.push(msgid);
                }
                Some(v) => {
                    left = left.saturating_sub(v.len());
                    out.replies.extend(v.into_iter().map(|r| encode(msgid, r)));
                }
            }
            if left == 0 {
                break;
            }
        }
        Ok(out)
    }

    fn end_session(&mut self, id: u64) {
        let Some(s) = self.sessions.remove(&id) else {
            return;
        };
        if let Some((_, inst)) = self.instances.get_mut(&s.instance) {
            for w in s.watches.keys() {
                inst.dir.close_watch(*w);
            }
        }
    }

    fn drop_instance(&mut self, key: &str) {
        let ids: Vec<u64> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.instance == key)
            .map(|(i, _)| *i)
            .collect();
        for i in ids {
            self.end_session(i);
        }
        self.instances.remove(key);
    }

    /// Job `number` ended: its sessions, and the instances it served.
    pub fn job_ended(&mut self, number: u32) {
        let ids: Vec<u64> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.job == number)
            .map(|(i, _)| *i)
            .collect();
        for i in ids {
            self.end_session(i);
        }
        let gone: Vec<String> = self
            .instances
            .iter()
            .filter(|(_, (j, _))| *j == number)
            .map(|(k, _)| k.clone())
            .collect();
        for k in gone {
            self.drop_instance(&k);
        }
    }

    /// The configuration of instance `name` changed outside the
    /// directory (`CHGDIRSVRA`, `dir_admin`): an open instance reads it
    /// again and applies what takes effect at once.
    ///
    /// # Errors
    /// The IFS's, or a file that does not check.
    #[allow(dead_code)] // dir_admin's (CHGDIRSVRA, Navigator, QAP)
    pub fn config_changed(&mut self, host: &mut impl Host, name: &str) -> Result<(), DirError> {
        match self.instances.get_mut(&name.trim().to_ascii_uppercase()) {
            Some((_, inst)) => inst.reload(host),
            None => Ok(()),
        }
    }

    /// Run `f` over open instance `name` (`dir_admin`'s questions:
    /// counters, settings, the schema); `None` when it is not open.
    #[allow(dead_code)] // dir_admin's (CHGDIRSVRA, Navigator, QAP)
    pub fn with_instance<T>(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut Instance) -> T,
    ) -> Option<T> {
        self.instances
            .get_mut(&name.trim().to_ascii_uppercase())
            .map(|(_, i)| f(i))
    }
}

/// One page of cursor `c`, its state in the answer's terms.
#[allow(clippy::many_single_char_names)]
fn page(t: &mut Txn<'_>, inst: &mut Instance, s: &mut Sess, c: u64, b: Budget) -> DirAnswer {
    let msgid = s.cursor_msg.get(&c).copied().unwrap_or_default();
    let p = inst.dir.next(t, &mut s.cursors, c, b);
    let state = match p.state {
        PageState::More => DirState::More { cursor: c },
        PageState::Paused => DirState::Paused { cursor: c },
        PageState::Done => {
            s.cursor_msg.remove(&c);
            DirState::Done
        }
        PageState::Watching(w) => {
            s.cursor_msg.remove(&c);
            s.watches.insert(w, msgid);
            DirState::Watching { watch: w }
        }
    };
    DirAnswer {
        replies: p.replies.into_iter().map(|r| encode(msgid, r)).collect(),
        state,
        start_tls: false,
        bound: None,
    }
}

/// The service as an instance's [`Host`].
pub struct SystemHost<'a>(pub &'a mut System);

impl Host for SystemHost<'_> {
    fn read(&mut self, path: &str) -> Result<Option<String>, String> {
        crate::stgsvc::dirconf::read(self.0, path)
    }

    fn write(&mut self, path: &str, text: &str) -> Result<(), String> {
        crate::stgsvc::dirconf::write(self.0, path, text)
    }

    fn instances(&mut self) -> Result<InstanceList, dirsrv::config::ConfigError> {
        crate::stgsvc::dirconf::instances(self.0)
    }

    fn bind_ctx(&self) -> BindCtx {
        let get = |n: &str| crate::stgsvc::security::sysval_text(self.0, n);
        BindCtx {
            values: sec::signon::Values::from_sysvals(&get),
            now: now_unix(),
            qsecurity: get("QSECURITY").trim().parse().unwrap_or(40),
        }
    }

    fn with_store<T>(&mut self, f: impl FnOnce(Txn<'_>, ObjectId) -> T) -> Option<T> {
        let s = self.0.storage.as_mut()?;
        let qsys = s.qsys?;
        let txn = s.store.begin().ok()?;
        Some(f(txn, qsys))
    }
}

/// Run `f` over the state with the service as host, as the system (the
/// directory checks access itself).
fn with<T>(system: &mut System, f: impl FnOnce(&mut State, &mut SystemHost<'_>) -> T) -> T {
    let mut host = SystemHost(system);
    sec::guard::as_system(|| STATE.with(|s| f(&mut s.borrow_mut(), &mut host)))
}

/// 1560.
///
/// # Errors
/// Not the instance's job; the instance's or its file's.
pub fn open(
    system: &mut System,
    job: &JobInfo,
    instance: &str,
    conn: DirConn,
) -> Result<DirOpened, DirError> {
    with(system, |s, h| s.open(h, job, instance, conn)).map_err(|e| worded(system, e))
}

/// 1561.
///
/// # Errors
/// No such session, a message that does not decode.
pub fn bind(
    system: &mut System,
    job: &JobInfo,
    id: u64,
    msg: &[u8],
    conn: DirConn,
) -> Result<DirAnswer, DirError> {
    with(system, |s, h| s.bind(h, job, id, msg, conn)).map_err(|e| worded(system, e))
}

/// 1562.
///
/// # Errors
/// No such session, a message that does not decode, the IFS's.
pub fn op(system: &mut System, job: &JobInfo, id: u64, msg: &[u8]) -> Result<DirAnswer, DirError> {
    with(system, |s, h| s.op(h, job, id, msg)).map_err(|e| worded(system, e))
}

/// 1563.
///
/// # Errors
/// No such session, a message that does not decode.
pub fn search(
    system: &mut System,
    job: &JobInfo,
    id: u64,
    msg: &[u8],
    b: DirBudget,
) -> Result<DirAnswer, DirError> {
    with(system, |s, h| s.search(h, job, id, msg, b)).map_err(|e| worded(system, e))
}

/// 1564.
///
/// # Errors
/// No such session.
pub fn next(
    system: &mut System,
    job: &JobInfo,
    id: u64,
    cursor: u64,
    b: DirBudget,
) -> Result<DirAnswer, DirError> {
    with(system, |s, h| s.next(h, job, id, cursor, b)).map_err(|e| worded(system, e))
}

/// 1565.
///
/// # Errors
/// No such session.
pub fn close(system: &mut System, job: &JobInfo, id: u64, what: DirClose) -> Result<(), DirError> {
    with(system, |s, _| s.close(job, id, what)).map_err(|e| worded(system, e))
}

/// 1566.
///
/// # Errors
/// Not the instance's job, or the instance is not open.
pub fn conns(
    system: &mut System,
    job: &JobInfo,
    instance: &str,
    c: DirConns,
) -> Result<(), DirError> {
    STATE
        .with(|s| s.borrow_mut().conns(job, instance, c))
        .map_err(|e| worded(system, e))
}

/// 1567.
///
/// # Errors
/// No such session.
pub fn changes(
    system: &mut System,
    job: &JobInfo,
    id: u64,
    max: u32,
) -> Result<DirChanges, DirError> {
    with(system, |s, _| s.changes(job, id, max)).map_err(|e| worded(system, e))
}

/// A job ended (`served.rs`'s end of a job).
pub fn job_ended(number: u32) {
    STATE.with(|s| s.borrow_mut().job_ended(number));
}

/// For `dir_admin` and `CHGDIRSVRA`: the file of instance `name`
/// changed; an open instance applies it.
///
/// # Errors
/// The IFS's, or a file that does not check.
#[allow(dead_code)] // dir_admin's (CHGDIRSVRA, Navigator, QAP)
pub fn config_changed(system: &mut System, name: &str) -> Result<(), DirError> {
    with(system, |s, h| s.config_changed(h, name)).map_err(|e| worded(system, e))
}

/// For `dir_admin`: `f` over open instance `name`.
#[allow(dead_code)] // dir_admin's (CHGDIRSVRA, Navigator, QAP)
pub fn with_instance<T>(name: &str, f: impl FnOnce(&mut Instance) -> T) -> Option<T> {
    STATE.with(|s| s.borrow_mut().with_instance(name, f))
}
