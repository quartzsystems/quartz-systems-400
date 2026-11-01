// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The servers this crate starts: `*TELNET` and the `*HTTP`
//! administration instance; the hooks of `*FTP`, whose server is
//! `net/ftp`; and [`Standard`], the hooks of any entry that needs
//! nothing beyond the defaults.

use tcpcfg::model::svr::ServerAttrs;

use super::{Env, JobStart, Refusal, Request, TcpServer, check_instance, listener_job};
use crate::attrs::ftp::FtpAttrs;
use crate::attrs::http::HttpAttrs;
use crate::msg;
use crate::registry::{self, ServerSpec};

/// The default hooks for a registry entry: its listener job, started
/// and ended by name. A server whose job reads its own attributes
/// needs no more than `Standard::of("*NTP")`.
pub struct Standard(pub &'static ServerSpec);

impl Standard {
    /// The hooks of `special`'s entry.
    pub fn of(special: &str) -> Option<Self> {
        registry::lookup(special).map(Self)
    }
}

impl TcpServer for Standard {
    fn spec(&self) -> &'static ServerSpec {
        self.0
    }
}

/// `*TELNET`: `QTVTELNET` in `QSYSWRK`, which reads `CHGTELNA`'s
/// attributes itself when it starts.
pub struct Telnet;

impl TcpServer for Telnet {
    fn spec(&self) -> &'static ServerSpec {
        registry::lookup("*TELNET").expect("in the registry")
    }
}

/// `*HTTP` with `HTTPSVR(*ADMIN)`: the administration listener `ADMIN`
/// in `QHTTPSVR` and Navigator's `NAVIGATOR` next to it, their ports
/// in the request data; `ENDTCPSVR` ends both.
pub struct HttpAdmin;

impl TcpServer for HttpAdmin {
    fn spec(&self) -> &'static ServerSpec {
        registry::lookup("*HTTP").expect("in the registry")
    }

    fn jobs(&self, req: &Request, attrs: &ServerAttrs) -> Result<Vec<JobStart>, Refusal> {
        check_instance(self.spec(), req)?;
        let mut jobs: Vec<JobStart> = listener_job(self.spec(), HttpAttrs::from_bag(&attrs.bag).rqsdta())
            .into_iter()
            .collect();
        if let Some(nav) = self.spec().jobs.iter().find(|j| j.role == "HTTPNAV") {
            jobs.push(JobStart {
                name: nav.name.to_string(),
                user: nav.user.to_string(),
                subsystem: nav.subsystem.to_string(),
                role: nav.role.to_string(),
                rqsdta: HttpAttrs::navigator(&attrs.bag).rqsdta(),
                ..JobStart::default()
            });
        }
        Ok(jobs)
    }
}

/// `*FTP`: the listener `QTFTP00001` in `QSYSWRK` (or `CHGFTPA SBSD`'s
/// subsystem), its ports in the request data; it starts a `QTFTP00nnn`
/// job for each connection.
pub struct Ftp;

impl Ftp {
    /// The listener's job name.
    fn listener_name(&self) -> String {
        self.spec().listener().map_or_else(|| "QTFTP00001".into(), |l| l.numbered(1))
    }
}

impl TcpServer for Ftp {
    fn spec(&self) -> &'static ServerSpec {
        registry::lookup("*FTP").expect("in the registry")
    }

    fn jobs(&self, req: &Request, attrs: &ServerAttrs) -> Result<Vec<JobStart>, Refusal> {
        check_instance(self.spec(), req)?;
        let a = FtpAttrs::from_bag(&attrs.bag);
        let mut j = listener_job(self.spec(), a.rqsdta()).into_iter().collect::<Vec<_>>();
        for job in &mut j {
            job.subsystem = a.subsystem();
        }
        Ok(j)
    }

    fn active(&self, env: &mut dyn Env, req: &Request) -> bool {
        let _ = req;
        env.active(&self.listener_name())
    }

    fn end(&self, env: &mut dyn Env, req: &Request, immediate: bool) -> Result<String, Refusal> {
        let spec = self.spec();
        check_instance(spec, req)?;
        if !self.active(env, req) {
            return Ok(format!("{} {} server not active.", msg::NOT_ACTIVE, spec.name));
        }
        env.end_job(&self.listener_name(), immediate)
            .map_err(|e| Refusal::new(msg::NOT_ACTIVE, format!("{} not ended: {e}", spec.special)))?;
        Ok(format!("{} {} server ending.", msg::ENDING, spec.name))
    }
}
