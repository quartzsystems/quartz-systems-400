// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Mutual TLS 1.3 with self-signed Ed25519 certificates over graviola,
//! in memory: pinned both ways completes; a certificate not pinned is
//! refused at either end; pairing open lets an unpinned peer in, marked
//! as such.

use std::sync::{Arc, Mutex};

use qsitrust::{Fingerprint, Identity, Pins, tls};
use rustls::{ClientConnection, Connection, ServerConnection};

#[derive(Debug, Default)]
struct Set {
    pins: Mutex<Vec<Fingerprint>>,
    open: bool,
}

impl Pins for Set {
    fn pinned(&self, peer: &Fingerprint) -> bool {
        self.pins.lock().unwrap().contains(peer)
    }
    fn pairing_open(&self) -> bool {
        self.open
    }
}

fn set(pins: &[Fingerprint], open: bool) -> Arc<Set> {
    Arc::new(Set {
        pins: Mutex::new(pins.to_vec()),
        open,
    })
}

/// Pump bytes between the two until neither has anything to say.
fn handshake(client: &mut Connection, server: &mut Connection) -> Result<(), rustls::Error> {
    for _ in 0..20 {
        let a = pass(client, server)?;
        let b = pass(server, client)?;
        if !a && !b {
            break;
        }
    }
    Ok(())
}

/// What `from` has to send, given to `to`; whether there was any.
fn pass(from: &mut Connection, to: &mut Connection) -> Result<bool, rustls::Error> {
    let mut buf = Vec::new();
    while from.wants_write() {
        from.write_tls(&mut buf).unwrap();
    }
    if buf.is_empty() {
        return Ok(false);
    }
    let mut cur = std::io::Cursor::new(buf);
    while usize::try_from(cur.position()).unwrap() < cur.get_ref().len() {
        to.read_tls(&mut cur).unwrap();
        to.process_new_packets()?;
    }
    Ok(true)
}

fn ids() -> (Identity, Identity) {
    // 2026-11-12 13:05:00 UTC, as QS/VS's identity test.
    (
        Identity::from_seed(
            &[7; 32],
            "vs-01",
            "M525400123456",
            1_794_488_700,
            [0xff; 16],
        ),
        Identity::from_seed(&[3; 32], "QSMC01", "", 1_794_488_700, [0x11; 16]),
    )
}

fn connect(
    host: &Identity,
    host_pins: Arc<Set>,
    console: &Identity,
    console_pins: Arc<Set>,
) -> (Connection, Connection, Result<(), rustls::Error>) {
    let sc = tls::server_config(host, host_pins).unwrap();
    let cc = tls::client_config(console, console_pins).unwrap();
    let mut server = Connection::Server(ServerConnection::new(sc).unwrap());
    let mut client = Connection::Client(ClientConnection::new(cc, tls::server_name()).unwrap());
    let r = handshake(&mut client, &mut server);
    (client, server, r)
}

#[test]
fn ed25519_pinned_both_ways_completes_tls13() {
    let (host, console) = ids();
    let (client, server, r) = connect(
        &host,
        set(&[console.fingerprint()], false),
        &console,
        set(&[host.fingerprint()], false),
    );
    r.unwrap();
    assert!(!client.is_handshaking() && !server.is_handshaking());
    assert_eq!(
        server.protocol_version(),
        Some(rustls::ProtocolVersion::TLSv1_3)
    );
    assert_eq!(client.alpn_protocol(), Some(tls::ALPN));
    let hp = set(&[console.fingerprint()], false);
    let seen = tls::peer(&server, hp.as_ref()).unwrap();
    assert_eq!(seen.fingerprint, console.fingerprint());
    assert!(seen.pinned);
    let cp = set(&[host.fingerprint()], false);
    assert_eq!(
        tls::peer(&client, cp.as_ref()).unwrap().fingerprint,
        host.fingerprint()
    );
    // Both ends export the same binding, mirrored.
    let a = tls::binding(&client, &console).unwrap();
    let b = tls::binding(&server, &host).unwrap();
    assert_eq!((a.mine, a.peer, a.exporter), (b.peer, b.mine, b.exporter));
}

#[test]
fn a_certificate_not_pinned_is_refused_at_either_end() {
    let (host, console) = ids();
    let stranger = Identity::from_seed(&[9; 32], "other", "", 0, [2; 16]);
    // The host does not know the console.
    let (_, _, r) = connect(
        &host,
        set(&[stranger.fingerprint()], false),
        &console,
        set(&[host.fingerprint()], false),
    );
    assert!(r.is_err(), "{r:?}");
    // The console does not know the host.
    let (_, _, r) = connect(
        &host,
        set(&[console.fingerprint()], false),
        &console,
        set(&[stranger.fingerprint()], false),
    );
    assert!(r.is_err(), "{r:?}");
}

#[test]
fn pairing_open_lets_an_unpinned_peer_in_marked() {
    let (host, console) = ids();
    let (client, server, r) = connect(&host, set(&[], true), &console, set(&[], true));
    r.unwrap();
    let seen = tls::peer(&server, set(&[], true).as_ref()).unwrap();
    assert!(!seen.pinned);
    assert!(!tls::peer(&client, set(&[], true).as_ref()).unwrap().pinned);
}

#[test]
fn a_key_from_qsvs_identity_files_loads() {
    // QS/VS writes PEM: PKCS #8 of the seed alone, and the certificate.
    let (host, _) = ids();
    let cert = qsitrust::to_pem("CERTIFICATE", &host.cert);
    let key = qsitrust::to_pem("PRIVATE KEY", &host.key);
    let back = Identity::from_pem(&cert, &key).unwrap();
    assert_eq!(back.fingerprint(), host.fingerprint());
    tls::server_config(&back, set(&[], false)).unwrap();
}
