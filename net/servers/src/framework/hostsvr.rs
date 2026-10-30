// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*HOSTSVR`: one daemon job per host server, each told its
//! `STRHOSTSVR` keyword in the request data, as `STRHOSTSVR
//! SERVER(*ALL)` starts them. The server is active while any daemon is;
//! `STRTCPSVR` starts the ones that are not.

use tcpcfg::model::svr::ServerAttrs;

use super::{Env, Instance, JobStart, Refusal, Request, TcpServer, check_instance};
use crate::msg;
use crate::registry::{self, JobKind, ServerSpec};

/// Each daemon job and its `STRHOSTSVR` keyword
/// (`hostserver::jobs::DAEMONS` holds the same pairs).
pub const HOST_DAEMONS: &[(&str, &str)] = &[
    ("QZSOSMAPD", "*SVRMAP"),
    ("QZSOSGND", "*SIGNON"),
    ("QZSCSRVSD", "*CENTRAL"),
    ("QZDASRVSD", "*DATABASE"),
    ("QZHQSRVD", "*DTAQ"),
    ("QPWFSERVSD", "*FILE"),
    ("QNPSERVD", "*NETPRT"),
    ("QZRCSRVSD", "*RMTCMD"),
];

/// `*HOSTSVR`: the daemons in `QSYSWRK` and `QSERVER` as `QUSER`.
pub struct HostSvr;

impl HostSvr {
    fn daemons(&self) -> impl Iterator<Item = &'static registry::JobSpec> {
        self.spec().jobs.iter().filter(|j| j.kind == JobKind::Listener)
    }

    fn job(j: &registry::JobSpec) -> JobStart {
        let keyword = HOST_DAEMONS
            .iter()
            .find(|(n, _)| *n == j.name)
            .map_or("", |(_, k)| k);
        JobStart {
            name: j.name.to_string(),
            user: j.user.to_string(),
            subsystem: j.subsystem.to_string(),
            role: j.role.to_string(),
            rqsdta: keyword.to_string(),
            jobd: "QSYS/QZBSJOBD".to_string(),
            class: "QGPL/QCASERVR".to_string(),
        }
    }
}

impl TcpServer for HostSvr {
    fn spec(&self) -> &'static ServerSpec {
        registry::lookup("*HOSTSVR").expect("in the registry")
    }

    fn jobs(&self, req: &Request, _: &ServerAttrs) -> Result<Vec<JobStart>, Refusal> {
        check_instance(self.spec(), req)?;
        Ok(self.daemons().map(Self::job).collect())
    }

    fn active(&self, env: &mut dyn Env, _: &Request) -> bool {
        self.daemons().any(|j| env.active(j.name))
    }

    fn start(&self, env: &mut dyn Env, req: &Request) -> Result<String, Refusal> {
        let spec = self.spec();
        check_instance(spec, req)?;
        let attrs = env.attrs(spec.special);
        if req.instance == Instance::Autostart && !attrs.autostart {
            return Ok(String::new());
        }
        let mut idle = self.jobs(req, &attrs)?;
        idle.retain(|j| !env.active(&j.name));
        if idle.is_empty() {
            return Ok(format!("{} {} server already started.", msg::ALREADY, spec.name));
        }
        for j in &idle {
            env.start_job(j)
                .map_err(|e| Refusal::new(msg::NOT_STARTED, format!("{} could not be started: {e}", spec.special)))?;
        }
        Ok(format!("{} {} server starting.", msg::STARTING, spec.name))
    }
}
