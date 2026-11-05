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

use sql::exec::catalog::services::host::{ActiveJob, HttpRequest, HttpResponse, SysValue, SystemHost, SystemStatus};

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
}
