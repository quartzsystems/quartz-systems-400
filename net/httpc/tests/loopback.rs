// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The client against a server on loopback: a body, a redirect, a
//! chunked answer.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

/// Serve `n` connections; each answer from the request line.
#[allow(clippy::many_single_char_names)]
fn serve(n: usize) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in l.incoming().take(n) {
            let mut s = s.unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut first = String::new();
            r.read_line(&mut first).unwrap();
            let mut len = 0usize;
            loop {
                let mut h = String::new();
                r.read_line(&mut h).unwrap();
                if h.trim().is_empty() {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; len];
            r.read_exact(&mut body).unwrap();
            let answer = if first.contains("/old") {
                "HTTP/1.1 302 Found\r\nLocation: /new\r\nContent-Length: 0\r\n\r\n".to_string()
            } else if first.contains("/chunked") {
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nok\r\n0\r\n\r\n"
                    .to_string()
            } else {
                let b = format!("{} {}", first.trim(), String::from_utf8_lossy(&body));
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{b}",
                    b.len()
                )
            };
            s.write_all(answer.as_bytes()).unwrap();
        }
    });
    port
}

#[test]
fn requests_over_loopback() {
    let port = serve(4);
    let req = |method: &str, path: &str, body: &[u8]| httpc::Request {
        method: method.into(),
        url: format!("http://127.0.0.1:{port}{path}"),
        body: body.to_vec(),
        redirects: 5,
        timeout: 5,
        ..httpc::Request::default()
    };
    let r = httpc::send(&req("POST", "/echo", b"hi")).unwrap();
    assert_eq!(r.status, 200);
    assert_eq!(r.body, b"POST /echo HTTP/1.1 hi");
    let r = httpc::send(&req("GET", "/old", b"")).unwrap();
    assert_eq!(r.body, b"GET /new HTTP/1.1 ");
    let r = httpc::send(&req("GET", "/chunked", b"")).unwrap();
    assert_eq!(r.body, b"ok");
}
