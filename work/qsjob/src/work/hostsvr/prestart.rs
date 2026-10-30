// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A host server prestart job (`QZRCSRVS`, `QZDASOINIT`, ...): it
//! waits for a connection its daemon gives the server's pool, serves
//! it as the user who signs on, runs as its own user again, and waits
//! for the next until `MAXUSE` connections were served.

use std::cell::RefCell;
use std::net::TcpStream;
use std::rc::Rc;
use std::time::Duration;

use hostserver::central::Central;
use hostserver::database::{Database, DbConfig};
use hostserver::dtaq::DtaqServer;
use hostserver::file::FileServer;
use hostserver::netprt::NetPrint;
use hostserver::rmtcmd::RmtCmd;
use hostserver::signon::Signon;
use hostserver::Handler;
use hostsvr::server::ServerId;
use ipc::chan::Chan;
use sockets::relay;
use stg::{Client, Shared};
use wm::ctl::{FromJob, Start};
use wm::job::Status;

use super::auth::JobAuth;
use super::job::{Job, JobCommands, JobExits, JobQueues, JobSpool};
use crate::console::Console;

/// `MAXUSE(n)` of the request data the monitor passes; 0 for none.
fn maxuse(rqsdta: &str) -> u32 {
    rqsdta
        .split_once("MAXUSE(")
        .and_then(|(_, r)| r.split(')').next())
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0)
}

/// The request loop for `server` over `job`.
fn handler(server: ServerId, job: &Job, link: &Shared, start: &Start, system: &str) -> Box<dyn Handler> {
    let name = format!("{:06}/{}/{}", start.number, start.user, start.name);
    let auth = JobAuth {
        link: link.clone(),
        job: name,
    };
    let exits = JobExits(job.clone());
    let me = (start.name.clone(), start.user.clone(), format!("{:06}", start.number));
    match server {
        ServerId::Signon => Box::new(Signon::new(auth, exits)),
        ServerId::Central => Box::new(Central::new(auth, exits)),
        ServerId::RemoteCommand => Box::new(RmtCmd::new(auth, JobCommands(job.clone()), exits)),
        ServerId::Database => Box::new(Database::new(
            auth,
            stg::remote::RemoteSql::new(link.clone()),
            exits,
            DbConfig {
                rdb: system.to_string(),
                job: me,
            },
        )),
        ServerId::DataQueue => Box::new(DtaqServer::new(
            auth,
            JobQueues {
                job: job.clone(),
                link: link.clone(),
            },
            exits,
        )),
        ServerId::File => Box::new(FileServer::new(auth, stg::remote::RemoteIfs::new(link.clone()), exits)),
        ServerId::NetPrint => Box::new(NetPrint::new(
            auth,
            JobSpool {
                job: job.clone(),
                link: link.clone(),
                system: system.to_string(),
            },
            exits,
            me,
        )),
    }
}

/// Serve one connection.
fn serve(h: &mut dyn Handler, stream: TcpStream, tls: bool, link: &Shared, system: &str) -> Result<(), String> {
    let _ = stream.set_nodelay(true);
    if tls {
        let config = crate::work::tls::server_config(&mut link.borrow_mut(), system)?;
        let conn = rustls::ServerConnection::new(config).map_err(|e| e.to_string())?;
        let mut s = rustls::StreamOwned::new(conn, stream);
        hostserver::io::serve(&mut s, h).map_err(|e| e.to_string())
    } else {
        let mut s = stream;
        hostserver::io::serve(&mut s, h).map_err(|e| e.to_string())
    }
}

/// Run the prestart job. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let mut ctl = ctl;
    let program = start.program.rsplit('/').next().unwrap_or("").to_string();
    let Some(server) = hostserver::jobs::server_of_prestart(&program) else {
        crate::start::end(&mut ctl, &format!("{program} serves no host server"));
    };
    let ending = ctl.try_clone().ok();
    let job: Job = Rc::new(RefCell::new(Console::new(ctl, link, start)));
    job.borrow_mut().started();
    let system = crate::display::dds::system_name(&job.borrow().host);
    let pool = relay::pool(&program);
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
        let mut h = handler(server, &job, link, start, &system);
        if let Err(e) = serve(&mut *h, stream, taken.note.first() == Some(&b'T'), link, &system) {
            eprintln!("qsjob: {}: {e}", start.name);
        }
        drop(h);
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
