// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*NETSVR`: `NetServer`. `QZLSSERVER` is not the command's child: the
//! storage service submits it to `QSERVER` (routing entry 800, as
//! `QZLSSTRS` does) and ends it (as `QZLSENDS`), so `STRTCPSVR`, the
//! APIs and `STRTCP`'s autostart start the same job the same way.

use super::{Env, Instance, Refusal, Request, TcpServer, check_instance};
use crate::msg;
use crate::registry::{self, ServerSpec};

/// `*NETSVR`'s hooks.
pub struct NetSvr;

impl TcpServer for NetSvr {
    fn spec(&self) -> &'static ServerSpec {
        registry::lookup("*NETSVR").expect("in the registry")
    }

    fn start(&self, env: &mut dyn Env, req: &Request) -> Result<String, Refusal> {
        let spec = self.spec();
        check_instance(spec, req)?;
        if self.active(env, req) {
            return Ok(format!(
                "{} {} server already started.",
                msg::ALREADY,
                spec.name
            ));
        }
        let autostart = req.instance == Instance::Autostart;
        if autostart && !env.attrs(spec.special).autostart {
            return Ok(String::new());
        }
        env.netsvr(true, autostart)?;
        Ok(format!("{} {} server starting.", msg::STARTING, spec.name))
    }

    fn end(&self, env: &mut dyn Env, req: &Request, _immediate: bool) -> Result<String, Refusal> {
        let spec = self.spec();
        check_instance(spec, req)?;
        if !self.active(env, req) {
            return Ok(format!(
                "{} {} server not active.",
                msg::NOT_ACTIVE,
                spec.name
            ));
        }
        env.netsvr(false, false)?;
        Ok(format!("{} {} server ending.", msg::ENDING, spec.name))
    }
}
