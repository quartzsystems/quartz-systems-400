// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An HTTP server instance (`STRTCPSVR *HTTP HTTPSVR(name)`): a job
//! named after the instance in `QHTTPSVR` under its `ServerUserID`. It
//! reads `/www/instances.json` and the instance's `httpd.conf` through
//! the IFS as its user sees them, binds every `Listen` (a port below
//! 1024 needs `*IOSYSCFG`; TLS ports take the certificate `*SYSTEM`
//! assigns their `SSLAppName`), and serves with `net/httpd` until
//! `ENDTCPSVR`, `ENDJOB` or its subsystem ends it. Connection threads
//! reach the files and the user check through this, the main thread,
//! which owns the link. The job's function shows `CNN-n`, its open
//! connections, for the administration pages. The registry records
//! the start, the end and how it ended; the job log gets `HTP8000`,
//! `HTP8001` and `HTP8002` (message file `QHTTPSVR/QZHBMSG`).

use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use httpd::bridge::{self, Pump, Remote};
use httpd::files::Files;
use httpd::instance;
use httpd::registry::Registry;
use httpd::serve::auth::{Authenticator, Refused, SYSTEM};
use httpd::serve::log::{Level, system_clock};
use httpd::serve::site::{Listener, Plan, TlsSpec};
use httpd::serve::{Ctx, Deps, Server, Transport};
use ipc::chan::Chan;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use sec::profile::spcaut::SpecialAuth;
use servers::listen::{self, Conn, Running, Service};
use stg::{Client, Shared};
use wm::ctl::{FromJob, Start, ToJob};

use super::ifsfiles::IfsFiles;
use super::netsvc;

/// The program the job log names.
const PGM: &str = "QZHBHTTP";
/// `HTTP server instance &1 starting.`
const STARTING: &str = "HTP8000";
/// `HTTP server instance &1 started, listening on &2.`
const STARTED: &str = "HTP8001";
/// `HTTP server instance &1 ended.`
const ENDED: &str = "HTP8002";
/// `Configuration error at line &1 column &2: &3`
const CONF_ERROR: &str = "HTP8100";
/// `Server user profile &1 not found or not authorized.`
const NOT_AUTHORIZED: &str = "HTP8201";

/// One user check a connection thread asks for.
type AuthAsk = (String, String, Sender<Result<String, Refused>>);

/// Checks users through the main thread: `%%SYSTEM%%` signs on as a
/// server does (no display).
struct JobAuth {
    tx: Mutex<Sender<AuthAsk>>,
}

impl Authenticator for JobAuth {
    fn check(&self, source: &str, user: &str, password: &str) -> Result<String, Refused> {
        if !source.eq_ignore_ascii_case(SYSTEM) {
            // BLOCKED(rel): validation lists (*VLDL) do not exist; a PasswdFile
            // naming one refuses every user, the reason in the error log.
            return Err(Refused::Unavailable(format!("Validation list {source} cannot be checked yet.")));
        }
        let (tx, rx) = mpsc::channel();
        let sent = self
            .tx
            .lock()
            .map_err(|_| Refused::Bad)?
            .send((user.to_string(), password.to_string(), tx));
        if sent.is_err() {
            return Err(Refused::Unavailable("The server is ending.".into()));
        }
        rx.recv().unwrap_or(Err(Refused::Unavailable("The server is ending.".into())))
    }
}

/// An accepted connection as the engine wants it.
struct Wire(Conn);

impl Read for Wire {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for Wire {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl Transport for Wire {
    fn set_timeouts(&mut self, read: Option<Duration>, write: Option<Duration>) {
        let _ = self.0.tcp().set_read_timeout(read);
        let _ = self.0.tcp().set_write_timeout(write);
    }
}

/// The listener's service: one engine for every port.
struct Svc(Arc<Server>);

impl Service for Svc {
    fn serve(&self, conn: Conn, peer: SocketAddr) {
        let Ok(local) = conn.tcp().local_addr() else {
            return;
        };
        let ctx = Ctx {
            local,
            peer,
            secure: conn.secure(),
        };
        let mut w = Wire(conn);
        self.0.serve_conn(&mut w, ctx);
        w.0.close();
    }
}

/// What the main thread owns.
struct Job<'a> {
    link: &'a Shared,
    name: String,
    remote: Arc<Remote>,
    pump: Pump,
    local: IfsFiles,
    auth_rx: Receiver<AuthAsk>,
}

impl Job<'_> {
    /// Run `work` over the instance's files.
    fn files<T: Send>(&self, work: impl FnOnce(&dyn Files) -> T + Send) -> T {
        self.pump.run(&self.remote, &self.local, work)
    }

    fn log(&self, msgid: &str, text: &str) {
        eprintln!("qsjob: {}: {msgid} {text}", self.name);
        netsvc::joblog(self.link, PGM, msgid, text);
    }

    /// Answer the connection threads for up to `wait`.
    fn pump(&self, wait: Duration) {
        self.pump.serve(&self.local, wait);
        while let Ok((user, password, back)) = self.auth_rx.try_recv() {
            let r = self.link.borrow_mut().with(|l| {
                l.sign_on(user.trim().to_ascii_uppercase(), password.clone(), String::new(), false)
            });
            let _ = back.send(r.map(|s| s.user).map_err(|_| Refused::Bad));
        }
    }

    fn record(&self, f: impl FnOnce(&mut httpd::registry::Entry) + Send) {
        let name = self.name.clone();
        if let Err(e) = self.files(|files| Registry::update(files, &name, f)) {
            eprintln!("qsjob: {name}: registry: {e}");
        }
    }

    /// End the job: the registry, the job log, the parent.
    fn finish(&self, ctl: &mut Chan, how: &str, code: i32) -> ! {
        let now = now();
        let how2 = how.to_string();
        self.record(move |e| {
            e.last_ended = Some(now);
            e.ended_how = how2;
            e.end_delay = None;
        });
        self.log(ENDED, &format!("HTTP server instance {} ended.", self.name));
        let _ = ctl.send(&FromJob::Ending { reason: how.to_string() });
        std::process::exit(code)
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The TLS configuration of a listener from the `*SYSTEM` store.
fn tls_config(link: &Shared, spec: &TlsSpec) -> Result<Arc<rustls::ServerConfig>, String> {
    let t = link
        .borrow_mut()
        .with(|l| l.cert_app(spec.app_id.clone()))
        .map_err(|e| format!("application {}: {e}", spec.app_id))?;
    if t.label.is_empty() || t.chain.is_empty() {
        return Err(format!("no certificate is assigned to application {}", spec.app_id));
    }
    let info = certs::x509::CertInfo::from_der(&t.chain[0]).map_err(|e| e.to_string())?;
    let identity = tls::config::Identity {
        label: t.label.clone(),
        chain: t.chain.into_iter().map(CertificateDer::from).collect(),
        key: Arc::new(PrivateKeyDer::Pkcs8(t.key.into())),
        info,
    };
    let mut roots = rustls::RootCertStore::empty();
    for r in t.roots {
        let _ = roots.add(CertificateDer::from(r));
    }
    let app = match t.client_auth {
        1 => tls::config::ClientAuthMode::Optional,
        2 => tls::config::ClientAuthMode::Required,
        _ => tls::config::ClientAuthMode::None,
    };
    let policy = super::tls::policy(&mut link.borrow_mut());
    httpd::serve::tls::config(spec, &identity, &policy, roots, app)
}

fn shown(l: &Listener) -> String {
    let addr = l.addr.map_or_else(|| "*".to_string(), |a| a.to_string());
    format!("{addr}:{}{}", l.port, if l.tls.is_some() { " (TLS)" } else { "" })
}

/// How the job is told to end.
enum EndBy {
    /// `ENDTCPSVR` through the parent: `*IMMED`?
    Parent(bool),
    /// `ENDJOB` or the subsystem: `*IMMED`, and a delay when it has one.
    System(bool, Option<u32>, &'static str),
}

/// Run instance `start.name`. Never returns.
#[allow(clippy::too_many_lines)]
pub fn run(mut ctl: Chan, link: &Shared, start: &Start) -> ! {
    let name = start.name.trim().to_ascii_uppercase();
    let (remote, pump) = bridge::pair();
    let (auth_tx, auth_rx) = mpsc::channel();
    let job = Job {
        link,
        name: name.clone(),
        remote: Arc::new(remote),
        pump,
        local: IfsFiles::new(link),
        auth_rx,
    };
    job.log(STARTING, &format!("HTTP server instance {name} starting."));
    let n2 = name.clone();
    let loaded = job.files(move |files| {
        let reg = Registry::load(files)?;
        let e = reg.get(&n2).cloned().ok_or_else(|| httpd::Error::NotFound(n2.clone()))?;
        let conf = instance::load_conf(files, &e.root)?;
        Ok::<_, httpd::Error>((e, conf))
    });
    let (entry, conf) = match loaded {
        Ok(v) => v,
        Err(httpd::Error::Conf(c)) => {
            let path = format!("/www/{}/{}", name.to_ascii_lowercase(), instance::CONF);
            job.log(CONF_ERROR, &format!("Configuration error at line {} column {}: {} ({path})", c.line, c.col, c.text));
            job.finish(&mut ctl, "configuration error", 1)
        }
        Err(e) => {
            job.log(servers::msg::NOT_STARTED, &format!("HTTP server instance {name} could not be started: {e}."));
            job.finish(&mut ctl, &format!("not started: {e}"), 1)
        }
    };
    let plan = Plan::new(&name, &entry.root, &conf);
    // Ports below 1024 need *IOSYSCFG on the server's profile.
    let me = link.borrow_mut().with(Client::whoami);
    let special = SpecialAuth(me.spcaut | me.group_spcaut);
    let may_low = special.has(SpecialAuth::IOSYSCFG) || special.has(SpecialAuth::ALLOBJ);
    if !may_low && plan.listeners.iter().any(|l| l.port < 1024) {
        job.log(
            NOT_AUTHORIZED,
            &format!("Server user profile {} not found or not authorized: ports below 1024 need *IOSYSCFG.", me.user),
        );
        job.finish(&mut ctl, "not authorized", 1)
    }
    let server = Arc::new(Server::new(
        plan.clone(),
        Deps {
            files: job.remote.clone(),
            auth: Arc::new(JobAuth { tx: Mutex::new(auth_tx) }),
            clock: system_clock(),
        },
    ));
    let s2 = Arc::clone(&server);
    job.files(move |_| s2.maintain_logs());
    let mut running: Vec<Running> = Vec::new();
    let mut ports = Vec::new();
    for l in &plan.listeners {
        let tls = match &l.tls {
            Some(spec) => match tls_config(link, spec) {
                Ok(c) => Some(c),
                Err(e) => {
                    let text = format!("Port {} not started: {e}.", l.port);
                    let s2 = Arc::clone(&server);
                    let t2 = text.clone();
                    job.files(move |_| s2.log(Level::Error, &t2));
                    job.log(servers::msg::NOT_STARTED, &text);
                    continue;
                }
            },
            None => None,
        };
        let svc: Arc<dyn Service> = Arc::new(Svc(Arc::clone(&server)));
        match listen::bind_at(l.socket(), 20).and_then(|b| listen::serve(b, tls, svc)) {
            Ok(r) => {
                running.push(r);
                ports.push(shown(l));
            }
            Err(e) => {
                let text = format!("Port {} not started: {e}.", l.port);
                let s2 = Arc::clone(&server);
                let t2 = text.clone();
                job.files(move |_| s2.log(Level::Error, &t2));
                job.log(servers::msg::NOT_STARTED, &text);
            }
        }
    }
    if running.is_empty() {
        job.finish(&mut ctl, "no port could be started", 1)
    }
    let started = now();
    job.record(move |e| {
        e.last_started = Some(started);
        e.restart_required = false;
        e.ended_how = String::new();
        e.end_delay = None;
    });
    job.log(STARTED, &format!("HTTP server instance {name} started, listening on {}.", ports.join(", ")));

    // Only an explicit End from the parent counts; ENDJOB and ENDSBS
    // come through the service's state.
    let mut reader = ctl.try_clone().expect("ctl");
    let (end_tx, end_rx) = mpsc::channel::<bool>();
    std::thread::spawn(move || {
        while let Ok(m) = reader.recv::<ToJob>() {
            if let ToJob::End { immediate } = m {
                let _ = end_tx.send(immediate);
                return;
            }
        }
    });
    let me = start.number;
    let mut shown_count = usize::MAX;
    let mut looked = Instant::now();
    let end = loop {
        job.pump(Duration::from_millis(50));
        if let Ok(immediate) = end_rx.try_recv() {
            break EndBy::Parent(immediate);
        }
        if looked.elapsed() >= Duration::from_millis(500) {
            looked = Instant::now();
            let n = server.connections();
            if n != shown_count {
                shown_count = n;
                stg::remote::status(link, wm::job::Status::Selw, &format!("CNN-{n}"));
            }
            let state = link.borrow_mut().with(Client::sbs_state);
            if let Some((immediate, delay)) = state.ending {
                break EndBy::System(immediate, Some(delay), "ENDSBS");
            }
            if let Some((_, immediate)) = state.end_jobs.iter().find(|(n, _)| *n == me) {
                break EndBy::System(*immediate, None, "ENDJOB");
            }
        }
    };
    for r in running {
        r.stop();
    }
    let (immediate, how) = match &end {
        EndBy::Parent(i) => (*i, "ENDTCPSVR *HTTP"),
        EndBy::System(i, _, how) => (*i, *how),
    };
    if immediate {
        job.finish(&mut ctl, &format!("{how}, immediate"), 0)
    }
    let n3 = name.clone();
    let asked = job.files(move |files| Registry::load(files).ok().and_then(|r| r.get(&n3).and_then(|e| e.end_delay)));
    let delay = match end {
        EndBy::System(_, Some(d), _) => d,
        _ => asked.unwrap_or(servers::framework::DEFAULT_DELAY),
    };
    server.stop();
    let until = Instant::now() + Duration::from_secs(u64::from(delay));
    while server.connections() > 0 && Instant::now() < until {
        job.pump(Duration::from_millis(50));
    }
    job.finish(&mut ctl, &format!("{how}, controlled"), 0)
}
