// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*NTP` and `*LPD`. `QTOTNTP` is told which of the SNTP client and
//! server to run: both on `STRTCPSVR`, the ones whose autostart is on
//! when TCP/IP starts. `*LPD`'s listener has a numbered name
//! (`QTLPD00001`), which the default hooks would look for as written.

use tcpcfg::model::svr::ServerAttrs;

use super::{Env, Instance, JobStart, Refusal, Request, TcpServer, check_instance, listener_job};
use crate::attrs::ntp::NtpAttrs;
use crate::msg;
use crate::registry::{self, ServerSpec};

/// `*NTP`: `QTOTNTP` in `QSYSWRK` as `QNTP`.
pub struct Ntp;

impl TcpServer for Ntp {
    fn spec(&self) -> &'static ServerSpec {
        registry::lookup("*NTP").expect("in the registry")
    }

    fn jobs(&self, req: &Request, attrs: &ServerAttrs) -> Result<Vec<JobStart>, Refusal> {
        check_instance(self.spec(), req)?;
        let (client, server) = if req.instance == Instance::Autostart {
            let a = NtpAttrs::from_bag(&attrs.bag);
            (a.autostart, a.svrautostr)
        } else {
            (true, true)
        };
        Ok(listener_job(self.spec(), NtpAttrs::rqsdta(client, server))
            .into_iter()
            .collect())
    }
}

/// `*LPD`: `QTLPD00001` in `QSYSWRK` as `QTMPLPD`.
pub struct Lpd;

impl Lpd {
    fn job() -> String {
        registry::lookup("*LPD")
            .and_then(ServerSpec::listener)
            .map(|l| l.numbered(1))
            .unwrap_or_default()
    }
}

impl TcpServer for Lpd {
    fn spec(&self) -> &'static ServerSpec {
        registry::lookup("*LPD").expect("in the registry")
    }

    fn active(&self, env: &mut dyn Env, _: &Request) -> bool {
        env.active(&Self::job())
    }

    fn end(&self, env: &mut dyn Env, req: &Request, immediate: bool) -> Result<String, Refusal> {
        let spec = self.spec();
        check_instance(spec, req)?;
        if !self.active(env, req) {
            return Ok(format!("{} {} server not active.", msg::NOT_ACTIVE, spec.name));
        }
        env.end_job(&Self::job(), immediate)
            .map_err(|e| Refusal::new(msg::NOT_ACTIVE, format!("{} not ended: {e}", spec.special)))?;
        Ok(format!("{} {} server ending.", msg::ENDING, spec.name))
    }
}
