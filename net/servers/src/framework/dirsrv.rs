// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*DIRSRV`: the directory server's instances (docs/DIRSRV.md §4).
//! `INSTANCE` names them as `idsinstances.ldif` lists them: `*DFT` is
//! `QUSRDIR`, `*AUTOSTART` the ones with `ids-instanceAutostart: 1`,
//! `*ALL` every one, a name that one; an unknown name is `GLD0203`.
//! Each instance is one job named after it, user `QDIRSRV`, job
//! description `QSYS/QDIRSRV`, in `QSYSWRK` ([`DirSrv`]); `STRTCP`
//! starts the autostart instances when `*DIRSRV` autostarts, and
//! `ENDTCP` ends every running one.

use super::{Env, Instance, JobStart, Refusal, Request, TcpServer};
use crate::msg;
use crate::registry::{self, ServerSpec};

/// The instance job's role (`work/qsjob/src/work/dirsrv/instance.rs`).
pub const ROLE: &str = "DIRSRV";
/// The instance jobs' user profile.
pub const USER: &str = "QDIRSRV";
/// Their subsystem.
pub const SUBSYSTEM: &str = "QSYSWRK";
/// Their job description.
pub const JOBD: &str = "QSYS/QDIRSRV";

/// `QUSRDIR`, the instance every system has.
pub const DEFAULT_INSTANCE: &str = "QUSRDIR";

/// `Directory server instance &1 not found.` (`QGLDMSG`).
pub const NO_INSTANCE: &str = "GLD0203";

/// A directory server instance as `STRTCPSVR` sees it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirInstance {
    /// The name (and the job's), upper case.
    pub name: String,
    /// `ids-instanceAutostart: 1`.
    pub autostart: bool,
}

/// The instance list a system starts with.
#[must_use]
pub fn shipped() -> Vec<DirInstance> {
    vec![DirInstance {
        name: DEFAULT_INSTANCE.into(),
        autostart: true,
    }]
}

/// The instances `instance` names, from the job's instance list.
///
/// # Errors
///
/// `GLD0203` for a name the list does not have (and for `*DFT` when
/// `QUSRDIR` is gone).
pub fn resolve(env: &mut dyn Env, instance: &Instance) -> Result<Vec<DirInstance>, Refusal> {
    let list = env.dir_instances();
    let one = |n: &str| {
        let n = n.trim().to_ascii_uppercase();
        list.iter()
            .find(|i| i.name.eq_ignore_ascii_case(&n))
            .map(|i| vec![i.clone()])
            .ok_or_else(|| {
                Refusal::new(
                    NO_INSTANCE,
                    format!("Directory server instance {n} not found."),
                )
            })
    };
    match instance {
        Instance::Default => one(DEFAULT_INSTANCE),
        Instance::Named(n) => one(n),
        Instance::All => Ok(list.clone()),
        Instance::Autostart => Ok(list.iter().filter(|i| i.autostart).cloned().collect()),
    }
}

/// The job of instance `name`.
#[must_use]
pub fn instance_job(name: &str) -> JobStart {
    JobStart {
        name: name.to_ascii_uppercase(),
        user: USER.into(),
        subsystem: SUBSYSTEM.into(),
        role: ROLE.into(),
        rqsdta: format!("INSTANCE({})", name.to_ascii_uppercase()),
        jobd: JOBD.into(),
        class: String::new(),
    }
}

/// `*DIRSRV`'s hooks.
pub struct DirSrv;

fn join(lines: &[String]) -> String {
    lines
        .iter()
        .filter(|l| !l.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(
            "
",
        )
}

impl TcpServer for DirSrv {
    fn spec(&self) -> &'static ServerSpec {
        registry::lookup("*DIRSRV").expect("in the registry")
    }

    /// A named instance's job; otherwise any instance's.
    fn active(&self, env: &mut dyn Env, req: &Request) -> bool {
        let list = match &req.instance {
            Instance::Named(_) => resolve(env, &req.instance).unwrap_or_default(),
            _ => env.dir_instances(),
        };
        list.iter().any(|i| env.active(&i.name))
    }

    fn start(&self, env: &mut dyn Env, req: &Request) -> Result<String, Refusal> {
        let list = resolve(env, &req.instance)?;
        if req.instance == Instance::Autostart && !env.attrs("*DIRSRV").autostart {
            return Ok(String::new());
        }
        let mut lines = Vec::new();
        let mut started = 0;
        let mut first_refusal = None;
        for i in &list {
            if env.active(&i.name) {
                lines.push(format!(
                    "{} DIRSRV server already started: instance {}.",
                    msg::ALREADY,
                    i.name
                ));
                continue;
            }
            match env.start_job(&instance_job(&i.name)) {
                Ok(()) => {
                    started += 1;
                    lines.push(format!(
                        "{} DIRSRV server starting: instance {}.",
                        msg::STARTING,
                        i.name
                    ));
                }
                Err(e) => {
                    let r = Refusal::new(
                        msg::NOT_STARTED,
                        format!(
                            "Directory server instance {} could not be started: {e}",
                            i.name
                        ),
                    );
                    lines.push(r.line());
                    first_refusal.get_or_insert(r);
                }
            }
        }
        match first_refusal {
            Some(e) if started == 0 => Err(e),
            _ => Ok(join(&lines)),
        }
    }

    fn end(&self, env: &mut dyn Env, req: &Request, immediate: bool) -> Result<String, Refusal> {
        let list = resolve(env, &req.instance)?;
        let mut lines = Vec::new();
        for i in &list {
            if !env.active(&i.name) {
                continue;
            }
            env.end_job(&i.name, immediate).map_err(|e| {
                Refusal::new(
                    msg::NOT_ACTIVE,
                    format!("Directory server instance {} not ended: {e}", i.name),
                )
            })?;
            lines.push(format!(
                "{} DIRSRV server ending: instance {}.",
                msg::ENDING,
                i.name
            ));
        }
        if lines.is_empty() {
            return Ok(format!("{} DIRSRV server not active.", msg::NOT_ACTIVE));
        }
        Ok(join(&lines))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Framework, JobStart, Request, Starter};
    use super::*;
    use tcpcfg::model::svr::ServerAttrs;
    use tcpcfg::server::ServerStarter;

    #[derive(Default)]
    struct Two {
        running: Vec<String>,
        started: Vec<JobStart>,
        autostart: bool,
    }

    impl Env for Two {
        fn active(&mut self, job: &str) -> bool {
            self.running.iter().any(|r| r == job)
        }
        fn start_job(&mut self, j: &JobStart) -> Result<(), String> {
            self.running.push(j.name.clone());
            self.started.push(j.clone());
            Ok(())
        }
        fn end_job(&mut self, job: &str, _: bool) -> Result<(), String> {
            self.running.retain(|r| r != job);
            Ok(())
        }
        fn attrs(&mut self, server: &str) -> ServerAttrs {
            ServerAttrs {
                server: server.into(),
                autostart: self.autostart,
                bag: Vec::new(),
            }
        }
        fn start_subsystem(&mut self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn dir_instances(&mut self) -> Vec<DirInstance> {
            let mut v = shipped();
            v.push(DirInstance {
                name: "OTHER".into(),
                autostart: false,
            });
            v
        }
    }

    fn req(i: Instance) -> Request {
        Request {
            instance: i,
            ..Request::new("*DIRSRV")
        }
    }

    fn fw() -> Framework {
        let mut f = Framework::new();
        f.register(Box::new(DirSrv));
        f
    }

    #[test]
    fn instances_resolve() {
        let names = |i| {
            resolve(&mut Two::default(), &i)
                .unwrap()
                .into_iter()
                .map(|d| d.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(Instance::Default), ["QUSRDIR"]);
        assert_eq!(names(Instance::All), ["QUSRDIR", "OTHER"]);
        assert_eq!(names(Instance::Autostart), ["QUSRDIR"]);
        assert_eq!(names(Instance::Named("other".into())), ["OTHER"]);
        let mut env = Two::default();
        let e = fw()
            .start(&mut env, &req(Instance::Named("NOPE".into())))
            .unwrap_err();
        assert_eq!(e.msgid, NO_INSTANCE);
        assert!(e.text.contains("NOPE"));
        let e = fw()
            .end(&mut env, &req(Instance::Named("NOPE".into())), false)
            .unwrap_err();
        assert_eq!(e.msgid, NO_INSTANCE);
    }

    #[test]
    fn one_job_per_instance() {
        let f = fw();
        let mut env = Two::default();
        let ok = f.start(&mut env, &req(Instance::Default)).unwrap();
        assert!(ok.contains("starting: instance QUSRDIR"), "{ok}");
        let j = &env.started[0];
        assert_eq!(
            (j.name.as_str(), j.user.as_str(), j.subsystem.as_str()),
            ("QUSRDIR", "QDIRSRV", "QSYSWRK")
        );
        assert_eq!((j.role.as_str(), j.jobd.as_str()), (ROLE, JOBD));
        assert_eq!(j.rqsdta, "INSTANCE(QUSRDIR)");
        let ok = f.start(&mut env, &req(Instance::All)).unwrap();
        assert!(ok.contains("already started: instance QUSRDIR"), "{ok}");
        assert!(ok.contains("starting: instance OTHER"), "{ok}");
        assert_eq!(env.running, ["QUSRDIR", "OTHER"]);
        assert!(f.active(&mut env, "*DIRSRV"));
        let ok = f
            .end(&mut env, &req(Instance::Named("OTHER".into())), false)
            .unwrap();
        assert!(ok.contains("ending: instance OTHER"), "{ok}");
        let ok = f
            .end(&mut env, &req(Instance::Named("OTHER".into())), false)
            .unwrap();
        assert!(ok.contains("DIRSRV server not active"), "{ok}");
    }

    #[test]
    fn autostart_needs_the_server_attribute() {
        let f = fw();
        let mut env = Two::default();
        assert_eq!(f.start(&mut env, &req(Instance::Autostart)).unwrap(), "");
        assert!(env.started.is_empty());
        env.autostart = true;
        Starter::new(&f, &mut env).start("*DIRSRV").unwrap();
        assert_eq!(env.running, ["QUSRDIR"]);
        env.running.push("OTHER".into());
        Starter::new(&f, &mut env).end("*DIRSRV", false).unwrap();
        assert!(env.running.is_empty(), "ENDTCP ends every instance");
    }
}
