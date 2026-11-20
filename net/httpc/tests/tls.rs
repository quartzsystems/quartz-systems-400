// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The client against servers on loopback: TLS trusted by a pin or by
//! a root the caller gives, and refused otherwise; downloads under a
//! limit, cut and resumed by range, and from a server that ignores
//! ranges.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use httpc::{Error, Request, Trust};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use x509kit::crypto::keys::{KeyAlgorithm, PrivateKey};
use x509kit::x509::build::{self, CertKind, CertSpec, Subject};

/// A CA, a server certificate it issued for 127.0.0.1, and the
/// server's configuration.
struct Pki {
    ca: Vec<u8>,
    leaf: Vec<u8>,
    server: Arc<ServerConfig>,
}

fn pki() -> Pki {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let ca_key = PrivateKey::generate(KeyAlgorithm::EcdsaP256).unwrap();
    let ca = build::self_signed(
        &CertSpec {
            subject: Subject::cn("Loopback CA"),
            kind: CertKind::Ca,
            san: vec![],
            not_before: now - 60,
            days: 2,
        },
        &ca_key,
    )
    .unwrap();
    let key = PrivateKey::generate(KeyAlgorithm::EcdsaP256).unwrap();
    let mut spec = CertSpec::server("loopback", now - 60, 2);
    spec.san = vec!["127.0.0.1".into()];
    let leaf = build::issue(&spec, &key, &ca, &ca_key).unwrap();
    let server = ServerConfig::builder_with_provider(Arc::new(rustls_graviola::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(leaf.clone()), CertificateDer::from(ca.clone())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.to_pkcs8_der().unwrap().to_vec())),
        )
        .unwrap();
    Pki {
        ca,
        leaf,
        server: Arc::new(server),
    }
}

/// The body the file paths serve.
fn file() -> Vec<u8> {
    (0..10_000u32).map(|i| (i % 251) as u8).collect()
}

/// Answer one request on `s`. `/file` honours a range and `/noranges`
/// does not; the first whole answer of each is cut after 4000 bytes.
fn answer(s: &mut (impl Read + Write), cuts: &AtomicUsize) {
    let mut r = BufReader::new(&mut *s);
    let mut first = String::new();
    if r.read_line(&mut first).is_err() {
        return;
    }
    let mut range = None;
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).is_err() || h.trim().is_empty() {
            break;
        }
        if let Some(v) = h.to_ascii_lowercase().strip_prefix("range: bytes=") {
            range = v.trim().trim_end_matches('-').parse::<usize>().ok();
        }
    }
    drop(r);
    let path = first.split_whitespace().nth(1).unwrap_or("/").to_string();
    let body = file();
    let out: Vec<u8> = match path.as_str() {
        "/hello" => b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello".to_vec(),
        "/big" => {
            let mut v = b"HTTP/1.1 200 OK\r\nContent-Length: 100000\r\n\r\n".to_vec();
            v.extend(vec![b'x'; 100_000]);
            v
        }
        "/chunked" => {
            let mut v = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
            for _ in 0..10 {
                v.extend(b"2710\r\n");
                v.extend(vec![b'y'; 10_000]);
                v.extend(b"\r\n");
            }
            v.extend(b"0\r\n\r\n");
            v
        }
        "/file" if range.is_some_and(|n| n >= body.len()) => {
            format!("HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{}\r\nContent-Length: 0\r\n\r\n", body.len()).into_bytes()
        }
        "/file" if range.is_some() => {
            let n = range.unwrap_or(0);
            let mut v = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {n}-{}/{}\r\nContent-Length: {}\r\n\r\n",
                body.len() - 1,
                body.len(),
                body.len() - n
            )
            .into_bytes();
            v.extend(&body[n..]);
            v
        }
        "/file" | "/noranges" => {
            let mut v = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len())
                .into_bytes();
            let cut = cuts.fetch_add(1, Ordering::SeqCst) == 0;
            v.extend(&body[..if cut { 4000 } else { body.len() }]);
            v
        }
        _ => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(),
    };
    let _ = s.write_all(&out);
    let _ = s.flush();
}

/// Serve every connection to a port, TLS when `tls` is given.
fn serve(tls: Option<Arc<ServerConfig>>) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let cuts = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(s): std::io::Result<TcpStream> = s else {
                continue;
            };
            let tls = tls.clone();
            let cuts = cuts.clone();
            std::thread::spawn(move || {
                if let Some(cfg) = tls {
                    let Ok(c) = ServerConnection::new(cfg) else {
                        return;
                    };
                    let mut t = StreamOwned::new(c, s);
                    answer(&mut t, &cuts);
                    t.conn.send_close_notify();
                    let _ = t.flush();
                } else {
                    let mut s = s;
                    answer(&mut s, &cuts);
                }
            });
        }
    });
    port
}

fn get(url: String, trust: Trust) -> Request {
    Request {
        method: "GET".into(),
        url,
        timeout: 10,
        trust,
        ..Request::default()
    }
}

#[test]
fn what_a_connection_trusts() {
    let p = pki();
    let port = serve(Some(p.server.clone()));
    let url = format!("https://127.0.0.1:{port}/hello");
    // The certificate pinned: accepted, and the pin in text too.
    let pin = Trust::sha256(&p.leaf);
    let r = httpc::send(&get(url.clone(), Trust::Pinned(pin))).unwrap();
    assert_eq!((r.status, r.body.as_slice()), (200, &b"hello"[..]));
    let text: Vec<String> = pin.iter().map(|b| format!("{b:02X}")).collect();
    let r = httpc::send(&get(url.clone(), Trust::pin(&text.join(":")).unwrap())).unwrap();
    assert_eq!(r.status, 200);
    // Another pin, the CA's own: refused.
    let ca_pin = Trust::sha256(&p.ca);
    let e = httpc::send(&get(url.clone(), Trust::Pinned(ca_pin))).unwrap_err();
    assert!(matches!(&e, Error::Tls(s) if s.contains("not the one pinned")), "{e}");
    // The CA given as a root: accepted.
    let r = httpc::send(&get(url.clone(), Trust::Roots(vec![p.ca.clone()]))).unwrap();
    assert_eq!(r.body, b"hello");
    // No root, or another root: refused.
    let e = httpc::send(&get(url.clone(), Trust::default())).unwrap_err();
    assert!(matches!(e, Error::Tls(_)), "{e}");
    let other = pki();
    let e = httpc::send(&get(url.clone(), Trust::Roots(vec![other.ca]))).unwrap_err();
    assert!(matches!(e, Error::Tls(_)), "{e}");
    // A configuration of the caller's own, given or made per connection.
    let mut roots = rustls::RootCertStore::empty();
    roots.add(CertificateDer::from(p.ca.clone())).unwrap();
    let own = Arc::new(
        rustls::ClientConfig::builder_with_provider(Arc::new(rustls_graviola::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    );
    let r = httpc::send(&get(url.clone(), Trust::Config(own.clone()))).unwrap();
    assert_eq!(r.status, 200);
    let made = Trust::Make(Arc::new(move || Ok(own.clone())));
    assert_eq!(httpc::send(&get(url.clone(), made)).unwrap().status, 200);
    let refused = Trust::Make(Arc::new(|| Err("no configuration".into())));
    let e = httpc::send(&get(url, refused)).unwrap_err();
    assert_eq!(e.to_string(), "TLS: no configuration");
}

#[test]
fn downloads_under_a_limit() {
    let p = pki();
    let port = serve(Some(p.server.clone()));
    let pin = Trust::Pinned(Trust::sha256(&p.leaf));
    let req = |path: &str| get(format!("https://127.0.0.1:{port}{path}"), pin.clone());
    // A sized body past the limit: refused before a byte is written.
    let mut out = Vec::new();
    let e = httpc::download(&req("/big"), 0, 50_000, &mut out).unwrap_err();
    assert!(matches!(e, Error::TooLarge(50_000)), "{e}");
    assert!(out.is_empty());
    // A chunked one: refused once it passes, no more than the limit kept.
    let e = httpc::download(&req("/chunked"), 0, 50_000, &mut out).unwrap_err();
    assert!(matches!(e, Error::TooLarge(50_000)), "{e}");
    assert!(out.len() <= 50_000);
    // Within the limit, whole.
    out.clear();
    let f = httpc::download(&req("/chunked"), 0, 100_000, &mut out).unwrap();
    assert_eq!((f.status, f.written, out.len()), (200, 100_000, 100_000));
    assert!(matches!(
        httpc::download(&req("/missing"), 0, 10, &mut out),
        Err(Error::Status(404))
    ));
}

#[test]
fn a_cut_download_resumes_by_range() {
    let port = serve(None);
    let dir = std::env::temp_dir().join(format!("httpc-resume-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("file");
    let _ = std::fs::remove_file(&path);
    let req = get(format!("http://127.0.0.1:{port}/file"), Trust::default());
    // Cut after 4000 bytes: an error, what came is kept.
    let e = httpc::download_file(&req, &path, 1 << 20).unwrap_err();
    assert!(matches!(&e, Error::Io(io) if io.kind() == std::io::ErrorKind::UnexpectedEof), "{e}");
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 4000);
    // Again: the rest by range.
    let f = httpc::download_file(&req, &path, 1 << 20).unwrap();
    assert_eq!((f.status, f.start, f.written, f.total), (206, 4000, 6000, Some(10_000)));
    assert_eq!(std::fs::read(&path).unwrap(), file());
    // Once more: nothing is left (416 for a range at the end).
    let f = httpc::download_file(&req, &path, 1 << 20).unwrap();
    assert_eq!((f.status, f.written), (416, 0));
    // The limit counts what the file had.
    let e = httpc::download(&req, 4000, 5000, &mut Vec::new()).unwrap_err();
    assert!(matches!(e, Error::TooLarge(5000)), "{e}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_server_that_ignores_ranges() {
    let port = serve(None);
    let dir = std::env::temp_dir().join(format!("httpc-norange-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("file");
    let _ = std::fs::remove_file(&path);
    let req = get(format!("http://127.0.0.1:{port}/noranges"), Trust::default());
    assert!(httpc::download_file(&req, &path, 1 << 20).is_err());
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 4000);
    // To a writer: refused, nothing written.
    let mut out = Vec::new();
    let e = httpc::download(&req, 4000, 1 << 20, &mut out).unwrap_err();
    assert!(matches!(e, Error::RangeIgnored), "{e}");
    assert!(out.is_empty());
    // To a file: started again from nothing, whole.
    let f = httpc::download_file(&req, &path, 1 << 20).unwrap();
    assert_eq!((f.status, f.start, f.written), (200, 0, 10_000));
    assert_eq!(std::fs::read(&path).unwrap(), file());
    std::fs::remove_dir_all(&dir).unwrap();
}
