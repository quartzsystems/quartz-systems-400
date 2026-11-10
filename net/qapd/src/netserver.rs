// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `NetServer` service (version 10, draft; docs/QAP.md "Service 12
//! `NetServer`") on a secondary thread: each request passes the exit
//! point `QS400_QAP_MGMT` (server `*QAPNETSVR`), then runs as the
//! `QZLS*` API a program would call (`netsvrcfg::client`), as the
//! signed-on profile, so the API checks its authority and words the
//! refusal. The host gives the API calls, the session table and the
//! disabled users ([`NetServerHost`]).

#![allow(clippy::doc_markdown)] // NetServer

use ipc::Wire;
use netsvrcfg::client::{self, Failure};
use netsvrcfg::{Kind, Permission, Qualified, Settings};
use qap_proto::ops::netserver::{
    Attributes, ChangeAttributes, Connection, ConnectionKind, Dialect, DisabledUser, DisabledUsers,
    EnableUser, FileShare, ListSessions, ListShares, NetSession, NetSessions, PrintShare,
    ServerAttributes, SessionRef, Share, ShareBody, ShareDetail, ShareKind, ShareRef, Shares,
    SpoolType, StartServer,
};
use qap_proto::types::{JobId, ObjectName};
use qap_proto::{Envelope, Op, ServerMessage, Status};

use crate::conn::Answer;
use crate::host::{Refusal, ThreadHost, no_database};
use crate::manage::{EXIT_FORMAT, EXIT_POINT, ExitRequest, NO_MANAGE, page};

/// The server field of this service's exit requests.
pub const SERVER: &str = "*QAPNETSVR";

/// What the service needs of the system, as the signed-on profile.
pub trait NetServerHost {
    /// Call API `api` (`netsvr_api`).
    ///
    /// # Errors
    /// The API's message and data.
    fn api(&mut self, api: &str, parms: Vec<Vec<u8>>) -> Result<Vec<u8>, Failure>;
    /// The sessions, as the service's table holds them.
    fn sessions(&mut self) -> Vec<netsvrcfg::Session>;
    /// The users disabled for NetServer and since when (seconds since
    /// 1970), by name.
    fn disabled(&mut self) -> Vec<(String, u64)>;
    /// `QZLSSERVER`'s job (`number/user/name`) while it is active.
    fn server_job(&mut self) -> Option<String>;
    /// Message `id` with data `data`, from its message file.
    fn message(&mut self, id: &str, data: &[u8]) -> ServerMessage;
}

/// How a refused API call answers: `NotAuthorized` for authority,
/// `NotFound` for a share or session not there, `Exists` for a name
/// taken, else `Error`.
#[must_use]
pub fn status_of(f: &Failure) -> Status {
    match (f.id.as_str(), f.bin4(22), f.bin4(25)) {
        ("CPFB684" | "CPDB680", _, _) => Status::NotAuthorized,
        ("CPFB68A", 1, _) => Status::Exists,
        ("CPFB68A", 2, _) | ("CPFB68E", _, 1 | 5) => Status::NotFound,
        _ => Status::Error,
    }
}

fn refusal(ns: &mut dyn NetServerHost, f: &Failure) -> Refusal {
    Refusal {
        status: status_of(f),
        message: ns.message(&f.id, &f.data),
    }
}

fn ms(secs: u64) -> i64 {
    i64::try_from(secs).unwrap_or(i64::MAX).saturating_mul(1000)
}

fn object(q: &Qualified) -> ObjectName {
    ObjectName {
        library: q.library.clone(),
        name: q.name.clone(),
    }
}

fn qualified(o: &ObjectName) -> Qualified {
    Qualified::new(
        &o.name.trim().to_ascii_uppercase(),
        &o.library.trim().to_ascii_uppercase(),
    )
}

fn opt(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

fn job_id(q: &str) -> Option<JobId> {
    let mut p = q.splitn(3, '/');
    let (number, user, name) = (p.next()?, p.next()?, p.next()?);
    Some(JobId {
        name: name.into(),
        user: user.into(),
        number: number.into(),
    })
}

/// A share as the protocol carries it.
fn share_out(s: &netsvrcfg::Share, sessions: &[netsvrcfg::Session]) -> Share {
    let users = sessions
        .iter()
        .flat_map(|x| x.connections.iter())
        .filter(|c| c.share.eq_ignore_ascii_case(&s.name))
        .count();
    let detail = match &s.kind {
        Kind::File(f) => ShareDetail::File {
            file: FileShare {
                path: f.path.clone(),
                read_only: f.permission == Permission::ReadOnly,
                max_users: u32::try_from(f.max_users).ok(),
                text_conversion: f.text_conversion,
                text_ccsid: u32::try_from(f.text_ccsid).unwrap_or(0),
                extensions: f.extensions.clone(),
            },
        },
        Kind::Print(p) => ShareDetail::Print {
            print: PrintShare {
                output_queue: object(&p.outq),
                driver: p.driver.clone(),
                spool_type: match p.spool_type {
                    netsvrcfg::SpoolType::Auto => SpoolType::Auto,
                    netsvrcfg::SpoolType::UserAscii => SpoolType::UserAscii,
                    netsvrcfg::SpoolType::Afp => SpoolType::Afp,
                    netsvrcfg::SpoolType::Scs => SpoolType::Scs,
                },
                printer_file: (!p.printer_file.is_empty()).then(|| object(&p.printer_file)),
            },
        },
    };
    Share {
        name: s.name.clone(),
        text: s.text.clone(),
        encryption_required: s.encryption_required,
        authorization_list: opt(&s.autl),
        current_users: u32::try_from(users).unwrap_or(u32::MAX),
        detail,
    }
}

/// The model share a request describes.
fn share_in(s: &Share) -> netsvrcfg::Share {
    let name = s.name.trim().to_ascii_uppercase();
    let mut m = match &s.detail {
        ShareDetail::File { file } => {
            let mut m = netsvrcfg::Share::file(&name, &file.path, &s.text);
            if let Kind::File(f) = &mut m.kind {
                f.permission = if file.read_only {
                    Permission::ReadOnly
                } else {
                    Permission::ReadWrite
                };
                f.max_users = file
                    .max_users
                    .map_or(-1, |n| i32::try_from(n).unwrap_or(i32::MAX));
                f.text_conversion = file.text_conversion;
                f.text_ccsid = i32::try_from(file.text_ccsid).unwrap_or(i32::MAX);
                f.extensions = file
                    .extensions
                    .iter()
                    .map(|e| e.trim().trim_start_matches('.').to_ascii_uppercase())
                    .collect();
            }
            m
        }
        ShareDetail::Print { print } => {
            let mut m = netsvrcfg::Share::print(&name, qualified(&print.output_queue), &s.text);
            if let Kind::Print(p) = &mut m.kind {
                p.driver.clone_from(&print.driver);
                p.spool_type = match print.spool_type {
                    SpoolType::Auto => netsvrcfg::SpoolType::Auto,
                    SpoolType::UserAscii => netsvrcfg::SpoolType::UserAscii,
                    SpoolType::Afp => netsvrcfg::SpoolType::Afp,
                    SpoolType::Scs => netsvrcfg::SpoolType::Scs,
                };
                p.printer_file = print
                    .printer_file
                    .as_ref()
                    .map(qualified)
                    .unwrap_or_default();
            }
            m
        }
    };
    m.encryption_required = s.encryption_required;
    m.autl = s
        .authorization_list
        .as_deref()
        .map(|a| a.trim().to_ascii_uppercase())
        .unwrap_or_default();
    m
}

fn dialect_out(d: netsvrcfg::Dialect) -> Dialect {
    match d {
        netsvrcfg::Dialect::Smb202 => Dialect::Smb202,
        netsvrcfg::Dialect::Smb21 => Dialect::Smb21,
        netsvrcfg::Dialect::Smb30 => Dialect::Smb30,
        netsvrcfg::Dialect::Smb302 => Dialect::Smb302,
        netsvrcfg::Dialect::Smb311 => Dialect::Smb311,
    }
}

fn dialect_in(d: Dialect) -> netsvrcfg::Dialect {
    match d {
        Dialect::Smb202 => netsvrcfg::Dialect::Smb202,
        Dialect::Smb21 => netsvrcfg::Dialect::Smb21,
        Dialect::Smb30 => netsvrcfg::Dialect::Smb30,
        Dialect::Smb302 => netsvrcfg::Dialect::Smb302,
        Dialect::Smb311 => netsvrcfg::Dialect::Smb311,
    }
}

fn attrs_out(s: &Settings) -> ServerAttributes {
    ServerAttributes {
        server_name: s.server_name.clone(),
        domain: s.domain.clone(),
        text: s.text.clone(),
        allow_system_name: s.system_name_access,
        guest: opt(&s.guest),
        code_page: s.code_page,
        idle_timeout: s.idle_timeout,
        oplock_timeout: s.oplock_timeout,
        browse_interval: s.browse_interval,
        message_auth: u8::try_from(s.message_auth).unwrap_or(u8::MAX),
        encryption_required: s.encryption_required(),
        authorization_list: opt(&s.server_autl),
        allow_netbios: s.allow_netbios,
        min_dialect: dialect_out(s.min_dialect),
        log_signon: s.log_signon,
    }
}

/// `base` with what the request asks for (the attributes the protocol
/// has no field for kept).
fn attrs_in(base: &Settings, a: &ServerAttributes) -> Settings {
    let up = |s: &str| s.trim().to_ascii_uppercase();
    let mut s = base.clone();
    s.server_name = up(&a.server_name);
    s.domain = up(&a.domain);
    s.text = a.text.trim().to_string();
    s.system_name_access = a.allow_system_name;
    s.guest = a.guest.as_deref().map(up).unwrap_or_default();
    s.code_page = a.code_page;
    s.idle_timeout = a.idle_timeout;
    s.oplock_timeout = a.oplock_timeout;
    s.browse_interval = a.browse_interval;
    s.message_auth = i32::from(a.message_auth);
    s.encryption = i32::from(a.encryption_required);
    s.server_autl = a.authorization_list.as_deref().map(up).unwrap_or_default();
    s.allow_netbios = a.allow_netbios;
    s.min_dialect = dialect_in(a.min_dialect);
    s.log_signon = a.log_signon;
    s
}

fn session_out(s: netsvrcfg::Session) -> NetSession {
    NetSession {
        id: s.id,
        workstation: s.workstation,
        expanded: s.expanded,
        user: s.user,
        guest: s.guest,
        job: job_id(&s.job),
        since: ms(s.since),
        last_active: ms(s.last_active),
        connections: s
            .connections
            .into_iter()
            .map(|c| Connection {
                share: c.share,
                kind: match c.kind {
                    1 => ConnectionKind::Print,
                    3 => ConnectionKind::Ipc,
                    _ => ConnectionKind::Disk,
                },
                open_files: c.opens,
                since: ms(c.since),
            })
            .collect(),
    }
}

/// The exit request for `op` with body `req`; `None` when the body
/// does not decode.
fn exit_request(user: &str, op: Op, req: &Envelope) -> Option<ExitRequest> {
    let detail_of = |s: &Share| match &s.detail {
        ShareDetail::File { file } => file.path.clone(),
        ShareDetail::Print { print } => {
            format!("{}/{}", print.output_queue.library, print.output_queue.name)
        }
    };
    let (target, detail) = match op {
        Op::ListShares => {
            let b = req.body_as::<ListShares>().ok()?;
            let k = match b.kind {
                None => "*ALL",
                Some(ShareKind::File) => "*FILE",
                Some(ShareKind::Print) => "*PRINT",
            };
            (k.to_string(), String::new())
        }
        Op::GetShare | Op::RemoveShare => (req.body_as::<ShareRef>().ok()?.name, String::new()),
        Op::AddShare | Op::ChangeShare => {
            let b = req.body_as::<ShareBody>().ok()?;
            (b.share.name.clone(), detail_of(&b.share))
        }
        Op::EndSession => (
            req.body_as::<SessionRef>().ok()?.id.to_string(),
            String::new(),
        ),
        Op::EnableUser => (req.body_as::<EnableUser>().ok()?.user, String::new()),
        Op::StartServer => {
            let b = req.body_as::<StartServer>().ok()?;
            (String::new(), format!("RESET({})", u8::from(b.reset)))
        }
        Op::ListSessions => {
            req.body_as::<ListSessions>().ok()?;
            ("*ALL".into(), String::new())
        }
        Op::ChangeServerAttributes => {
            req.body_as::<ChangeAttributes>().ok()?;
            (String::new(), String::new())
        }
        Op::GetServerAttributes | Op::ListDisabledUsers | Op::EndServer => {
            (String::new(), String::new())
        }
        _ => return None,
    };
    let (_, code) = op.code();
    Some(ExitRequest {
        user: user.to_string(),
        server: SERVER.to_string(),
        op: code,
        name: format!("{op:?}"),
        target,
        detail,
    })
}

fn body<B: Wire>(req: &Envelope, f: impl FnOnce(B) -> Answer) -> Answer {
    match req.body_as::<B>() {
        Ok(b) => f(b),
        Err(_) => Answer::status(Status::InvalidRequest),
    }
}

/// `f` over the APIs; a refusal as [`status_of`] says.
fn run<T>(
    ns: &mut dyn NetServerHost,
    f: impl FnOnce(&mut dyn client::Api) -> Result<T, Failure>,
) -> Result<T, Refusal> {
    let r = {
        let mut call = |api: &str, parms: Vec<Vec<u8>>| ns.api(api, parms);
        f(&mut call)
    };
    r.map_err(|e| refusal(ns, &e))
}

fn answer<T: Wire>(r: Result<T, Refusal>) -> Answer {
    match r {
        Ok(b) => Answer::ok(&b),
        Err(r) => Answer::refused(r),
    }
}

fn done(r: Result<(), Refusal>) -> Answer {
    match r {
        Ok(()) => Answer::status(Status::Ok),
        Err(r) => Answer::refused(r),
    }
}

fn get_share(ns: &mut dyn NetServerHost, name: &str) -> Result<Share, Refusal> {
    let n = name.trim().to_ascii_uppercase();
    let found = run(ns, |a| client::share(a, &n, 37))?;
    let sessions = ns.sessions();
    if let Some(s) = found {
        return Ok(share_out(&s, &sessions));
    }
    // As the change APIs say it: no such share.
    let f = Failure {
        id: "CPFB68A".into(),
        data: netsvrcfg::api::ApiError::of("CPFB68A", "QZLSOLST")
            .text(&n, 12)
            .bin4(2)
            .data,
    };
    Err(refusal(ns, &f))
}

fn attributes(ns: &mut dyn NetServerHost) -> Result<Attributes, Refusal> {
    let a = run(ns, client::read_attributes)?;
    let job = ns.server_job();
    Ok(Attributes {
        active: job.is_some(),
        job: job.as_deref().and_then(job_id),
        current: attrs_out(&a.current),
        pending: attrs_out(&a.pending),
    })
}

/// Answer one `NetServer` operation as `user`, on a secondary thread.
pub(crate) fn serve(host: &mut dyn ThreadHost, user: &str, op: Op, req: &Envelope) -> Answer {
    let Some(exit) = exit_request(user, op, req) else {
        return Answer::status(Status::InvalidRequest);
    };
    if let Err(r) = host.exit_allows(EXIT_POINT, EXIT_FORMAT, &exit) {
        return Answer::refused(r);
    }
    if host.netserver().is_none() {
        let m = host.message(NO_MANAGE, &[]);
        return Answer::refused(no_database(m));
    }
    let Some(ns) = host.netserver() else {
        return Answer::status(Status::Error);
    };
    match op {
        Op::ListShares => body(req, |b: ListShares| {
            answer(run(ns, |a| client::list_shares(a, 37)).map(|all| {
                let sessions = ns.sessions();
                let picked: Vec<Share> = all
                    .iter()
                    .filter(|s| match b.kind {
                        None => true,
                        Some(ShareKind::File) => s.as_file().is_some(),
                        Some(ShareKind::Print) => s.as_print().is_some(),
                    })
                    .map(|s| share_out(s, &sessions))
                    .collect();
                let (total, shares) = page(picked, b.start, b.max);
                Shares { total, shares }
            }))
        }),
        Op::GetShare => body(req, |b: ShareRef| answer(get_share(ns, &b.name))),
        Op::AddShare => body(req, |b: ShareBody| {
            let s = share_in(&b.share);
            done(run(ns, |a| client::add_share(a, &s)))
        }),
        Op::ChangeShare => body(req, |b: ShareBody| {
            let s = share_in(&b.share);
            done(run(ns, |a| client::change_share(a, &s)))
        }),
        Op::RemoveShare => body(req, |b: ShareRef| {
            let n = b.name.trim().to_ascii_uppercase();
            done(run(ns, |a| client::remove_share(a, &n)))
        }),
        Op::ListSessions => body(req, |b: ListSessions| {
            let mut all = ns.sessions();
            all.sort_by_key(|s| s.id);
            let (total, sessions) = page(all, b.start, b.max);
            Answer::ok(&NetSessions {
                total,
                sessions: sessions.into_iter().map(session_out).collect(),
            })
        }),
        Op::EndSession => body(req, |b: SessionRef| {
            done(run(ns, |a| client::end_session(a, b.id)))
        }),
        Op::GetServerAttributes => answer(attributes(ns)),
        Op::ChangeServerAttributes => body(req, |b: ChangeAttributes| {
            let r = run(ns, |a| {
                let now = client::read_attributes(a)?;
                let to = attrs_in(&now.pending, &b.pending);
                client::change_attributes(a, &now.pending, &to)?;
                client::read_attributes(a)
            });
            answer(r.map(|a| attrs_out(&a.pending)))
        }),
        Op::ListDisabledUsers => Answer::ok(&DisabledUsers {
            users: ns
                .disabled()
                .into_iter()
                .map(|(user, since)| DisabledUser {
                    user,
                    since: ms(since),
                })
                .collect(),
        }),
        Op::EnableUser => body(req, |b: EnableUser| {
            let u = b.user.trim().to_ascii_uppercase();
            done(run(ns, |a| client::enable_users(a, &[u])))
        }),
        Op::StartServer => body(req, |b: StartServer| {
            done(run(ns, |a| client::start(a, b.reset)))
        }),
        Op::EndServer => done(run(ns, client::end)),
        _ => Answer::status(Status::InvalidRequest),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_of_the_apis_refusals() {
        let f = |id: &str, e: netsvrcfg::api::ApiError| Failure {
            id: id.into(),
            data: e.data,
        };
        let share = |r| {
            netsvrcfg::api::ApiError::of("CPFB68A", "QZLSADFS")
                .text("X", 12)
                .bin4(r)
        };
        assert_eq!(status_of(&f("CPFB68A", share(1))), Status::Exists);
        assert_eq!(status_of(&f("CPFB68A", share(2))), Status::NotFound);
        assert_eq!(status_of(&f("CPFB68A", share(7))), Status::Error);
        let sess = netsvrcfg::api::ApiError::of("CPFB68E", "QZLSENSS")
            .text("*SESSID", 15)
            .bin4(5);
        assert_eq!(status_of(&f("CPFB68E", sess)), Status::NotFound);
        let auth = netsvrcfg::api::ApiError::of("CPFB684", "QZLSRMS").bin4(1);
        assert_eq!(status_of(&f("CPFB684", auth)), Status::NotAuthorized);
    }

    #[test]
    fn shares_cross_over() {
        let mut s = netsvrcfg::Share::print("P", Qualified::new("PRT01", "QUSRSYS"), "x");
        s.autl = "L".into();
        let out = share_out(&s, &[]);
        assert_eq!(share_in(&out), s);
        let f = netsvrcfg::Share::file("F", "/a", "y");
        assert_eq!(share_in(&share_out(&f, &[])), f);
        let a = Settings::new("QS400");
        assert_eq!(attrs_in(&a, &attrs_out(&a)), a);
    }
}
