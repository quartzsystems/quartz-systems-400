// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Serving an instance. [`site::Plan`] compiles `httpd.conf`; a
//! [`Server`] answers requests over it through [`Deps`] (the files as
//! the instance's job sees them, the user check, the clock) and runs
//! connections with `KeepAlive`, `MaxKeepAliveRequests`,
//! `KeepAliveTimeout` and `TimeOut`. The job binds the
//! [`site::Listener`]s (TLS from [`tls::config`]) and hands each
//! accepted connection to [`Server::serve_conn`]. A controlled end
//! ([`Server::stop`]) lets requests in progress finish and closes idle
//! connections; [`Server::drain`] waits for them.

pub mod auth;
pub mod dirs;
pub mod handle;
pub mod log;
pub mod site;
pub mod statics;
pub mod time;
pub mod tls;

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use qsweb::http::request::{self, ReadError};
use qsweb::http::{Body, Request, Response, plain};

pub use handle::Ctx;
use log::{Clock, Level, Logs, Record};
use site::{Plan, Site};

use crate::files::Files;

/// How often a waiting connection looks at the stop flag.
const POLL: Duration = Duration::from_millis(250);

/// What the server runs over.
#[derive(Clone)]
pub struct Deps {
    /// The files, as the instance's job sees them.
    pub files: Arc<dyn Files>,
    /// The user check for Basic authentication.
    pub auth: Arc<dyn auth::Authenticator>,
    /// Seconds since 1970.
    pub clock: Clock,
}

/// A connection the server can set timeouts on.
pub trait Transport: Read + Write {
    /// The read and write timeouts.
    fn set_timeouts(&mut self, read: Option<Duration>, write: Option<Duration>);
}

impl Transport for TcpStream {
    fn set_timeouts(&mut self, read: Option<Duration>, write: Option<Duration>) {
        let _ = self.set_read_timeout(read);
        let _ = self.set_write_timeout(write);
    }
}

impl Transport for ::tls::stream::TlsStream<TcpStream> {
    fn set_timeouts(&mut self, read: Option<Duration>, write: Option<Duration>) {
        self.get_mut().set_timeouts(read, write);
    }
}

#[derive(Default)]
struct State {
    connections: AtomicUsize,
    busy: AtomicUsize,
    requests: AtomicU64,
    stopping: AtomicBool,
}

/// One running instance.
pub struct Server {
    plan: Plan,
    deps: Deps,
    logs: Logs,
    state: Arc<State>,
}

/// Counts a connection while it lives.
struct Counted(Arc<State>);

impl Drop for Counted {
    fn drop(&mut self) {
        self.0.connections.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Reads with a deadline, looking at the stop flag while idle.
struct Patient<'a> {
    conn: &'a mut dyn Transport,
    state: &'a State,
    /// Until when an idle connection waits for a request.
    idle_until: Instant,
    /// How long a started request may take.
    timeout: Duration,
    /// When the current request's first byte came, if it has.
    started: Option<Instant>,
}

impl Read for Patient<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            match self.conn.read(buf) {
                Ok(n) => {
                    if n > 0 && self.started.is_none() {
                        self.started = Some(Instant::now());
                    }
                    return Ok(n);
                }
                Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted) => {
                    let late = match self.started {
                        None => self.state.stopping.load(Ordering::SeqCst) || Instant::now() >= self.idle_until,
                        Some(t) => t.elapsed() >= self.timeout,
                    };
                    if late {
                        return Err(io::Error::new(io::ErrorKind::TimedOut, "idle"));
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }
}

fn secs(n: u32) -> Duration {
    Duration::from_secs(u64::from(n.max(1)))
}

impl Server {
    /// A server for `plan`.
    pub fn new(plan: Plan, deps: Deps) -> Self {
        let logs = Logs::new(Arc::clone(&deps.files), Arc::clone(&deps.clock), &plan);
        Self {
            plan,
            deps,
            logs,
            state: Arc::new(State::default()),
        }
    }

    /// The compiled configuration.
    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    /// Client connections open now.
    pub fn connections(&self) -> usize {
        self.state.connections.load(Ordering::SeqCst)
    }

    /// Requests being answered now.
    pub fn busy(&self) -> usize {
        self.state.busy.load(Ordering::SeqCst)
    }

    /// Requests answered since start.
    pub fn requests(&self) -> u64 {
        self.state.requests.load(Ordering::SeqCst)
    }

    /// A controlled end: no request after the one in progress, idle
    /// connections close.
    pub fn stop(&self) {
        self.state.stopping.store(true, Ordering::SeqCst);
    }

    /// Whether [`Server::stop`] was called.
    pub fn stopping(&self) -> bool {
        self.state.stopping.load(Ordering::SeqCst)
    }

    /// Wait up to `wait` for every connection to close; whether they
    /// did.
    pub fn drain(&self, wait: Duration) -> bool {
        let until = Instant::now() + wait;
        while self.connections() > 0 {
            if Instant::now() >= until {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        true
    }

    /// Rotate and prune the logs `LogMaint` names.
    pub fn maintain_logs(&self) {
        self.logs.maintain();
    }

    /// A line in the main server's error log.
    pub fn log(&self, level: Level, text: &str) {
        self.log_error(&self.plan.main, level, None, text);
    }

    fn log_error(&self, site: &Site, level: Level, client: Option<&str>, text: &str) {
        if level <= site.level
            && let Some(p) = &site.error_log
        {
            self.logs.error(p, level, client, text);
        }
    }

    /// Answer one request and write its access log lines.
    pub fn respond(&self, req: &Request, ctx: &Ctx) -> Response {
        let started = Instant::now();
        let at = self.logs.now();
        let site = self.plan.site(ctx.local, req.header("host"));
        let (resp, out) = self.route(req, site, ctx, 0);
        self.state.requests.fetch_add(1, Ordering::SeqCst);
        if !site.access_logs.is_empty() {
            let bytes = match &resp.body {
                Body::Bytes(b) if req.method != "HEAD" => b.len() as u64,
                _ => 0,
            };
            let rec = Record {
                client: ctx.peer.ip().to_string(),
                local: ctx.local.ip().to_string(),
                port: ctx.local.port(),
                user: out.user,
                at,
                method: req.method.clone(),
                path: req.path.clone(),
                query: req.query.clone(),
                protocol: req.version.clone(),
                headers: req.headers.clone(),
                out_headers: resp.headers.clone(),
                status: resp.status,
                bytes,
                micros: u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
                server: site.server_name.clone().unwrap_or_default(),
                file: out.file,
            };
            for l in &site.access_logs {
                self.logs.write(&l.path, &log::format_line(&l.format, &rec));
            }
        }
        resp
    }

    /// Serve one connection until the client closes, keep-alive ends,
    /// a timeout passes or the server stops. The caller closes it.
    pub fn serve_conn(&self, conn: &mut dyn Transport, ctx: Ctx) {
        self.state.connections.fetch_add(1, Ordering::SeqCst);
        let _counted = Counted(Arc::clone(&self.state));
        let main = &self.plan.main;
        conn.set_timeouts(Some(POLL), Some(secs(main.timeout)));
        let mut pending = Vec::new();
        let mut idle = secs(main.timeout);
        let mut n: u32 = 0;
        loop {
            let mut r = Patient {
                conn: &mut *conn,
                state: &self.state,
                idle_until: Instant::now() + idle,
                timeout: secs(main.timeout),
                started: if pending.is_empty() { None } else { Some(Instant::now()) },
            };
            let req = match request::read_request(&mut r, &mut pending) {
                Ok(q) => q,
                Err(ReadError::Gone) => return,
                Err(e) => {
                    let (status, says) = match e {
                        ReadError::HeadTooLarge => (431, "The request was too long."),
                        ReadError::BodyTooLarge => (413, "The request body was too large."),
                        _ => (400, "The request was not understood."),
                    };
                    let _ = plain(status, says).write(conn, false, false);
                    return;
                }
            };
            n += 1;
            if req.version != "HTTP/1.1" && req.version != "HTTP/1.0" {
                let _ = plain(505, "This server speaks HTTP/1.1 and HTTP/1.0.").write(conn, false, false);
                return;
            }
            self.state.busy.fetch_add(1, Ordering::SeqCst);
            let site = self.plan.site(ctx.local, req.header("host"));
            let keep = req.keep_alive()
                && site.keep_alive
                && (site.max_keep_alive == 0 || n < site.max_keep_alive)
                && !self.stopping();
            idle = secs(site.keep_alive_timeout);
            let head_only = req.method == "HEAD";
            let resp = self.respond(&req, &ctx);
            let wrote = resp.write(conn, head_only, keep);
            self.state.busy.fetch_sub(1, Ordering::SeqCst);
            if wrote.is_err() || !keep {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests;
