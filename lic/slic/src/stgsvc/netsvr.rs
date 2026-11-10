// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SMB file server's side of the storage service
//! (`docs/NETSERVER.md` section 4 and the APIs): the configuration files `QAZLSCFG` and `QAZLSSHR` under
//! `/QIBM/UserData/OS400/NetServer/` (made with the shipped `QIBM`
//! share at first use, owned by `QSYS`, `*PUBLIC *EXCLUDE`), the `QZLS*`
//! APIs over them (`netsvrcfg::api`), and the session table the
//! `QZLSSERVER` and `QZLSFILE` jobs report into.

#![allow(clippy::doc_markdown)] // NetServer

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use netsvrcfg::api::{self, ApiError, Caller, Enabled, GuestCheck, Host, ServerControl};
use netsvrcfg::file::{self, CFG_PATH, SHR_PATH};
use netsvrcfg::{Config, Counter, Qualified, Session, SessionTable};
use sec::auth::principal::Principal;
use sec::profile::SpecialAuth;
use stg::netsvr::{NsConn, NsOp, NsReply, NsSession};
use store::obj::types;

use super::certs::{as_system, read_file, write_file};
use super::devices::in_store;
use crate::ipl::system::{System, now_unix};

/// The session table; one per storage service, so per IPL.
static TABLE: LazyLock<Mutex<SessionTable>> = LazyLock::new(|| Mutex::new(SessionTable::new()));

/// Whether this IPL made the pending attributes current yet.
static APPLIED: AtomicBool = AtomicBool::new(false);

fn table() -> MutexGuard<'static, SessionTable> {
    TABLE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The jobs that may change the session table.
const SERVER_JOBS: &[&str] = &["QZLSSERVER", "QZLSFILE"];

/// The configuration the files hold, and whether they must be written
/// (one is missing, or this IPL makes the pending values current).
fn merge(
    cfg: Option<Vec<u8>>,
    shr: Option<Vec<u8>>,
    name: &str,
    apply: bool,
) -> Result<(Config, bool), String> {
    let mut save = cfg.is_none() || shr.is_none();
    let shipped = Config::shipped(name);
    let (a, s) = file::write(&shipped).map_err(|e| e.to_string())?;
    let c = cfg.unwrap_or(a);
    let s = shr.unwrap_or(s);
    let mut config = file::read(&c, &s, name).map_err(|e| e.to_string())?;
    if apply && config.attrs.apply_pending() {
        save = true;
    }
    Ok((config, save))
}

/// Read the configuration, writing it first when missing; the first
/// read of an IPL makes the pending attributes current.
pub fn load(system: &mut System) -> Result<Config, String> {
    let name = system.system_name();
    let (cfg, shr) = as_system(system, |c| {
        Ok::<_, String>((read_file(c, CFG_PATH)?, read_file(c, SHR_PATH)?))
    })?;
    let apply = !APPLIED.load(Ordering::Acquire);
    let (config, save_it) = merge(cfg, shr, &name, apply)?;
    if save_it {
        save(system, &config)?;
    }
    APPLIED.store(true, Ordering::Release);
    Ok(config)
}

/// Write both files (each replaced whole).
pub fn save(system: &mut System, c: &Config) -> Result<(), String> {
    let (a, s) = file::write(c).map_err(|e| e.to_string())?;
    as_system(system, |cl| {
        write_file(cl, CFG_PATH, &a)?;
        write_file(cl, SHR_PATH, &s)
    })
}

/// `QZLSSERVER`'s job: its name, user, job description, the job queue
/// of `QSERVER` it is submitted to and the routing data of `QSERVER`'s
/// entry 800.
const SERVER_JOB_NAME: &str = "QZLSSERVER";
const SERVER_JOBD: &str = "QSYS/QZLSSERVER";
const SERVER_JOBQ: &str = "QSYS/QPWFSERVER";
const SERVER_RTGDTA: &str = "QZLSSTART";
const SERVER_USER: &str = "QPGMR";

/// The number of the `QZLSSERVER` job last submitted, while it waits on
/// its job queue.
static SUBMITTED: Mutex<Option<u32>> = Mutex::new(None);

/// `QZLSENDS` (or `ENDTCPSVR`) asked the server to end; cleared at
/// the next start, so every `QZLSFILE` sees it however soon
/// `QZLSSERVER` ends.
static ENDING: AtomicBool = AtomicBool::new(false);

/// One job's routes for reconnects (`docs/NETSERVER.md` section 3).
#[derive(Clone, Debug, Default)]
struct Route {
    job: String,
    relay: Vec<u8>,
    guids: Vec<Vec<u8>>,
    peers: Vec<String>,
}

/// Every job's routes.
static ROUTES: Mutex<Vec<Route>> = Mutex::new(Vec::new());

fn routes() -> MutexGuard<'static, Vec<Route>> {
    ROUTES.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether `QZLSSERVER` is active: in the job table in `QSERVER`, or
/// submitted and still on its queue.
fn server_active(system: &System) -> bool {
    let running = system
        .jobs
        .all()
        .iter()
        .any(|j| j.name == SERVER_JOB_NAME && j.subsystem == "QSERVER");
    let mut sub = SUBMITTED.lock().unwrap_or_else(PoisonError::into_inner);
    if running {
        *sub = None;
        return true;
    }
    match *sub {
        Some(n) if system.queued.contains_key(&n) => true,
        _ => {
            *sub = None;
            false
        }
    }
}

/// What the APIs asked of the server; done once the call returns,
/// since the control cannot hold the system the host holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Start,
    End,
}

/// The server control the service has: start submits `QZLSSERVER` to
/// `QSERVER`, end marks the server ending (the jobs see it through
/// [`NsOp::State`]), ending a session leaves the table's mark for the
/// serving job.
struct Jobs {
    active: bool,
    action: Option<Action>,
}

impl Jobs {
    fn of(system: &System) -> Self {
        Self {
            active: server_active(system),
            action: None,
        }
    }
}

impl ServerControl for Jobs {
    fn active(&mut self) -> bool {
        self.active
    }

    fn start(&mut self, _reset: bool) -> Result<(), ApiError> {
        self.action = Some(Action::Start);
        Ok(())
    }

    fn end(&mut self) -> Result<(), ApiError> {
        if !self.active {
            return Err(ApiError::new(api::error::msg::JOB_ENDED));
        }
        self.action = Some(Action::End);
        Ok(())
    }

    fn end_session(&mut self, _job: &str, _id: u64) -> Result<(), ApiError> {
        // The table marks it; the serving job asks `Ending` and ends it.
        Ok(())
    }
}

/// Submit `QZLSSERVER` to `QSERVER`'s queue as `QPGMR` (the service's
/// own request, not the caller's authority to the profile).
fn submit(system: &mut System) -> Result<(), String> {
    ENDING.store(false, Ordering::Release);
    let me = system.current_job;
    let r = wm::obj::jobq::SbmJob {
        job: SERVER_JOB_NAME.into(),
        jobd: SERVER_JOBD.into(),
        jobq: SERVER_JOBQ.into(),
        user: SERVER_USER.into(),
        rtgdta: SERVER_RTGDTA.into(),
        msgq: "*NONE".into(),
        inllibl: Some(Vec::new()),
        curlib: "*CRTDFT".into(),
        ..wm::obj::jobq::SbmJob::default()
    };
    let (number, _) = sec::guard::as_system(|| super::jobqs::sbmjob(system, me, &r))?;
    *SUBMITTED.lock().unwrap_or_else(PoisonError::into_inner) = Some(number);
    Ok(())
}

/// Carry out what the call asked of the server.
fn act(system: &mut System, action: Option<Action>) -> Result<(), ApiError> {
    match action {
        Some(Action::Start) => submit(system).map_err(|e| {
            eprintln!("slic: netserver: {e}");
            ApiError::new(api::error::msg::INTERNAL)
        }),
        Some(Action::End) => {
            ENDING.store(true, Ordering::Release);
            Ok(())
        }
        None => Ok(()),
    }
}

/// The APIs' view of the system for one call.
struct SvcHost<'a> {
    system: &'a mut System,
    me: Principal,
    table: MutexGuard<'static, SessionTable>,
    jobs: Jobs,
    ccsid: u16,
    /// The call is the system's own (`STRTCP`'s autostart).
    as_system: bool,
}

impl SvcHost<'_> {
    fn store<T>(
        &mut self,
        f: impl FnOnce(&mut store::txn::Txn<'_>, store::obj::ObjectId) -> Result<T, String>,
    ) -> Option<T> {
        in_store(self.system, f).ok()
    }
}

fn internal(e: &str) -> ApiError {
    eprintln!("slic: netserver: {e}");
    ApiError::new(api::error::msg::INTERNAL)
}

impl Host for SvcHost<'_> {
    fn load(&mut self) -> Result<Config, ApiError> {
        load(self.system).map_err(|e| internal(&e))
    }

    fn save(&mut self, c: &Config) -> Result<(), ApiError> {
        save(self.system, c).map_err(|e| internal(&e))
    }

    fn caller(&mut self) -> Caller {
        Caller {
            user: self.me.user.clone(),
            iosyscfg: self.as_system || self.me.has(SpecialAuth::IOSYSCFG),
            secadm: self.me.has(SpecialAuth::SECADM),
        }
    }

    fn path_owner(&mut self, path: &str) -> Option<String> {
        as_system(self.system, |c| c.stat(path, true).ok().map(|s| s.owner))
    }

    fn outq_owner(&mut self, q: &Qualified) -> Option<String> {
        let (lib, name) = (q.library.clone(), q.name.clone());
        self.store(|txn, qsys| {
            let ctx = store::obj::ops::NameContext::default();
            let id = wm::obj::find(txn, qsys, &ctx, &lib, &name, types::OUTQ)
                .map_err(|e| e.to_string())?;
            let h = store::obj::ops::describe(txn, id).map_err(|e| e.to_string())?;
            Ok(ccsid::decode_trimmed(&h.owner))
        })
    }

    fn autl_exists(&mut self, name: &str) -> bool {
        let name = name.to_string();
        self.store(|txn, qsys| sec::auth::autl::find(txn, qsys, &name).map_err(|e| e.to_string()))
            .is_some()
    }

    fn guest_check(&mut self, name: &str) -> GuestCheck {
        let name = name.to_string();
        self.store(|txn, qsys| {
            let Ok((id, p)) = sec::profile::objects::read(txn, qsys, &name) else {
                return Ok(GuestCheck::NotFound);
            };
            if !sec::guard::allowed(txn, qsys, "usrprf.use", id) {
                return Ok(GuestCheck::NotFound);
            }
            let mut sp = p.spcaut;
            for g in std::iter::once(&p.grpprf).chain(p.supgrpprf.iter()) {
                if g != "*NONE"
                    && let Ok((_, gp)) = sec::profile::objects::read(txn, qsys, g)
                {
                    sp = SpecialAuth(sp.0 | gp.spcaut.0);
                }
            }
            Ok(match sp.names().first() {
                Some(n) => GuestCheck::Special((*n).to_string()),
                None => GuestCheck::Usable,
            })
        })
        .unwrap_or(GuestCheck::NotFound)
    }

    fn enable_user(&mut self, name: &str) -> Enabled {
        let name = name.to_ascii_uppercase();
        let n = name.clone();
        // *OBJMGT (the bits `obj.rename` asks) and *USE to the profile.
        let ok = self.store(|txn, qsys| {
            let Ok((id, _)) = sec::profile::objects::read(txn, qsys, &n) else {
                return Ok(None);
            };
            Ok(Some(
                sec::guard::allowed(txn, qsys, "usrprf.use", id)
                    && sec::guard::allowed(txn, qsys, "obj.rename", id),
            ))
        });
        match ok.flatten() {
            None => Enabled::Failed(super::msgs::text(self.system, "CPF2204", &[])),
            Some(false) => Enabled::NotAuthorized,
            Some(true) => {
                sec::netsvr::enable_netserver_user(&name);
                Enabled::Done
            }
        }
    }

    fn disabled_users(&mut self) -> Vec<String> {
        sec::netsvr::users()
            .disabled()
            .into_iter()
            .map(|(u, _)| u)
            .collect()
    }

    fn sessions(&mut self) -> &mut SessionTable {
        &mut self.table
    }

    fn server(&mut self) -> &mut dyn ServerControl {
        &mut self.jobs
    }

    fn now(&mut self) -> u64 {
        now_unix()
    }

    fn stamp(&mut self, unix: u64) -> String {
        let local = super::perfcol::local_now(self.system);
        let off = i64::try_from(local).unwrap_or(0) - i64::try_from(now_unix()).unwrap_or(0);
        api::host::stamp_utc(u64::try_from(i64::try_from(unix).unwrap_or(0) + off).unwrap_or(unix))
    }

    fn job_ccsid(&mut self) -> u16 {
        self.ccsid
    }
}

/// `SERVER_SHARE_INFO`'s rows.
pub fn sql_shares(
    system: &mut System,
) -> Result<Vec<sql::exec::catalog::services::host::ServerShare>, String> {
    use sql::exec::catalog::services::host::ServerShare;
    let c = load(system)?;
    let t = table();
    Ok(c.shares
        .values()
        .map(|s| {
            let current =
                i32::try_from(netsvrcfg::fmt::zlsl::current_users(&t, &s.name)).unwrap_or(i32::MAX);
            let mut r = ServerShare {
                name: s.name.clone(),
                text: s.text.clone(),
                encryption: s.encryption_required,
                autl: s.autl.clone(),
                current,
                ..ServerShare::default()
            };
            match &s.kind {
                netsvrcfg::Kind::File(f) => {
                    r.path.clone_from(&f.path);
                    r.read_only = f.permission == netsvrcfg::Permission::ReadOnly;
                    r.max_users = f.max_users;
                    r.text_conversion = f.text_conversion;
                    r.text_ccsid = f.text_ccsid;
                    r.extension_count = i32::try_from(f.extensions.len()).unwrap_or(0);
                    for e in &f.extensions {
                        r.extensions
                            .extend(i32::try_from(e.len().min(46)).unwrap_or(0).to_be_bytes());
                        r.extensions.extend(netsvrcfg::fmt::text::fixed(e, 46));
                    }
                }
                netsvrcfg::Kind::Print(p) => {
                    r.print = true;
                    r.spool_type = p.spool_type.special().trim_start_matches('*').into();
                    r.outq = (p.outq.library.clone(), p.outq.name.clone());
                    r.driver.clone_from(&p.driver);
                    r.printer_file = (p.printer_file.library.clone(), p.printer_file.name.clone());
                    r.publish = p.publish;
                }
            }
            r
        })
        .collect())
}

/// A `QZLS*` API for the caller `me`.
pub fn call(
    system: &mut System,
    me: &Principal,
    ccsid: u16,
    name: &str,
    parms: &[Vec<u8>],
) -> Result<Vec<u8>, (String, Vec<u8>)> {
    run_api(system, me, ccsid, name, parms, false).map_err(|e| (e.id.to_string(), e.data))
}

/// [`call`] over the host, then what it asked of the server.
fn run_api(
    system: &mut System,
    me: &Principal,
    ccsid: u16,
    name: &str,
    parms: &[Vec<u8>],
    as_system: bool,
) -> Result<Vec<u8>, ApiError> {
    let jobs = Jobs::of(system);
    let mut h = SvcHost {
        system,
        me: me.clone(),
        table: table(),
        jobs,
        ccsid,
        as_system,
    };
    let out = api::call(&mut h, name, parms)?;
    let action = h.jobs.action;
    drop(h);
    act(system, action)?;
    Ok(out)
}

/// A refused API as the job reports it: the message and its text.
fn refused(system: &mut System, e: &ApiError) -> NsReply {
    let id = e.id.to_string();
    let file = (sysapi::param::file_of(&id).to_string(), "*LIBL".to_string());
    let text = super::msgs::description(system, &id, &file)
        .map_or_else(String::new, |d| d.format(&e.data));
    NsReply::Refused { msgid: id, text }
}

/// Whether `*NETSVR` starts with TCP/IP (the server table).
fn autostarts(system: &mut System) -> bool {
    super::tcp::with(system, |_, rt| {
        rt.cfg.servers.get("*NETSVR").is_some_and(|a| a.autostart)
    })
    .unwrap_or(false)
}

fn to_wire(s: &Session) -> NsSession {
    NsSession {
        id: s.id,
        workstation: s.workstation.clone(),
        expanded: s.expanded.clone(),
        user: s.user.clone(),
        job: s.job.clone(),
        guest: s.guest,
        since: s.since,
        last_active: s.last_active,
        ending: s.ending,
        connections: s
            .connections
            .iter()
            .map(|c| NsConn {
                id: c.id,
                share: c.share.clone(),
                kind: c.kind,
                opens: c.opens,
                since: c.since,
            })
            .collect(),
    }
}

fn counter(n: u8) -> Option<Counter> {
    Some(match n {
        0 => Counter::Opens,
        1 => Counter::PrintJobs,
        2 => Counter::AutoDisconnects,
        3 => Counter::DuplicateSessions,
        4 => Counter::SessionErrors,
        5 => Counter::PasswordViolations,
        6 => Counter::UnknownUsers,
        7 => Counter::BytesSent,
        8 => Counter::BytesReceived,
        9 => Counter::Response,
        _ => return None,
    })
}

/// A message's text: an identifier and its values.
type Say<'a> = &'a mut dyn FnMut(&str, &[&str]) -> String;

/// The message a tree connection past a share's maximum users is
/// refused with.
const MAX_USERS_MSGID: &str = "TXT0E30";

/// A session table operation on `t` for job `job` (`number/user/name`,
/// its name `name`) at `now`; `max` is the share's maximum users for
/// [`NsOp::Admit`]; `say` gives a message's text.
#[allow(clippy::too_many_lines)] // one match over the operations
fn table_op(
    t: &mut SessionTable,
    job: &str,
    name: &str,
    now: u64,
    max: i32,
    op: NsOp,
    say: Say<'_>,
) -> NsReply {
    let err = |e: &dyn std::fmt::Display| NsReply::Error {
        text: e.to_string(),
    };
    let changes = !matches!(
        op,
        NsOp::List | NsOp::Ending { .. } | NsOp::Config | NsOp::State
    );
    if changes && !SERVER_JOBS.contains(&name) {
        return NsReply::Error {
            text: say("TXT0E2D", &[job]),
        };
    }
    match op {
        NsOp::Started => {
            t.started(now);
            NsReply::Done
        }
        NsOp::Register { session } => {
            let s = Session {
                id: 0,
                workstation: session.workstation,
                expanded: session.expanded,
                user: session.user,
                job: job.into(),
                guest: session.guest,
                since: now,
                last_active: now,
                connections: Vec::new(),
                ending: false,
            };
            NsReply::Session { id: t.register(s) }
        }
        NsOp::Unregister { id, idle } => t
            .unregister(id, idle)
            .map_or_else(|e| err(&e), |_| NsReply::Done),
        NsOp::Connect { id, share, kind } => t
            .connect(id, &share, kind, now)
            .map_or_else(|e| err(&e), |c| NsReply::Conn { id: c }),
        NsOp::Admit { id, share, kind } => match t.connect_within(id, &share, kind, max, now) {
            Ok(c) => NsReply::Conn { id: c },
            Err(netsvrcfg::TableError::MaxUsers(s)) => NsReply::Refused {
                msgid: MAX_USERS_MSGID.into(),
                text: say(MAX_USERS_MSGID, &[&s]),
            },
            Err(e) => err(&e),
        },
        NsOp::Disconnect { id, conn } => t
            .disconnect(id, conn)
            .map_or_else(|e| err(&e), |()| NsReply::Done),
        NsOp::Opens { id, conn, opens } => t
            .set_opens(id, conn, opens, now)
            .map_or_else(|e| err(&e), |()| NsReply::Done),
        NsOp::Touch { id } => t
            .touch(id, now)
            .map_or_else(|e| err(&e), |()| NsReply::Done),
        NsOp::Count { counter: c, n } => match counter(c) {
            Some(k) => {
                t.count(k, n);
                NsReply::Done
            }
            None => NsReply::Error {
                text: say("TXT0E2E", &[&c.to_string()]),
            },
        },
        NsOp::List => NsReply::Sessions {
            list: t.sessions().map(to_wire).collect(),
        },
        NsOp::Ending { id } => NsReply::Flag {
            yes: t.is_ending(id),
        },
        NsOp::JobEnded => {
            t.job_ended(job);
            routes().retain(|r| r.job != job);
            NsReply::Done
        }
        NsOp::Route {
            relay,
            guids,
            peers,
        } => {
            let mut r = routes();
            r.retain(|x| x.job != job);
            if !guids.is_empty() || !peers.is_empty() {
                r.push(Route {
                    job: job.into(),
                    relay,
                    guids,
                    peers,
                });
            }
            NsReply::Done
        }
        NsOp::Lookup { guid, peer } => {
            let r = routes();
            // The newest registration first: a job still being released
            // may hold the client's guid a moment longer.
            let by_guid = if guid.len() == 16 {
                r.iter().rev().find(|x| x.guids.contains(&guid))
            } else {
                None
            };
            let found = by_guid.or_else(|| {
                if peer.is_empty() {
                    None
                } else {
                    r.iter().rev().find(|x| x.peers.contains(&peer))
                }
            });
            NsReply::Target {
                relay: found.map(|x| x.relay.clone()).unwrap_or_default(),
            }
        }
        NsOp::State => NsReply::State {
            active: false,
            ending: ENDING.load(Ordering::Acquire),
        },
        NsOp::Config | NsOp::Start { .. } | NsOp::End => NsReply::Error {
            text: say("TXT0E2F", &[]),
        },
    }
}

/// The `netsvr` request from job `job`, whose current profile is `me`.
pub fn request(system: &mut System, job: &wm::table::JobInfo, me: &Principal, op: NsOp) -> NsReply {
    let q = format!("{:06}/{}/{}", job.number, job.user, job.name);
    match op {
        NsOp::Config => {
            if !SERVER_JOBS.contains(&job.name.as_str()) {
                return NsReply::Error {
                    text: super::msgs::text(system, "TXT0E2D", &[&q]),
                };
            }
            return match load(system).and_then(|c| file::write(&c).map_err(|e| e.to_string())) {
                Ok((cfg, shr)) => NsReply::Files { cfg, shr },
                Err(e) => NsReply::Error { text: e },
            };
        }
        NsOp::Start { reset, autostart } => {
            if autostart && (!autostarts(system) || server_active(system)) {
                return NsReply::Done;
            }
            // The reset flag as the API takes it: EBCDIC '1' or '0'.
            let parms = vec![vec![if reset { 0xF1 } else { 0xF0 }]];
            return match run_api(system, me, 37, "QZLSSTRS", &parms, autostart) {
                Ok(_) => NsReply::Done,
                Err(e) => refused(system, &e),
            };
        }
        NsOp::End => {
            return match run_api(system, me, 37, "QZLSENDS", &[], false) {
                Ok(_) => NsReply::Done,
                Err(e) => refused(system, &e),
            };
        }
        NsOp::State => {
            return NsReply::State {
                active: server_active(system),
                ending: ENDING.load(Ordering::Acquire),
            };
        }
        _ => {}
    }
    let max = match &op {
        NsOp::Admit { share, .. } => load(system)
            .ok()
            .and_then(|c| {
                c.share(share)
                    .and_then(|s| s.as_file().map(|f| f.max_users))
            })
            .unwrap_or(-1),
        _ => -1,
    };
    let mut say = |id: &str, v: &[&str]| super::msgs::text(system, id, v);
    table_op(&mut table(), &q, &job.name, now_unix(), max, op, &mut say)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(t: &mut SessionTable, job: &str, name: &str, now: u64, op: NsOp) -> NsReply {
        table_op(t, job, name, now, -1, op, &mut |id, _| id.to_string())
    }

    #[test]
    fn missing_files_are_made_and_pending_applies_once() {
        let (c, save) = merge(None, None, "QS400", true).expect("merges");
        assert!(save);
        assert!(c.share("QIBM").is_some());
        let mut changed = c.clone();
        changed.attrs.pending.text = "Files".into();
        let (a, s) = file::write(&changed).expect("writes");
        let (c, save) = merge(Some(a.clone()), Some(s.clone()), "QS400", false).expect("merges");
        assert!(!save);
        assert_eq!(c.attrs.current.text, "QS/400");
        let (c, save) = merge(Some(a), Some(s), "QS400", true).expect("merges");
        assert!(save);
        assert_eq!(c.attrs.current.text, "Files");
        assert!(merge(Some(b"junk".to_vec()), None, "QS400", true).is_err());
    }

    #[test]
    fn only_netserver_jobs_change_the_table() {
        let mut t = SessionTable::new();
        let reg = || NsOp::Register {
            session: NsSession {
                workstation: "PC1".into(),
                user: "BOB".into(),
                ..NsSession::default()
            },
        };
        assert!(matches!(
            run(&mut t, "000001/QPGMR/QPADEV0001", "QPADEV0001", 5, reg()),
            NsReply::Error { .. }
        ));
        let NsReply::Session { id } = run(&mut t, "000002/QUSER/QZLSFILE", "QZLSFILE", 5, reg())
        else {
            panic!("registers")
        };
        let NsReply::Conn { id: c } = run(
            &mut t,
            "000002/QUSER/QZLSFILE",
            "QZLSFILE",
            6,
            NsOp::Connect {
                id,
                share: "qibm".into(),
                kind: 0,
            },
        ) else {
            panic!("connects")
        };
        assert_eq!(
            run(
                &mut t,
                "000002/QUSER/QZLSFILE",
                "QZLSFILE",
                7,
                NsOp::Opens {
                    id,
                    conn: c,
                    opens: 1
                }
            ),
            NsReply::Done
        );
        let NsReply::Sessions { list } = run(&mut t, "x", "QPADEV0001", 8, NsOp::List) else {
            panic!("lists")
        };
        assert_eq!(list[0].job, "000002/QUSER/QZLSFILE");
        assert_eq!(list[0].connections[0].share, "QIBM");
        assert_eq!(
            run(&mut t, "x", "QPADEV0001", 8, NsOp::Ending { id }),
            NsReply::Flag { yes: false }
        );
        assert!(matches!(
            run(
                &mut t,
                "000002/QUSER/QZLSFILE",
                "QZLSFILE",
                9,
                NsOp::Count { counter: 99, n: 1 }
            ),
            NsReply::Error { .. }
        ));
        assert_eq!(
            run(
                &mut t,
                "000002/QUSER/QZLSFILE",
                "QZLSFILE",
                9,
                NsOp::JobEnded
            ),
            NsReply::Done
        );
        assert_eq!(t.sessions().count(), 0);
    }
}
