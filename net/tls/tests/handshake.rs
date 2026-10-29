// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Handshakes between our client and server engines: every suite under
//! TLS 1.2 and 1.3, client authentication, protocol restrictions from
//! the system values, trust, the blocking stream, application IDs, and
//! `openssl s_client` against our server when the tool is present.

mod common;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use certs::apps::{AppRegistry, ClientAuth};
use certs::files::StoreFiles;
use common::{Fixture, HOST};
use tls::config::{self, AppConfig, ClientAuthMode, ServerAuthMode, client_config, server_name};
use tls::engine::{Engine, pump};
use tls::policy::{Protocol, TlsPolicy};
use tls::stream::TlsStream;
use tls::suites;

fn pair(
    server: std::sync::Arc<rustls::ServerConfig>,
    client: std::sync::Arc<rustls::ClientConfig>,
) -> (Engine, Engine) {
    (
        Engine::server(server).unwrap(),
        Engine::client(client, server_name(HOST).unwrap()).unwrap(),
    )
}

#[test]
fn every_supported_suite_under_its_protocol() {
    let f = Fixture::new();
    for suite in suites::opsys() {
        let protocol = if suite.tls13 {
            Protocol::Tls13
        } else {
            Protocol::Tls12
        };
        let policy = TlsPolicy::opsys()
            .restricted_to(&[protocol])
            .with_suites(&[suite.code]);
        let label = if suite.name.contains("ECDSA") || suite.tls13 {
            "ECDSA"
        } else {
            "RSA"
        };
        let (mut s, mut c) = pair(
            f.server(label, &policy, ClientAuthMode::None),
            f.client(&policy, None, ServerAuthMode::Full),
        );
        pump(&mut s, &mut c).unwrap_or_else(|e| panic!("{}: {e}", suite.name));
        assert!(!s.is_handshaking() && !c.is_handshaking(), "{}", suite.name);
        let ss = s.session().unwrap();
        let cs = c.session().unwrap();
        assert_eq!(ss.suite_code, suite.code, "{}", suite.name);
        assert_eq!(cs.suite.unwrap().name, suite.name);
        assert_eq!(ss.protocol, protocol);
        assert_eq!(ss.server_name.as_deref(), Some(HOST));
        assert_eq!(cs.peer_certs.len(), 1, "the leaf; the root is not sent");
        assert!(ss.peer_certs.is_empty());

        // Data both ways, then a clean close.
        c.write_plain(b"hello from the client").unwrap();
        pump(&mut s, &mut c).unwrap();
        let mut buf = [0u8; 64];
        let n = s.read_plain(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"hello from the client");
        s.write_plain(b"and back").unwrap();
        s.close();
        pump(&mut s, &mut c).unwrap();
        let n = c.read_plain(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"and back");
        assert_eq!(c.read_plain(&mut buf).unwrap(), 0, "clean close");
    }
}

#[test]
fn an_rsa_only_suite_needs_an_rsa_certificate() {
    let f = Fixture::new();
    let policy = TlsPolicy::opsys()
        .restricted_to(&[Protocol::Tls12])
        .with_suites(&[0xC02F]);
    let (mut s, mut c) = pair(
        f.server("ECDSA", &policy, ClientAuthMode::None),
        f.client(&policy, None, ServerAuthMode::Full),
    );
    assert!(pump(&mut s, &mut c).is_err());
}

#[test]
fn client_authentication_modes() {
    let f = Fixture::new();
    let policy = TlsPolicy::opsys();
    for protocol in [Protocol::Tls13, Protocol::Tls12] {
        let policy = policy.restricted_to(&[protocol]);
        // Required: no certificate fails, a certificate succeeds.
        let (mut s, mut c) = pair(
            f.server("RSA", &policy, ClientAuthMode::Required),
            f.client(&policy, None, ServerAuthMode::Full),
        );
        assert!(
            pump(&mut s, &mut c).is_err(),
            "{protocol:?}: required without a cert"
        );
        let (mut s, mut c) = pair(
            f.server("RSA", &policy, ClientAuthMode::Required),
            f.client(&policy, Some("CLIENT"), ServerAuthMode::Full),
        );
        pump(&mut s, &mut c).unwrap();
        let ss = s.session().unwrap();
        assert_eq!(ss.peer_certs.len(), 1);
        let info = certs::x509::CertInfo::from_der(&ss.peer_certs[0]).unwrap();
        assert_eq!(info.common_name.as_deref(), Some("client one"));
        // Optional: both work.
        let (mut s, mut c) = pair(
            f.server("RSA", &policy, ClientAuthMode::Optional),
            f.client(&policy, None, ServerAuthMode::Full),
        );
        pump(&mut s, &mut c).unwrap();
        assert!(s.session().unwrap().peer_certs.is_empty());
        let (mut s, mut c) = pair(
            f.server("RSA", &policy, ClientAuthMode::Optional),
            f.client(&policy, Some("CLIENT"), ServerAuthMode::Full),
        );
        pump(&mut s, &mut c).unwrap();
        assert_eq!(s.session().unwrap().peer_certs.len(), 1);
    }
    // Required with no trusted CA is refused at configuration.
    let id = f.identity("RSA");
    assert!(matches!(
        config::server_config(
            &id,
            &policy,
            ClientAuthMode::Required,
            Fixture::empty_roots()
        ),
        Err(tls::TlsError::NoTrustedCas)
    ));
}

#[test]
fn protocol_restrictions_from_the_system_values() {
    let f = Fixture::new();
    let only12 = TlsPolicy::from_sysvals(&["*TLSV1.2"], &[], "*OPSYS");
    let only13 = TlsPolicy::from_sysvals(&["*TLSV1.3"], &[], "*OPSYS");
    let both = TlsPolicy::from_sysvals(&["*OPSYS"], &[], "*OPSYS");
    let (mut s, mut c) = pair(
        f.server("RSA", &only12, ClientAuthMode::None),
        f.client(&only13, None, ServerAuthMode::Full),
    );
    assert!(pump(&mut s, &mut c).is_err());
    let (mut s, mut c) = pair(
        f.server("RSA", &both, ClientAuthMode::None),
        f.client(&only12, None, ServerAuthMode::Full),
    );
    pump(&mut s, &mut c).unwrap();
    assert_eq!(s.session().unwrap().protocol, Protocol::Tls12);
    let (mut s, mut c) = pair(
        f.server("RSA", &both, ClientAuthMode::None),
        f.client(&both, None, ServerAuthMode::Full),
    );
    pump(&mut s, &mut c).unwrap();
    assert_eq!(s.session().unwrap().protocol, Protocol::Tls13);
    // A user-defined cipher list narrows what is negotiated.
    let user = TlsPolicy::from_sysvals(&["*OPSYS"], &["*ECDHE_RSA_AES_256_GCM_SHA384"], "*USRDFN");
    let (mut s, mut c) = pair(
        f.server("RSA", &user, ClientAuthMode::None),
        f.client(&both, None, ServerAuthMode::Full),
    );
    pump(&mut s, &mut c).unwrap();
    assert_eq!(s.session().unwrap().suite_code, 0xC030);
    assert_eq!(s.session().unwrap().protocol, Protocol::Tls12);
    // Nothing enabled is refused at configuration.
    let none = TlsPolicy::from_sysvals(&["*TLSV1"], &[], "*OPSYS");
    assert!(
        config::server_config(&f.identity("RSA"), &none, ClientAuthMode::None, f.roots()).is_err()
    );
}

#[test]
fn an_untrusted_server_fails_unless_passthru() {
    let f = Fixture::new();
    let policy = TlsPolicy::opsys();
    let strict =
        client_config(&policy, Fixture::empty_roots(), None, ServerAuthMode::Full).unwrap();
    let (mut s, mut c) = pair(f.server("RSA", &policy, ClientAuthMode::None), strict);
    assert!(pump(&mut s, &mut c).is_err());
    let lax = client_config(
        &policy,
        Fixture::empty_roots(),
        None,
        ServerAuthMode::Passthru,
    )
    .unwrap();
    let (mut s, mut c) = pair(f.server("RSA", &policy, ClientAuthMode::None), lax);
    pump(&mut s, &mut c).unwrap();
    assert_eq!(
        c.session().unwrap().peer_certs.len(),
        1,
        "the application can still look"
    );
    // The wrong name fails under full checking.
    let (mut s, _) = pair(
        f.server("RSA", &policy, ClientAuthMode::None),
        f.client(&policy, None, ServerAuthMode::Full),
    );
    let mut c = Engine::client(
        f.client(&policy, None, ServerAuthMode::Full),
        server_name("other.test").unwrap(),
    )
    .unwrap();
    assert!(pump(&mut s, &mut c).is_err());
}

#[test]
fn blocking_stream_over_tcp() {
    let f = Fixture::new();
    let policy = TlsPolicy::opsys();
    let server = f.server("ECDSA", &policy, ClientAuthMode::None);
    let client = f.client(&policy, None, ServerAuthMode::Full);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let t = std::thread::spawn(move || {
        let (sock, _) = listener.accept().unwrap();
        let mut s = TlsStream::accept(server, sock).unwrap();
        let mut line = Vec::new();
        let mut b = [0u8; 1];
        while s.read(&mut b).unwrap() == 1 && b[0] != b'\n' {
            line.push(b[0]);
        }
        s.write_all(b"ECHO ").unwrap();
        s.write_all(&line).unwrap();
        s.write_all(b"\n").unwrap();
        s.flush().unwrap();
        s.close().unwrap();
        s.session().unwrap()
    });
    let sock = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let mut c = TlsStream::connect(client, server_name(HOST).unwrap(), sock).unwrap();
    c.write_all(b"ping\n").unwrap();
    c.flush().unwrap();
    let mut reply = String::new();
    c.read_to_string(&mut reply).unwrap();
    assert_eq!(reply, "ECHO ping\n");
    let ss = t.join().unwrap();
    assert_eq!(ss.protocol, Protocol::Tls13);
    assert_eq!(c.session().unwrap().suite_code, ss.suite_code);
}

#[test]
fn application_ids_resolve_to_configurations() {
    let f = Fixture::new();
    let mut apps = AppRegistry::load(&f.files).unwrap();
    let policy = TlsPolicy::opsys();
    // A server with nothing assigned uses the store's default; one with
    // an assignment uses it; a client with none has no identity.
    match config::for_app(&f.system, &apps, "QIBM_QTV_TELNET_SERVER", &policy, f.now).unwrap() {
        AppConfig::Server {
            identity,
            client_auth,
            ..
        } => {
            assert_eq!(
                identity.label, "RSA",
                "the first identity became the default"
            );
            assert_eq!(client_auth, ClientAuthMode::None);
        }
        AppConfig::Client { .. } => panic!("telnet is a server"),
    }
    apps.assign("QIBM_QTMF_FTP_SERVER", Some("ECDSA")).unwrap();
    apps.set_client_auth("QIBM_QTMF_FTP_SERVER", ClientAuth::Required)
        .unwrap();
    apps.assign("QIBM_QTMF_FTP_CLIENT", Some("CLIENT")).unwrap();
    let AppConfig::Server {
        config: sc,
        identity,
        client_auth,
    } = config::for_app(&f.system, &apps, "QIBM_QTMF_FTP_SERVER", &policy, f.now).unwrap()
    else {
        panic!("server")
    };
    assert_eq!(identity.label, "ECDSA");
    assert_eq!(client_auth, ClientAuthMode::Required);
    let AppConfig::Client {
        config: cc,
        identity,
    } = config::for_app(&f.system, &apps, "QIBM_QTMF_FTP_CLIENT", &policy, f.now).unwrap()
    else {
        panic!("client")
    };
    assert_eq!(identity.unwrap().label, "CLIENT");
    let (mut s, mut c) = pair(sc, cc);
    pump(&mut s, &mut c).unwrap();
    assert_eq!(s.session().unwrap().peer_certs.len(), 1);
    assert!(matches!(
        config::for_app(&f.system, &apps, "NOT_THERE", &policy, f.now),
        Err(tls::TlsError::Certs(certs::CertError::NotRegistered(_)))
    ));
    // An expired certificate is refused.
    assert!(matches!(
        config::for_app(
            &f.system,
            &apps,
            "QIBM_QTMF_FTP_SERVER",
            &policy,
            f.now + 40 * 86_400
        ),
        Err(tls::TlsError::Expired(_))
    ));
}

fn openssl() -> Option<std::path::PathBuf> {
    let name = if cfg!(windows) {
        "openssl.exe"
    } else {
        "openssl"
    };
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join(name))
            .find(|f| f.is_file())
    })
}

/// `openssl s_client` against our server on a localhost port, for both
/// protocol versions. Skipped, with a note, when openssl is absent.
#[test]
fn openssl_s_client_connects_to_our_server() {
    let Some(ssl) = openssl() else {
        eprintln!("openssl not on PATH; skipping the s_client check");
        return;
    };
    let f = Fixture::new();
    let dir = std::env::temp_dir().join(format!("qs400-tls-sclient-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ca_pem = dir.join("ca.pem");
    std::fs::write(
        &ca_pem,
        f.files.read(certs::paths::LOCAL_CA_CERT).unwrap().unwrap(),
    )
    .unwrap();
    for (flag, expect) in [("-tls1_3", "TLSv1.3"), ("-tls1_2", "TLSv1.2")] {
        let server = f.server("RSA", &TlsPolicy::opsys(), ClientAuthMode::None);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let t = std::thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            let mut s = TlsStream::accept(server, sock).unwrap();
            let mut got = [0u8; 16];
            let n = s.read(&mut got).unwrap();
            s.write_all(b"QS400 SAYS HELLO\n").unwrap();
            s.flush().unwrap();
            s.close().unwrap();
            (s.session().unwrap(), got[..n].to_vec())
        });
        let mut child = std::process::Command::new(&ssl)
            .args([
                "s_client",
                "-connect",
                &format!("127.0.0.1:{port}"),
                "-servername",
                HOST,
                "-CAfile",
            ])
            .arg(&ca_pem)
            .args([flag, "-ign_eof"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        {
            let mut stdin = child.stdin.take().unwrap();
            // Give the handshake a moment, then send a line.
            std::thread::sleep(std::time::Duration::from_millis(300));
            stdin.write_all(b"ping\n").unwrap();
            stdin.flush().unwrap();
        }
        let out = child.wait_with_output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout) + String::from_utf8_lossy(&out.stderr);
        let (session, got) = t.join().unwrap();
        assert_eq!(got, b"ping\n", "{flag}: {text}");
        assert!(
            text.contains("Verify return code: 0 (ok)"),
            "{flag}: {text}"
        );
        assert!(text.contains(expect), "{flag}: {text}");
        assert!(text.contains("QS400 SAYS HELLO"), "{flag}: {text}");
        assert_eq!(session.server_name.as_deref(), Some(HOST));
    }
    let _ = std::fs::remove_dir_all(dir);
}
