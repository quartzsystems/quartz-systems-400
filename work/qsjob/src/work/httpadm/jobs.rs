// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `httpadmin::env::Jobs`, `Files` and `Certs` over the job's thread:
//! the instances' jobs from the active job table, `STRTCPSVR` and
//! `ENDTCPSVR` run through the command analyzer as the web user, the
//! IFS as this job sees it.

use std::io;

use httpadmin::env::{Certs, EndHow, HttpJob, Jobs};
use httpd::files::{Entry, Files};
use qsweb::msg::{Kind, Message};
use stg::Client;

use super::bridge::{Bridge, Main};
use crate::httpenv::{files, reply};

/// The instances' jobs.
pub struct LiveJobs {
    /// The job's thread.
    pub bridge: Bridge,
}

fn gone() -> Message {
    Message::new(Kind::Error, "CPF9897", "The administration server is ending; the request was not run.")
}

/// Run `line` as `user`; its outcome as a message.
fn run_as(m: &mut Main, user: &str, line: &str) -> Result<Message, Message> {
    let o = m.as_user(user, |m| m.console.run(line, crate::command::RunMode::Run))?;
    // CPF0001 says only that the command failed: its diagnostic says why.
    let fail = match (o.escape, o.diag) {
        (Some(e), Some(d)) if e.msgid == "CPF0001" => Some(d),
        (e, _) => e,
    };
    let escape = fail.map(|f| {
        let text = if f.text.is_empty() {
            let (id, file, lib) = (f.msgid.clone(), "QCPFMSG".to_string(), "QSYS".to_string());
            m.link
                .borrow_mut()
                .with(|l| l.describe(id, file, lib))
                .map_or_else(|| f.msgid.clone(), |d| d.text)
        } else {
            f.text
        };
        (f.msgid, text)
    });
    reply::outcome(&o.text, escape.as_ref().map(|(i, t)| (i.as_str(), t.as_str())))
}

impl Jobs for LiveJobs {
    fn active(&self) -> Vec<HttpJob> {
        self.bridge
            .call(|m| reply::instance_jobs(&m.link.borrow_mut().with(Client::active_jobs)))
            .unwrap_or_default()
    }

    fn start(&self, user: &str, instance: &str) -> Result<Message, Message> {
        let (user, line) = (user.to_string(), reply::start_command(instance));
        self.bridge.call(move |m| run_as(m, &user, &line)).unwrap_or_else(|| Err(gone()))
    }

    fn end(&self, user: &str, instance: &str, how: EndHow) -> Result<Message, Message> {
        let (user, line) = (user.to_string(), reply::end_command(instance, how));
        self.bridge.call(move |m| run_as(m, &user, &line)).unwrap_or_else(|| Err(gone()))
    }
}

/// The IFS as the `ADMIN` job (`QTMHHTTP`) sees it.
pub struct LiveFiles {
    /// The job's thread.
    pub bridge: Bridge,
}

impl LiveFiles {
    fn on<T: Send + 'static>(
        &self,
        f: impl FnOnce(&dyn ifs::client::IfsClient) -> io::Result<T> + Send + 'static,
    ) -> io::Result<T> {
        self.bridge
            .call(move |m| f(&m.ifs))
            .unwrap_or_else(|| Err(io::Error::other("the administration server is ending")))
    }
}

impl Files for LiveFiles {
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let p = path.to_string();
        self.on(move |c| files::read(c, &p))
    }

    fn write_atomic(&self, path: &str, data: &[u8]) -> io::Result<()> {
        let (p, d) = (path.to_string(), data.to_vec());
        self.on(move |c| files::write_atomic(c, &p, &d))
    }

    fn append(&self, path: &str, data: &[u8]) -> io::Result<()> {
        let (p, d) = (path.to_string(), data.to_vec());
        self.on(move |c| files::append(c, &p, &d))
    }

    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        let (f, t) = (from.to_string(), to.to_string());
        self.on(move |c| files::rename(c, &f, &t))
    }

    fn mkdir_all(&self, path: &str) -> io::Result<()> {
        let p = path.to_string();
        self.on(move |c| files::mkdir_all(c, &p))
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        let p = path.to_string();
        self.on(move |c| files::remove(c, &p))
    }

    fn remove_tree(&self, path: &str) -> io::Result<()> {
        let p = path.to_string();
        self.on(move |c| files::remove_tree(c, &p))
    }

    fn list(&self, dir: &str) -> io::Result<Vec<Entry>> {
        let p = dir.to_string();
        self.on(move |c| files::list(c, &p))
    }

    fn stat(&self, path: &str) -> io::Result<Entry> {
        let p = path.to_string();
        self.on(move |c| files::stat(c, &p))
    }
}

/// The certificate manager's side.
// TODO(httpadmin): the Certs trait's methods once the certificate pages
// shape it (steps 12 to 15); they go to the service as `cert_*`
// requests over the bridge.
pub struct LiveCerts;

impl Certs for LiveCerts {}
