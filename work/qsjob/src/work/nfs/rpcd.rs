// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QNFSRPCD`: the binder (portmap 2, rpcbind 3 and 4) on TCP and UDP
//! 111. Every change of its table is written to `/etc/RPCBTAB` (CCSID
//! 37, `-rw-rw-rw-`, owned by the job's user, as the oracle's is); with
//! `RTVRPCREG(*YES)` the file is read back at the start. A registration
//! read back whose daemon is gone is a stale row, as on IBM's: a client
//! that uses it finds nobody on the port, and the daemon's next start
//! replaces it.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError, mpsc};

use ifs::client::IfsClient;
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_CCSID, O_CREAT, O_RDONLY, O_TRUNC, O_WRONLY};
use nfs::rpc::{Answer, CallContext, Dispatcher, LocalProgram};
use nfs::rpcb::binder::PROGRAM;
use nfs::rpcb::{Binder, rpcbtab};
use nfs::server::run::{self, Common};
use nfs::types::rpcb::rpcb;
use nfs::xdr::Decoder;

use super::host::Host;
use super::running::{self, Fail, Prog, Running};
use super::{DaemonEnv, Serve, Started};

/// The binder's table on disk.
pub const RPCBTAB: &str = "/etc/RPCBTAB";

/// The binder's versions.
const VERSIONS: &[u32] = &[2, 3, 4];

/// No program of a thread's own: the binder is the dispatcher's.
struct NoLocal;

impl LocalProgram for NoLocal {
    fn program(&self) -> u32 {
        u32::MAX
    }
    fn versions(&self) -> &[u32] {
        &[]
    }
    fn call(&self, _: &CallContext, _: &mut Decoder<'_>) -> Answer {
        Ok(None)
    }
}

/// The binder's own registrations: its versions on port `port`, owner
/// `0`, as the oracle's table has them.
fn own(l: &nfs::server::run::Listeners) -> Vec<rpcb> {
    let mut v = running::entries(
        l,
        Prog {
            number: PROGRAM,
            versions: VERSIONS,
        },
    );
    for r in &mut v {
        r.r_owner = b"0".to_vec();
    }
    v
}

/// The table read back (`RTVRPCREG(*YES)`): what the file holds, the
/// binder's own rows left out (they are made again).
///
/// # Errors
///
/// The file system's, or a file not in the table's form.
pub fn read_back(fs: &dyn IfsClient) -> Result<Vec<rpcb>, String> {
    let fd = match fs.open(RPCBTAB, OpenArgs::new(O_RDONLY)) {
        Ok(fd) => fd,
        Err(ifs::Errno::ENOENT) => return Ok(Vec::new()),
        Err(e) => return Err(format!("{RPCBTAB}: {}", e.text())),
    };
    let data = ifs::client::read_all(fs, fd);
    let _ = fs.close(fd);
    let data = data.map_err(|e| format!("{RPCBTAB}: {}", e.text()))?;
    let t = rpcbtab::decode(&data).map_err(|e| format!("{RPCBTAB}: {e}"))?;
    Ok(t.into_iter().filter(|r| r.r_prog != PROGRAM).collect())
}

/// Write the table.
///
/// # Errors
///
/// The file system's.
pub fn write(fs: &dyn IfsClient, table: &[rpcb]) -> Result<(), String> {
    let bytes = rpcbtab::encode(table).map_err(|e| e.to_string())?;
    let fd = fs
        .open(
            RPCBTAB,
            OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC | O_CCSID)
                .mode(0o666)
                .conv(37),
        )
        .map_err(|e| format!("{RPCBTAB}: {}", e.text()))?;
    let r = ifs::client::write_all(fs, fd, &bytes);
    let _ = fs.close(fd);
    r.map_err(|e| format!("{RPCBTAB}: {}", e.text()))?;
    // The umask does not narrow it: every user may read and write it.
    let _ = fs.chmod(RPCBTAB, 0o666);
    Ok(())
}

/// Serve: bind 111, start the binder, write its table at each change.
pub fn serve<H: Host>(_env: &mut dyn DaemonEnv, started: Started, host: &Arc<H>) -> Serve {
    let net = host.net();
    let listeners = match running::bind(net, net.rpc_port) {
        Ok(l) => l,
        Err(f) => return Serve::Unavailable(f),
    };
    let own = own(&listeners);
    let fs = match host.fs() {
        Ok(f) => f,
        Err(e) => return Serve::Unavailable(Fail::new(e)),
    };
    let before = if started.restore {
        read_back(&fs).unwrap_or_else(|e| {
            eprintln!("qsjob: QNFSRPCD: {e}: not read back");
            Vec::new()
        })
    } else {
        Vec::new()
    };
    drop(fs);
    // Every change goes to a writer thread with a link of its own; only
    // the latest table is written.
    let (tx, rx) = mpsc::channel::<Vec<rpcb>>();
    let tx = Mutex::new(tx);
    let binder = Binder::default().on_change(move |t| {
        let _ = tx
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .send(t.to_vec());
    });
    for r in own.iter().cloned().chain(before) {
        binder.register(r);
    }
    let h = Arc::clone(host);
    let writer = std::thread::Builder::new()
        .name("nfs-rpcbtab".into())
        .spawn(move || {
            let fs = match h.fs() {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("qsjob: QNFSRPCD: {RPCBTAB} not written: {e}");
                    return;
                }
            };
            while let Ok(mut t) = rx.recv() {
                while let Ok(newer) = rx.try_recv() {
                    t = newer;
                }
                if let Err(e) = write(&fs, &t) {
                    eprintln!("qsjob: QNFSRPCD: {e}");
                }
            }
        });
    let writer = match writer {
        Ok(w) => w,
        Err(e) => return Serve::Unavailable(Fail::io("a thread", &e)),
    };
    let mut d = Dispatcher::new();
    d.add(Arc::new(binder));
    let ports = running::ports(&own);
    let r = Running::spawn("nfs-rpcd", move |stop: &AtomicBool, ready| {
        let c = Common {
            listeners,
            binder: None,
            stop,
            ready: Some(ready),
        };
        let r = run::serve(c, &d, PROGRAM, VERSIONS, || NoLocal, None)
            .map_err(|e| Fail::io("the binder", &e));
        // The dispatcher (and with it the writer's channel) goes here:
        // the writer ends after the last table.
        drop(d);
        r
    });
    match r {
        // The last table is on disk before the job ends.
        Ok(r) => Serve::Ready(r.with_ports(ports).on_end(move || {
            let _ = writer.join();
        })),
        Err(f) => Serve::Unavailable(f),
    }
}
