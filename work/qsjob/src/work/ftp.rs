// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*FTP` (docs/FTP.md): role `FTP` is the listener `QTFTP00001`, which
//! accepts on 21 (and 990, TLS from the first byte) and starts a
//! `QTFTP00nnn` job of user `QTCP` for each connection, handing it the
//! socket over its control channel; role `FTPSESS` is that job: it runs
//! `net/ftp`'s session over the socket, signs the user on through the
//! storage service and takes the user's profile (`QSYGETPH`/`QWTSETP`),
//! reaches the IFS over its link, runs `RCMD` in its own runtime and
//! calls the exit programs of `QIBM_QTMF_SERVER_REQ` and
//! `QIBM_QTMF_SVR_LOGON`. `AUTH TLS` and port 990 present the
//! certificate assigned to `QIBM_QTMF_FTP_SERVER` in `*SYSTEM`, read
//! as `QTCP` before sign-on, else the system's own (`tls.rs`).

use std::net::TcpStream;
use std::os::fd::OwnedFd;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ftp::host::{CmdFail, DbFile, Host, Profile, Refused};
use ftp::net::tcp::TcpNet;
use ftp::session::{Config, Session};
use ifs::client::IfsClient;
use ipc::chan::Chan;
use regfac::{ExitAnswer, ExitRegistry, ExitRequest};
use servers::attrs::ftp::FtpAttrs;
use servers::listen::{self, Conn, Running, Service};
use stg::remote::RemoteIfs;
use stg::tcp::{TcpOp, TcpReply};
use stg::{Client, Shared};
use wm::ctl::{FromJob, Start, ToJob};

use super::spawn::{self, Spawned};

/// The server's attributes from the server table.
fn attrs(link: &Shared) -> FtpAttrs {
    match link.borrow_mut().with(|l| l.tcp(TcpOp::Servers)) {
        TcpReply::Servers { list } => list
            .into_iter()
            .find(|s| s.server == "*FTP")
            .map(|s| FtpAttrs::from_bag(&s.bag))
            .unwrap_or_default(),
        _ => FtpAttrs::default(),
    }
}

/// The system's name.
fn system_name(link: &Shared) -> String {
    link.borrow_mut()
        .with(|l| l.sysval("QSYSNAME".into()))
        .map_or_else(|| "QS400".to_string(), |v| ccsid::from_ebcdic(&v.data).trim().to_string())
}

/// `KWD(n)` of the request data.
fn number_of(rqsdta: &str, kwd: &str) -> Option<u16> {
    servers::attrs::keyword(rqsdta, kwd).and_then(|v| v.trim().parse().ok())
}

/// A connection the accept threads hand the main thread: the socket and
/// whether it came in on the implicit TLS port.
type Accepted = (TcpStream, bool);

/// The accept loop's service: connections go to the main thread, which
/// owns the link and starts the jobs.
struct Accept {
    tx: Mutex<Sender<Accepted>>,
    implicit: bool,
}

impl Service for Accept {
    fn serve(&self, conn: Conn, _peer: std::net::SocketAddr) {
        if let Conn::Plain(s) = conn {
            let _ = self.tx.lock().map(|tx| tx.send((s, self.implicit)));
        }
    }
}

/// Start a session job for a connection and hand it the socket.
fn start_session(link: &Shared, start: &Start, seq: u32, sock: TcpStream, implicit: bool) -> Result<Spawned, String> {
    let number = link.borrow_mut().with(Client::job_number)?;
    let name = servers::registry::lookup("*FTP")
        .and_then(|s| s.jobs.iter().find(|j| j.kind == servers::JobKind::Worker))
        .map_or_else(|| format!("QTFTP{seq:05}"), |j| j.numbered(seq));
    let s = Start {
        name,
        user: "QTCP".into(),
        number,
        handle: u64::from(number),
        kind: 'B',
        subsystem: start.subsystem.clone(),
        role: "FTPSESS".into(),
        jobd: "QSYS/QSYSJOBD".into(),
        class: "QSYS/QSYSCLS20".into(),
        pool: "*BASE".into(),
        rqsdta: if implicit { "IMPLICIT(1)".into() } else { String::new() },
        ..Start::default()
    };
    let mut job = spawn::job(s)?;
    spawn::ready(&mut job)?;
    let chan = job.chan.try_clone().map_err(|e| e.to_string())?;
    let via = sockets::Fd::from(chan.into_fd());
    let fd = sockets::Fd::from(OwnedFd::from(sock));
    sockets::sys::send_fd(&via, &fd).map_err(|e| format!("passing the connection: {e:?}"))?;
    Ok(job)
}

/// Run the listener. Never returns.
pub fn run(mut ctl: Chan, link: &Shared, start: &Start) -> ! {
    let a = attrs(link);
    let port = number_of(&start.rqsdta, "PORT").unwrap_or(a.port);
    let tls_port = number_of(&start.rqsdta, "TLSPORT").unwrap_or(a.tls_port);
    let (tx, rx): (Sender<Accepted>, Receiver<Accepted>) = mpsc::channel();
    let mut running: Vec<Running> = Vec::new();
    for (p, implicit, on) in [(port, false, a.plain()), (tls_port, true, a.secure())] {
        if !on {
            continue;
        }
        let service = Arc::new(Accept {
            tx: Mutex::new(tx.clone()),
            implicit,
        });
        match listen::bind(p).and_then(|l| listen::serve(l, None, service)) {
            Ok(r) => {
                eprintln!("qsjob: {} listening on port {p}{}", start.name, if implicit { " (TLS)" } else { "" });
                running.push(r);
            }
            Err(e) if !implicit => crate::start::end(&mut ctl, &format!("port {p}: {e}")),
            Err(e) => eprintln!("qsjob: {} port {p}: {e}", start.name),
        }
    }
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
    stg::remote::status(link, wm::job::Status::Selw, "");
    let mut sessions: Vec<Spawned> = Vec::new();
    let mut seq: u32 = 1;
    loop {
        if let Ok((sock, implicit)) = rx.recv_timeout(Duration::from_millis(500)) {
            seq = if seq >= 999 { 2 } else { seq + 1 };
            match start_session(link, start, seq, sock, implicit) {
                Ok(j) => sessions.push(j),
                Err(e) => eprintln!("qsjob: {}: no session job: {e}", start.name),
            }
        }
        sessions.retain_mut(|j| !matches!(j.child.try_wait(), Ok(Some(_))));
        if end_rx.try_recv().is_ok() {
            break;
        }
        let state = link.borrow_mut().with(Client::sbs_state);
        if state.ending.is_some() || state.end_jobs.iter().any(|(n, _)| *n == me) {
            break;
        }
    }
    for r in running {
        r.stop();
    }
    // The sessions end with the server.
    for mut j in sessions {
        let _ = j.child.kill();
        let _ = j.child.wait();
    }
    let _ = ctl.send(&FromJob::Ending {
        reason: "ENDTCPSVR".into(),
    });
    std::process::exit(0)
}

/// What a session job reaches: its link, the IFS over it, its runtime
/// for `RCMD` and exit programs.
struct JobHost {
    link: Shared,
    ifs: RemoteIfs,
    console: crate::console::Console,
    ccsid: u16,
}

impl JobHost {
    fn sec(&self, q: sec::api::Query) -> Result<sec::api::Answer, String> {
        self.link
            .borrow_mut()
            .with(|l| l.sec_api(q))
            .map_err(|e| e.msgid)
    }
}

impl Host for JobHost {
    fn ifs(&self) -> &dyn IfsClient {
        &self.ifs
    }

    fn sign_on(&mut self, user: &str, password: &str) -> Result<String, Refused> {
        let r = self
            .link
            .borrow_mut()
            .with(|l| l.sign_on(user.to_ascii_uppercase(), password.to_string(), String::new(), false));
        r.map(|s| s.user).map_err(|e| Refused {
            msgid: e.msgid,
            text: e.text,
        })
    }

    fn profile(&mut self, user: &str) -> Option<Profile> {
        match self.sec(sec::api::Query::Profile {
            user: user.to_ascii_uppercase(),
        }) {
            Ok(sec::api::Answer::Profile { data }) => {
                let p = sec::profile::UsrPrf::decode(&data).ok()?;
                Some(Profile {
                    user: user.to_ascii_uppercase(),
                    home: if p.homedir.trim().is_empty() || p.homedir.starts_with('*') {
                        format!("/home/{}", user.to_ascii_uppercase())
                    } else {
                        p.homedir.trim().to_string()
                    },
                    curlib: p.curlib.trim().to_string(),
                    ccsid: u16::try_from(p.ccsid).unwrap_or(0),
                })
            }
            _ => None,
        }
    }

    fn become_user(&mut self, user: &str, password: Option<&str>) -> Result<(), String> {
        let handle = match self.sec(sec::api::Query::GetHandle {
            user: user.to_ascii_uppercase(),
            password: password.unwrap_or("*NOPWD").to_string(),
        })? {
            sec::api::Answer::Bytes { data } => data,
            other => return Err(format!("unexpected answer {other:?}")),
        };
        self.sec(sec::api::Query::SetHandle { handle })?;
        Ok(())
    }

    fn command(&mut self, line: &str) -> Result<(), CmdFail> {
        let o = self.console.run(line, crate::command::RunMode::Run);
        match o.escape {
            Some(f) => Err(CmdFail {
                text: if f.text.is_empty() { f.msgid.clone() } else { f.text },
                msgid: f.msgid,
            }),
            None => Ok(()),
        }
    }

    fn exit(&mut self, point: &str, req: &ExitRequest) -> ExitAnswer {
        let mut reg = stg::remote::RemoteRegistry::new(Rc::clone(&self.link));
        match reg.programs(point, req.format()) {
            Ok(p) if p.is_empty() => ExitAnswer {
                allow: true,
                outputs: req.parms(),
                called: Vec::new(),
                error: None,
            },
            _ => regfac::call_exit(&mut reg, self.console.job(), point, req),
        }
    }

    fn file(&mut self, lib: &str, file: &str) -> Option<DbFile> {
        let f = self.link.borrow_mut().with(|l| l.file(lib.to_string(), file.to_string()))?;
        let ccsid = self
            .ifs
            .stat(&ftp::objects::file_path(lib, file), true)
            .map_or(self.ccsid, |s| s.ccsid);
        let len = usize::try_from(f.rec_len).unwrap_or(0);
        Some(DbFile {
            rec_len: if f.source { len.saturating_sub(12) } else { len },
            source: f.source,
            ccsid,
            logical: f.attr == "LF",
        })
    }

    fn sysval(&mut self, name: &str) -> Option<String> {
        let v = self.link.borrow_mut().with(|l| l.sysval(name.to_string()))?;
        Some(match v.kind {
            sysapi::SysValKind::Bin if v.data.len() == 4 => {
                i32::from_be_bytes([v.data[0], v.data[1], v.data[2], v.data[3]]).to_string()
            }
            _ => ccsid::from_ebcdic(&v.data).trim().to_string(),
        })
    }

    fn job_ccsid(&self) -> u16 {
        self.ccsid
    }

    fn now(&self) -> i64 {
        let n = self.link.borrow_mut().with(Client::now);
        i64::try_from(n.unix).unwrap_or(0)
    }

    fn utc_offset(&self) -> i64 {
        i64::from(self.link.borrow_mut().with(Client::now).offset_min) * 60
    }
}

/// The socket the listener passed over the control channel.
fn take_socket(ctl: &Chan) -> Result<TcpStream, String> {
    let chan = ctl.try_clone().map_err(|e| e.to_string())?;
    let via = sockets::Fd::from(chan.into_fd());
    let fd = sockets::sys::recv_fd(&via).map_err(|e| format!("{e:?}"))?;
    Ok(TcpStream::from(fd.into_owned()))
}

/// Run one session. Never returns.
pub fn session(mut ctl: Chan, link: &Shared, start: &Start) -> ! {
    let sock = match take_socket(&ctl) {
        Ok(s) => s,
        Err(e) => crate::start::end(&mut ctl, &format!("no connection: {e}")),
    };
    let a = attrs(link);
    let system = system_name(link);
    let implicit = number_of(&start.rqsdta, "IMPLICIT") == Some(1);
    let tls = if a.secure() {
        super::tls::app_server_config(&mut link.borrow_mut(), ftp::server::APP_ID, &system)
            .map_err(|e| eprintln!("qsjob: {} TLS: {e}", start.name))
            .ok()
    } else {
        None
    };
    let ports = a.dtacnnport;
    let mut cfg = Config::new(a, &system);
    cfg.implicit = implicit;
    let mut net = match TcpNet::new(sock, tls, implicit, ports) {
        Ok(n) => n,
        Err(e) => crate::start::end(&mut ctl, &format!("connection: {e}")),
    };
    let console = crate::console::Console::new(ctl, link, start);
    let jattrs = console.host.attrs();
    let ccsid = match u16::try_from(jattrs.ccsid) {
        Ok(65535) | Err(_) => u16::try_from(jattrs.dftccsid).unwrap_or(37),
        Ok(c) => c,
    };
    let host = JobHost {
        link: Rc::clone(link),
        ifs: RemoteIfs::new(Rc::clone(link)),
        console,
        ccsid,
    };
    stg::remote::status(link, wm::job::Status::Timw, "");
    let mut session = Session::new(host, cfg);
    ftp::server::serve(&mut net, &mut session);
    let ctl = Rc::clone(&session.host.console.host.ctl);
    let _ = ctl.borrow_mut().send(&FromJob::Ending {
        reason: "QUIT".into(),
    });
    std::process::exit(0)
}
