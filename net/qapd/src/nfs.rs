// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `Nfs` service (version 11, draft; docs/QAP.md "Service 13
//! `Nfs`") on a secondary thread: each request passes the exit point
//! `QS400_QAP_MGMT` (server `*QAPNFS`), then runs as the signed-on
//! profile: reads through the storage service's export table, daemon
//! registry, mount table and NFS options, every change the command a
//! program would run (`EXPORTFS`, `STRNFSSVR`, `ENDNFSSVR`, `MOUNT`,
//! `UNMOUNT`) or `QZNFNFSO`'s set, so each checks the authority and
//! words the refusal. The host gives them ([`NfsHost`]).

use std::fmt::Write as _;

use ipc::Wire;
use nfs::exports::options::print_hosts;
use nfs::exports::{DataCcsid, PathCcsid};
use qap_proto::ops::nfs::{
    DaemonJob, DaemonType, Daemons, EndDaemons, Export, ExportOptions, ExportRef, Exports,
    HostOption, ListExports, MountNfs, MountedFs, Mounts, NfsOptions, PutExport, Registration,
    RemoveExport, StartDaemons, Unmount, UnmountKind,
};
use qap_proto::types::JobId;
use qap_proto::{Envelope, Op, ServerMessage, Status};

use crate::conn::Answer;
use crate::host::{Refusal, ThreadHost, no_database};
use crate::manage::{EXIT_FORMAT, EXIT_POINT, ExitRequest, NO_MANAGE, page};

/// The server field of this service's exit requests.
pub const SERVER: &str = "*QAPNFS";

/// One entry of the export table, as the storage service keeps it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportEntry {
    /// Its export ID.
    pub id: u32,
    /// The entry as a line of `/etc/exports` (`nfs::exports::file`).
    pub entry: String,
    /// The anonymous UID; `None` when unknown users are refused.
    pub anon_uid: Option<u32>,
    /// That UID's profile, `*NONE` when none.
    pub anon_user: String,
    /// The job that exported it, `number/user/name`.
    pub by: String,
    /// When, seconds since 1970.
    pub since: u64,
}

/// One daemon job, as the registry keeps it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DaemonEntry {
    /// Its type.
    pub daemon: DaemonType,
    /// The job, `number/user/name`.
    pub job: String,
    /// It reported ready.
    pub ready: bool,
    /// It was asked to end.
    pub ending: bool,
    /// What it registered.
    pub registrations: Vec<Registration>,
    /// When it said hello, seconds since 1970.
    pub since: u64,
}

/// What the service needs of the system, as the signed-on profile.
pub trait NfsHost {
    /// The export table.
    ///
    /// # Errors
    /// The service's refusal.
    fn exports(&mut self) -> Result<Vec<ExportEntry>, Refusal>;
    /// The daemon jobs.
    ///
    /// # Errors
    /// The service's refusal.
    fn daemons(&mut self) -> Result<Vec<DaemonEntry>, Refusal>;
    /// The mount table.
    ///
    /// # Errors
    /// Why it cannot be read.
    fn mounts(&mut self) -> Result<Vec<MountedFs>, Refusal>;
    /// Run CL command `line` (one of the five above) as the profile.
    ///
    /// # Errors
    /// Its escape message, with the status [`status_of`] gives it.
    fn command(&mut self, line: &str) -> Result<(), Refusal>;
    /// The NFS options.
    ///
    /// # Errors
    /// The service's refusal.
    fn options(&mut self) -> Result<NfsOptions, Refusal>;
    /// Change the NFS options (`QZNFNFSO`'s set).
    ///
    /// # Errors
    /// `CPFA1B8` without `*IOSYSCFG`; `CPFA1BB` for a flavor not 1.
    fn set_options(&mut self, o: &NfsOptions) -> Result<(), Refusal>;
    /// Message `id` with `data`, from its message file.
    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage;
}

/// The status a command's message stands for.
#[must_use]
pub fn status_of(id: &str) -> Status {
    match id {
        "CPFA1B8" | "CPF2189" | "CPF2182" | "CPF9802" | "CPD0032" => Status::NotAuthorized,
        "CPD0030" => Status::NotFound,
        _ => Status::Error,
    }
}

fn ms(secs: u64) -> i64 {
    i64::try_from(secs).unwrap_or(i64::MAX).saturating_mul(1000)
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

fn hosts(list: &[nfs::exports::HostSpec]) -> Vec<String> {
    list.iter()
        .map(|h| print_hosts(std::slice::from_ref(h)))
        .collect()
}

/// An entry as the protocol carries it; `None` when its line does not
/// read (the service printed it, so it always does).
#[must_use]
#[allow(clippy::many_single_char_names)]
pub fn export_out(e: &ExportEntry) -> Option<Export> {
    let x = nfs::exports::file::entry(1, &e.entry).ok()?;
    let o = &x.options;
    let v = o.versions();
    Some(Export {
        id: e.id,
        path: x.path.clone(),
        options: ExportOptions {
            read_only: o.ro,
            rw_hosts: o.rw.as_deref().map(hosts).unwrap_or_default(),
            access_hosts: o.access.as_deref().map(hosts),
            root_hosts: hosts(&o.root_hosts),
            anon: o.anon,
            nosuid: o.nosuid,
            v3: v.v3,
            v4: v.v4,
            public: o.public,
            pseudo_root: o.pseudo_root,
            nfsroot: o.nfsroot.clone(),
            exname: o.exname.clone(),
        },
        hosts: x
            .hosts
            .iter()
            .map(|h| HostOption {
                host: print_hosts(std::slice::from_ref(&h.host)),
                data_ccsid: match h.data {
                    DataCcsid::Ccsid(n) => u32::from(n),
                    _ => 65535,
                },
                path_ccsid: match h.path {
                    PathCcsid::Ccsid(n) => u32::from(n),
                    _ => 0,
                },
                sync: h.sync,
            })
            .collect(),
        anon_uid: e.anon_uid,
        anon_user: (e.anon_user != "*NONE" && !e.anon_user.is_empty()).then(|| e.anon_user.clone()),
        exported_by: job_id(&e.by),
        since: ms(e.since),
    })
}

/// A CL string literal.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The options list of `EXPORTFS -O`; the defaults left out.
#[must_use]
pub fn options_list(o: &ExportOptions) -> String {
    let mut v: Vec<String> = Vec::new();
    if o.read_only {
        v.push("ro".into());
    }
    if !o.rw_hosts.is_empty() {
        v.push(format!("rw={}", o.rw_hosts.join(":")));
    }
    if let Some(a) = &o.access_hosts {
        v.push(format!("access={}", a.join(":")));
    }
    if !o.root_hosts.is_empty() {
        v.push(format!("root={}", o.root_hosts.join(":")));
    }
    if let Some(a) = o.anon {
        v.push(format!("anon={a}"));
    }
    if o.nosuid {
        v.push("nosuid".into());
    }
    match (o.v3, o.v4) {
        (true, true) => {}
        (true, false) => v.push("vers=3".into()),
        (false, true) => v.push("vers=4".into()),
        // Neither: the command says the value is not valid.
        (false, false) => v.push("vers=".into()),
    }
    if o.public {
        v.push("public".into());
    }
    if o.pseudo_root {
        v.push("root".into());
    }
    if let Some(p) = &o.nfsroot {
        v.push(format!("nfsroot={p}"));
    }
    if let Some(p) = &o.exname {
        v.push(format!("exname={p}"));
    }
    v.join(",")
}

/// `EXPORTFS` exporting `e` (with `-F` when `file`).
#[must_use]
pub fn export_command(e: &Export, file: bool) -> String {
    let mut flags = String::from("-I");
    if file {
        flags.push_str(" -F");
    }
    let list = options_list(&e.options);
    if !list.is_empty() {
        flags.push_str(" -O ");
        flags.push_str(&list);
    }
    let mut s = format!(
        "EXPORTFS OPTIONS({}) DIR({})",
        quote(&flags),
        quote(e.path.trim())
    );
    if !e.hosts.is_empty() {
        s.push_str(" HOSTOPT(");
        for (i, h) in e.hosts.iter().enumerate() {
            let data = if h.data_ccsid == 65535 {
                "*BINARY".to_string()
            } else {
                h.data_ccsid.to_string()
            };
            let path = if h.path_ccsid == 0 {
                "*ASCII".to_string()
            } else {
                h.path_ccsid.to_string()
            };
            let w = if h.sync { "*SYNC" } else { "*ASYNC" };
            let sep = if i == 0 { "" } else { " " };
            let _ = write!(s, "{sep}({} {data} {path} {w})", quote(h.host.trim()));
        }
        s.push(')');
    }
    s
}

/// `EXPORTFS` ending the export of `path`.
#[must_use]
pub fn unexport_command(path: &str, file: bool) -> String {
    let flags = if file { "-U -I -F" } else { "-U" };
    format!("EXPORTFS OPTIONS({}) DIR({})", quote(flags), quote(path))
}

fn server(d: Option<DaemonType>) -> &'static str {
    d.map_or("*ALL", DaemonType::keyword)
}

fn secs(t: Option<u32>) -> String {
    t.map_or_else(|| "*NOMAX".to_string(), |n| n.to_string())
}

/// `STRNFSSVR` for `b`.
#[must_use]
pub fn start_command(b: &StartDaemons) -> String {
    let mut s = format!("STRNFSSVR SERVER({})", server(b.daemon));
    if b.servers > 0 && matches!(b.daemon, None | Some(DaemonType::Svr)) {
        let _ = write!(s, " NBRSVR({})", b.servers);
    }
    if b.block_io > 0 && matches!(b.daemon, None | Some(DaemonType::Bio)) {
        let _ = write!(s, " NBRBIO({})", b.block_io);
    }
    if b.retrieve_registrations {
        s.push_str(" RTVRPCREG(*YES)");
    }
    let _ = write!(s, " STRJOBTIMO({})", secs(b.timeout));
    s
}

/// `ENDNFSSVR` for `b`.
#[must_use]
pub fn end_command(b: &EndDaemons) -> String {
    format!(
        "ENDNFSSVR SERVER({}) ENDJOBTIMO({})",
        server(b.daemon),
        secs(b.timeout)
    )
}

/// `MOUNT TYPE(*NFS)` for `b`.
#[must_use]
pub fn mount_command(b: &MountNfs) -> String {
    let mut s = format!(
        "MOUNT TYPE(*NFS) MFS({}) MNTOVRDIR({})",
        quote(&format!("{}:{}", b.host.trim(), b.path.trim())),
        quote(b.over.trim())
    );
    if let Some(o) = b.options.as_deref().filter(|o| !o.trim().is_empty()) {
        let _ = write!(s, " OPTIONS({})", quote(o.trim()));
    }
    s
}

/// `UNMOUNT` for `b`.
#[must_use]
pub fn unmount_command(b: &Unmount) -> String {
    let kind = match b.kind {
        UnmountKind::Nfs => "*NFS",
        UnmountKind::Udfs => "*UDFS",
    };
    format!("UNMOUNT TYPE({kind}) MNTOVRDIR({})", quote(b.over.trim()))
}

/// The exit request for `op` with body `req`; `None` when the body
/// does not decode.
fn exit_request(user: &str, op: Op, req: &Envelope) -> Option<ExitRequest> {
    let none = || (String::new(), String::new());
    let (target, detail) = match op {
        Op::ListExports => {
            req.body_as::<ListExports>().ok()?;
            ("*ALL".into(), String::new())
        }
        Op::GetExport => (req.body_as::<ExportRef>().ok()?.path, String::new()),
        Op::PutExport => {
            let b = req.body_as::<PutExport>().ok()?;
            (b.export.path.clone(), options_list(&b.export.options))
        }
        Op::RemoveExport => (req.body_as::<RemoveExport>().ok()?.path, String::new()),
        Op::ExportAll => ("/etc/exports".into(), String::new()),
        Op::ListDaemons | Op::ListMounts => ("*ALL".into(), String::new()),
        Op::StartDaemons => {
            let b = req.body_as::<StartDaemons>().ok()?;
            (server(b.daemon).into(), String::new())
        }
        Op::EndDaemons => {
            let b = req.body_as::<EndDaemons>().ok()?;
            (server(b.daemon).into(), String::new())
        }
        Op::MountNfs => {
            let b = req.body_as::<MountNfs>().ok()?;
            (b.over.clone(), format!("{}:{}", b.host, b.path))
        }
        Op::Unmount => (req.body_as::<Unmount>().ok()?.over, String::new()),
        Op::GetNfsOptions => none(),
        Op::SetNfsOptions => {
            req.body_as::<NfsOptions>().ok()?;
            none()
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

fn exports(h: &mut dyn NfsHost) -> Result<Vec<Export>, Refusal> {
    Ok(h.exports()?.iter().filter_map(export_out).collect())
}

fn get_export(h: &mut dyn NfsHost, path: &str) -> Result<Export, Refusal> {
    let found = exports(h)?
        .into_iter()
        .find(|e| nfs::exports::file::same_path(&e.path, path));
    found.ok_or_else(|| Refusal {
        status: Status::NotFound,
        // As `EXPORTFS -U` says it: not exported (reason 20).
        message: h.message("CPDA1C5", &[path, "20"]),
    })
}

fn daemon_out(d: DaemonEntry) -> DaemonJob {
    DaemonJob {
        daemon: d.daemon,
        job: job_id(&d.job).unwrap_or_default(),
        ready: d.ready,
        ending: d.ending,
        registrations: d.registrations,
        since: ms(d.since),
    }
}

/// Answer one `Nfs` operation as `user`, on a secondary thread.
pub(crate) fn serve(host: &mut dyn ThreadHost, user: &str, op: Op, req: &Envelope) -> Answer {
    let Some(exit) = exit_request(user, op, req) else {
        return Answer::status(Status::InvalidRequest);
    };
    if let Err(r) = host.exit_allows(EXIT_POINT, EXIT_FORMAT, &exit) {
        return Answer::refused(r);
    }
    if host.nfs().is_none() {
        let m = host.message(NO_MANAGE, &[]);
        return Answer::refused(no_database(m));
    }
    let Some(h) = host.nfs() else {
        return Answer::status(Status::Error);
    };
    match op {
        Op::ListExports => body(req, |b: ListExports| {
            answer(exports(h).map(|all| {
                let (total, exports) = page(all, b.start, b.max);
                Exports { total, exports }
            }))
        }),
        Op::GetExport => body(req, |b: ExportRef| answer(get_export(h, &b.path))),
        Op::PutExport => body(req, |b: PutExport| {
            done(h.command(&export_command(&b.export, b.update_file)))
        }),
        Op::RemoveExport => body(req, |b: RemoveExport| {
            done(h.command(&unexport_command(&b.path, b.update_file)))
        }),
        Op::ExportAll => done(h.command("EXPORTFS OPTIONS('-A')")),
        Op::ListDaemons => answer(h.daemons().map(|mut v| {
            v.sort_by(|a, b| {
                (a.daemon as u8, a.job.as_str()).cmp(&(b.daemon as u8, b.job.as_str()))
            });
            Daemons {
                jobs: v.into_iter().map(daemon_out).collect(),
            }
        })),
        Op::StartDaemons => body(req, |b: StartDaemons| done(h.command(&start_command(&b)))),
        Op::EndDaemons => body(req, |b: EndDaemons| done(h.command(&end_command(&b)))),
        Op::ListMounts => answer(h.mounts().map(|mounts| Mounts { mounts })),
        Op::MountNfs => body(req, |b: MountNfs| done(h.command(&mount_command(&b)))),
        Op::Unmount => body(req, |b: Unmount| done(h.command(&unmount_command(&b)))),
        Op::GetNfsOptions => answer(h.options()),
        Op::SetNfsOptions => body(req, |b: NfsOptions| {
            answer(h.set_options(&b).and_then(|()| h.options()))
        }),
        _ => Answer::status(Status::InvalidRequest),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_cross_over_as_commands() {
        let e = ExportEntry {
            id: 2,
            entry: "/home -ro,rw=a:@10.0.0.0/8,access=b,anon=-1,vers=4,exname=/h HOSTOPT pc1:819:1208:*ASYNC".into(),
            anon_uid: None,
            anon_user: "*NONE".into(),
            by: "000123/QSECOFR/QPADEV0001".into(),
            since: 2,
        };
        let x = export_out(&e).expect("reads");
        assert_eq!(x.options.rw_hosts, ["a", "@10.0.0.0/8"]);
        assert!(!x.options.v3 && x.options.v4);
        assert_eq!(
            x.exported_by.as_ref().map(|j| j.number.as_str()),
            Some("000123")
        );
        assert_eq!(x.since, 2000);
        assert_eq!(
            export_command(&x, true),
            "EXPORTFS OPTIONS('-I -F -O ro,rw=a:@10.0.0.0/8,access=b,anon=-1,vers=4,exname=/h') DIR('/home') HOSTOPT(('pc1' 819 1208 *ASYNC))"
        );
        assert_eq!(
            unexport_command("/home", false),
            "EXPORTFS OPTIONS('-U') DIR('/home')"
        );
    }

    #[test]
    fn daemons_and_mounts_as_commands() {
        assert_eq!(
            start_command(&StartDaemons {
                daemon: Some(DaemonType::Svr),
                servers: 4,
                block_io: 2,
                retrieve_registrations: false,
                timeout: Some(60),
            }),
            "STRNFSSVR SERVER(*SVR) NBRSVR(4) STRJOBTIMO(60)"
        );
        assert_eq!(
            end_command(&EndDaemons::default()),
            "ENDNFSSVR SERVER(*ALL) ENDJOBTIMO(*NOMAX)"
        );
        assert_eq!(
            mount_command(&MountNfs {
                host: "srv".into(),
                path: "/x".into(),
                over: "/mnt/x".into(),
                options: None,
            }),
            "MOUNT TYPE(*NFS) MFS('srv:/x') MNTOVRDIR('/mnt/x')"
        );
        assert_eq!(
            unmount_command(&Unmount {
                kind: UnmountKind::Udfs,
                over: "/u".into(),
            }),
            "UNMOUNT TYPE(*UDFS) MNTOVRDIR('/u')"
        );
        assert_eq!(status_of("CPFA1B8"), Status::NotAuthorized);
        assert_eq!(status_of("CPFA1C7"), Status::Error);
    }
}
