// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The client side a lock manager calls back: a tiny portmapper (answering
//! `GETPORT` for NLM only) and an NLM version 4 service taking `GRANTED`.
//! Nothing is registered with any binder; the server finds the lock manager
//! through this portmapper at the client's address.

use std::io::{self, Cursor, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use onc_rpc_client::rpc::{
    accept_stat_data, accepted_reply, msg_body, opaque_auth, reply_body, rpc_msg,
};
use onc_xdr::{Pack, Unpack};

use crate::ctx::prog;
use crate::rpc::transport::read_record;
use crate::wire::nlm::{self, Lock};
use crate::wire::xdr::{R, W};

/// The listeners and what they were told.
#[derive(Debug)]
pub struct Callbacks {
    /// The portmapper's address.
    pub pmap: SocketAddr,
    /// The lock manager's address.
    pub nlm: SocketAddr,
    /// Every `GRANTED` received: the lock.
    pub granted: Arc<Mutex<Vec<Lock>>>,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

#[derive(Clone)]
struct Shared {
    nlm_port: u16,
    granted: Arc<Mutex<Vec<Lock>>>,
    stop: Arc<AtomicBool>,
}

impl Callbacks {
    /// Binds the portmapper on `pmap_port` and the lock manager on
    /// `nlm_port` (0: any), TCP and UDP each, at `host`.
    pub fn start(host: &str, pmap_port: u16, nlm_port: u16) -> io::Result<Callbacks> {
        let nlm_udp = UdpSocket::bind((host, nlm_port))?;
        let nlm = nlm_udp.local_addr()?;
        let nlm_tcp = TcpListener::bind(nlm)?;
        let pmap_udp = UdpSocket::bind((host, pmap_port))?;
        let pmap = pmap_udp.local_addr()?;
        let pmap_tcp = TcpListener::bind(pmap)?;
        let sh = Shared {
            nlm_port: nlm.port(),
            granted: Arc::new(Mutex::new(Vec::new())),
            stop: Arc::new(AtomicBool::new(false)),
        };
        let mut threads = Vec::new();
        for u in [nlm_udp, pmap_udp] {
            let sh = sh.clone();
            threads.push(std::thread::spawn(move || udp_loop(&u, &sh)));
        }
        for t in [nlm_tcp, pmap_tcp] {
            let sh = sh.clone();
            threads.push(std::thread::spawn(move || tcp_loop(&t, &sh)));
        }
        Ok(Callbacks {
            pmap,
            nlm,
            granted: sh.granted,
            stop: sh.stop,
            threads,
        })
    }

    /// Waits up to `limit` for a `GRANTED` of `svid`.
    pub fn wait_granted(&self, svid: i32, limit: Duration) -> Option<Lock> {
        let end = std::time::Instant::now() + limit;
        while std::time::Instant::now() < end {
            if let Some(l) = self
                .granted
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .find(|l| l.svid == svid)
            {
                return Some(l.clone());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        None
    }
}

impl Drop for Callbacks {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

fn udp_loop(u: &UdpSocket, sh: &Shared) {
    let _ = u.set_read_timeout(Some(Duration::from_millis(100)));
    let mut buf = vec![0u8; 65_536];
    while !sh.stop.load(Ordering::Relaxed) {
        if let Ok((n, peer)) = u.recv_from(&mut buf)
            && let Some(reply) = answer(&buf[..n], sh)
        {
            let _ = u.send_to(&reply, peer);
        }
    }
}

fn tcp_loop(t: &TcpListener, sh: &Shared) {
    let _ = t.set_nonblocking(true);
    let mut conns = Vec::new();
    while !sh.stop.load(Ordering::Relaxed) {
        match t.accept() {
            Ok((s, _)) => {
                let sh = sh.clone();
                conns.push(std::thread::spawn(move || conn(s, &sh)));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    for c in conns {
        let _ = c.join();
    }
}

fn conn(mut s: TcpStream, sh: &Shared) {
    let _ = s.set_nonblocking(false);
    let _ = s.set_read_timeout(Some(Duration::from_millis(200)));
    while !sh.stop.load(Ordering::Relaxed) {
        match read_record(&mut s) {
            Ok(m) => {
                if let Some(reply) = answer(&m, sh) {
                    let mut rec = (u32::try_from(reply.len()).unwrap_or(0) | 0x8000_0000)
                        .to_be_bytes()
                        .to_vec();
                    rec.extend_from_slice(&reply);
                    if s.write_all(&rec).is_err() {
                        return;
                    }
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return,
        }
    }
}

fn reply(xid: u32, data: accept_stat_data, results: &[u8]) -> Vec<u8> {
    let m = rpc_msg {
        xid,
        body: msg_body::REPLY(reply_body::MSG_ACCEPTED(accepted_reply {
            verf: opaque_auth::default(),
            reply_data: data,
        })),
    };
    let mut out = Vec::new();
    m.pack(&mut out).expect("a Vec takes any write");
    out.extend_from_slice(results);
    out
}

/// The reply to one call.
fn answer(msg: &[u8], sh: &Shared) -> Option<Vec<u8>> {
    let mut c = Cursor::new(msg);
    let (m, _) = rpc_msg::unpack(&mut c).ok()?;
    let msg_body::CALL(call) = m.body else {
        return None;
    };
    let args = &msg[usize::try_from(c.position()).ok()?..];
    let ok = |b: &[u8]| Some(reply(m.xid, accept_stat_data::SUCCESS, b));
    match (call.prog, call.vers, call.proc) {
        (prog::PMAP, 2, 0) | (prog::NLM, 4, 0) => ok(&[]),
        (prog::PMAP, 2, 3) => {
            let mut r = R::new(args);
            let (p, v) = (r.u32().ok()?, r.u32().ok()?);
            let port = if p == prog::NLM && v == 4 {
                u32::from(sh.nlm_port)
            } else {
                0
            };
            ok(&W::new().u32(port).done())
        }
        (prog::NLM, 4, nlm::proc4::GRANTED) => {
            let mut r = R::new(args);
            let cookie = r.opaque(1024).ok()?.to_vec();
            r.bool().ok()?;
            let l = Lock::get(&mut r).ok()?;
            sh.granted
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(l);
            ok(&W::new().opaque(&cookie).u32(nlm::stat::GRANTED).done())
        }
        (prog::PMAP | prog::NLM, _, _) if [2, 4].contains(&call.vers) => {
            Some(reply(m.xid, accept_stat_data::PROC_UNAVAIL, &[]))
        }
        (prog::PMAP, _, _) => Some(reply(
            m.xid,
            accept_stat_data::PROG_MISMATCH { low: 2, high: 2 },
            &[],
        )),
        (prog::NLM, _, _) => Some(reply(
            m.xid,
            accept_stat_data::PROG_MISMATCH { low: 4, high: 4 },
            &[],
        )),
        _ => Some(reply(m.xid, accept_stat_data::PROG_UNAVAIL, &[])),
    }
}
