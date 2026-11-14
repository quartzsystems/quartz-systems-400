// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A directory server instance's job (`QUSRDIR`, user `QDIRSRV`, in
//! `QSYSWRK`; docs/DIRSRV.md §4 "The job"): the settings from the
//! storage service, the certificate assigned to
//! `QS400_DIRECTORY_SERVER`, the listeners, and `dirsrv::server`'s
//! connection loop with a link of its own per connection thread. It
//! ends when `ENDTCPSVR`, `ENDJOB` or its subsystem says so: the
//! listeners close and every connection gets the notice of
//! disconnection.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use dirsrv::server::{
    Answer, Budget, Changes, Close, ConnInfo, Counts, JobSettings, Links, Listeners, Opened,
    Server, Service, State, SvcError, SvcResult,
};
use ipc::chan::Chan;
use stg::dir::{
    DirAnswer, DirBudget, DirClose, DirConn, DirConns, DirError, DirSettings, DirState,
};
use stg::{Client, Shared};
use wm::ctl::{FromJob, Start};

use super::super::netsvc::{self, Ender};

/// The role the instance jobs start in (`servers::framework::dirsrv`).
pub const ROLE: &str = servers::framework::dirsrv::ROLE;

/// The program the job log names.
const PGM: &str = "QGLDSVR";

/// `Directory server instance &1 started on ports &2.`
pub const STARTED: &str = "GLD0269";
/// `Directory server instance &1 ended.`
pub const ENDED: &str = "GLD0265";
/// `Directory server instance &1 could not listen on port &2: error &3.`
pub const LISTEN_FAILED: &str = "GLD0246";
/// `Directory server instance &1 could not be started.`
pub const NOT_STARTED: &str = "GLD0268";

/// How long a controlled end gives the connections.
const END_WAIT: Duration = Duration::from_secs(10);

fn conn(c: &ConnInfo) -> DirConn {
    DirConn {
        client: c.client.clone(),
        tls: c.tls,
        client_subject: c.client_subject.clone(),
    }
}

fn settings(s: DirSettings) -> JobSettings {
    JobSettings {
        instance: s.instance,
        port: s.port,
        secure_port: s.secure_port,
        security: s.security,
        idle_timeout: s.idle_timeout,
        write_timeout: s.write_timeout,
        app_id: s.app_id,
        max_message: s.max_message,
    }
}

fn error(e: DirError) -> SvcError {
    SvcError::new(&e.msgid, e.text)
}

fn answer(a: DirAnswer) -> Answer {
    Answer {
        replies: a.replies,
        state: match a.state {
            DirState::Done => State::Done,
            DirState::More { cursor } => State::More(cursor),
            DirState::Paused { cursor } => State::Paused(cursor),
            DirState::Watching { watch } => State::Watching(watch),
        },
        start_tls: a.start_tls,
        bound: a.bound,
    }
}

const fn budget(b: Budget) -> DirBudget {
    DirBudget {
        entries: b.entries,
        bytes: b.bytes,
    }
}

/// One thread's link as the connection loop's service. A lost link
/// ends the job, as for every job.
struct JobLink {
    link: Shared,
}

impl Service for JobLink {
    fn open(&mut self, instance: &str, c: &ConnInfo) -> SvcResult<Opened> {
        let o = self
            .link
            .borrow_mut()
            .with(|l| l.dir_open(instance.into(), conn(c)))
            .map_err(error)?;
        Ok(Opened {
            session: o.session,
            settings: settings(o.settings),
        })
    }

    fn bind(&mut self, session: u64, msg: &[u8], c: &ConnInfo) -> SvcResult<Answer> {
        self.link
            .borrow_mut()
            .with(|l| l.dir_bind(session, msg.to_vec(), conn(c)))
            .map(answer)
            .map_err(error)
    }

    fn op(&mut self, session: u64, msg: &[u8]) -> SvcResult<Answer> {
        self.link
            .borrow_mut()
            .with(|l| l.dir_op(session, msg.to_vec()))
            .map(answer)
            .map_err(error)
    }

    fn search(&mut self, session: u64, msg: &[u8], b: Budget) -> SvcResult<Answer> {
        self.link
            .borrow_mut()
            .with(|l| l.dir_search(session, msg.to_vec(), budget(b)))
            .map(answer)
            .map_err(error)
    }

    fn next(&mut self, session: u64, cursor: u64, b: Budget) -> SvcResult<Answer> {
        self.link
            .borrow_mut()
            .with(|l| l.dir_next(session, cursor, budget(b)))
            .map(answer)
            .map_err(error)
    }

    fn close(&mut self, session: u64, what: Close) -> SvcResult<()> {
        let what = match what {
            Close::Message(id) => DirClose::Message { id },
            Close::Session => DirClose::Session,
        };
        self.link
            .borrow_mut()
            .with(|l| l.dir_close(session, what))
            .map_err(error)
    }

    fn conns(&mut self, instance: &str, c: Counts) -> SvcResult<()> {
        let counts = DirConns {
            current: c.current,
            total: c.total,
            tls: c.tls,
            ssl: c.ssl,
        };
        self.link
            .borrow_mut()
            .with(|l| l.dir_conns(instance.into(), counts))
            .map_err(error)
    }

    fn changes(&mut self, session: u64, max: u32) -> SvcResult<Changes> {
        self.link
            .borrow_mut()
            .with(|l| l.dir_changes(session, max))
            .map(|c| Changes {
                replies: c.replies,
                ended: c.ended,
            })
            .map_err(error)
    }
}

/// A link per thread, each the job's own (`hello_thread`).
struct JobLinks {
    number: u32,
    pid: u32,
}

impl Links for JobLinks {
    type S = JobLink;

    fn link(&self) -> Result<JobLink, String> {
        super::super::nfs::host::thread_link(self.number, self.pid).map(|link| JobLink { link })
    }
}

/// The instance the job serves: its request data's `INSTANCE(name)`,
/// else its name.
fn instance_of(start: &Start) -> String {
    let r = start.rqsdta.trim();
    r.strip_prefix("INSTANCE(")
        .and_then(|x| x.strip_suffix(')'))
        .map_or_else(|| start.name.clone(), str::to_string)
        .trim()
        .to_ascii_uppercase()
}

/// The instance's settings: a session opened and closed at once.
fn read_settings(link: &Shared, instance: &str) -> Result<JobSettings, DirError> {
    let mut l = link.borrow_mut();
    let o = l.with(|c| c.dir_open(instance.into(), DirConn::default()))?;
    let _ = l.with(|c| c.dir_close(o.session, DirClose::Session));
    Ok(settings(o.settings))
}

/// The certificate's server configuration when the instance wants TLS
/// and one is assigned; why not to the job log and `QSYSOPR`.
fn server_tls(link: &Shared, s: &JobSettings) -> Option<Arc<tls::rustls::ServerConfig>> {
    if !s.ssl() && !s.start_tls() {
        return None;
    }
    let found = crate::work::tls::from_store(&mut link.borrow_mut(), &s.app_id);
    let why = match found {
        Ok(Some(c)) => return Some(c),
        Ok(None) => format!("no certificate is assigned to {}", s.app_id),
        Err(e) => e,
    };
    let port = if s.ssl() { s.secure_port } else { s.port };
    netsvc::tls_not_started(link, PGM, "DIRSRV", port, &why);
    None
}

fn log(link: &Shared, msgid: &str, text: &str) {
    eprintln!("qsjob: {PGM}: {msgid} {text}");
    netsvc::joblog(link, PGM, msgid, text);
}

/// The job.
pub fn run(mut ctl: Chan, link: &Shared, start: &Start) -> ! {
    let instance = instance_of(start);
    let s = match read_settings(link, &instance) {
        Ok(s) => s,
        Err(e) => {
            log(link, &e.msgid, &e.text);
            let text = format!("Directory server instance {instance} could not be started.");
            log(link, NOT_STARTED, &text);
            crate::start::end(&mut ctl, &text)
        }
    };
    let tls = server_tls(link, &s);
    let bind = Listeners::bind(IpAddr::V4(Ipv4Addr::UNSPECIFIED), &s, tls);
    let (listeners, secure_failed) = match bind {
        Ok(l) => l,
        Err(e) => {
            let text = format!(
                "Directory server instance {instance} could not listen on port {}: error {}.",
                e.port,
                e.error.raw_os_error().unwrap_or(0)
            );
            log(link, LISTEN_FAILED, &text);
            crate::start::end(&mut ctl, &text)
        }
    };
    if let Some(e) = secure_failed {
        netsvc::tls_not_started(link, PGM, "DIRSRV", e.port, &e.error.to_string());
    }
    let (plain, secure) = listeners.ports();
    let ports = secure.map_or_else(|| plain.to_string(), |p| format!("{plain} {p}"));
    let links = Arc::new(JobLinks {
        number: start.number,
        pid: std::process::id(),
    });
    let server = Server::start(links, &instance, listeners);
    log(
        link,
        STARTED,
        &format!("Directory server instance {instance} started on ports {ports}."),
    );
    let ender = Ender::new(&ctl, start.number);
    stg::remote::status(link, wm::job::Status::Selw, "");
    while !ender.ended(link, Duration::from_secs(1)) && !server.finished() {}
    server.stop(END_WAIT);
    log(
        link,
        ENDED,
        &format!("Directory server instance {instance} ended."),
    );
    let _ = ctl.send(&FromJob::Ending {
        reason: "ENDTCPSVR".into(),
    });
    std::process::exit(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_instance_from_the_request_data() {
        let mut s = Start {
            name: "QUSRDIR".into(),
            ..Start::default()
        };
        assert_eq!(instance_of(&s), "QUSRDIR");
        s.rqsdta = "INSTANCE(other)".into();
        assert_eq!(instance_of(&s), "OTHER");
    }
}
