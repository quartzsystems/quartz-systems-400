// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job's SQL connections (`net/drdar`): `CONNECT TO` an entry of the
//! relational database directory reaches that server over DRDA, and
//! the job's statements go there while it is current. One set per job
//! process. The servers are found and signed on to as DDM files do:
//! the directory's host and port, `USER ... USING` or the job user's
//! server authentication entry.

use std::cell::RefCell;
use std::net::TcpStream;
use std::time::Duration;

use ddm::client::{ConnectOpts, Connection};
use ddm::sec::Mechanism;
use drdar::values::coded;
use drdar::{Connector, Remote, Routed, Server, Sessions};
use sql::embed::{Embedded, Reply, Request};
use stg::Shared;

thread_local! {
    static SESSIONS: RefCell<Sessions> = RefCell::new(Sessions::default());
}

/// The package a server that wants one gets bound: Interactive SQL's
/// on IBM i.
const PACKAGE: (&str, &str) = ("QGPL", "QSQL400");

struct JobNet<'a> {
    link: &'a Shared,
}

impl Connector for JobNet<'_> {
    fn local(&self) -> String {
        let dir = crate::command::ddm::directory_of(self.link);
        dir.entries.iter().find(|e| e.is_local()).map_or_else(|| crate::work::ddm::system_name(self.link), |e| e.rdb.clone())
    }

    fn connect(&mut self, rdb: &str, user: Option<(&str, &str)>) -> Result<Box<dyn Server>, sql::Sqlca> {
        let dir = crate::command::ddm::directory_of(self.link);
        let e = dir.find(rdb).ok_or_else(|| coded(-950, "42705", rdb))?;
        let host = if e.is_local() { "127.0.0.1".to_string() } else { e.rmtloc.clone() };
        let port = e.tcp_port();
        let (user, password) = match user {
            Some((u, p)) => (u.to_string(), p.to_string()),
            None => crate::work::ddm::requester::credentials(self.link, &e.rdb).ok_or_else(|| coded(-30_082, "08001", &e.rdb))?,
        };
        let addrs = crate::work::netsvc::resolve(self.link, &host, port);
        let stream = addrs
            .iter()
            .find_map(|a| TcpStream::connect_timeout(a, Duration::from_secs(10)).ok())
            .ok_or_else(|| coded(-30_080, "08001", &format!("{host}:{port}")))?;
        let _ = stream.set_nodelay(true);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(120)));
        let mut opts = ConnectOpts::new(&e.rdb, &user, &password);
        opts.mechanisms = vec![Mechanism::EUsrIdPwd, Mechanism::UsrEncPwd, Mechanism::UsrSsbPwd, Mechanism::UsrSbsPwd];
        opts.srvnam = crate::work::ddm::system_name(self.link);
        opts.collection = PACKAGE.0.into();
        opts.package = PACKAGE.1.into();
        let conn = Connection::connect(stream, &opts).map_err(|x| coded(-30_082, "08001", &x.to_string()))?;
        Ok(Box::new(Remote::new(conn)))
    }
}

/// Answer `req` when it is a connection statement or the current
/// server is not the local one; `None` leaves it to the local server.
pub fn route(link: &Shared, req: &Request) -> Option<Reply> {
    SESSIONS.with(|s| match s.borrow_mut().route(&mut JobNet { link }, req) {
        Routed::Done(r) => Some(r),
        Routed::Local => None,
    })
}

/// Whether a server other than the local one is current: the job's
/// statements go there.
pub fn remote_current() -> bool {
    SESSIONS.with(|s| matches!(s.borrow().current, drdar::Current::Remote(_)))
}

/// The current server's name: the local one's, a remote one's, or
/// empty when none is current (after `RELEASE` or `DISCONNECT`).
pub fn current_rdb(link: &Shared) -> String {
    SESSIONS.with(|s| match &s.borrow().current {
        drdar::Current::Local => JobNet { link }.local(),
        drdar::Current::Remote(r) => r.clone(),
        drdar::Current::None => String::new(),
    })
}

/// STRSQL's query while a remote server is current, refused SQL0084.
/// BLOCKED(sql): the requester's query brings no column descriptions
/// back (`drdar::Server::query`), and Display Data shows rows by them.
pub fn remote_query_refused() -> sql::Sqlca {
    coded(-84, "42612", "a query at a remote server in STRSQL")
}

/// A program's embedded SQL with the job's connections in front.
pub struct Routing<E> {
    inner: E,
    link: Shared,
}

impl<E> Routing<E> {
    /// `inner` runs what the local server runs.
    pub fn new(inner: E, link: Shared) -> Self {
        Self { inner, link }
    }
}

impl<E: Embedded> Embedded for Routing<E> {
    fn run(&mut self, req: &Request) -> Reply {
        route(&self.link, req).unwrap_or_else(|| self.inner.run(req))
    }

    fn take_outs(&mut self) -> Vec<(usize, sql::Value)> {
        self.inner.take_outs()
    }
}
