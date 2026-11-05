// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The client side of SQL CLI server mode: a connection is a socket
//! pair, one end given through the descriptor relay to the `QSQSRVR`
//! pool of `QSYSWRK` (whose monitor starts jobs for it), the other
//! kept here. The job's ticket from the storage service tells the
//! server job whom it serves.

use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::rc::Rc;
use std::time::Duration;

use ipc::chan::Chan;
use sockets::relay;
use sql::Value;
use sql::embed::{Reply, Request};
use sqlcli::model::diag::Diag;
use sqlcli::server::proto::{FromServer, ToServer, reply_of, run_message};
use sqlcli::server::{ConnectReq, Connector, Runner};
use stg::{Client, Shared};

/// The server jobs' program.
pub const PROGRAM: &str = "QSQSRVR";

/// How long a connection waits for a server job to answer: one may
/// have to start first.
const ANSWER: Duration = Duration::from_secs(60);

/// A connection's server job.
struct ServerJob {
    chan: Chan,
    open: bool,
}

impl Runner for ServerJob {
    fn run(&mut self, req: &Request) -> (Reply, Vec<(usize, Value)>) {
        let lost = || {
            let ca = sql::Sqlca { sqlcode: -30080, sqlstate: "08S01".into(), sqlerrmc: "The connection's server job ended.".into(), ..sql::Sqlca::default() };
            (Reply { sqlca: ca, rows: Vec::new(), call: None }, Vec::new())
        };
        if !self.open || self.chan.send(&run_message(req)).is_err() {
            self.open = false;
            return lost();
        }
        if let Ok(FromServer::Done { reply, outs }) = self.chan.recv::<FromServer>() {
            reply_of(&reply, &outs)
        } else {
            self.open = false;
            lost()
        }
    }

    fn close(&mut self) {
        if self.open {
            let _ = self.chan.send(&ToServer::Bye);
            self.open = false;
        }
    }
}

impl Drop for ServerJob {
    fn drop(&mut self) {
        self.close();
    }
}

/// Start a connection's server job.
fn connect(link: &Shared, me: &sockets::JobId, req: &ConnectReq) -> Result<Box<dyn Runner>, Diag> {
    let failed = |why: &str| Diag::cli("08001", &format!("No server job could be started for the connection: {why}."));
    let (mine, theirs) = UnixStream::pair().map_err(|e| failed(&e.to_string()))?;
    let ticket = link.borrow_mut().with(Client::sqlsrv_ticket);
    let fd = sockets::Fd::from(OwnedFd::from(theirs));
    relay::give(relay::SOCKET, &fd, &relay::pool(PROGRAM), me, b"Q").map_err(|e| failed(&format!("{e:?}")))?;
    drop(fd);
    let mut chan = Chan::new(mine);
    chan.set_timeout(Some(ANSWER)).map_err(|e| failed(&e.to_string()))?;
    let hello = ToServer::Hello {
        ticket,
        user: req.user.clone().unwrap_or_default(),
        password: req.password.clone().unwrap_or_default(),
    };
    chan.send(&hello).map_err(|e| failed(&e.to_string()))?;
    match chan.recv::<FromServer>() {
        Ok(FromServer::Ready { .. }) => {
            chan.set_timeout(None).map_err(|e| failed(&e.to_string()))?;
            Ok(Box::new(ServerJob { chan, open: true }))
        }
        Ok(FromServer::Refused { state, code, text }) => Err(Diag { state, native: code, text }),
        Ok(FromServer::Done { .. }) => Err(failed("the server job answered out of turn")),
        Err(e) => Err(failed(&e.to_string())),
    }
}

/// The job's connector.
pub fn connector(link: &Shared, me: sockets::JobId) -> Connector {
    let link = link.clone();
    Rc::new(move |req| connect(&link, &me, req))
}
