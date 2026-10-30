// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Starting and ending servers. A server implements [`TcpServer`]
//! (usually only [`TcpServer::spec`], taking the defaults); the job
//! that runs `STRTCPSVR`, `ENDTCPSVR`, `STRTCP` or `ENDTCP` gives the
//! hooks an [`Env`]; the [`Framework`] holds the registered servers and
//! answers the commands. [`Starter`] is the framework as `tcpcfg`'s
//! `ServerStarter`.

mod builtin;
mod small;
mod starter;

pub use builtin::{Ftp, HttpAdmin, Standard, Telnet};
pub use small::{Lpd, Ntp};
pub use starter::Starter;

use tcpcfg::model::svr::ServerAttrs;

use crate::msg;
use crate::registry::{self, JobKind, Readiness, ServerSpec};

/// Why a server did not start or end: a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// The identifier.
    pub msgid: String,
    /// The first-level text.
    pub text: String,
}

impl Refusal {
    /// A refusal.
    pub fn new(msgid: &str, text: impl Into<String>) -> Self {
        Self {
            msgid: msgid.to_string(),
            text: text.into(),
        }
    }

    /// `msgid text`.
    pub fn line(&self) -> String {
        format!("{} {}", self.msgid, self.text)
    }
}

/// Which of a server's instances a command means (`INSTANCE`,
/// `HTTPSVR`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Instance {
    /// The server's only one, or its default.
    Default,
    /// `*ALL`.
    All,
    /// `*AUTOSTART`: the ones that start with TCP/IP.
    Autostart,
    /// One by name.
    Named(String),
}

/// One server named on a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// The special value (`*TELNET`).
    pub server: String,
    /// The instance.
    pub instance: Instance,
}

impl Request {
    /// The default instance of `server`.
    pub fn new(server: &str) -> Self {
        Self {
            server: server.trim().to_ascii_uppercase(),
            instance: Instance::Default,
        }
    }

    /// The autostart instances of `server` (as `STRTCP` starts them).
    pub fn autostart(server: &str) -> Self {
        Self {
            instance: Instance::Autostart,
            ..Self::new(server)
        }
    }
}

/// A server job for the job that starts it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobStart {
    /// The job name.
    pub name: String,
    /// The user profile.
    pub user: String,
    /// The subsystem.
    pub subsystem: String,
    /// The job process's role.
    pub role: String,
    /// Request data the role reads (`PORT(2001)`).
    pub rqsdta: String,
}

/// What the hooks reach in the job that runs the command.
pub trait Env {
    /// Whether a job of this name is active.
    fn active(&mut self, job: &str) -> bool;
    /// Start a server job.
    fn start_job(&mut self, job: &JobStart) -> Result<(), String>;
    /// End the active job of this name; `immediate` as `*IMMED`.
    fn end_job(&mut self, job: &str, immediate: bool) -> Result<(), String>;
    /// A server's autostart and attributes from the `QUSRSYS` server
    /// table.
    fn attrs(&mut self, server: &str) -> ServerAttrs;
    /// Start a subsystem unless it is active.
    fn start_subsystem(&mut self, name: &str) -> Result<(), String>;
}

/// A server's hooks. Every method but [`TcpServer::spec`] has a
/// default: start the listener job of the registry entry, end every
/// job the framework started, active while the listener runs.
pub trait TcpServer {
    /// Its registry entry.
    fn spec(&self) -> &'static ServerSpec;

    /// The jobs `STRTCPSVR` starts for `req`, given the server's
    /// attributes; a refusal when the request does not fit.
    fn jobs(&self, req: &Request, attrs: &ServerAttrs) -> Result<Vec<JobStart>, Refusal> {
        let _ = attrs;
        check_instance(self.spec(), req)?;
        Ok(listener_job(self.spec(), String::new()).into_iter().collect())
    }

    /// Whether the server runs.
    fn active(&self, env: &mut dyn Env, req: &Request) -> bool {
        let _ = req;
        self.spec().listener().is_some_and(|l| env.active(l.name))
    }

    /// `STRTCPSVR`: the completion text.
    fn start(&self, env: &mut dyn Env, req: &Request) -> Result<String, Refusal> {
        let spec = self.spec();
        check_instance(spec, req)?;
        if self.active(env, req) {
            return Ok(format!("{} {} server already started.", msg::ALREADY, spec.name));
        }
        let attrs = env.attrs(spec.special);
        if req.instance == Instance::Autostart && !attrs.autostart {
            return Ok(String::new());
        }
        let jobs = self.jobs(req, &attrs)?;
        if spec.start_subsystem
            && let Some(sbs) = jobs.first().map(|j| j.subsystem.clone())
            && let Err(e) = env.start_subsystem(&sbs)
        {
            tracing::warn!("{}: subsystem {sbs}: {e}", spec.special);
        }
        for j in &jobs {
            env.start_job(j)
                .map_err(|e| Refusal::new(msg::NOT_STARTED, format!("{} could not be started: {e}", spec.special)))?;
        }
        Ok(format!("{} {} server starting.", msg::STARTING, spec.name))
    }

    /// `ENDTCPSVR`: the completion text.
    fn end(&self, env: &mut dyn Env, req: &Request, immediate: bool) -> Result<String, Refusal> {
        let spec = self.spec();
        check_instance(spec, req)?;
        if !self.active(env, req) {
            return Ok(format!("{} {} server not active.", msg::NOT_ACTIVE, spec.name));
        }
        for j in spec.jobs.iter().filter(|j| j.kind == JobKind::Listener) {
            if env.active(j.name) {
                env.end_job(j.name, immediate)
                    .map_err(|e| Refusal::new(msg::NOT_ACTIVE, format!("{} not ended: {e}", spec.special)))?;
            }
        }
        Ok(format!("{} {} server ending.", msg::ENDING, spec.name))
    }
}

/// The listener job of `spec` with request data `rqsdta`.
pub fn listener_job(spec: &ServerSpec, rqsdta: String) -> Option<JobStart> {
    spec.listener().map(|l| JobStart {
        name: l.numbered(1),
        user: l.user.to_string(),
        subsystem: l.subsystem.to_string(),
        role: l.role.to_string(),
        rqsdta,
    })
}

/// A named instance must be one the server has.
pub fn check_instance(spec: &ServerSpec, req: &Request) -> Result<(), Refusal> {
    match &req.instance {
        Instance::Named(n) if !spec.instances.iter().any(|i| i.eq_ignore_ascii_case(n)) => Err(Refusal::new(
            msg::NO_INSTANCE,
            format!("Server instance {n} of {} not found.", spec.special),
        )),
        _ => Ok(()),
    }
}

/// The servers this job can start, by special value.
#[derive(Default)]
pub struct Framework {
    servers: Vec<Box<dyn TcpServer>>,
}

impl Framework {
    /// No server registered.
    pub fn new() -> Self {
        Self::default()
    }

    /// The servers this crate builds: `*TELNET` and `*HTTP`.
    pub fn standard() -> Self {
        let mut f = Self::new();
        f.register(Box::new(Telnet));
        f.register(Box::new(HttpAdmin));
        f
    }

    /// Add a server, replacing one with the same special value.
    pub fn register(&mut self, s: Box<dyn TcpServer>) {
        let special = s.spec().special;
        self.servers.retain(|x| x.spec().special != special);
        self.servers.push(s);
    }

    /// The registered server of `special`.
    pub fn get(&self, special: &str) -> Option<&dyn TcpServer> {
        self.servers
            .iter()
            .find(|s| s.spec().special.eq_ignore_ascii_case(special.trim()))
            .map(AsRef::as_ref)
    }

    /// The registered special values, in the registry's order.
    pub fn registered(&self) -> Vec<&'static str> {
        registry::SPECS
            .iter()
            .filter(|s| self.get(s.special).is_some())
            .map(|s| s.special)
            .collect()
    }

    /// Why `special` cannot be used: unknown or not in this release.
    fn unavailable(special: &str) -> Refusal {
        let note = match registry::lookup(special).map(|s| s.readiness) {
            Some(Readiness::Slot(what) | Readiness::Blocked(what)) => format!(" It waits for {what}."),
            _ => String::new(),
        };
        Refusal::new(
            msg::NOT_AVAILABLE,
            format!("{special} cannot be started in this release.{note}"),
        )
    }

    /// `STRTCPSVR` for one server.
    pub fn start(&self, env: &mut dyn Env, req: &Request) -> Result<String, Refusal> {
        match self.get(&req.server) {
            Some(s) => s.start(env, req),
            None => Err(Self::unavailable(&req.server)),
        }
    }

    /// `ENDTCPSVR` for one server. A server this release does not run
    /// is never active.
    pub fn end(&self, env: &mut dyn Env, req: &Request, immediate: bool) -> Result<String, Refusal> {
        match self.get(&req.server) {
            Some(s) => s.end(env, req, immediate),
            None if registry::lookup(&req.server).is_some() => {
                let name = registry::lookup(&req.server).map_or("", |s| s.name);
                Ok(format!("{} {name} server not active.", msg::NOT_ACTIVE))
            }
            None => Err(Self::unavailable(&req.server)),
        }
    }

    /// Whether a server runs.
    pub fn active(&self, env: &mut dyn Env, server: &str) -> bool {
        self.get(server)
            .is_some_and(|s| s.active(env, &Request::new(server)))
    }

    /// The servers a `SERVER` list means: `*ALL` is every registered
    /// server (for an end, the active ones), `*AUTOSTART` the
    /// registered ones whose autostart is on.
    pub fn expand(&self, env: &mut dyn Env, list: &[String], starting: bool) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut add = |s: String| {
            if !out.contains(&s) {
                out.push(s);
            }
        };
        for item in list {
            match item.trim().to_ascii_uppercase().as_str() {
                "*ALL" => {
                    for s in self.registered() {
                        if starting || self.active(env, s) {
                            add(s.to_string());
                        }
                    }
                }
                "*AUTOSTART" => {
                    for s in self.registered() {
                        if env.attrs(s).autostart {
                            add(s.to_string());
                        }
                    }
                }
                "" => {}
                other => add(other.to_string()),
            }
        }
        out
    }

    /// `STRTCPSVR SERVER(list)` or `ENDTCPSVR SERVER(list)`: one answer
    /// per server.
    pub fn run(
        &self,
        env: &mut dyn Env,
        list: &[String],
        instance: &Instance,
        starting: bool,
        immediate: bool,
    ) -> Vec<(String, Result<String, Refusal>)> {
        let autostart = list.iter().any(|s| s.trim().eq_ignore_ascii_case("*AUTOSTART"));
        self.expand(env, list, starting)
            .into_iter()
            .map(|server| {
                let req = Request {
                    instance: if autostart {
                        Instance::Autostart
                    } else {
                        instance.clone()
                    },
                    ..Request::new(&server)
                };
                let r = if starting {
                    self.start(env, &req)
                } else {
                    self.end(env, &req, immediate)
                };
                (server, r)
            })
            .collect()
    }
}
