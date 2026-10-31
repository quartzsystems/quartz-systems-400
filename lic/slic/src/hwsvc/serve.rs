// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The socket: one thread per connection, each request answered from
//! the shared table.

use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

use hwrsrc::proto::{Service, dispatch};
use hwrsrc::{Resource, Table};
use ipc::WireError;
use ipc::chan::Chan;

/// Listen on the service's socket, a stale one removed first.
pub fn bind() -> std::io::Result<UnixListener> {
    let path = Path::new(hwrsrc::client::SOCKET);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _ = std::fs::remove_file(path);
    UnixListener::bind(path)
}

/// Serve connections for ever.
pub fn accept(listener: &UnixListener, table: &Arc<Mutex<Table>>) -> ! {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                let table = Arc::clone(table);
                thread::spawn(move || serve(Chan::new(stream), &table));
            }
            Err(e) => {
                eprintln!("qshwr: accept: {e}");
                thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
}

/// Answer one client until it hangs up.
fn serve(mut chan: Chan, table: &Arc<Mutex<Table>>) {
    let mut svc = Svc(table);
    loop {
        let req = match chan.recv() {
            Ok(r) => r,
            Err(WireError::Closed) => return,
            Err(e) => {
                eprintln!("qshwr: request: {e}");
                return;
            }
        };
        if chan.send_raw(&dispatch(&mut svc, req)).is_err() {
            return;
        }
    }
}

struct Svc<'a>(&'a Arc<Mutex<Table>>);

impl Svc<'_> {
    fn with<T>(&self, f: impl FnOnce(&mut Table) -> T) -> T {
        let mut t = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut t)
    }
}

impl Service for Svc<'_> {
    fn table(&mut self, refresh: bool) -> Table {
        self.with(|t| {
            if refresh {
                t.refresh_ports(Path::new(super::SYS));
            }
            t.clone()
        })
    }

    fn resource(&mut self, name: String) -> Option<Resource> {
        self.with(|t| t.get(&name).cloned())
    }

    fn rescan(&mut self) -> Vec<String> {
        self.with(|t| t.rescan(Path::new(super::SYS)))
    }
}
