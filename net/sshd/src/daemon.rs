// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The listening daemon: binds every `ListenAddress`/`Port` pair and
//! serves each connection on a thread of its own until stopped. This is
//! what `STRTCPSVR SERVER(*SSHD)` starts and `ENDTCPSVR` stops.

use std::io;
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::server::Server;

/// A bound server.
pub struct Daemon {
    server: Arc<Server>,
    listeners: Vec<TcpListener>,
    stop: Arc<AtomicBool>,
}

impl Daemon {
    /// Bind the configured addresses. An address that cannot be bound
    /// is logged and skipped (IPv6 may be off); none bound is an error.
    pub fn bind(server: Arc<Server>) -> io::Result<Self> {
        let mut listeners = Vec::new();
        let mut last = None;
        for (host, port) in server.config.binds() {
            match TcpListener::bind((host.as_str(), port)) {
                Ok(l) => {
                    l.set_nonblocking(true)?;
                    tracing::info!(host, port, "sshd listening");
                    listeners.push(l);
                }
                Err(e) => {
                    tracing::warn!(host, port, error = %e, "sshd could not listen");
                    last = Some(e);
                }
            }
        }
        if listeners.is_empty() {
            return Err(last.unwrap_or_else(|| io::Error::from(io::ErrorKind::AddrNotAvailable)));
        }
        Ok(Self {
            server,
            listeners,
            stop: Arc::new(AtomicBool::new(false)),
        })
    }

    /// The addresses bound.
    #[must_use]
    pub fn local_addrs(&self) -> Vec<SocketAddr> {
        self.listeners
            .iter()
            .filter_map(|l| l.local_addr().ok())
            .collect()
    }

    /// Set to stop [`Daemon::run`]; connections already open go on.
    #[must_use]
    pub fn stopper(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    /// Accept connections until stopped.
    pub fn run(self) {
        while !self.stop.load(Ordering::SeqCst) {
            let mut any = false;
            for l in &self.listeners {
                match l.accept() {
                    Ok((s, addr)) => {
                        any = true;
                        let _ = s.set_nonblocking(false);
                        let server = self.server.clone();
                        std::thread::spawn(move || {
                            if let Err(e) = server.serve_tcp(&s) {
                                tracing::info!(peer = %addr, error = %e, "ssh connection ended");
                            }
                        });
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => tracing::warn!(error = %e, "accept failed"),
                }
            }
            if !any {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}
