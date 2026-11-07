// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `QSYS2` services' view of the system: [`SqlHost`] answers from
//! the state the service keeps for `WRKACTJOB`, `WRKSYSSTS`, `WRKSPLF`
//! and the rest. A statement asks while it runs on its fiber with the
//! service parked ([`super::turns`]) and its transaction given back,
//! so the store goes back to the system for the answer and is lent
//! again after. Elsewhere (a statement not taking turns) the host
//! cannot reach the system and the service says so (SQL0443).

use sql::exec::catalog::services::host::{self, ActiveJob, HttpRequest, HttpResponse, SysValue, SystemHost, SystemStatus};

use licpgm::ProductStore as _;

use crate::ipl::system::System;
use crate::svc::ile;

/// The host of one job's statements.
pub struct SqlHost {
    /// The asking job.
    pub job: u32,
}

/// Run `f` over the system with the store back in it; `None` when no
/// statement is taking its turn or the store is in use.
fn with_system<T>(f: impl FnOnce(&mut System) -> T) -> Option<T> {
    super::turns::with_parked(|system| {
        if system.lent.try_borrow_mut().is_err() {
            return None;
        }
        let lent = system.lent.borrow().is_some();
        if lent {
            ile::restore(system);
        }
        let r = f(system);
        if lent && ile::lend(system).is_err() {
            eprintln!("qsstg: the store could not be lent again after a service read it");
        }
        Some(r)
    })
    .flatten()
}

fn group_name(g: sysval::Group) -> String {
    format!("*{g:?}").to_ascii_uppercase()
}

impl SystemHost for SqlHost {
    fn library_list(&self) -> Option<wm::job::LibList> {
        ile::current_libl().or_else(|| with_system(|s| s.attrs.get(&self.job).map(|a| a.libl.clone())).flatten())
    }

    fn system_values(&self) -> Option<Vec<SysValue>> {
        with_system(|system| {
            let vals = system.sysvals.as_ref()?;
            Some(
                vals.catalog()
                    .entries()
                    .map(|e| {
                        let v = vals.get(&e.name).ok();
                        let (number, text) = match v {
                            Some(sysval::Value::Dec(n)) => (Some(n), None),
                            Some(sysval::Value::List(items)) => (None, Some(items.join(" "))),
                            Some(other) => (None, Some(other.text())),
                            None => (None, None),
                        };
                        SysValue {
                            name: e.name.clone(),
                            number,
                            text,
                            description: e.text.clone(),
                            group: group_name(e.group),
                            shipped: e.default_value().text(),
                            changeable: !e.readonly,
                        }
                    })
                    .collect(),
            )
        })
        .flatten()
    }

    fn active_jobs(&self) -> Option<Vec<ActiveJob>> {
        with_system(|system| {
            let data = super::stats::actjob_data(system, false);
            data.jobs
                .into_iter()
                .map(|a| {
                    let (cpu_ms, _, _, aux) = super::stats::job_figures(system, a.job.number);
                    ActiveJob {
                        attrs: system.attrs.get(&a.job.number).cloned(),
                        cpu_ms,
                        cpu_tenths: a.cpu,
                        threads: a.threads,
                        temp_mb: a.tmp_mb,
                        aux_io: aux,
                        elapsed_aux_io: a.aux_io,
                        pool_number: a.pool,
                        transactions: a.txns,
                        response_ms: a.rsp * 100 * a.txns,
                        elapsed_ms: data.elapsed * 1000,
                        job: a.job,
                    }
                })
                .collect()
        })
    }

    fn system_status(&self) -> Option<SystemStatus> {
        with_system(super::stats::sql_status)
    }

    fn http(&self, req: &HttpRequest) -> Option<Result<HttpResponse, String>> {
        // BLOCKED(rel): the request holds the service until it answers
        // (at most its timeout); it does not yet take turns.
        let r = httpc::send(&httpc::Request {
            method: req.method.clone(),
            url: req.url.clone(),
            headers: req.headers.clone(),
            body: req.body.clone(),
            timeout: if req.timeout == 0 { 30 } else { req.timeout.min(300) },
            redirects: req.redirects,
            tolerate: req.tolerate,
        });
        Some(r.map(|r| HttpResponse { status: r.status, headers: r.headers, body: r.body }).map_err(|e| e.to_string()))
    }

    fn spooled_files(&self) -> Option<Vec<wm::obj::splf::SplfAttrs>> {
        with_system(|system| system.spool.files.values().map(|e| e.attrs.clone()).collect())
    }

    fn spooled_data(&self, id: u32) -> Option<Result<Vec<u8>, String>> {
        with_system(|system| super::spool::data(system, id).map(|(_, d)| d))
    }

    fn job_log(&self, job: &str) -> Option<sql::exec::catalog::services::host::JobLog> {
        super::turns::job_log_of(job, self.job)
    }

    fn netstat(&self) -> Option<Result<sql::exec::catalog::services::host::Netstat, String>> {
        with_system(super::tcp::netstat::sql)
    }

    fn products(&self) -> Option<Result<Vec<host::Product>, String>> {
        with_system(|system| {
            super::licpgm::with(system, |p, _, _| {
                let loads = p.loads();
                Ok(licpgm::directory::software_resources(p)
                    .into_iter()
                    .map(|r| {
                        let library = loads
                            .iter()
                            .find(|(_, l)| l.key.product == r.product && l.key.option == r.option && l.key.load_id == r.feature)
                            .and_then(|(_, l)| l.principal().map(|e| e.installed.clone()))
                            .unwrap_or_default();
                        host::Product {
                            id: r.product.to_string(),
                            option: r.option.to_string(),
                            load_id: r.feature.to_string(),
                            code: r.load_type == licpgm::id::LoadType::Code,
                            release: r.release.to_string(),
                            library,
                            text: r.text,
                        }
                    })
                    .collect())
            })
        })
    }

    fn ptfs(&self) -> Option<Result<Vec<host::Ptf>, String>> {
        use ptf::status::{Action, Status};
        with_system(|system| {
            let db = crate::cpp::ptfcmd::load_db(system)?;
            Ok(db
                .ptfs
                .values()
                .map(|r| host::Ptf {
                    id: r.ptf.clone(),
                    product: r.product.clone(),
                    option: r.option.clone(),
                    release: r.release.clone(),
                    status: match r.status {
                        Status::NotLoaded => "NOT LOADED",
                        Status::Loaded | Status::TempRemoved => "LOADED",
                        Status::TempApplied => "APPLIED",
                        Status::PermApplied => "PERMANENTLY APPLIED",
                        Status::PermRemoved => "PERMANENTLY REMOVED",
                        Status::Superseded => "SUPERSEDED",
                        Status::Damaged => "DAMAGED",
                        Status::OnOrder => "ON ORDER",
                        Status::SaveFileOnly => "SAVE FILE ONLY",
                        Status::CoverLetterOnly => "COVER LETTER ONLY",
                    }
                    .into(),
                    ipl_action: match r.action {
                        Action::ApplyAtIpl => "TEMPORARY APPLY",
                        Action::RemoveAtIpl => "TEMPORARY REMOVE",
                        Action::None => "NONE",
                    }
                    .into(),
                    delayed: r.delayed,
                    savf: !r.savf.is_empty(),
                    superseded_by: r.superseded_by.clone(),
                    changed: r.changed,
                })
                .collect())
        })
    }

    fn ifs_objects(&self, path: &str, subtree: bool) -> Option<Result<host::IfsWalk, String>> {
        self.with_ifs(|c| {
            let start = c.stat(path, false).map_err(|e| e.to_string())?;
            let mut out = vec![(path.to_string(), Ok(object(c, path, &start)))];
            if subtree && start.objtype == "*DIR" {
                let walk = c.walk(path, ifs::ops::WalkOpts::default(), usize::MAX).map_err(|e| e.to_string())?;
                for w in walk.into_iter().filter(|w| w.path != path) {
                    let o = w.stat.as_ref().map(|s| object(c, &w.path, s)).map_err(ToString::to_string);
                    out.push((w.path, o));
                }
            }
            Ok(out)
        })
    }

    fn ifs_read(&self, path: &str) -> Option<Result<(Vec<u8>, u16), String>> {
        self.with_ifs(|c| {
            let st = c.stat(path, true).map_err(|e| e.to_string())?;
            let fd = c.open(path, ifs::ops::OpenArgs::new(ifs::ops::flags::O_RDONLY)).map_err(|e| e.to_string())?;
            let data = ifs::client::read_all(c, fd);
            let _ = c.close(fd);
            Ok((data.map_err(|e| e.to_string())?, st.ccsid))
        })
    }

    fn ifs_write(&self, path: &str, data: &[u8], append: bool, ccsid: u16) -> Option<Result<(), String>> {
        use ifs::ops::flags::{O_APPEND, O_CCSID, O_CREAT, O_TRUNC, O_WRONLY};
        self.with_ifs(|c| {
            // A new file is tagged with `ccsid`; the bytes go as given.
            let flags = O_WRONLY | O_CREAT | O_CCSID | if append { O_APPEND } else { O_TRUNC };
            let args = ifs::ops::OpenArgs::new(flags).mode(0o644).conv(u32::from(ccsid));
            let fd = c.open(path, args).map_err(|e| e.to_string())?;
            let w = ifs::client::write_all(c, fd, data);
            let closed = c.close(fd);
            w.and(closed).map_err(|e| e.to_string())
        })
    }

    fn command(&self, cmd: &str) -> Option<Result<(), (String, String)>> {
        // BLOCKED(rel): commands whose processing program runs in the
        // job (CALL, the job's own commands) need the job and are
        // refused CPF0006 here; the reply's messages do not reach the
        // job log.
        let job = self.job;
        with_system(|system| {
            let prev = std::mem::replace(&mut system.current_job, job);
            let r = crate::cpp::opacmd::run_cl(system, cmd);
            system.current_job = prev;
            match r.escape {
                None => Ok(()),
                Some(e) => {
                    let text = r.text.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
                    Err((e.msgid, text))
                }
            }
        })
    }

    fn memory_pools(&self) -> Option<Vec<host::MemoryPool>> {
        with_system(super::stats::sql_pools)
    }

    fn disk_units(&self) -> Option<Vec<host::DiskUnit>> {
        with_system(super::stats::sql_disks)
    }

    fn hardware(&self) -> Option<Result<Vec<host::HardwareResource>, String>> {
        use hwrsrc::table::Kind;
        // The hardware resource service answers on its own socket.
        let t = match hwrsrc::client::table(false) {
            Ok(t) => t,
            Err(e) => return Some(Err(e.to_string())),
        };
        Some(Ok(t
            .resources
            .iter()
            .map(|r| host::HardwareResource {
                name: r.name.clone(),
                category: match r.kind {
                    Kind::Processor | Kind::Memory => "PROCESSOR",
                    Kind::Lan | Kind::Port | Kind::Comm | Kind::Channel => "COMM",
                    Kind::Keyboard => "LWS",
                    Kind::Storage | Kind::Disk | Kind::Optical | Kind::Tape | Kind::TapeLibrary => "STORAGE",
                    Kind::Bridge => "PACKAGING",
                    Kind::Other => "OTHER",
                }
                .into(),
                operational: r.operational(),
                type_model: r.type_model(),
                serial: r.serial.clone(),
                text: r.description.clone(),
                location: r.location.clone(),
                parent: r.parent.clone(),
                mac: r.port.as_ref().map_or_else(String::new, |p| p.mac.replace(':', "").to_ascii_uppercase()),
            })
            .collect()))
    }

    fn media_libraries(&self) -> Option<Vec<host::MediaLibrary>> {
        use wm::obj::devmed::MediaKind;
        with_system(|system| {
            super::media::devices::all(system)
                .unwrap_or_default()
                .into_iter()
                .filter(|(_, d)| d.media_kind() == Some(MediaKind::TapeLibrary))
                .map(|(n, d)| host::MediaLibrary {
                    device: n,
                    status: d.status.text().to_ascii_uppercase(),
                    type_model: (d.devtype.clone(), d.model.clone()),
                    resource: d.media.as_ref().map_or_else(String::new, |m| m.rsrcname.clone()),
                    text: d.text.clone(),
                })
                .collect()
        })
    }

    fn collection_services(&self) -> Option<host::CollectionServices> {
        with_system(|system| {
            let c = super::perfcol::config(system);
            let active = system.stats.perf.active.as_ref().map(|a| {
                (a.library.clone(), a.name.clone(), a.header.profile.clone(), a.header.started)
            });
            let cats = perf::profile::categories(&c.profile, &c.custom).unwrap_or_default();
            host::CollectionServices {
                active,
                library: c.library.clone(),
                profile: c.profile.clone(),
                interval: c.interval,
                retention_hours: c.retention.map(|(n, days)| u32::from(n) * if days { 24 } else { 1 }),
                cycle: (c.cycle_time.get(..2).and_then(|h| h.parse().ok()).unwrap_or(0), u32::from(c.cycle_hours)),
                create_files: c.create_files,
                categories: cats.iter().map(|k| k.name().to_string()).collect(),
            }
        })
    }

    fn exit_points(&self) -> Option<Result<Vec<host::ExitPoint>, String>> {
        with_system(|system| {
            let s = system.storage.as_mut()?;
            let qsys = s.qsys?;
            let reg = match crate::svc::db::with_txn(&mut s.store, |txn| regfac::store::load(txn, qsys).map_err(|e| e.to_string())) {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            Some(Ok(reg
                .points
                .iter()
                .map(|p| host::ExitPoint {
                    name: p.name.clone(),
                    format: p.format.clone(),
                    registered: p.registered,
                    allow_deregistration: p.allow_dereg,
                    allow_change: p.allow_change,
                    max_programs: p.max.and_then(|m| i32::try_from(m).ok()),
                    text: p.desc.as_text().to_string(),
                    programs: p
                        .programs
                        .iter()
                        .map(|g| host::ExitProgram {
                            number: g.number,
                            library: g.lib.clone(),
                            name: g.pgm.clone(),
                            text: g.desc.as_text().to_string(),
                            data: ccsid::decode_trimmed(&g.data),
                            threadsafe: g.threadsafe.special().into(),
                            action: g.mt_action.special().into(),
                        })
                        .collect(),
                })
                .collect()))
        })
        .flatten()
    }

    fn environment_variables(&self) -> Option<Vec<host::EnvVar>> {
        // BLOCKED(rel): the job's own variables live in the job; the
        // service reaches the system-level ones only.
        with_system(|system| {
            Some(
                super::qsh::system_vars(system)?
                .into_iter()
                .map(|v| host::EnvVar {
                    system: true,
                    name: v.name,
                    value: v.value.unwrap_or_default(),
                    ccsid: u16::try_from(v.ccsid).ok().filter(|c| *c != 0).unwrap_or(37),
                })
                .collect(),
            )
        })
        .flatten()
    }

    fn ptf_groups(&self) -> Option<Result<Vec<host::PtfGroup>, String>> {
        with_system(|system| {
            let db = crate::cpp::ptfcmd::load_db(system)?;
            let groups = crate::cpp::ptfcmd::load_groups(system)?;
            let keys = crate::cpp::trusted_keys();
            let release = release::RELEASE.to_string();
            Ok(groups
                .rows(&db, &keys)
                .into_iter()
                .map(|r| {
                    let ptfs = groups
                        .groups
                        .get(&r.name)
                        .and_then(|k| k.group(&keys))
                        .map(|(g, _)| g.ptfs)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|id| {
                            let (product, status) = db.ptfs.values().find(|p| p.ptf == id).map_or_else(
                                || (String::new(), "NOT INSTALLED".to_string()),
                                |p| (p.product.clone(), format!("{:?}", p.status).to_ascii_uppercase()),
                            );
                            (id, product, status)
                        })
                        .collect();
                    host::PtfGroup {
                        name: r.name,
                        text: r.text,
                        level: r.level,
                        release: release.clone(),
                        status: r.status.display().to_ascii_uppercase(),
                        ptfs,
                    }
                })
                .collect())
        })
    }

    fn connection(&self) -> Option<host::Connection> {
        // The service does not know a job's socket: the addresses are
        // null.
        Some(host::Connection {
            host_name: std::fs::read_to_string("/proc/sys/kernel/hostname").map(|h| h.trim().to_string()).unwrap_or_default(),
            client: None,
            server: None,
        })
    }

    fn dns_lookup(&self, name: &str, _server: Option<&str>) -> Option<Result<Vec<std::net::IpAddr>, String>> {
        use std::net::ToSocketAddrs as _;
        // DOMAIN_SERVER is refused before the host is asked.
        // BLOCKED(rel): the lookup holds the service until the resolver
        // answers; it does not yet take turns.
        Some(match (name, 0u16).to_socket_addrs() {
            Ok(addrs) => {
                let mut v: Vec<std::net::IpAddr> = Vec::new();
                for a in addrs {
                    if !v.contains(&a.ip()) {
                        v.push(a.ip());
                    }
                }
                Ok(v)
            }
            Err(e) => Err(format!("{name}: {e}")),
        })
    }

    fn ifs_authorities(&self, path: &str) -> Option<Result<host::IfsAuthorities, String>> {
        self.with_ifs(|c| {
            let a = c.dspaut(path).map_err(|e| e.to_string())?;
            Ok(host::IfsAuthorities {
                kind: a.objtype,
                owner: a.owner,
                group: a.group.unwrap_or_default(),
                autl: a.autl.unwrap_or_default(),
                entries: a.entries.into_iter().map(|e| (e.who, e.data, e.auth.0)).collect(),
            })
        })
    }

    fn ifs_opens(&self, path: &str) -> Option<Result<Vec<host::IfsOpen>, String>> {
        use ifs::lock::share::Share;
        self.with_ifs(|c| {
            let locks = c.object_locks(path).map_err(|e| e.to_string())?;
            Ok(locks
                .into_iter()
                .filter_map(|l| match l.what {
                    ifs::lock::Held::Open(a, s) => Some(host::IfsOpen {
                        job: jrn::JobInfo::from_handle(store::obj::JobHandle(l.owner.0)).qualified(),
                        access: (a.read, a.write),
                        share: match s {
                            Share::ReadOnly => "R",
                            Share::WriteOnly => "W",
                            Share::ReadWrite => "RW",
                            Share::None => "NONE",
                        }
                        .into(),
                    }),
                    ifs::lock::Held::Range(_) => None,
                })
                .collect())
        })
    }
}

impl SqlHost {
    /// Run `f` against the IFS as the asking job.
    fn with_ifs<T>(&self, f: impl FnOnce(&dyn ifs::client::IfsClient) -> Result<T, String>) -> Option<Result<T, String>> {
        let principal = super::turns::principal_of(self.job)?;
        let job = self.job;
        with_system(|system| {
            let handle = system.jobs.get(job).map_or(u64::from(job), |j| j.handle);
            let ccsid = super::ifs::job_ccsid(system, job);
            super::ifs::with_client(system, handle, super::ifs::owner_of(handle), &principal, ccsid, f)
        })
    }
}

/// Nanoseconds since the epoch as whole seconds.
fn secs(ns: i64) -> u64 {
    u64::try_from(ns.div_euclid(1_000_000_000)).unwrap_or(0)
}

/// An IFS object as the services describe it.
fn object(c: &dyn ifs::client::IfsClient, path: &str, s: &ifs::ops::Stat) -> host::IfsObject {
    host::IfsObject {
        path: path.to_string(),
        kind: s.objtype.clone(),
        owner: s.owner.clone(),
        group: s.group.clone().unwrap_or_default(),
        size: s.size,
        allocated: s.allocsize,
        created: secs(s.crtime),
        changed: secs(s.ctime),
        accessed: secs(s.atime),
        data_changed: secs(s.mtime),
        ccsid: s.ccsid,
        links: s.nlink,
        file_id: s.ino,
        mode: s.mode,
        link_target: if s.objtype == "*SYMLNK" { c.readlink(path).unwrap_or_default() } else { String::new() },
        text: String::new(),
        autl: String::new(),
    }
}
