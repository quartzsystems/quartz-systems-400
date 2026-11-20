// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Downloads as a management console makes them: an update service of
//! QSDEMO's own on the loopback (TLS, a certificate made here) serves a
//! catalog QSDEMO signed and an image; `httpc` pinned to the
//! certificate fetches them (another pin refused), `updcat` verifies
//! the catalog against QSDEMO's key (another key refused) and names the
//! latest cumulative level, and the image comes under a size limit, cut
//! once and resumed by range.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use updcat::{Catalog, CatalogGroup, GroupKind, SigningKey};

/// QSDEMO's catalog key (a fixed seed: the fixture's, not a secret).
const SEED: [u8; 32] = [0x51; 32];

/// The image's bytes.
fn image() -> Vec<u8> {
    (0..10_000u32).map(|i| (i % 253) as u8).collect()
}

fn catalog() -> Catalog {
    let group = |name: &str, level| CatalogGroup {
        name: name.into(),
        kind: GroupKind::Cumulative,
        level,
        release: "V1R0M0".into(),
        text: "QSDEMO cumulative".into(),
        ptfs: Vec::new(),
        related: Vec::new(),
    };
    Catalog {
        format: updcat::FORMAT,
        sequence: 3,
        generated: 1_775_779_200,
        products: Vec::new(),
        ptfs: Vec::new(),
        groups: vec![group("C25350110", 1), group("C26100110", 2)],
    }
}

/// Answer one request: `/catalog`, or `/image` (its second whole answer
/// cut after 4000 bytes, a range honoured).
fn answer(s: &mut (impl Read + Write), signed: &str, whole: &AtomicUsize) {
    let mut r = BufReader::new(&mut *s);
    let mut first = String::new();
    if r.read_line(&mut first).is_err() {
        return;
    }
    let mut from = None;
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).is_err() || h.trim().is_empty() {
            break;
        }
        if let Some(v) = h.to_ascii_lowercase().strip_prefix("range: bytes=") {
            from = v.trim().trim_end_matches('-').parse::<usize>().ok();
        }
    }
    drop(r);
    let image = image();
    let out = match (first.split_whitespace().nth(1), from) {
        (Some("/catalog"), _) => {
            let mut v = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", signed.len())
                .into_bytes();
            v.extend(signed.as_bytes());
            v
        }
        (Some("/image"), Some(n)) if n < image.len() => {
            let mut v = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {n}-{}/{}\r\nContent-Length: {}\r\n\r\n",
                image.len() - 1,
                image.len(),
                image.len() - n
            )
            .into_bytes();
            v.extend(&image[n..]);
            v
        }
        (Some("/image"), None) => {
            let mut v = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", image.len())
                .into_bytes();
            // The second whole answer (after the one over the limit) is cut.
            let cut = whole.fetch_add(1, Ordering::SeqCst) == 1;
            v.extend(&image[..if cut { 4000 } else { image.len() }]);
            v
        }
        _ => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(),
    };
    let _ = s.write_all(&out);
    let _ = s.flush();
}

fn request(url: String, trust: httpc::Trust) -> httpc::Request {
    httpc::Request {
        method: "GET".into(),
        url,
        timeout: 20,
        trust,
        agent: "QSDEMO".into(),
        ..httpc::Request::default()
    }
}

/// Serve, fetch and check. The console's lines.
pub fn prove() -> Result<Vec<String>, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let id = x509kit::server::self_signed("qsdemo-updates", &["127.0.0.1"], now - 60, 30)
        .map_err(|e| format!("certificate: {e}"))?;
    let pkcs8 = id.key.to_pkcs8_der().map_err(|e| e.to_string())?.to_vec();
    let server = ServerConfig::builder_with_provider(Arc::new(rustls_graviola::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(id.cert.clone())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(pkcs8)),
        )
        .map_err(|e| format!("server TLS: {e}"))?;
    let server = Arc::new(server);
    let key = SigningKey::from_bytes(&SEED);
    let signed = catalog().sign(&key);
    let l = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("listen: {e}"))?;
    let port = l.local_addr().map_err(|e| e.to_string())?.port();
    let whole = Arc::new(AtomicUsize::new(0));
    thread::spawn(move || {
        for s in l.incoming().flatten() {
            let (server, signed, whole) = (server.clone(), signed.clone(), whole.clone());
            thread::spawn(move || {
                let Ok(c) = ServerConnection::new(server) else {
                    return;
                };
                let mut t = StreamOwned::new(c, s);
                answer(&mut t, &signed, &whole);
                t.conn.send_close_notify();
                let _ = t.flush();
            });
        }
    });
    let base = format!("https://127.0.0.1:{port}");
    let pin = httpc::Trust::Pinned(httpc::Trust::sha256(&id.cert));
    let mut said = Vec::new();

    // The catalog, pinned; another pin refused.
    match httpc::send(&request(format!("{base}/catalog"), httpc::Trust::Pinned([0; 32]))) {
        Err(httpc::Error::Tls(_)) => {}
        r => return Err(format!("a wrong pin was not refused: {r:?}")),
    }
    let r = httpc::send(&request(format!("{base}/catalog"), pin.clone()))
        .map_err(|e| format!("the catalog: {e}"))?;
    if r.status != 200 {
        return Err(format!("the catalog: HTTP {}", r.status));
    }
    said.push("pinned HTTPS GET of the catalog; a wrong pin refused".into());
    let text = String::from_utf8(r.body).map_err(|e| e.to_string())?;
    let other = SigningKey::from_bytes(&[0x52; 32]).verifying_key().to_bytes();
    if Catalog::parse(&text, &[other]).is_ok() {
        return Err("a catalog signed by another key was accepted".into());
    }
    let cat = Catalog::parse(&text, &[key.verifying_key().to_bytes()])
        .map_err(|e| format!("the catalog: {e}"))?;
    let latest = cat.latest("V1R0M0").ok_or("no cumulative level")?;
    let date = latest.date.ok_or("the level's name has no date")?;
    said.push(format!(
        "catalog verified, latest cumulative {} level {} of {date}; another key refused",
        latest.name, latest.level
    ));

    // The image: over a limit refused, cut once, resumed by range.
    let url = format!("{base}/image");
    let mut got = Vec::new();
    match httpc::download(&request(url.clone(), pin.clone()), 0, 5000, &mut got) {
        Err(httpc::Error::TooLarge(5000)) if got.is_empty() => {}
        r => return Err(format!("a download over its limit was not refused: {r:?}")),
    }
    if httpc::download(&request(url.clone(), pin.clone()), 0, 1 << 20, &mut got).is_ok() {
        return Err("the cut download did not fail".into());
    }
    let had = got.len();
    let f = httpc::download(&request(url, pin), had as u64, 1 << 20, &mut got)
        .map_err(|e| format!("the resumed download: {e}"))?;
    if f.status != 206 || got != image() {
        return Err(format!(
            "the resumed download: HTTP {}, {} bytes",
            f.status,
            got.len()
        ));
    }
    said.push(format!(
        "download over its limit refused; cut at {had} bytes and resumed by range to {}",
        got.len()
    ));
    Ok(said)
}
