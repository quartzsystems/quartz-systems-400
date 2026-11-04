// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The agent on the machine (`/init qsga`, started by `slic` once the
//! IPL completes): the guest's facts from `slic`'s state file, netlink,
//! the IFS mount and `uname`; a power down asked of `slic` as a job's
//! `PWRDWNSYS` asks it; one thread on the virtio-serial port and one on
//! vsock where the hypervisor offers it.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use netlink::route::Rtnl;

use crate::agent::Agent;
use crate::facts::{Address, Filesystem, Guest, Interface, Kernel, Stop};
use crate::status::{self, Status};

/// Where `slic` reads a power down's options before `SIGUSR1`
/// (`slic`'s `ipl::power::REQUEST_FILE`): `immediate delay restart
/// iplsrc`.
pub const POWER_REQUEST: &str = "/run/qs400/pwrdwn";
/// The seconds a controlled end waits for jobs, as `PWRDWNSYS` the
/// system starts on its own behalf.
pub const DELAY: u32 = 600;
/// The IFS mount.
const IFS: &str = "/run/qs400/ifs";

/// The machine's facts.
pub struct LinuxGuest {
    sys: PathBuf,
}

impl LinuxGuest {
    /// Over the sysfs at `sys`.
    #[must_use]
    pub fn new(sys: &Path) -> Self {
        Self { sys: sys.to_path_buf() }
    }
}

fn mac_text(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(":")
}

impl Guest for LinuxGuest {
    fn status(&mut self) -> Status {
        Status::parse(&std::fs::read_to_string(status::PATH).unwrap_or_default())
    }

    fn kernel(&mut self) -> Kernel {
        let u = rustix::system::uname();
        Kernel {
            release: u.release().to_string_lossy().into_owned(),
            version: u.version().to_string_lossy().into_owned(),
            machine: u.machine().to_string_lossy().into_owned(),
        }
    }

    fn interfaces(&mut self) -> Vec<Interface> {
        let Ok(mut nl) = Rtnl::open() else { return Vec::new() };
        let links = nl.links().unwrap_or_default();
        let addrs = nl.addresses(None).unwrap_or_default();
        links
            .into_iter()
            .map(|l| {
                let s = l.stats.unwrap_or_default();
                Interface {
                    addresses: addrs
                        .iter()
                        .filter(|a| a.index == l.index)
                        .map(|a| Address {
                            family: if a.addr.is_ipv4() { "ipv4" } else { "ipv6" },
                            address: a.addr.to_string(),
                            prefix: a.prefix,
                        })
                        .collect(),
                    mac: if l.mac.iter().all(|b| *b == 0) && !l.is_loopback() { String::new() } else { mac_text(&l.mac) },
                    name: l.name,
                    stats: [
                        s.rx_bytes,
                        s.rx_packets,
                        s.rx_errors,
                        s.rx_dropped,
                        s.tx_bytes,
                        s.tx_packets,
                        s.tx_errors,
                        s.tx_dropped,
                    ],
                }
            })
            .collect()
    }

    fn filesystems(&mut self) -> Vec<Filesystem> {
        let mut v = Vec::new();
        if let Ok(s) = rustix::fs::statvfs(IFS) {
            let total = s.f_blocks * s.f_frsize;
            let free = s.f_bfree * s.f_frsize;
            v.push(Filesystem {
                name: "IFS".into(),
                mountpoint: "/".into(),
                kind: "xfs".into(),
                used: total.saturating_sub(free),
                total,
            });
        }
        v
    }

    fn time_ns(&mut self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
    }

    fn cpus(&mut self) -> usize {
        std::fs::read_dir(self.sys.join("devices/system/cpu")).map_or(1, |rd| {
            rd.filter_map(Result::ok)
                .filter(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    n.strip_prefix("cpu").is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
                })
                .count()
                .max(1)
        })
    }

    fn stop(&mut self, how: Stop) -> Result<(), String> {
        let s = self.status();
        if s.state != "complete" {
            return Err(format!("The system cannot power down now (state {})", if s.state.is_empty() { "unknown" } else { &s.state }));
        }
        let restart = u8::from(how == Stop::Restart);
        std::fs::write(POWER_REQUEST, format!("0 {DELAY} {restart} 0\n")).map_err(|e| e.to_string())?;
        rustix::process::kill_process(rustix::process::Pid::INIT, rustix::process::Signal::USR1)
            .map_err(|e| e.to_string())
    }
}

/// The agent both channels share.
type Shared = Arc<Mutex<Agent<LinuxGuest>>>;

fn answer(agent: &Shared, input: &[u8]) -> Vec<u8> {
    agent.lock().unwrap_or_else(PoisonError::into_inner).feed(input)
}

/// Serve the virtio-serial port `/dev/<node>`. A read of nothing means
/// the host side is not connected: wait and read again.
fn serve_port(agent: &Shared, dev: &Path) -> ! {
    let mut buf = [0u8; 4096];
    loop {
        let Ok(mut f) = OpenOptions::new().read(true).write(true).open(dev) else {
            thread::sleep(Duration::from_secs(2));
            continue;
        };
        loop {
            match f.read(&mut buf) {
                Ok(0) => thread::sleep(Duration::from_millis(250)),
                Ok(n) => {
                    let out = answer(agent, &buf[..n]);
                    if !out.is_empty() && f.write_all(&out).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
}

/// Serve vsock connections, one at a time, each with a fresh framer.
fn serve_vsock(agent: &Shared, port: u32) {
    let Ok(l) = nettools::sys::vsock::listen(port) else {
        eprintln!("qsga: vsock port {port} not available");
        return;
    };
    loop {
        let Ok(c) = nettools::sys::vsock::accept(&l) else {
            thread::sleep(Duration::from_secs(1));
            continue;
        };
        let mut f = File::from(c);
        let mut own = Agent::new(LinuxGuest::new(Path::new("/sys")));
        let mut buf = [0u8; 4096];
        while let Ok(n) = f.read(&mut buf) {
            if n == 0 {
                break;
            }
            // Facts are read fresh; the shared agent keeps the stop.
            let out = own.feed(&buf[..n]);
            if let Some(how) = own.stopping.take() {
                agent.lock().unwrap_or_else(PoisonError::into_inner).stopping = Some(how);
            }
            if !out.is_empty() && f.write_all(&out).is_err() {
                break;
            }
        }
    }
}

/// Run as the agent. Never returns.
pub fn run() -> ! {
    let sys = Path::new("/sys");
    let agent: Shared = Arc::new(Mutex::new(Agent::new(LinuxGuest::new(sys))));
    if crate::port::vsock(sys) {
        let a = Arc::clone(&agent);
        thread::spawn(move || serve_vsock(&a, crate::VSOCK_PORT));
    }
    match crate::port::find(sys, crate::PORT_NAME) {
        Some(node) => {
            eprintln!("qsga: guest agent on /dev/{node}");
            serve_port(&agent, &crate::port::node(Path::new("/dev"), &node))
        }
        None => loop {
            thread::sleep(Duration::from_secs(3600));
        },
    }
}
