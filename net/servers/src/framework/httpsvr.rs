// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*HTTP` with its instances. `HTTPSVR(*ADMIN)` is the administration
//! instance ([`HttpAdmin`]); `HTTPSVR(NAVIGATOR)` is Navigator's, job
//! `NAVIGATOR` (role `HTTPNAV`), there only while `QS00NAV` is
//! installed; any other name is an instance of the registry
//! (`/www/instances.json`, read through [`Env::http_instances`]): one
//! job named after it in `QHTTPSVR` under its `ServerUserID`, role
//! [`HTTP_ROLE`]. `HTTPSVR(*ALL)` starts the administration instance
//! and every instance marked autostart and ends every running one;
//! `STRTCP` (`*AUTOSTART`) starts the administration instance when
//! `*HTTP` autostarts and every instance marked autostart.

use super::{Env, HttpAdmin, HttpInstance, Instance, JobStart, Refusal, Request, TcpServer};
use crate::attrs::http::HttpAttrs;
use crate::msg;
use crate::registry::{self, ServerSpec};

/// The job role of an instance (`work/qsjob/src/work/httpsvr.rs`).
pub const HTTP_ROLE: &str = "HTTPSVR";
/// The instances' subsystem.
pub const HTTP_SUBSYSTEM: &str = "QHTTPSVR";
/// Seconds a controlled end gives requests in progress when `DELAY`
/// is not given.
pub const DEFAULT_DELAY: u32 = 30;

/// Navigator's instance (and job) name. A registry entry of this name
/// is Navigator's own (made by the `ADMIN` job), never a user instance.
pub const NAVIGATOR: &str = "NAVIGATOR";
/// The licensed program Navigator is.
pub const NAVIGATOR_PRODUCT: &str = "QS00NAV";
/// Navigator's job role (`work/qsjob/src/work/httpnav/`).
const NAVIGATOR_ROLE: &str = "HTTPNAV";

/// `*HTTP`: the administration instance and the registry's instances.
pub struct HttpServer;

/// Whether `name` means the administration instance.
fn is_admin(name: &str) -> bool {
    name.eq_ignore_ascii_case("*ADMIN") || name.eq_ignore_ascii_case("ADMIN")
}

/// Whether `name` means Navigator's instance.
fn is_navigator(name: &str) -> bool {
    name.trim().eq_ignore_ascii_case(NAVIGATOR)
}

/// Navigator's job, its ports from `CHGHTTPA`'s `NAVPORT` and
/// `NAVTLSPORT`.
fn navigator_job(env: &mut dyn Env) -> JobStart {
    let spec = registry::lookup("*HTTP").expect("in the registry");
    let bag = env.attrs(spec.special).bag;
    let job = spec.jobs.iter().find(|j| j.role == NAVIGATOR_ROLE);
    JobStart {
        name: job.map_or(NAVIGATOR, |j| j.name).into(),
        user: job.map_or("QTMHHTTP", |j| j.user).into(),
        subsystem: HTTP_SUBSYSTEM.into(),
        role: NAVIGATOR_ROLE.into(),
        rqsdta: HttpAttrs::navigator(&bag).rqsdta(),
        ..JobStart::default()
    }
}

/// Start Navigator: refused as an unknown instance while `QS00NAV`
/// is not installed.
fn start_navigator(env: &mut dyn Env) -> Result<String, Refusal> {
    if !env.product_installed(NAVIGATOR_PRODUCT) {
        return Err(Refusal::new(
            msg::NO_INSTANCE,
            format!("Server instance {NAVIGATOR} of *HTTP not found: {NAVIGATOR_PRODUCT} is not installed."),
        ));
    }
    if env.active(NAVIGATOR) {
        return Err(Refusal::new(msg::ALREADY, format!("HTTP server instance {NAVIGATOR} already started.")));
    }
    if let Err(e) = env.start_subsystem(HTTP_SUBSYSTEM) {
        tracing::warn!("*HTTP: subsystem {HTTP_SUBSYSTEM}: {e}");
    }
    let job = navigator_job(env);
    env.start_job(&job).map_err(|e| {
        Refusal::new(msg::NOT_STARTED, format!("HTTP server instance {NAVIGATOR} could not be started: {e}"))
    })?;
    Ok(format!("{} HTTP server starting: instance {NAVIGATOR}.", msg::STARTING))
}

fn admin_req(req: &Request) -> Request {
    Request {
        instance: match &req.instance {
            Instance::Named(_) => Instance::Named("*ADMIN".into()),
            other => other.clone(),
        },
        ..req.clone()
    }
}

/// The job of instance `i`.
pub fn instance_job(i: &HttpInstance) -> JobStart {
    JobStart {
        name: i.name.clone(),
        user: if i.user.is_empty() { "QTMHHTTP".into() } else { i.user.clone() },
        subsystem: HTTP_SUBSYSTEM.into(),
        role: HTTP_ROLE.into(),
        rqsdta: format!("INSTANCE({})", i.name),
        ..JobStart::default()
    }
}

impl HttpServer {
    fn find(env: &mut dyn Env, name: &str) -> Result<HttpInstance, Refusal> {
        env.http_instances()
            .into_iter()
            .find(|i| i.name.eq_ignore_ascii_case(name.trim()))
            .ok_or_else(|| {
                Refusal::new(
                    msg::NO_INSTANCE,
                    format!("Server instance {} of *HTTP not found.", name.trim().to_ascii_uppercase()),
                )
            })
    }

    fn start_one(env: &mut dyn Env, i: &HttpInstance) -> Result<String, Refusal> {
        if let Err(e) = env.start_subsystem(HTTP_SUBSYSTEM) {
            tracing::warn!("*HTTP: subsystem {HTTP_SUBSYSTEM}: {e}");
        }
        env.start_job(&instance_job(i)).map_err(|e| {
            Refusal::new(msg::NOT_STARTED, format!("HTTP server instance {} could not be started: {e}", i.name))
        })?;
        Ok(format!("{} HTTP server starting: instance {}.", msg::STARTING, i.name))
    }

    fn end_one(env: &mut dyn Env, name: &str, immediate: bool, delay: u32) -> Result<String, Refusal> {
        env.http_ending(name, immediate, delay);
        env.end_job(name, immediate)
            .map_err(|e| Refusal::new(msg::NOT_ACTIVE, format!("HTTP server instance {name} not ended: {e}")))?;
        Ok(format!("{} HTTP server ending: instance {name}.", msg::ENDING))
    }

    /// Every registry instance (not the administration one or
    /// Navigator's).
    fn instances(env: &mut dyn Env) -> Vec<HttpInstance> {
        env.http_instances()
            .into_iter()
            .filter(|i| !is_admin(&i.name) && !is_navigator(&i.name))
            .collect()
    }

    /// Whether Navigator's registry entry is marked autostart (it is
    /// not when made).
    fn navigator_autostart(env: &mut dyn Env) -> bool {
        env.http_instances().iter().any(|i| is_navigator(&i.name) && i.autostart)
    }
}

fn join(lines: Vec<String>) -> String {
    lines.into_iter().filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

impl TcpServer for HttpServer {
    fn spec(&self) -> &'static ServerSpec {
        registry::lookup("*HTTP").expect("in the registry")
    }

    fn active(&self, env: &mut dyn Env, req: &Request) -> bool {
        match &req.instance {
            Instance::Named(n) if is_admin(n) => HttpAdmin.active(env, req),
            Instance::Named(n) => env.active(&n.trim().to_ascii_uppercase()),
            _ => {
                HttpAdmin.active(env, req)
                    || env.active(NAVIGATOR)
                    || Self::instances(env).iter().any(|i| env.active(&i.name))
            }
        }
    }

    fn start(&self, env: &mut dyn Env, req: &Request) -> Result<String, Refusal> {
        match &req.instance {
            Instance::Named(n) if is_admin(n) => HttpAdmin.start(env, &admin_req(req)),
            Instance::Named(n) if is_navigator(n) => start_navigator(env),
            Instance::Named(n) => {
                let i = Self::find(env, n)?;
                if env.active(&i.name) {
                    return Err(Refusal::new(
                        msg::ALREADY,
                        format!("HTTP server instance {} already started.", i.name),
                    ));
                }
                Self::start_one(env, &i)
            }
            Instance::Default | Instance::All | Instance::Autostart => {
                let mut lines = Vec::new();
                let mut first_refusal = None;
                let admin_up = HttpAdmin.active(env, req);
                if !(req.instance == Instance::Autostart && admin_up) {
                    match HttpAdmin.start(env, req) {
                        Ok(t) => lines.push(t),
                        Err(e) => first_refusal = Some(e),
                    }
                }
                let mut started = 0;
                if Self::navigator_autostart(env)
                    && !env.active(NAVIGATOR)
                    && env.product_installed(NAVIGATOR_PRODUCT)
                {
                    match start_navigator(env) {
                        Ok(t) => {
                            started += 1;
                            lines.push(t);
                        }
                        Err(e) => lines.push(e.line()),
                    }
                }
                for i in Self::instances(env).into_iter().filter(|i| i.autostart) {
                    if env.active(&i.name) {
                        continue;
                    }
                    match Self::start_one(env, &i) {
                        Ok(t) => {
                            started += 1;
                            lines.push(t);
                        }
                        Err(e) => lines.push(e.line()),
                    }
                }
                match first_refusal {
                    Some(e) if started == 0 => Err(e),
                    Some(e) => {
                        lines.insert(0, e.line());
                        Ok(join(lines))
                    }
                    None => Ok(join(lines)),
                }
            }
        }
    }

    fn end(&self, env: &mut dyn Env, req: &Request, immediate: bool) -> Result<String, Refusal> {
        let delay = req.delay.unwrap_or(DEFAULT_DELAY);
        match &req.instance {
            Instance::Named(n) if is_admin(n) => HttpAdmin.end(env, &admin_req(req), immediate),
            Instance::Named(n) if is_navigator(n) => {
                if !env.active(NAVIGATOR) {
                    return Err(Refusal::new(
                        msg::NOT_ACTIVE,
                        format!("HTTP server instance {NAVIGATOR} not active."),
                    ));
                }
                Self::end_one(env, NAVIGATOR, immediate, delay)
            }
            Instance::Named(n) => {
                let i = Self::find(env, n)?;
                if !env.active(&i.name) {
                    return Err(Refusal::new(
                        msg::NOT_ACTIVE,
                        format!("HTTP server instance {} not active.", i.name),
                    ));
                }
                Self::end_one(env, &i.name, immediate, delay)
            }
            Instance::Default | Instance::All | Instance::Autostart => {
                let mut lines = Vec::new();
                if HttpAdmin.active(env, req) {
                    lines.push(HttpAdmin.end(env, req, immediate).unwrap_or_else(|e| e.line()));
                }
                if env.active(NAVIGATOR) {
                    lines.push(Self::end_one(env, NAVIGATOR, immediate, delay).unwrap_or_else(|e| e.line()));
                }
                for i in Self::instances(env) {
                    if env.active(&i.name) {
                        lines.push(Self::end_one(env, &i.name, immediate, delay).unwrap_or_else(|e| e.line()));
                    }
                }
                if lines.is_empty() {
                    return Ok(format!("{} HTTP server not active.", msg::NOT_ACTIVE));
                }
                Ok(join(lines))
            }
        }
    }
}
