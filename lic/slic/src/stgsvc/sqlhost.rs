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
        // BLOCKED(sql): the request holds the service until it answers
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
