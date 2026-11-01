// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `httpadmin::env::System` over the job's thread: the common part
//! (`web::system::Core`) and the interfaces.

use httpadmin::env::{Audit, SignOnRefused, System};
use qsweb::msg::SecondLevel;
use stg::tcp::{TcpOp, TcpReply};

use crate::work::web::system::Core;

/// The system as the `ADMIN` job sees it.
pub struct LiveSystem {
    /// Name, dates, sign-on, profiles, audit, messages.
    pub core: Core,
    /// The qualified job name.
    pub job: String,
}

impl System for LiveSystem {
    fn name(&self) -> String {
        self.core.name()
    }

    fn now(&self) -> u64 {
        self.core.now()
    }

    fn stamp(&self, secs: u64) -> (String, String) {
        self.core.stamp(secs)
    }

    fn sign_on(&self, user: &str, password: &str, _from: &str) -> Result<String, SignOnRefused> {
        self.core.sign_on(user, password)
    }

    fn special(&self, user: &str) -> Vec<String> {
        self.core.special(user)
    }

    fn groups(&self, user: &str) -> Vec<String> {
        self.core.groups(user)
    }

    fn user_class(&self, user: &str) -> String {
        self.core.user_class(user)
    }

    fn profile_exists(&self, user: &str) -> bool {
        self.core.profile_exists(user)
    }

    fn audit(&self, entry: Audit) {
        match entry {
            Audit::Password { user, code } => self.core.audit("PW", user, code.to_string(), String::new()),
            Audit::Refused { user, object, need } => self.core.audit("AF", user, object, need),
            Audit::Change { user, object, what } => self.core.audit("ZC", user, object, what),
        }
    }

    fn message(&self, id: &str, data: &[&str]) -> Option<SecondLevel> {
        self.core.message(id, data)
    }

    fn job(&self) -> String {
        self.job.clone()
    }

    fn interfaces(&self) -> Vec<String> {
        self.core
            .bridge()
            .call(|m| match m.link.borrow_mut().with(|l| l.tcp(TcpOp::Snapshot)) {
                TcpReply::Snapshot { snap } => {
                    let mut out: Vec<String> = Vec::new();
                    for i in snap.ifcs {
                        if !i.addr.is_empty() && !out.contains(&i.addr) {
                            out.push(i.addr);
                        }
                    }
                    out
                }
                _ => Vec::new(),
            })
            .unwrap_or_default()
    }
}
