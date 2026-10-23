// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QTVTELNET`: the TN5250E server. It listens on the telnet port (and
//! on 992 with TLS) and runs one session per connection on its own
//! thread, registered as a `QTVDEVICE` job of user `QTCP`: the
//! negotiation, a virtual display allocated through the storage
//! service, the sign-on display, then the interactive job it starts
//! for the user, whose records it moves between the job's device
//! socket and the connection. When the connection drops, `QDEVRCYACN`
//! says what becomes of the job; a disconnected job comes back when
//! its user signs on again, and ends after `QDSCJOBITV`.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ipc::chan::Chan;
use stg::{Client, JobInfo, Link, Shared};
use tn5250::ds::wdsf::{Model, printer_type_model};
use tn5250::record::Record;
use tn5250::screen::{Read as ModelRead, ScreenModel};
use tn5250::telnet;
use tn5250::telnet::server::{Negotiated, Server};
use wm::ctl::{FromJob, Start, ToJob};
use wm::dev::{self, FromDevice, ToDevice};
use wm::obj::jobd::JobD;
use wm::obj::sbsd::{PoolKind, Sbsd};

use super::spawn::{self, Spawned};
use crate::display;

/// The telnet port when `CHGTELNA` names none.
pub const PORT: u16 = 23;
/// The TLS port.
pub const TLS_PORT: u16 = 992;

/// What a session asks the server's main thread for.
enum Ask {
    /// A job number; the answer comes back on the channel.
    JobNumber(Sender<Result<u32, String>>),
}

/// A job whose connection dropped under `*DSC*` recovery.
struct Disconnected {
    user: String,
    device: String,
    job: Spawned,
    link: Chan,
    since: Instant,
    /// Where the job's channel readers deliver: the next session
    /// swaps in its own sender, so no second reader is started.
    dev_tx: SharedTx,
    ctl_tx: SharedTx,
}

/// A sender the reader threads look up before each delivery.
type SharedTx = Arc<Mutex<Sender<Ev>>>;

/// Read a job channel for the sessions, whichever one holds the job.
fn relay<T: ipc::Wire>(mut reader: Chan, tx: SharedTx, map: impl Fn(T) -> Ev + Send + 'static) {
    std::thread::spawn(move || {
        while let Ok(m) = reader.recv::<T>() {
            let ev = map(m);
            let _ = tx.lock().expect("sender").send(ev);
        }
        let _ = tx.lock().expect("sender").send(Ev::JobGone);
    });
}

fn disconnected() -> &'static Mutex<Vec<Disconnected>> {
    static LIST: OnceLock<Mutex<Vec<Disconnected>>> = OnceLock::new();
    LIST.get_or_init(|| Mutex::new(Vec::new()))
}

/// The attributes `CHGTELNA` keeps, read when the server starts.
struct Attrs {
    port: u16,
    /// `*NO`, `*YES` or `*ONLY`.
    alwssl: String,
}

fn attrs(link: &Shared) -> Attrs {
    let text = link
        .borrow_mut()
        .with(|l| l.da_read("QTVTELNA".into(), "QUSRSYS".into()))
        .map(|v| ccsid::from_ebcdic(&v.value).to_ascii_uppercase())
        .unwrap_or_default();
    let value = |kwd: &str| -> Option<String> {
        text.split_once(&format!("{kwd}("))
            .and_then(|(_, r)| r.split(')').next())
            .map(|v| v.trim().to_string())
    };
    Attrs {
        port: value("PORT").and_then(|p| p.parse().ok()).unwrap_or(PORT),
        alwssl: value("ALWSSL").unwrap_or_else(|| "*YES".into()),
    }
}

/// A second link to the service for the TLS material, as the server
/// job itself.
fn own_link(start: &Start) -> Option<Link> {
    let chan = Chan::connect(Path::new(stg::SOCKET)).ok()?;
    let mut l = Link::over(chan);
    l.hello(JobInfo {
        name: "QTVTELNET".into(),
        user: "QTCP".into(),
        number: start.number,
        kind: 'B',
        subtype: ' ',
        subsystem: "QSYSWRK".into(),
        pid: std::process::id(),
        handle: start.handle,
        jobd: start.jobd.clone(),
        class: start.class.clone(),
        pool: start.pool.clone(),
        ..JobInfo::default()
    })
    .ok()?
    .ok()?;
    Some(l)
}

/// Run the server job. Never returns.
pub fn run(mut ctl: Chan, link: &Shared, start: &Start) -> ! {
    let a = attrs(link);
    let port = port_of(&start.rqsdta).unwrap_or(a.port);
    let host = link
        .borrow_mut()
        .with(|l| l.sysval("QSYSNAME".into()))
        .map_or_else(
            || "QS400".to_string(),
            |v| ccsid::from_ebcdic(&v.data).trim().to_string(),
        );
    let (ask_tx, ask_rx) = mpsc::channel::<Ask>();
    if a.alwssl != "*ONLY" {
        match bind(port) {
            Ok(l) => {
                eprintln!("qsjob: QTVTELNET listening on port {port}");
                accept_loop(l, None, &ask_tx, &host);
            }
            Err(e) => crate::start::end(&mut ctl, &format!("port {port}: {e}")),
        }
    }
    if a.alwssl != "*NO" {
        let tls = own_link(start).and_then(|mut l| {
            super::tls::server_config(&mut l, &host)
                .map_err(|e| eprintln!("qsjob: QTVTELNET TLS: {e}"))
                .ok()
        });
        if let Some(cfg) = tls {
            match bind(TLS_PORT) {
                Ok(l) => {
                    eprintln!("qsjob: QTVTELNET listening on port {TLS_PORT} (TLS)");
                    accept_loop(l, Some(cfg), &ask_tx, &host);
                }
                Err(e) => eprintln!("qsjob: QTVTELNET port {TLS_PORT}: {e}"),
            }
        }
    }
    // The parent (the job that ran STRTCPSVR) may end long before the
    // server: only an explicit End from it counts, else ENDJOB and
    // ENDSBS reach the server through the subsystem's state.
    let mut ctl_reader = ctl.try_clone().expect("ctl");
    let (end_tx, end_rx) = mpsc::channel::<bool>();
    std::thread::spawn(move || {
        while let Ok(m) = ctl_reader.recv::<ToJob>() {
            if let ToJob::End { immediate } = m {
                let _ = end_tx.send(immediate);
                return;
            }
        }
    });
    let me = start.number;
    stg::remote::status(link, wm::job::Status::Selw, "");
    let mut last_sweep = Instant::now();
    loop {
        match ask_rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ask::JobNumber(reply)) => {
                let n = link.borrow_mut().with(Client::job_number);
                let _ = reply.send(n);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if end_rx.try_recv().is_ok() {
            break;
        }
        let state = link.borrow_mut().with(Client::sbs_state);
        if state.ending.is_some() || state.end_jobs.iter().any(|(n, _)| *n == me) {
            break;
        }
        if last_sweep.elapsed() >= Duration::from_secs(30) {
            last_sweep = Instant::now();
            let minutes = link
                .borrow_mut()
                .with(|l| l.sysval("QDSCJOBITV".into()))
                .map(|v| ccsid::from_ebcdic(&v.data).trim().to_string())
                .and_then(|s| s.parse::<u64>().ok());
            sweep(minutes);
        }
    }
    for d in disconnected().lock().expect("list").drain(..) {
        end_job(d.job);
    }
    let _ = ctl.send(&FromJob::Ending {
        reason: "ENDTCPSVR".into(),
    });
    std::process::exit(0)
}

fn accept_loop(
    listener: TcpListener,
    tls: Option<Arc<rustls::ServerConfig>>,
    ask_tx: &Sender<Ask>,
    host: &str,
) {
    let ask_tx = ask_tx.clone();
    let host = host.to_string();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(stream) = conn else {
                continue;
            };
            let ask = ask_tx.clone();
            let host = host.clone();
            let tls = tls.clone();
            std::thread::spawn(move || {
                if let Err(e) = session(stream, tls, &ask, &host) {
                    eprintln!("qsjob: QTVDEVICE: {e}");
                }
            });
        }
    });
}

/// `PORT(n)` in the request data.
fn port_of(rqsdta: &str) -> Option<u16> {
    rqsdta
        .to_ascii_uppercase()
        .split_once("PORT(")
        .and_then(|(_, r)| r.split(')').next())
        .and_then(|n| n.trim().parse().ok())
}

fn bind(port: u16) -> std::io::Result<TcpListener> {
    let mut last = None;
    for _ in 0..20 {
        match TcpListener::bind(("0.0.0.0", port)) {
            Ok(l) => return Ok(l),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
    Err(last.expect("tried"))
}

/// Disconnected jobs past `QDSCJOBITV` end.
fn sweep(minutes: Option<u64>) {
    let Some(m) = minutes else {
        return;
    };
    let limit = Duration::from_secs(m * 60);
    let expired: Vec<Disconnected> = {
        let mut list = disconnected().lock().expect("list");
        let (gone, keep): (Vec<_>, Vec<_>) =
            list.drain(..).partition(|d| d.since.elapsed() >= limit);
        *list = keep;
        gone
    };
    for d in expired {
        end_job(d.job);
    }
}

fn end_job(mut job: Spawned) {
    let _ = job.chan.send(&ToJob::End { immediate: false });
    std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(5);
        while Instant::now() < until {
            if matches!(job.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = job.child.kill();
    });
}

/// What a session's threads report to it.
enum Ev {
    /// Bytes from the connection.
    Tcp(Vec<u8>),
    /// The connection ended.
    TcpGone,
    /// The job wrote to its display.
    Write(u8, Vec<u8>),
    /// The job released the display.
    Release,
    /// The job's control channel: it is ending, or something else.
    Job(FromJob),
    /// The job's channels closed.
    JobGone,
}

/// The connection, plain or TLS, owned by its I/O thread.
enum Stream {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ServerConnection, TcpStream>>),
}

impl Stream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(s) => s.read(buf),
            Self::Tls(s) => s.read(buf),
        }
    }

    fn write_all(&mut self, b: &[u8]) -> std::io::Result<()> {
        match self {
            Self::Plain(s) => s.write_all(b),
            Self::Tls(s) => s.write_all(b),
        }
    }

    fn shutdown(&mut self) {
        match self {
            Self::Plain(s) => {
                let _ = s.shutdown(std::net::Shutdown::Both);
            }
            Self::Tls(s) => {
                s.conn.send_close_notify();
                let _ = s.flush();
                let _ = s.sock.shutdown(std::net::Shutdown::Both);
            }
        }
    }
}

/// The session's side of the connection: bytes out go on `out`, bytes
/// in arrive as `Ev::Tcp` on the event channel.
struct Transport {
    out: Sender<Vec<u8>>,
}

impl Transport {
    fn send(&self, bytes: Vec<u8>) -> Result<(), String> {
        self.out
            .send(bytes)
            .map_err(|_| "the connection is closed".to_string())
    }

    fn record(&self, opcode: u8, data: Vec<u8>) -> Result<(), String> {
        self.send(telnet::record(&Record::new(opcode, data).encode()))
    }
}

/// The I/O thread: writes what the session sends, reads what arrives.
fn io_thread(mut s: Stream, out_rx: &Receiver<Vec<u8>>, ev_tx: &Sender<Ev>) {
    let mut buf = vec![0u8; 65536];
    loop {
        loop {
            match out_rx.try_recv() {
                Ok(b) => {
                    if s.write_all(&b).is_err() {
                        let _ = ev_tx.send(Ev::TcpGone);
                        return;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    s.shutdown();
                    return;
                }
            }
        }
        match s.read(&mut buf) {
            Ok(0) => {
                let _ = ev_tx.send(Ev::TcpGone);
                return;
            }
            Ok(n) => {
                if ev_tx.send(Ev::Tcp(buf[..n].to_vec())).is_err() {
                    s.shutdown();
                    return;
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => {
                let _ = ev_tx.send(Ev::TcpGone);
                return;
            }
        }
    }
}

/// The start-up response record (RFC 2877): the code and the device.
fn startup_response(code: &str, device: &str) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend(ccsid::to_ebcdic(code).unwrap_or_default());
    data.extend(ccsid::to_ebcdic(&format!("{device:<10}")).unwrap_or_default());
    telnet::record(&Record::new(0, data).encode())
}

/// One connection, start to end.
#[allow(clippy::too_many_lines)] // the negotiation, then the device
fn session(
    stream: TcpStream,
    tls: Option<Arc<rustls::ServerConfig>>,
    ask: &Sender<Ask>,
    host: &str,
) -> Result<(), String> {
    stream.set_nodelay(true).map_err(|e| e.to_string())?;
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_default();
    let s = match tls {
        None => {
            stream
                .set_read_timeout(Some(Duration::from_millis(50)))
                .map_err(|e| e.to_string())?;
            Stream::Plain(stream)
        }
        Some(cfg) => {
            let mut conn = rustls::ServerConnection::new(cfg).map_err(|e| e.to_string())?;
            let mut tcp = stream;
            while conn.is_handshaking() {
                conn.complete_io(&mut tcp)
                    .map_err(|e| format!("{peer}: TLS handshake: {e}"))?;
            }
            tcp.set_read_timeout(Some(Duration::from_millis(50)))
                .map_err(|e| e.to_string())?;
            Stream::Tls(Box::new(rustls::StreamOwned::new(conn, tcp)))
        }
    };
    let (ev_tx, ev_rx) = mpsc::channel::<Ev>();
    let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();
    {
        let ev_tx = ev_tx.clone();
        std::thread::spawn(move || io_thread(s, &out_rx, &ev_tx));
    }
    let tr = Transport { out: out_tx };
    // The session is a job of its own in the active job table.
    let mut link = {
        let (tx, rx) = mpsc::channel();
        ask.send(Ask::JobNumber(tx)).map_err(|e| e.to_string())?;
        let number = rx.recv().map_err(|e| e.to_string())??;
        let chan = Chan::connect(Path::new(stg::SOCKET)).map_err(|e| e.to_string())?;
        let mut l = Link::over(chan);
        l.hello(JobInfo {
            name: "QTVDEVICE".into(),
            user: "QTCP".into(),
            number,
            kind: 'B',
            subtype: ' ',
            subsystem: "QSYSWRK".into(),
            pid: std::process::id(),
            handle: u64::from(number),
            jobd: "QSYS/QSYSJOBD".into(),
            class: "QSYS/QSYSCLS20".into(),
            pool: "*BASE".into(),
            ..JobInfo::default()
        })
        .map_err(|e| e.to_string())??;
        l
    };
    let seed = seed();
    let mut server = Server::new(seed);
    tr.send(server.opening())?;
    let deadline = Instant::now() + Duration::from_secs(30);
    while !server.done {
        let ev = ev_rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| format!("{peer}: negotiation timed out"))?;
        match ev {
            Ev::Tcp(bytes) => {
                let (reply, _records) = server.feed(&bytes);
                if !reply.is_empty() {
                    tr.send(reply)?;
                }
            }
            Ev::TcpGone => return Err(format!("{peer}: closed during negotiation")),
            _ => {}
        }
    }
    let neg = server.negotiated.clone();
    let requested = neg.var("DEVNAME").unwrap_or_default().to_ascii_uppercase();
    if let Some((ptype, pmodel)) = printer_type_model(&neg.terminal_type) {
        return printer_session(&tr, &mut server, &mut link, &ev_rx, &requested, ptype, pmodel, &peer);
    }
    let model = Model::from_terminal_type(&neg.terminal_type).unwrap_or_default();
    let (devtype, devmodel) = model.device_type_model();
    let kbdtype = neg.var("KBDTYPE").unwrap_or_else(|| "USB".into());
    let device = match link
        .device_allocate(requested.clone(), devtype.into(), devmodel.into(), kbdtype)
        .map_err(|e| e.to_string())?
    {
        Ok(d) => d,
        Err(code) => {
            if !requested.is_empty() {
                let _ = tr.send(startup_response(&code[..4.min(code.len())], &requested));
            }
            return Err(format!("{peer}: no device: {code}"));
        }
    };
    if !requested.is_empty() {
        tr.send(startup_response("I902", &device))?;
    }
    let auto = auto_signon(&neg, seed, &mut link, &device);
    let r = serve_device(
        &tr,
        &mut server,
        &mut link,
        &ev_rx,
        &ev_tx,
        &device,
        model,
        host,
        auto,
        &peer,
    );
    // A device whose job waits disconnected stays in recovery pending.
    let kept = disconnected()
        .lock()
        .expect("list")
        .iter()
        .any(|d| d.device == device);
    if !kept {
        let _ = link.device_status(device.clone(), 1);
    }
    r
}

/// A printer session: the named printer device, its writer started,
/// and the writer's records (SCS) carried to the client until the
/// connection drops. `BLOCKED(phase8)`: the printer record framing is
/// RFC 2877 as read, unverified against a real printer client.
#[allow(clippy::too_many_arguments)]
fn printer_session(
    tr: &Transport,
    server: &mut Server,
    link: &mut Link,
    ev_rx: &Receiver<Ev>,
    requested: &str,
    devtype: &str,
    model: &str,
    peer: &str,
) -> Result<(), String> {
    let device = match link
        .device_allocate(requested.to_string(), devtype.into(), model.into(), String::new())
        .map_err(|e| e.to_string())?
    {
        Ok(d) => d,
        Err(code) => {
            let _ = tr.send(startup_response(&code[..4.min(code.len())], requested));
            return Err(format!("{peer}: no printer device: {code}"));
        }
    };
    tr.send(startup_response("I902", &device))?;
    let path = dev::socket(&device);
    let _ = std::fs::create_dir_all(dev::DIR);
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).map_err(|e| format!("{path}: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let started = link
        .exec(format!("STRPRTWTR DEV({device}) WTR({device})"))
        .map_err(|e| e.to_string())?;
    if started.text.lines().next().is_some_and(|l| l.starts_with("CPF") && !l.starts_with("CPF3313")) {
        eprintln!("qsjob: QTVDEVICE: {device}: {}", started.text.trim());
    }
    let mut writer: Option<Chan> = None;
    let r = loop {
        if writer.is_none() {
            match listener.accept() {
                Ok((c, _)) => {
                    c.set_nonblocking(false).map_err(|e| e.to_string())?;
                    c.set_read_timeout(Some(Duration::from_millis(50))).map_err(|e| e.to_string())?;
                    writer = Some(Chan::new(c));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => break Err(e.to_string()),
            }
        }
        if let Some(w) = writer.as_mut() {
            match w.recv::<ToDevice>() {
                Ok(ToDevice::Write { opcode, data }) => tr.record(opcode, data)?,
                Err(ipc::WireError::Io(ref e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Ok(ToDevice::Release) | Err(_) => writer = None,
            }
        }
        match ev_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Ev::Tcp(bytes)) => {
                let (reply, records) = server.feed(&bytes);
                if !reply.is_empty() {
                    tr.send(reply)?;
                }
                for b in records {
                    if let (Ok(r), Some(w)) = (Record::decode(&b), writer.as_mut()) {
                        let _ = w.send(&FromDevice::Record {
                            flags: r.flags,
                            opcode: r.opcode,
                            data: r.data,
                        });
                    }
                }
            }
            Ok(Ev::TcpGone) | Err(mpsc::RecvTimeoutError::Disconnected) => break Ok(()),
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    };
    if let Some(mut w) = writer.take() {
        let _ = w.send(&FromDevice::Gone);
    }
    let _ = std::fs::remove_file(&path);
    let _ = link.exec(format!("ENDWTR WTR({device}) OPTION(*IMMED)"));
    let _ = link.device_status(device, 1);
    r
}

fn seed() -> [u8; 8] {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut s = [0u8; 8];
    let mut x = u64::try_from(t & u128::from(u64::MAX)).unwrap_or(0)
        ^ u64::from(std::process::id()).rotate_left(32)
        ^ 0x9E37_79B9_7F4A_7C15;
    for b in &mut s {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *b = (x & 0xFF) as u8;
    }
    s
}

/// RFC 4777: the user's substitute checked against the password
/// store; the signed-on user when it matched.
fn auto_signon(
    neg: &Negotiated,
    seed: [u8; 8],
    link: &mut Link,
    device: &str,
) -> Option<sec::signon::SignedOn> {
    let user = neg.var("USER")?.to_ascii_uppercase();
    let client_seed = neg.var_bytes("IBMRSEED")?;
    let sub = neg.var_bytes("IBMSUBSPW")?;
    if client_seed.len() != 8 {
        return None;
    }
    let mut cs = [0u8; 8];
    cs.copy_from_slice(client_seed);
    let proof = sec::signon::Substitute {
        server_seed: seed,
        client_seed: cs,
        substitute: sub.to_vec(),
    };
    match link.sign_on_substitute(user, device.to_string(), proof) {
        Ok(Ok(s)) => Some(s),
        _ => None,
    }
}

/// The sign-on display and the jobs behind it, until the connection
/// goes.
#[allow(clippy::too_many_arguments)] // the session's parts
fn serve_device(
    tr: &Transport,
    server: &mut Server,
    link: &mut Link,
    ev_rx: &Receiver<Ev>,
    ev_tx: &Sender<Ev>,
    device: &str,
    model: Model,
    host: &str,
    mut auto: Option<sec::signon::SignedOn>,
    peer: &str,
) -> Result<(), String> {
    let mut message = String::new();
    loop {
        link.device_status(device.to_string(), 2)
            .map_err(|e| e.to_string())??;
        let signed = match auto.take() {
            Some(s) => s,
            None => {
                match sign_on_display(tr, server, link, ev_rx, device, model, host, &message)? {
                    Some(s) => s,
                    None => return Ok(()),
                }
            }
        };
        message.clear();
        link.device_status(device.to_string(), 3)
            .map_err(|e| e.to_string())??;
        let outcome = run_job(tr, server, link, ev_rx, ev_tx, device, model, signed, peer)?;
        match outcome {
            Outcome::SignedOff(m) => message = m,
            Outcome::ConnectionGone => return Ok(()),
        }
    }
}

enum Outcome {
    SignedOff(String),
    ConnectionGone,
}

/// Show the sign-on display until a sign-on is accepted. `None` when
/// the connection went away.
#[allow(clippy::too_many_arguments)]
fn sign_on_display(
    tr: &Transport,
    server: &mut Server,
    link: &mut Link,
    ev_rx: &Receiver<Ev>,
    device: &str,
    model: Model,
    host: &str,
    message: &str,
) -> Result<Option<sec::signon::SignedOn>, String> {
    let subsystem = subsystem_for(link, device).map_or_else(|| "QINTER".into(), |(s, _)| s);
    let long = link
        .sysval("QPWDLVL".into())
        .ok()
        .flatten()
        .and_then(|v| ccsid::from_ebcdic(&v.data).trim().parse::<u8>().ok())
        .is_some_and(|l| l >= 2);
    let mut m = ScreenModel::new(model.wide());
    let mut message = message.to_string();
    // The sign-on is the shipped display file, QDSIGNON2 from password
    // level 2; the interim panel stands in when the file is missing.
    let file = if long { "QDSIGNON2" } else { "QDSIGNON" };
    let mut rt = link
        .dsp_open(file.into(), "QSYS".into(), "*JOB".into())
        .ok()
        .and_then(Result::ok)
        .and_then(|(b, _)| dspf::decode(&b).ok())
        .map(|df| dspf::rt::Runtime::new(df, model.wide()));
    let env = sign_on_env(link, host);
    loop {
        if let Some(rt) = rt.as_mut() {
            let rec = rt.file().record("SIGNON").cloned();
            let Some(rec) = rec else {
                return Err("QDSIGNON has no SIGNON format".into());
            };
            let out = display::dds::out_buffer(
                &rec,
                &[("QSYSNAME", host), ("SBSNAME", &subsystem), ("DEVNAME", device)],
            );
            rt.write(&mut m, &env, "SIGNON", &out, &[])
                .map_err(|e| e.to_string())?;
            if !message.is_empty() {
                m.error_line(&message, tn5250::ds::attr::WHITE);
                m.alarm();
            }
        } else {
            let p = wm::panels::sign_on(host, &subsystem, device, long, &message);
            display::render(&p, &mut m);
        }
        let out = m.render(ModelRead::MdtFields);
        tr.record(out.opcode, out.data)?;
        let rec = loop {
            match ev_rx.recv() {
                Ok(Ev::Tcp(bytes)) => {
                    let (reply, records) = server.feed(&bytes);
                    if !reply.is_empty() {
                        tr.send(reply)?;
                    }
                    if let Some(r) = records.into_iter().find_map(|b| Record::decode(&b).ok()) {
                        break r;
                    }
                }
                Ok(Ev::TcpGone) | Err(_) => return Ok(None),
                Ok(_) => {}
            }
        };
        let Some(inb) = m.accept(&rec.data) else {
            continue;
        };
        let key = display::key_of(rec.flags, inb.aid);
        if key != Some(0) {
            continue;
        }
        let v: Vec<String> = match rt.as_mut() {
            Some(rt) => match rt.input(&mut m, &env, "SIGNON", inb.aid, (inb.row, inb.col)) {
                Ok(input) => rt.file().record("SIGNON").map_or_else(Vec::new, |rec| {
                    ["USERID", "PASSWRD", "PROGRAM", "MENU", "CURLIB"]
                        .iter()
                        .map(|n| display::dds::in_text(rec, &input.record, n).unwrap_or_default())
                        .collect()
                }),
                Err(p) => {
                    message = p.to_string();
                    continue;
                }
            },
            None => m.values(),
        };
        let user = v.first().map_or("", |s| s.trim()).to_ascii_uppercase();
        let password = v.get(1).map_or("", |s| s.trim()).to_string();
        if user.is_empty() {
            message = "Type a user profile name.".into();
            continue;
        }
        let checked = link
            .sign_on(user, password, device.to_string(), false)
            .map_err(|e| e.to_string())?;
        match checked {
            Ok(mut s) => {
                apply_typed(&mut s, &v);
                return Ok(Some(s));
            }
            Err(r) => message = r.text,
        }
    }
}

/// The sign-on display's environment: the clock and the system's date
/// format, no user yet.
fn sign_on_env(link: &mut Link, host: &str) -> dspf::rt::Env {
    let now = link.now().unwrap_or_default();
    let mut sysval = |name: &str| {
        link.sysval(name.into())
            .ok()
            .flatten()
            .map(|v| ccsid::from_ebcdic(&v.data).trim().to_string())
            .unwrap_or_default()
    };
    let datfmt = sysval("QDATFMT");
    let datsep = sysval("QDATSEP");
    display::dds::env_from(&now, &datfmt, &datsep, "", host)
}

/// What the sign-on display's fields may change, as `LMTCPB` allows.
fn apply_typed(s: &mut sec::signon::SignedOn, v: &[String]) {
    let typed = |i: usize| {
        v.get(i)
            .map(|f| f.trim().to_ascii_uppercase())
            .filter(|f| !f.is_empty())
    };
    let (program, menu, curlib) = match s.lmtcpb.as_str() {
        "*YES" => (false, false, false),
        "*PARTIAL" => (false, true, false),
        _ => (true, true, true),
    };
    if let (true, Some(m)) = (menu, typed(3)) {
        s.inlmnu = m;
    }
    if let (true, Some(c)) = (curlib, typed(4)) {
        s.curlib = c;
    }
    if let (true, Some(p)) = (program, typed(2)) {
        s.inlpgm = p;
    }
}

/// The active subsystem whose workstation entries take the device: a
/// name entry for it, else a type entry for every device; with the
/// subsystem's description.
/// Whether a work station type entry takes `device`: every display,
/// the virtual ones, their types, or the console for `*CONS`.
fn type_entry_matches(entry: &str, device: &str) -> bool {
    matches!(entry, "*ALL" | "*VRT" | "3179" | "3477")
        || (entry == "*CONS" && device == crate::display::CONSOLE_DEVICE)
}

fn subsystem_for(link: &mut Link, device: &str) -> Option<(String, Sbsd)> {
    let active = link.sbs_list().ok()?;
    let mut by_type = None;
    for s in active {
        let Ok(Ok(b)) = link.wm_object(format!("QSYS/{}", s.name), "*SBSD".into()) else {
            continue;
        };
        let Ok(d) = Sbsd::decode(&b) else {
            continue;
        };
        if d.wsns.iter().any(|w| {
            w.name == device
                || w.name
                    .strip_suffix('*')
                    .is_some_and(|p| device.starts_with(p))
        }) {
            return Some((s.name, d));
        }
        if by_type.is_none()
            && d.wsts.iter().any(|w| type_entry_matches(&w.name, device))
        {
            by_type = Some((s.name.clone(), d));
        }
    }
    by_type
}

/// Where a sign-on's job runs: job description, class, program, pool.
struct Routed {
    subsystem: String,
    jobd: String,
    class: String,
    program: String,
    pool: String,
}

fn route(link: &mut Link, device: &str, profile_jobd: &str) -> Result<Routed, String> {
    let (sbs, d) = subsystem_for(link, device)
        .ok_or_else(|| format!("CPF1109 no subsystem has a work station entry for {device}"))?;
    let wse = d
        .wsns
        .iter()
        .find(|w| {
            w.name == device
                || w.name
                    .strip_suffix('*')
                    .is_some_and(|p| device.starts_with(p))
        })
        .or_else(|| d.wsts.iter().find(|w| type_entry_matches(&w.name, device)))
        .ok_or_else(|| format!("CPF1109 subsystem {sbs} has no work station entry for {device}"))?;
    let jobd = if wse.jobd.0 == "*USRPRF" && !profile_jobd.is_empty() {
        if profile_jobd.contains('/') {
            profile_jobd.to_string()
        } else {
            format!("QGPL/{profile_jobd}")
        }
    } else if wse.jobd.0.starts_with('*') {
        "QGPL/QDFTJOBD".to_string()
    } else {
        format!("{}/{}", wse.jobd.1, wse.jobd.0)
    };
    let jb = link
        .wm_object(jobd.clone(), "*JOBD".into())
        .map_err(|e| e.to_string())??;
    let jd = JobD::decode(&jb).map_err(|e| e.to_string())?;
    let r = d
        .route(&jd.rtgdta)
        .ok_or_else(|| format!("CPF1226 no routing entry in {sbs} for '{}'", jd.rtgdta))?;
    let kind = d.pool(r.pool).map_or(PoolKind::Base, |p| p.kind);
    Ok(Routed {
        pool: match kind {
            PoolKind::Private => format!("{sbs}/{}", r.pool),
            k => k.name(),
        },
        subsystem: sbs,
        jobd,
        class: format!("{}/{}", r.cls.1, r.cls.0),
        program: format!("{}/{}", r.pgm.1, r.pgm.0),
    })
}

/// Start the user's job (or take back a disconnected one) and move
/// records between it and the connection until it ends or the
/// connection drops.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // one arm per event
fn run_job(
    tr: &Transport,
    server: &mut Server,
    link: &mut Link,
    ev_rx: &Receiver<Ev>,
    ev_tx: &Sender<Ev>,
    device: &str,
    model: Model,
    signed: sec::signon::SignedOn,
    peer: &str,
) -> Result<Outcome, String> {
    let taken = {
        let mut list = disconnected().lock().expect("list");
        list.iter()
            .position(|d| d.user == signed.user)
            .map(|i| list.remove(i))
    };
    let (mut job, mut dev_chan, dev_tx, ctl_tx) = if let Some(d) = taken {
        let _ = link.device_status(d.device.clone(), 1);
        // The readers started by the first session go on; they
        // deliver here now.
        *d.dev_tx.lock().expect("sender") = ev_tx.clone();
        *d.ctl_tx.lock().expect("sender") = ev_tx.clone();
        let mut c = d.link;
        let _ = c.send(&FromDevice::Reset {
            device: device.to_string(),
        });
        (d.job, c, d.dev_tx, d.ctl_tx)
    } else {
        {
            let (job, dev_chan) = start_job(link, device, model, signed)?;
            // The job's device channel and control channel, read on
            // threads that outlive this session.
            let dev_tx: SharedTx = Arc::new(Mutex::new(ev_tx.clone()));
            let ctl_tx: SharedTx = Arc::new(Mutex::new(ev_tx.clone()));
            relay::<ToDevice>(
                dev_chan.try_clone().map_err(|e| e.to_string())?,
                Arc::clone(&dev_tx),
                |m| match m {
                    ToDevice::Write { opcode, data } => Ev::Write(opcode, data),
                    ToDevice::Release => Ev::Release,
                },
            );
            relay::<FromJob>(
                job.chan.try_clone().map_err(|e| e.to_string())?,
                Arc::clone(&ctl_tx),
                Ev::Job,
            );
            (job, dev_chan, dev_tx, ctl_tx)
        }
    };
    // `QINACTITV`: minutes without input before `QINACTMSGQ` acts.
    let inactive_after = link
        .sysval("QINACTITV".into())
        .ok()
        .flatten()
        .and_then(|v| ccsid::from_ebcdic(&v.data).trim().parse::<u64>().ok())
        .filter(|m| *m > 0)
        .map(|m| Duration::from_secs(m * 60));
    let mut last_input = Instant::now();
    let outcome = loop {
        let ev = match ev_rx.recv_timeout(Duration::from_secs(30)) {
            Ok(ev) => ev,
            Err(mpsc::RecvTimeoutError::Disconnected) => break Outcome::ConnectionGone,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if inactive_after.is_some_and(|d| last_input.elapsed() >= d) {
                    let action = link.sysval("QINACTMSGQ".into()).ok().flatten().map_or_else(
                        || "*ENDJOB".into(),
                        |v| ccsid::from_ebcdic(&v.data).trim().to_string(),
                    );
                    match action.split_whitespace().next().unwrap_or("*ENDJOB") {
                        "*ENDJOB" => {
                            end_job(job);
                            let _ = dev_chan.send(&FromDevice::Gone);
                            return Ok(Outcome::SignedOff(
                                "The job ended: the display was inactive too long.".into(),
                            ));
                        }
                        "*DSCJOB" => {
                            disconnected().lock().expect("list").push(Disconnected {
                                user: job.start.user.clone(),
                                device: device.to_string(),
                                job,
                                link: dev_chan,
                                since: Instant::now(),
                                dev_tx,
                                ctl_tx,
                            });
                            return Ok(Outcome::SignedOff(
                                "Job disconnected after inactivity; sign on again to return to it."
                                    .into(),
                            ));
                        }
                        queue => {
                            let (q, lib) = action
                                .split_once(' ')
                                .map_or((queue.to_string(), "QSYS".to_string()), |(q, l)| {
                                    (q.to_string(), l.trim().to_string())
                                });
                            let _ = link.exec(format!(
                                "SNDMSG MSG('Display {device} has been inactive for the QINACTITV interval.') TOMSGQ({lib}/{q})"
                            ));
                            last_input = Instant::now();
                        }
                    }
                }
                continue;
            }
        };
        match ev {
            Ev::Tcp(bytes) => {
                let (reply, records) = server.feed(&bytes);
                if !reply.is_empty() {
                    tr.send(reply)?;
                }
                for b in records {
                    if let Ok(r) = Record::decode(&b) {
                        last_input = Instant::now();
                        let _ = dev_chan.send(&FromDevice::Record {
                            flags: r.flags,
                            opcode: r.opcode,
                            data: r.data,
                        });
                    }
                }
            }
            Ev::TcpGone => {
                let action = link.sysval("QDEVRCYACN".into()).ok().flatten().map_or_else(
                    || "*DSCMSG".into(),
                    |v| ccsid::from_ebcdic(&v.data).trim().to_string(),
                );
                if action.starts_with("*ENDJOB") {
                    let _ = dev_chan.send(&FromDevice::Gone);
                    end_job(job);
                } else {
                    eprintln!(
                        "qsjob: QTVDEVICE: {peer} dropped; job of {} kept ({action})",
                        job.start.user
                    );
                    let _ = link.device_status(device.to_string(), 4);
                    disconnected().lock().expect("list").push(Disconnected {
                        user: job.start.user.clone(),
                        device: device.to_string(),
                        job,
                        link: dev_chan,
                        since: Instant::now(),
                        dev_tx,
                        ctl_tx,
                    });
                }
                return Ok(Outcome::ConnectionGone);
            }
            Ev::Write(opcode, data) => tr.record(opcode, data)?,
            Ev::Job(FromJob::Ending { .. }) | Ev::JobGone => {
                break Outcome::SignedOff(String::new());
            }
            Ev::Job(FromJob::Disconnect) => {
                // `DSCJOB`: the sign-on display, the job kept.
                disconnected().lock().expect("list").push(Disconnected {
                    user: job.start.user.clone(),
                    device: device.to_string(),
                    job,
                    link: dev_chan,
                    since: Instant::now(),
                    dev_tx,
                    ctl_tx,
                });
                return Ok(Outcome::SignedOff(
                    "Job disconnected; sign on again to return to it.".into(),
                ));
            }
            Ev::Job(FromJob::PowerDown {
                immediate,
                delay,
                restart,
            }) => {
                let _ = link.power_down(immediate, delay, restart);
            }
            Ev::Job(FromJob::Output { text }) => eprint!("{text}"),
            Ev::Release | Ev::Job(_) => {}
        }
    };
    let _ = job.child.wait();
    Ok(outcome)
}

/// Spawn the interactive job for a sign-on with the device socket
/// ready for it, and take its connection.
fn start_job(
    link: &mut Link,
    device: &str,
    model: Model,
    mut signed: sec::signon::SignedOn,
) -> Result<(Spawned, Chan), String> {
    let r = route(link, device, &signed.jobd)?;
    let number = link.job_number().map_err(|e| e.to_string())??;
    let path = dev::socket(device);
    let _ = std::fs::create_dir_all(dev::DIR);
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).map_err(|e| format!("{path}: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let program = if signed.inlpgm == "*NONE" {
        String::new()
    } else {
        std::mem::take(&mut signed.inlpgm)
    };
    let _ = model;
    let start = Start {
        name: device.to_string(),
        user: signed.user.clone(),
        number,
        handle: u64::from(number),
        kind: 'I',
        subsystem: r.subsystem,
        role: "INTER".into(),
        jobd: r.jobd,
        class: r.class,
        program: r.program,
        rqsdta: program,
        pool: r.pool,
        lda: Vec::new(),
        signon: Some(signed),
        device: device.to_string(),
    };
    let mut s = spawn::job(start)?;
    spawn::ready(&mut s)?;
    let until = Instant::now() + Duration::from_secs(10);
    let conn = loop {
        match listener.accept() {
            Ok((c, _)) => break c,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() > until {
                    let _ = s.chan.send(&ToJob::End { immediate: true });
                    return Err(format!("job {} did not open its display", s.start.name));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    conn.set_nonblocking(false).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&path);
    Ok((s, Chan::new(conn)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_from_request_data() {
        assert_eq!(port_of(""), None);
        assert_eq!(port_of("PORT(2323)"), Some(2323));
    }

    #[test]
    fn startup_response_is_a_record() {
        let r = startup_response("I902", "QPADEV0001");
        assert_eq!(&r[r.len() - 2..], &[0xFF, 0xEF]);
        assert_eq!(r[2..4], [0x12, 0xA0]);
    }
}
