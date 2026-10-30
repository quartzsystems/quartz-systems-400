// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The framework as `STRTCP` and `ENDTCP` see it: `tcpcfg`'s
//! [`ServerStarter`] over a [`Framework`] and the job's [`Env`].

use tcpcfg::server::ServerStarter;

use super::{Env, Framework, Request};

/// `STRTCP STRSVR(*YES)` and `ENDTCP ENDSVR(*YES)` through the
/// framework: the autostart instances start, every running one ends.
pub struct Starter<'a> {
    fw: &'a Framework,
    env: &'a mut dyn Env,
}

impl<'a> Starter<'a> {
    /// Over `fw` in `env`.
    pub fn new(fw: &'a Framework, env: &'a mut dyn Env) -> Self {
        Self { fw, env }
    }
}

impl ServerStarter for Starter<'_> {
    fn start(&mut self, server: &str) -> Result<String, String> {
        self.fw
            .start(self.env, &Request::autostart(server))
            .map_err(|r| r.line())
    }

    fn end(&mut self, server: &str, immediate: bool) -> Result<String, String> {
        self.fw
            .end(self.env, &Request::new(server), immediate)
            .map_err(|r| r.line())
    }

    fn active(&mut self, server: &str) -> bool {
        self.fw.active(self.env, server)
    }
}
