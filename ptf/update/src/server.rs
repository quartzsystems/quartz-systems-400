// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The reference server (`docs/UPDATE_PROTOCOL.md` §7): a directory of
//! signed catalogs, PTF save streams and group manifests, over plain
//! HTTP or TLS.

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use tls::rustls::ServerConfig;
use tls::stream::TlsStream;

use crate::catalog::Catalog;
use crate::http::{self, Request, Response};
use crate::order::{self, OrderRequest};

/// The server.
pub struct Server {
    dir: PathBuf,
    tls: Option<Arc<ServerConfig>>,
    orders: AtomicU32,
    tickets: AtomicU32,
}

/// A server running on a thread.
pub struct Running {
    /// Where it listens.
    pub addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Running {
    /// Stop it and wait.
    pub fn stop(mut self) {
        self.halt();
    }

    fn halt(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the accept.
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_secs(1));
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.halt();
    }
}

/// A PTF or group name a path may carry: letters and digits.
fn plain(name: &str) -> bool {
    !name.is_empty() && name.len() <= 10 && name.bytes().all(|b| b.is_ascii_alphanumeric())
}

impl Server {
    /// A server over `dir`.
    #[must_use]
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            tls: None,
            orders: AtomicU32::new(0),
            tickets: AtomicU32::new(0),
        }
    }

    /// Serve TLS with `config`.
    #[must_use]
    pub fn with_tls(mut self, config: Arc<ServerConfig>) -> Self {
        self.tls = Some(config);
        self
    }

    fn file(&self, rel: &str, kind: &str) -> Response {
        match std::fs::read(self.dir.join(rel)) {
            Ok(b) => Response::new(200, kind, b),
            Err(_) => Response::text(404, &format!("{rel} not found")),
        }
    }

    /// Answer one request. `prefix` is the URL path the server is
    /// mounted under (empty for the root).
    #[must_use]
    pub fn handle(&self, req: &Request, prefix: &str) -> Response {
        let target = req.target.split_once("://").map_or(req.target.as_str(), |(_, rest)| {
            rest.find('/').map_or("/", |i| &rest[i..])
        });
        let Some(path) = target.strip_prefix(prefix) else {
            return Response::text(404, "not under this service");
        };
        let path = path.split('?').next().unwrap_or("");
        match (req.method.as_str(), path) {
            ("GET", "/v1/catalog") => self.file("catalog.toml", "text/plain; charset=utf-8"),
            ("GET", p) if p.starts_with("/v1/ptf/") => {
                let id = &p[8..];
                if !plain(id) {
                    return Response::text(400, "not a PTF identifier");
                }
                self.file(&format!("ptf/{id}.savf"), "application/octet-stream")
            }
            ("GET", p) if p.starts_with("/v1/group/") => {
                let name = &p[10..];
                if !plain(name) {
                    return Response::text(400, "not a group name");
                }
                self.file(&format!("group/{name}.toml"), "text/plain; charset=utf-8")
            }
            ("POST", "/v1/order") => self.order(req),
            ("POST", "/v1/service") => {
                let text = String::from_utf8_lossy(&req.body);
                if !text.starts_with("QS400SERVICE 1") {
                    return Response::text(400, "not a service request");
                }
                let n = self.tickets.fetch_add(1, Ordering::SeqCst) + 1;
                Response::text(200, &format!("ok T{n:06}\n"))
            }
            ("GET" | "POST", _) => Response::text(404, "no such resource"),
            _ => Response::text(405, "method not allowed"),
        }
    }

    fn order(&self, req: &Request) -> Response {
        let Ok(text) = std::fs::read_to_string(self.dir.join("catalog.toml")) else {
            return Response::text(404, "no catalog");
        };
        let cat = match Catalog::parse_unverified(&text) {
            Ok(c) => c,
            Err(e) => return Response::text(500, &e.to_string()),
        };
        let o = match OrderRequest::decode(&String::from_utf8_lossy(&req.body)) {
            Ok(o) => o,
            Err(e) => return Response::text(400, &e.to_string()),
        };
        let mut r = order::resolve(&cat, &o);
        r.order = format!("{:07}", self.orders.fetch_add(1, Ordering::SeqCst) + 1);
        Response::text(200, &r.encode())
    }

    /// Serve one connection.
    pub fn serve_conn(&self, stream: TcpStream, prefix: &str) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
        match &self.tls {
            None => {
                let mut s = stream;
                if let Ok(req) = http::read_request(&mut s) {
                    let _ = http::write_response(&mut s, &self.handle(&req, prefix));
                }
            }
            Some(cfg) => {
                let Ok(mut s) = TlsStream::accept(Arc::clone(cfg), stream) else {
                    return;
                };
                if let Ok(req) = http::read_request(&mut s) {
                    let _ = http::write_response(&mut s, &self.handle(&req, prefix));
                    let _ = s.close();
                }
            }
        }
    }

    /// Serve `listener` on a thread, one connection after another.
    pub fn spawn(self, listener: TcpListener, prefix: &str) -> std::io::Result<Running> {
        let addr = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let prefix = prefix.to_string();
        let thread = std::thread::spawn(move || {
            for conn in listener.incoming() {
                if flag.load(Ordering::SeqCst) {
                    break;
                }
                if let Ok(s) = conn {
                    self.serve_conn(s, &prefix);
                }
            }
        });
        Ok(Running {
            addr,
            stop,
            thread: Some(thread),
        })
    }
}
