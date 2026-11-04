// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QRWTSRVR`, the DDM/DRDA server job: a prestart job of `QUSRWRK`
//! that takes a connection `QRWTLSTN` gives its pool, answers its
//! chains with `ddm::server::Session` as the user who signed on, runs
//! as `QUSER` again and waits for the next, until `MAXUSE` connections
//! were served.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ddm::objects::DdmTcpAttrs;
use ddm::server::{Config, Session};
use ipc::chan::Chan;
use sockets::relay;
use stg::{Client, Shared};
use wm::ctl::{FromJob, Start};
use wm::job::Status;

use super::backend::{JobAuth, JobFiles, JobSql};
use super::listener::SERVER_PROGRAM;
use crate::console::Console;
use crate::work::hostsvr::job::Job;

/// The certificate application ID of port 448.
const APP_ID: &str = "QS400_DDM_DRDA_SERVER";

/// `MAXUSE(n)` of the request data the monitor passes; 0 for none.
fn maxuse(rqsdta: &str) -> u32 {
    rqsdta
        .split_once("MAXUSE(")
        .and_then(|(_, r)| r.split(')').next())
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0)
}

/// The server's attributes (`CHGDDMTCPA`) from the server table.
pub fn attrs(link: &Shared) -> DdmTcpAttrs {
    DdmTcpAttrs::from_bag(&crate::work::netsvc::bag(link, "*DDM"))
}

/// Answer chains until the requester closes.
fn converse(s: &mut Session, io: &mut (impl Read + Write)) -> Result<(), String> {
    loop {
        let chain = match ddm::wire::read_chain(io) {
            Ok(c) => c,
            Err(ddm::Error::Io(_)) => return Ok(()),
            Err(e) => return Err(e.to_string()),
        };
        let out = s.handle(&chain);
        if !out.is_empty() {
            io.write_all(&ddm::wire::encode_chain(&out)).map_err(|e| e.to_string())?;
            io.flush().map_err(|e| e.to_string())?;
        }
    }
}

fn session(job: &Job, link: &Shared, start: &Start, system: &str) -> Session {
    let mut cfg = Config::new(system, system);
    cfg.extnam = format!("{:06}/{}/{}", start.number, start.user, start.name);
    cfg.mechanisms = attrs(link).mechanisms();
    Session::new(
        cfg,
        Box::new(JobAuth(link.clone())),
        Box::new(JobSql::new(link.clone())),
        Box::new(JobFiles::new(link.clone(), job.clone())),
    )
}

fn serve(s: &mut Session, stream: TcpStream, tls: Option<Arc<rustls::ServerConfig>>) -> Result<(), String> {
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(7200)));
    if let Some(config) = tls {
        let conn = rustls::ServerConnection::new(config).map_err(|e| e.to_string())?;
        let mut t = rustls::StreamOwned::new(conn, stream);
        converse(s, &mut t)
    } else {
        let mut t = stream;
        converse(s, &mut t)
    }
}

/// Run the server job. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let ending = ctl.try_clone().ok();
    let job: Job = Rc::new(RefCell::new(Console::new(ctl, link, start)));
    job.borrow_mut().started();
    let system = crate::display::dds::system_name(&job.borrow().host);
    let pool = relay::pool(SERVER_PROGRAM);
    let limit = maxuse(&start.rqsdta);
    let mut uses = 0;
    loop {
        stg::remote::status(link, Status::Evtw, "PSRW");
        if !job.borrow_mut().idle(Duration::ZERO) {
            break;
        }
        let taken = match relay::take(relay::SOCKET, &pool, None, 1000) {
            Ok(Some(t)) => t,
            Ok(None) => continue,
            Err(e) => {
                eprintln!("qsjob: {}: descriptor relay: {e:?}", start.name);
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
        };
        stg::remote::status(link, Status::Run, "");
        let stream = TcpStream::from(taken.fd.into_owned());
        let tls = if taken.note.first() == Some(&b'T') {
            match crate::work::tls::app_server_config(&mut link.borrow_mut(), APP_ID, &system) {
                Ok(c) => Some(c),
                Err(e) => {
                    eprintln!("qsjob: {}: {e}", start.name);
                    continue;
                }
            }
        } else {
            None
        };
        let mut s = session(&job, link, start, &system);
        if let Err(e) = serve(&mut s, stream, tls) {
            eprintln!("qsjob: {}: {e}", start.name);
        }
        drop(s);
        link.borrow_mut().with(Client::host_reset);
        uses += 1;
        if limit > 0 && uses >= limit {
            break;
        }
    }
    job.borrow_mut().ended(0);
    if let Some(mut c) = ending {
        let _ = c.send(&FromJob::Ending {
            reason: "prestart job ended".into(),
        });
    }
    std::process::exit(0)
}
