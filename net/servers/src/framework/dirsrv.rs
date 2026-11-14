// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*DIRSRV`: the directory server's instances (docs/DIRSRV.md §4).
//! `INSTANCE` names them as `idsinstances.ldif` lists them: `*DFT` is
//! `QUSRDIR`, `*AUTOSTART` the ones with `ids-instanceAutostart: 1`,
//! `*ALL` every one, a name that one; an unknown name is `GLD0203`.
//! The instance job is a slot ([`crate::registry::Readiness::Slot`]):
//! until it is registered, [`super::Framework`] resolves the instances
//! and then refuses as it refuses every slot.

use super::{Env, Instance, Refusal};

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

#[cfg(test)]
mod tests {
    use super::super::{Framework, JobStart, Request};
    use super::*;
    use tcpcfg::model::svr::ServerAttrs;

    struct Two;

    impl Env for Two {
        fn active(&mut self, _: &str) -> bool {
            false
        }
        fn start_job(&mut self, _: &JobStart) -> Result<(), String> {
            Err("no".into())
        }
        fn end_job(&mut self, _: &str, _: bool) -> Result<(), String> {
            Ok(())
        }
        fn attrs(&mut self, server: &str) -> ServerAttrs {
            ServerAttrs {
                server: server.into(),
                autostart: false,
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

    #[test]
    fn instances_resolve_then_the_slot_refuses() {
        let mut env = Two;
        let names = |i| {
            resolve(&mut Two, &i)
                .unwrap()
                .into_iter()
                .map(|d| d.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(Instance::Default), ["QUSRDIR"]);
        assert_eq!(names(Instance::All), ["QUSRDIR", "OTHER"]);
        assert_eq!(names(Instance::Autostart), ["QUSRDIR"]);
        assert_eq!(names(Instance::Named("other".into())), ["OTHER"]);
        let fw = Framework::new();
        let e = fw
            .start(&mut env, &req(Instance::Named("NOPE".into())))
            .unwrap_err();
        assert_eq!(e.msgid, NO_INSTANCE);
        assert!(e.text.contains("NOPE"));
        let e = fw.start(&mut env, &req(Instance::Default)).unwrap_err();
        assert_eq!(e.msgid, crate::msg::NOT_AVAILABLE);
        assert!(e.text.contains("net/dirsrv"), "{}", e.text);
        let e = fw
            .end(&mut env, &req(Instance::Named("NOPE".into())), false)
            .unwrap_err();
        assert_eq!(e.msgid, NO_INSTANCE);
        let ok = fw.end(&mut env, &req(Instance::All), false).unwrap();
        assert!(ok.contains("DIRSRV server not active"), "{ok}");
    }
}
