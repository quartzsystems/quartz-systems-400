// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The daemon on the machine (`/init qsid`): one thread, polling the
//! service port and the channel from `slic` on its standard input.
//!
//! The port is opened without blocking. A virtio-console port shows
//! `POLLHUP` while no peer has the hypervisor's side open, and takes a
//! write whole or not at all (a frame is never more than one write), so
//! frames wait in a queue while the peer is slow and are dropped when
//! it goes away: on its next connection it gets the hello, the current
//! code and the state again.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ipc::chan::Chan;
use qsipart::service::PowerDown;
use rustix::event::{PollFd, PollFlags, Timespec, poll};

use super::serve::{Host, Service};
use super::{Ask, Notice};
use crate::status::{self, Status};

/// Frames kept for a slow peer before the oldest go.
const QUEUE: usize = 1024;
/// `O_NONBLOCK`, `O_NOCTTY`.
const FLAGS: i32 = 0o4000 | 0o400;

/// The system as the protocol sees it.
struct LinuxHost {
    chan: Chan,
}

impl Host for LinuxHost {
    fn addresses(&mut self) -> Vec<String> {
        let Ok(mut nl) = netlink::route::Rtnl::open() else {
            return Vec::new();
        };
        nl.addresses(None)
            .unwrap_or_default()
            .into_iter()
            .filter(|a| !a.addr.is_loopback())
            .map(|a| format!("{}/{}", a.addr, a.prefix))
            .collect()
    }

    fn now_ms(&mut self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
    }

    fn power_down(&mut self, p: &PowerDown) -> Result<(), String> {
        crate::linux::request_power_down(p.immediate, p.delay, p.restart)
    }

    fn ask_dst(&mut self, id: u32) {
        let _ = self.chan.send(&Ask::Dst { id });
    }

    fn set_clock(&mut self, unix_ns: i64) -> Result<(), String> {
        let t = rustix::time::Timespec {
            tv_sec: unix_ns.div_euclid(1_000_000_000),
            tv_nsec: unix_ns.rem_euclid(1_000_000_000),
        };
        rustix::time::clock_settime(rustix::time::ClockId::Realtime, t).map_err(|e| e.to_string())
    }

    fn log(&mut self, line: &str) {
        eprintln!("{line}");
    }
}

/// The port and what waits to be written to it.
struct Port {
    file: File,
    queue: VecDeque<Vec<u8>>,
}

impl Port {
    fn push(&mut self, frames: Vec<Vec<u8>>) {
        for f in frames {
            if self.queue.len() >= QUEUE {
                self.queue.pop_front();
            }
            self.queue.push_back(f);
        }
    }

    /// Write what the port takes now; false when the peer is gone.
    fn flush(&mut self) -> bool {
        while let Some(f) = self.queue.front() {
            match self.file.write(f) {
                Ok(n) if n == f.len() => {
                    self.queue.pop_front();
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => return true,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                // The kernel takes a frame whole; anything else is a
                // peer gone mid-write.
                Ok(_) | Err(_) => return false,
            }
        }
        true
    }
}

/// Whether the peer has the port's other side open.
fn peer_present(f: &File) -> bool {
    let mut fds = [PollFd::new(f, PollFlags::IN)];
    let zero = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    match poll(&mut fds, Some(&zero)) {
        Ok(_) => !fds[0].revents().contains(PollFlags::HUP),
        Err(_) => false,
    }
}

/// Run as the daemon. Never returns; exits when `slic` closes the
/// channel.
pub fn run() -> ! {
    let Ok(chan) = Chan::from_stdin() else {
        eprintln!("qsid: no channel from slic");
        std::process::exit(1);
    };
    let (Ok(asks), Ok(notices)) = (chan.try_clone(), chan.try_clone()) else {
        eprintln!("qsid: channel not usable");
        std::process::exit(1);
    };
    let file = match open_port() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("qsid: {e}");
            std::process::exit(1);
        }
    };
    let status = Status::parse(&std::fs::read_to_string(status::PATH).unwrap_or_default());
    let svc = Service::new(LinuxHost { chan }, status);
    let port = Port {
        file,
        queue: VecDeque::new(),
    };
    serve(svc, port, notices, asks)
}

/// The service port, opened without blocking.
fn open_port() -> Result<File, String> {
    let node = crate::port::find(Path::new("/sys"), qsipart::SERVICE_PORT)
        .ok_or_else(|| format!("no port {}", qsipart::SERVICE_PORT))?;
    let dev = crate::port::node(Path::new("/dev"), &node);
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(FLAGS)
        .open(&dev)
        .map_err(|e| format!("{}: {e}", dev.display()))?;
    eprintln!(
        "qsid: service channel on {} ({})",
        dev.display(),
        qsipart::SERVICE_PORT
    );
    Ok(f)
}

fn serve(mut svc: Service<LinuxHost>, mut port: Port, mut notices: Chan, mut asks: Chan) -> ! {
    let mut tick = Instant::now();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let present = peer_present(&port.file);
        if present && !svc.connected() {
            eprintln!("qsid: peer connected");
            port.queue.clear();
            port.push(svc.connect());
        } else if !present && svc.connected() {
            eprintln!("qsid: peer gone; waiting for it");
            svc.disconnect();
            port.queue.clear();
        }
        // While no peer is there, the port polls as always readable:
        // only the channel is waited on then.
        let mut port_flags = PollFlags::IN;
        if !port.queue.is_empty() {
            port_flags |= PollFlags::OUT;
        }
        let wait = Timespec {
            tv_sec: 0,
            tv_nsec: if present {
                1_000_000_000 - 1
            } else {
                250_000_000
            },
        };
        let (chan_ready, port_ready) = {
            let mut fds = vec![PollFd::from_borrowed_fd(
                notices.stream().as_fd(),
                PollFlags::IN,
            )];
            if present {
                fds.push(PollFd::new(&port.file, port_flags));
            }
            if poll(&mut fds, Some(&wait)).is_err() {
                continue;
            }
            let chan = !fds[0].revents().is_empty();
            let port = fds.get(1).is_some_and(|f| !f.revents().is_empty());
            (chan, port)
        };
        if chan_ready {
            match notices.recv::<Notice>() {
                Ok(Notice::Flush { token }) => {
                    // slic is about to halt or power off: a little while
                    // for a slow peer to take the rest.
                    let until = Instant::now() + Duration::from_secs(2);
                    while svc.connected() && !port.queue.is_empty() && Instant::now() < until {
                        if !port.flush() {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    let _ = asks.send(&Ask::Flushed { token });
                }
                Ok(n) => {
                    let out = svc.notice(n);
                    port.push(out);
                }
                Err(ipc::WireError::Closed) => std::process::exit(0),
                Err(e) => eprintln!("qsid: from slic: {e}"),
            }
        }
        if present && port_ready {
            loop {
                match port.file.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let out = svc.input(&buf[..n]);
                        port.push(out);
                    }
                    Err(e) if e.kind() == ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
        }
        if tick.elapsed() >= Duration::from_secs(1) {
            tick = Instant::now();
            let out = svc.tick();
            port.push(out);
        }
        if svc.connected() && !port.flush() {
            svc.disconnect();
            port.queue.clear();
        }
    }
}
