// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A handler that panics answers that request with the error page and
//! the next request still runs ([`qsweb::http::guard`]). No libtest
//! harness: the failed requests' threads stay parked in the hook.

use std::sync::Arc;

use qsweb::http::{Handler, Peer, Request, Response, guard};

struct Shaky;

impl Handler for Shaky {
    fn handle(&self, req: &Request, _: &Peer) -> Response {
        assert!(req.path != "/boom", "boom");
        let none: Vec<u8> = Vec::new();
        if req.path == "/index" {
            let _ = none[req.path.len()];
        }
        Response::html(200, req.path.clone())
    }
}

fn get(path: &str) -> Request {
    Request {
        method: "GET".into(),
        path: path.into(),
        version: "HTTP/1.1".into(),
        ..Request::default()
    }
}

/// A failed request over a connection: the error page, then closed.
fn over_a_connection(h: &Arc<dyn Handler>, peer: &Peer) {
    struct Conn {
        input: std::io::Cursor<Vec<u8>>,
        out: Vec<u8>,
    }
    impl std::io::Read for Conn {
        fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
            self.input.read(b)
        }
    }
    impl std::io::Write for Conn {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.out.write(b)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let two =
        b"GET /boom HTTP/1.1\r\nHost: x\r\n\r\nGET /fine HTTP/1.1\r\nHost: x\r\n\r\n".to_vec();
    let mut c = Conn {
        input: std::io::Cursor::new(two),
        out: Vec::new(),
    };
    qsweb::http::serve_guarded(&mut c, peer, h);
    let out = String::from_utf8_lossy(&c.out);
    assert!(out.starts_with("HTTP/1.1 500"), "{out}");
    assert!(out.contains("could not be shown"), "{out}");
    assert!(
        !out.contains("/fine"),
        "the connection was not closed: {out}"
    );
}

fn main() {
    let h: Arc<dyn Handler> = Arc::new(Shaky);
    let peer = Peer {
        addr: "127.0.0.1:1".parse().expect("address"),
        secure: false,
    };
    let e = guard::handle(&h, get("/boom"), peer).expect_err("a panic");
    assert!(e.contains("boom") && e.contains("guard.rs"), "{e}");
    let e = guard::handle(&h, get("/index"), peer).expect_err("a panic");
    assert!(e.contains("index out of bounds"), "{e}");
    let ok = guard::handle(&h, get("/fine"), peer).expect("an answer");
    assert_eq!(ok.status, 200);
    over_a_connection(&h, &peer);
    println!("guard: panics answered, the next request served");
    std::process::exit(0);
}
