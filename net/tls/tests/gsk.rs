// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `GSKit` and `SSL_` semantics: call order and IBM's return codes,
//! then secure sessions between two API tables over loopback TCP.

mod common;

use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

use certs::apps::{AppRegistry, ClientAuth};
use certs::files::{MemFiles, StoreFiles};
use certs::paths::SYSTEM_STORE;
use certs::stash::KeySealer;
use certs::store::{CertStore, StoreKind};
use common::{Fixture, PASSWORD, fast};
use tls::gsk::codes::*;
use tls::gsk::{Context, Gsk, strerror};
use tls::policy::TlsPolicy;
use tls::ssl::codes::*;
use tls::ssl::{Ssl, SslInit, SslInitApp};

/// A transport with nothing on the other end.
struct Nothing;

impl std::io::Read for Nothing {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        Ok(0)
    }
}

impl std::io::Write for Nothing {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl tls::gsk::Transport for Nothing {}

const TRUST_STORE: &str = "/home/QSTEST/TRUST.KDB";

/// The fixture's files with `*SYSTEM` stashed, a CA-only store at
/// [`TRUST_STORE`], and the application definitions given.
fn system(apps: impl FnOnce(&mut AppRegistry)) -> (Arc<MemFiles>, u64) {
    let Fixture {
        files,
        system,
        ca,
        now,
    } = Fixture::new();
    system
        .stash(&files, PASSWORD, &KeySealer::for_tests())
        .unwrap();
    let mut trust = CertStore::create_with(
        &files,
        TRUST_STORE,
        "trust1",
        StoreKind::System,
        now,
        fast(),
    )
    .unwrap();
    let (ca_der, _) = certs::ca::credentials(&ca).unwrap();
    trust.add_ca("QS400 Test CA", ca_der, true, now).unwrap();
    trust.save(&files).unwrap();
    let mut reg = AppRegistry::load(&files).unwrap();
    apps(&mut reg);
    reg.save(&files).unwrap();
    (Arc::new(files), now)
}

fn ctx(files: &Arc<MemFiles>, now: u64, policy: TlsPolicy) -> Context {
    let files: Arc<dyn StoreFiles> = files.clone();
    Context {
        files,
        sealer: Arc::new(KeySealer::for_tests()),
        policy,
        now: Arc::new(move || now),
    }
}

fn set(g: &mut Gsk, h: u32, id: i32, v: &str) {
    g.attribute_set_buffer(h, id, v.as_bytes()).unwrap();
}

#[test]
#[allow(clippy::too_many_lines)] // one walk through the call order
fn call_order_and_return_codes() {
    let (files, now) = system(|_| {});
    let mut g = Gsk::new(ctx(&files, now, TlsPolicy::opsys()));

    assert_eq!(g.environment_init(99), Err(GSK_INVALID_HANDLE));
    let e = g.environment_open().unwrap();
    assert_eq!(g.secure_soc_open(e), Err(GSK_INVALID_STATE));
    assert_eq!(
        g.attribute_set_buffer(e, 9999, b"x"),
        Err(GSK_ATTRIBUTE_INVALID_ID)
    );
    assert_eq!(
        g.attribute_set_buffer(e, GSK_KEYRING_LABEL, b""),
        Err(GSK_ATTRIBUTE_INVALID_LENGTH)
    );
    assert_eq!(
        g.attribute_set_enum(e, GSK_SESSION_TYPE, 12345),
        Err(GSK_ATTRIBUTE_INVALID_ENUMERATION)
    );
    assert_eq!(
        g.attribute_set_numeric_value(e, GSK_V3_SESSION_TIMEOUT, -1),
        Err(GSK_ATTRIBUTE_INVALID_NUMERIC_VALUE)
    );
    assert_eq!(
        g.attribute_set_numeric_value(e, GSK_FD, 3),
        Err(GSK_ATTRIBUTE_INVALID_ID)
    );
    assert_eq!(
        g.attribute_set_buffer(e, GSK_V3_CIPHER_SPECS, b"2F3"),
        Err(GSK_ATTRIBUTE_INVALID_LENGTH)
    );
    assert_eq!(
        g.attribute_get_enum(e, GSK_SESSION_TYPE),
        Ok(GSK_CLIENT_SESSION)
    );
    assert_eq!(g.attribute_get_enum(e, GSK_PROTOCOL_TLSV13), Ok(GSK_TRUE));
    assert_eq!(g.attribute_get_enum(e, GSK_PROTOCOL_TLSV11), Ok(GSK_FALSE));

    // The keyring: missing, wrong password, unknown label, then right.
    set(&mut g, e, GSK_KEYRING_FILE, "/nowhere/X.KDB");
    set(&mut g, e, GSK_KEYRING_PW, "whatever");
    assert_eq!(g.environment_init(e), Err(GSK_KEYRING_OPEN_ERROR));
    set(&mut g, e, GSK_KEYRING_FILE, "*SYSTEM");
    assert_eq!(
        g.environment_init(e),
        Err(GSK_BAD_FORMAT_OR_INVALID_PASSWORD)
    );
    set(&mut g, e, GSK_KEYRING_PW, PASSWORD);
    set(&mut g, e, GSK_KEYRING_LABEL, "NOPE");
    assert_eq!(g.environment_init(e), Err(GSK_KEY_LABEL_NOT_FOUND));
    set(&mut g, e, GSK_KEYRING_LABEL, "RSA");
    g.environment_init(e).unwrap();

    // Initialized: the environment's attributes are fixed.
    assert_eq!(g.environment_init(e), Err(GSK_INVALID_STATE));
    assert_eq!(
        g.attribute_set_enum(e, GSK_SESSION_TYPE, GSK_SERVER_SESSION),
        Err(GSK_INVALID_STATE)
    );
    assert_eq!(
        g.attribute_get_buffer(e, GSK_KEYRING_PW),
        Err(GSK_ATTRIBUTE_INVALID_ID)
    );
    assert_eq!(
        g.attribute_get_buffer(e, GSK_KEYRING_LABEL).unwrap(),
        b"RSA"
    );

    // A socket before its descriptor and handshake.
    let s = g.secure_soc_open(e).unwrap();
    assert_eq!(
        g.attribute_get_buffer(s, GSK_KEYRING_LABEL).unwrap(),
        b"RSA",
        "inherited"
    );
    assert_eq!(
        g.attribute_set_numeric_value(s, GSK_FD, -1),
        Err(GSK_ATTRIBUTE_INVALID_NUMERIC_VALUE)
    );
    assert_eq!(
        g.attribute_set_buffer(s, GSK_KEYRING_FILE, b"x"),
        Err(GSK_ATTRIBUTE_INVALID_ID)
    );
    assert_eq!(
        g.secure_soc_init(s),
        Err(GSK_IBMI_ERROR_BAD_SOCKET_DESCRIPTOR)
    );
    let mut buf = [0u8; 8];
    assert_eq!(g.secure_soc_read(s, &mut buf), Err(GSK_INVALID_STATE));
    assert_eq!(g.secure_soc_write(s, b"x"), Err(GSK_INVALID_STATE));
    assert_eq!(g.secure_soc_read(s, &mut []), Err(GSK_INVALID_BUFFER_SIZE));
    assert_eq!(
        g.attribute_get_enum(s, GSK_PROTOCOL_USED),
        Err(GSK_INVALID_STATE)
    );
    assert_eq!(
        g.secure_soc_misc(s, GSK_CLOSE_NOTIFY),
        Err(GSK_INVALID_STATE)
    );
    assert_eq!(
        g.attribute_get_cert_info(s, GSK_PARTNER_CERT_INFO),
        Err(GSK_INVALID_STATE)
    );

    // A normal close with a socket open: the handle goes, the socket stays.
    g.environment_close(e).unwrap();
    assert_eq!(
        g.attribute_get_enum(e, GSK_SESSION_TYPE),
        Err(GSK_INVALID_HANDLE)
    );
    assert_eq!(g.secure_soc_open(e), Err(GSK_INVALID_HANDLE));
    assert_eq!(g.counts(), (1, 1));
    g.secure_soc_close(s).unwrap();
    assert_eq!(g.counts(), (0, 0));
    assert_eq!(g.secure_soc_close(s), Err(GSK_INVALID_HANDLE));

    // A delayed close: new sockets are refused until the last one closes.
    let e = g.environment_open().unwrap();
    g.attribute_set_enum(
        e,
        GSK_ENVIRONMENT_CLOSE_OPTIONS,
        GSK_DELAYED_ENVIRONMENT_CLOSE,
    )
    .unwrap();
    set(&mut g, e, GSK_KEYRING_FILE, SYSTEM_STORE);
    g.environment_init(e).unwrap();
    let s = g.secure_soc_open(e).unwrap();
    assert_eq!(
        g.attribute_set_enum(
            s,
            GSK_ENVIRONMENT_CLOSE_OPTIONS,
            GSK_NORMAL_ENVIRONMENT_CLOSE
        ),
        Err(GSK_ATTRIBUTE_INVALID_ID)
    );
    g.environment_close(e).unwrap();
    assert_eq!(g.secure_soc_open(e), Err(GSK_PENDING_CLOSE_ERROR));
    assert_eq!(
        g.attribute_get_enum(e, GSK_SESSION_TYPE),
        Ok(GSK_CLIENT_SESSION)
    );
    g.secure_soc_close(s).unwrap();
    assert_eq!(
        g.attribute_get_enum(e, GSK_SESSION_TYPE),
        Err(GSK_INVALID_HANDLE)
    );
    assert_eq!(g.counts(), (0, 0));

    // Asynchronous forms and callbacks are refused with IBM's codes.
    let e = g.environment_open().unwrap();
    assert_eq!(
        g.attribute_set_callback(e, GSK_IO_CALLBACK),
        Err(GSK_API_NOT_AVAILABLE)
    );
    g.environment_close(e).unwrap();

    // Every code has Quartz text of its own.
    assert_ne!(strerror(GSK_KEYRING_OPEN_ERROR), strerror(GSK_OK));
    assert!(!strerror(GSK_PENDING_CLOSE_ERROR).is_empty());
}

#[test]
fn application_ids_at_environment_init() {
    let (files, now) = system(|r| {
        r.assign("QIBM_QTMF_FTP_SERVER", Some("ECDSA")).unwrap();
    });
    let mut g = Gsk::new(ctx(&files, now, TlsPolicy::opsys()));
    let e = g.environment_open().unwrap();
    set(&mut g, e, GSK_OS400_APPLICATION_ID, "NOT_REGISTERED_APP");
    assert_eq!(g.environment_init(e), Err(GSK_IBMI_ERROR_NOT_REGISTERED));
    set(&mut g, e, GSK_OS400_APPLICATION_ID, "QIBM_QTMF_FTP_SERVER");
    g.environment_init(e).unwrap();
    g.environment_close(e).unwrap();

    // The assigned certificate expired: the environment will not start.
    let mut late = Gsk::new(ctx(&files, now + 40 * 86_400, TlsPolicy::opsys()));
    let e = late.environment_open().unwrap();
    set(
        &mut late,
        e,
        GSK_OS400_APPLICATION_ID,
        "QIBM_QTMF_FTP_SERVER",
    );
    assert_eq!(late.environment_init(e), Err(GSK_KEYFILE_CERT_EXPIRED));

    // Without the stash, the system store cannot open.
    files
        .remove(&certs::paths::stash_path(SYSTEM_STORE))
        .unwrap();
    let e = g.environment_open().unwrap();
    set(&mut g, e, GSK_OS400_APPLICATION_ID, "QIBM_QTMF_FTP_SERVER");
    assert!(g.environment_init(e).is_err());
}

/// What one side of a session saw.
#[derive(Debug)]
struct Seen {
    init: Result<(), i32>,
    protocol: Option<i32>,
    cipher: Option<String>,
    partner_cn: Option<String>,
    got: Vec<u8>,
    read: Result<Vec<u8>, i32>,
}

type Setup = Box<dyn FnOnce(&mut Gsk, u32) + Send>;

fn side(g: &mut Gsk, setup: Setup, sock: TcpStream, server: bool) -> Seen {
    let e = g.environment_open().unwrap();
    setup(g, e);
    g.environment_init(e).unwrap();
    let s = g.secure_soc_open(e).unwrap();
    g.attribute_set_numeric_value(s, GSK_FD, 7).unwrap();
    g.attach(s, Box::new(sock)).unwrap();
    let init = g.secure_soc_init(s);
    let mut seen = Seen {
        init,
        protocol: None,
        cipher: None,
        partner_cn: None,
        got: Vec::new(),
        read: Err(GSK_INVALID_STATE),
    };
    if seen.init.is_ok() {
        seen.protocol = g.attribute_get_enum(s, GSK_PROTOCOL_USED).ok();
        seen.cipher = g
            .attribute_get_buffer(s, GSK_CONNECT_CIPHER_SPEC_EX)
            .ok()
            .map(|b| String::from_utf8(b).unwrap());
        seen.partner_cn = g
            .attribute_get_cert_info(s, GSK_PARTNER_CERT_INFO)
            .ok()
            .and_then(|els| els.into_iter().find(|x| x.id == CERT_COMMON_NAME))
            .map(|x| String::from_utf8(x.data).unwrap());
        let mut buf = [0u8; 64];
        if server {
            let n = g.secure_soc_read(s, &mut buf).unwrap();
            seen.got = buf[..n].to_vec();
            g.secure_soc_write(s, b"pong").unwrap();
        } else {
            // Under TLS 1.3 a client hears of a refused certificate only
            // when it reads.
            g.secure_soc_write(s, b"ping").unwrap();
            seen.read = g.secure_soc_read(s, &mut buf).map(|n| buf[..n].to_vec());
            seen.got = seen.read.clone().unwrap_or_default();
        }
        assert_eq!(
            g.attribute_set_enum(s, GSK_SESSION_TYPE, GSK_CLIENT_SESSION),
            Err(GSK_INVALID_STATE)
        );
    }
    g.secure_soc_close(s).unwrap();
    g.environment_close(e).unwrap();
    assert_eq!(g.counts(), (0, 0));
    seen
}

/// A GSK server and a GSK client over loopback.
fn session(
    files: &Arc<MemFiles>,
    now: u64,
    policy: &TlsPolicy,
    server: Setup,
    client: Setup,
) -> (Seen, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut gs = Gsk::new(ctx(files, now, policy.clone()));
    let t = std::thread::spawn(move || {
        let (sock, _) = listener.accept().unwrap();
        side(&mut gs, server, sock, true)
    });
    let mut gc = Gsk::new(ctx(files, now, policy.clone()));
    let sock = TcpStream::connect(("127.0.0.1", port)).unwrap();
    sock.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    let c = side(&mut gc, client, sock, false);
    (t.join().unwrap(), c)
}

fn app_server(app: &'static str) -> Setup {
    Box::new(move |g, e| {
        set(g, e, GSK_OS400_APPLICATION_ID, app);
        g.attribute_set_enum(e, GSK_SESSION_TYPE, GSK_SERVER_SESSION)
            .unwrap();
    })
}

fn keyring_client(
    path: &'static str,
    pw: &'static str,
    label: Option<&'static str>,
    more: Setup,
) -> Setup {
    Box::new(move |g, e| {
        set(g, e, GSK_KEYRING_FILE, path);
        set(g, e, GSK_KEYRING_PW, pw);
        if let Some(l) = label {
            set(g, e, GSK_KEYRING_LABEL, l);
        }
        g.attribute_set_enum(e, GSK_SESSION_TYPE, GSK_CLIENT_SESSION)
            .unwrap();
        more(g, e);
    })
}

#[test]
fn gsk_sessions_under_tls_1_3_and_1_2() {
    let (files, now) = system(|r| {
        r.assign("QIBM_QTV_TELNET_SERVER", Some("ECDSA")).unwrap();
    });
    let policy = TlsPolicy::opsys();

    let (s, c) = session(
        &files,
        now,
        &policy,
        app_server("QIBM_QTV_TELNET_SERVER"),
        keyring_client(TRUST_STORE, "trust1", None, Box::new(|_, _| {})),
    );
    assert_eq!(s.init, Ok(()), "{s:?}");
    assert_eq!(c.init, Ok(()), "{c:?}");
    assert_eq!(c.protocol, Some(GSK_PROTOCOL_USED_TLSV13));
    assert_eq!(c.cipher.as_deref(), Some("TLS_AES_128_GCM_SHA256"));
    assert_eq!(s.cipher, c.cipher);
    assert_eq!(c.partner_cn.as_deref(), Some(common::HOST));
    assert_eq!(s.partner_cn, None, "no client certificate");
    assert_eq!(s.got, b"ping");
    assert_eq!(c.got, b"pong");

    // TLS 1.3 off and one suite named: the session is 1.2 with it.
    let (s, c) = session(
        &files,
        now,
        &policy,
        app_server("QIBM_QTV_TELNET_SERVER"),
        keyring_client(
            TRUST_STORE,
            "trust1",
            None,
            Box::new(|g, e| {
                g.attribute_set_enum(e, GSK_PROTOCOL_TLSV13, GSK_FALSE)
                    .unwrap();
                set(
                    g,
                    e,
                    GSK_TLSV12_CIPHER_SPECS_EX,
                    "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
                );
            }),
        ),
    );
    assert_eq!(c.init, Ok(()), "{c:?} {s:?}");
    assert_eq!(c.protocol, Some(GSK_PROTOCOL_USED_TLSV12));
    assert_eq!(s.protocol, Some(GSK_PROTOCOL_USED_TLSV12));
    assert_eq!(
        c.cipher.as_deref(),
        Some("TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384")
    );
    assert_eq!(s.got, b"ping");
}

#[test]
fn gsk_protocols_from_the_system_values() {
    let (files, now) = system(|r| {
        r.assign("QIBM_QTV_TELNET_SERVER", Some("RSA")).unwrap();
    });
    // QSSLPCL *TLSV1.2 only: a client asking for 1.3 alone gets nothing.
    let only12 = TlsPolicy::from_sysvals(&["*TLSV1.2"], &[], "*OPSYS");
    let mut g = Gsk::new(ctx(&files, now, only12.clone()));
    let e = g.environment_open().unwrap();
    assert_eq!(g.attribute_get_enum(e, GSK_PROTOCOL_TLSV13), Ok(GSK_FALSE));
    assert_eq!(g.attribute_get_enum(e, GSK_PROTOCOL_TLSV12), Ok(GSK_TRUE));
    g.environment_close(e).unwrap();

    let (s, c) = session(
        &files,
        now,
        &only12,
        app_server("QIBM_QTV_TELNET_SERVER"),
        keyring_client(TRUST_STORE, "trust1", None, Box::new(|_, _| {})),
    );
    assert_eq!(c.init, Ok(()), "{c:?} {s:?}");
    assert_eq!(c.protocol, Some(GSK_PROTOCOL_USED_TLSV12));
    assert_eq!(
        c.cipher.as_deref(),
        Some("TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256")
    );

    // QSSLCSLCTL *USRDFN with one suite: only it is negotiated.
    let one = TlsPolicy::from_sysvals(
        &["*OPSYS"],
        &["*ECDHE_RSA_CHACHA20_POLY1305_SHA256"],
        "*USRDFN",
    );
    let (_, c) = session(
        &files,
        now,
        &one,
        app_server("QIBM_QTV_TELNET_SERVER"),
        keyring_client(TRUST_STORE, "trust1", None, Box::new(|_, _| {})),
    );
    assert_eq!(c.init, Ok(()));
    assert_eq!(
        c.cipher.as_deref(),
        Some("TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256")
    );

    // A client limited to 1.3 against the 1.2-only system: no protocol.
    let mut g = Gsk::new(ctx(&files, now, only12));
    let e = g.environment_open().unwrap();
    set(&mut g, e, GSK_KEYRING_FILE, TRUST_STORE);
    set(&mut g, e, GSK_KEYRING_PW, "trust1");
    g.attribute_set_enum(e, GSK_PROTOCOL_TLSV12, GSK_FALSE)
        .unwrap();
    g.environment_init(e).unwrap();
    let s = g.secure_soc_open(e).unwrap();
    g.attribute_set_numeric_value(s, GSK_FD, 9).unwrap();
    g.attach(s, Box::new(Nothing)).unwrap();
    assert_eq!(g.secure_soc_init(s), Err(GSK_ERROR_NO_CIPHERS));
}

#[test]
fn gsk_client_authentication_from_the_application() {
    let (files, now) = system(|r| {
        r.assign("QIBM_QTMF_FTP_SERVER", Some("ECDSA")).unwrap();
        r.set_client_auth("QIBM_QTMF_FTP_SERVER", ClientAuth::Required)
            .unwrap();
        r.assign("QIBM_QTV_TELNET_SERVER", Some("ECDSA")).unwrap();
        r.set_client_auth("QIBM_QTV_TELNET_SERVER", ClientAuth::Optional)
            .unwrap();
    });
    let policy = TlsPolicy::opsys();

    // Required, and the client has a certificate.
    let (s, c) = session(
        &files,
        now,
        &policy,
        app_server("QIBM_QTMF_FTP_SERVER"),
        keyring_client(SYSTEM_STORE, PASSWORD, Some("CLIENT"), Box::new(|_, _| {})),
    );
    assert_eq!(s.init, Ok(()), "{s:?} {c:?}");
    assert_eq!(s.partner_cn.as_deref(), Some("client one"));

    // Required, and the client has none: the server refuses.
    let (s, c) = session(
        &files,
        now,
        &policy,
        app_server("QIBM_QTMF_FTP_SERVER"),
        keyring_client(TRUST_STORE, "trust1", None, Box::new(|_, _| {})),
    );
    assert_eq!(s.init, Err(GSK_ERROR_NO_CERTIFICATE));
    assert!(c.init.is_err() || c.read.is_err(), "{c:?}");

    // Optional: both with and without a certificate get through.
    let (s, _) = session(
        &files,
        now,
        &policy,
        app_server("QIBM_QTV_TELNET_SERVER"),
        keyring_client(TRUST_STORE, "trust1", None, Box::new(|_, _| {})),
    );
    assert_eq!(s.init, Ok(()));
    assert_eq!(s.partner_cn, None);
    let (s, _) = session(
        &files,
        now,
        &policy,
        app_server("QIBM_QTV_TELNET_SERVER"),
        keyring_client(SYSTEM_STORE, PASSWORD, Some("CLIENT"), Box::new(|_, _| {})),
    );
    assert_eq!(s.partner_cn.as_deref(), Some("client one"));
}

#[test]
fn gsk_untrusted_server() {
    let (files, now) = system(|_| {});
    // A self-signed server certificate the client's store does not trust.
    let mut store = CertStore::open(files.as_ref(), SYSTEM_STORE, PASSWORD).unwrap();
    store
        .create_self_signed(
            "SELF",
            &certs::x509::build::CertSpec::server(common::HOST, now, 30),
            certs::crypto::keys::KeyAlgorithm::EcdsaP256,
        )
        .unwrap();
    store.save(files.as_ref()).unwrap();
    let server: Setup = Box::new(|g, e| {
        set(g, e, GSK_KEYRING_FILE, SYSTEM_STORE);
        set(g, e, GSK_KEYRING_PW, PASSWORD);
        set(g, e, GSK_KEYRING_LABEL, "SELF");
        g.attribute_set_enum(e, GSK_SESSION_TYPE, GSK_SERVER_SESSION)
            .unwrap();
    });
    let (_, c) = session(
        &files,
        now,
        &TlsPolicy::opsys(),
        server,
        keyring_client(TRUST_STORE, "trust1", None, Box::new(|_, _| {})),
    );
    assert_eq!(c.init, Err(GSK_IBMI_ERROR_NOT_TRUSTED_ROOT));
}

#[test]
fn ssl_api_sessions() {
    let (files, now) = system(|r| {
        r.assign("QIBM_QTV_TELNET_SERVER", Some("RSA")).unwrap();
    });
    let policy = TlsPolicy::opsys();
    let mut early = Ssl::new(ctx(&files, now, policy.clone()));
    assert_eq!(
        early.create(3, SSL_ENCRYPT, Box::new(Nothing)).err(),
        Some(SSL_ERROR_NO_INIT)
    );
    assert_eq!(
        early.init_application(&SslInitApp {
            application_id: "NOT_THERE".into(),
            ..SslInitApp::default()
        }),
        Err(SSL_ERROR_NOT_REGISTERED)
    );
    assert_eq!(
        early.init(&SslInit {
            keyring_file: TRUST_STORE.into(),
            keyring_password: "wrong".into(),
            cipher_suite_list: vec![],
        }),
        Err(SSL_ERROR_NOT_KEYRING)
    );
    assert_eq!(
        early.init(&SslInit {
            keyring_file: "/nowhere/X.KDB".into(),
            keyring_password: "x".into(),
            cipher_suite_list: vec![],
        }),
        Err(SSL_ERROR_NO_KEYRING)
    );

    for (protocol, want) in [
        (TLS_VERSION_1_3, TLS_VERSION_1_3),
        (TLS_VERSION_1_2, TLS_VERSION_1_2),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut server = Ssl::new(ctx(&files, now, policy.clone()));
        let t = std::thread::spawn(move || {
            server
                .init_application(&SslInitApp {
                    application_id: "QIBM_QTV_TELNET_SERVER".into(),
                    ..SslInitApp::default()
                })
                .unwrap();
            assert_eq!(server.session_type(), SSL_REGISTERED_AS_SERVER);
            let (sock, _) = listener.accept().unwrap();
            let h = server.create(5, SSL_ENCRYPT, Box::new(sock)).unwrap();
            let mut buf = [0u8; 16];
            assert_eq!(server.read(h, &mut buf), Err(SSL_ERROR_BAD_STATE));
            server.handshake(h, SSL_HANDSHAKE_AS_SERVER).unwrap();
            let n = server.read(h, &mut buf).unwrap();
            server.write(h, b"pong").unwrap();
            let fields = server.handle(h).unwrap().clone();
            server.destroy(h).unwrap();
            (fields, buf[..n].to_vec())
        });
        let mut client = Ssl::new(ctx(&files, now, policy.clone()));
        client
            .init(&SslInit {
                keyring_file: TRUST_STORE.into(),
                keyring_password: "trust1".into(),
                cipher_suite_list: vec![],
            })
            .unwrap();
        let sock = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let h = client.create(6, SSL_ENCRYPT, Box::new(sock)).unwrap();
        client.set_before_handshake(h, protocol, 0, vec![]).unwrap();
        client.handshake(h, SSL_HANDSHAKE_AS_CLIENT).unwrap();
        assert_eq!(
            client.handshake(h, SSL_HANDSHAKE_AS_CLIENT),
            Err(SSL_ERROR_BAD_STATE)
        );
        client.write(h, b"ping").unwrap();
        let mut buf = [0u8; 16];
        let n = client.read(h, &mut buf).unwrap();
        assert_eq!(&buf[..n], b"pong");
        let fields = client.handle(h).unwrap().clone();
        assert_eq!(fields.protocol, want);
        assert!(!fields.peer_cert.is_empty());
        assert_eq!(fields.peer_cert_validate_rc, 0);
        client.destroy(h).unwrap();
        let (sf, got) = t.join().unwrap();
        assert_eq!(got, b"ping");
        assert_eq!(sf.protocol, want);
        assert_eq!(sf.cipher_suite, fields.cipher_suite);
    }
    assert!(!tls::ssl::strerror(SSL_ERROR_NO_INIT).0.is_empty());
}
