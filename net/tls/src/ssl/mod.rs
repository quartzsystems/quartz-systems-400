// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The older `SSL_` API semantics of `qsossl.h`: one initialization per
//! job (`SSL_Init` or `SSL_Init_Application`), then handles made with
//! `SSL_Create`, handshaken, read and written. The binding layer maps
//! the `SSLHandle` structure's fields onto [`SslHandle`].

pub mod codes;

use std::collections::BTreeMap;
use std::io;

use certs::apps::{AppKind, AppRegistry, CaTrust};
use certs::paths::{SYSTEM_STORE, resolve};
use certs::store::CertStore;
use certs::{CertError, x509};
use rustls::RootCertStore;

#[allow(clippy::wildcard_imports)] // the header's names
use self::codes::*;
use crate::config::{self, ClientAuthMode, Identity, ServerAuthMode};
use crate::engine::{Engine, Session};
use crate::error::TlsError;
use crate::gsk::{Context, Transport};
use crate::policy::{Protocol, TlsPolicy};

/// `SSLInit`: a keyring file and password with a cipher list.
#[derive(Clone, Debug, Default)]
pub struct SslInit {
    /// `keyringFileName` (`*SYSTEM` or a path); the system store when empty.
    pub keyring_file: String,
    /// `keyringPassword`; the stash is used when empty.
    pub keyring_password: String,
    /// `cipherSuiteList`, IANA codes; the system list when empty.
    pub cipher_suite_list: Vec<u16>,
}

/// `SSLInitApp`: an application ID.
#[derive(Clone, Debug, Default)]
pub struct SslInitApp {
    /// `applicationID`.
    pub application_id: String,
    /// `localCertificate`: a DER certificate to use instead of the
    /// application's assigned one (matched by fingerprint in the store).
    pub local_certificate: Option<Vec<u8>>,
    /// `cipherSuiteList`.
    pub cipher_suite_list: Vec<u16>,
    /// `protocol`: `SSL_VERSION_CURRENT`, `TLS_VERSION_1_2`, ...
    pub protocol: u32,
    /// `timeout`: session cache seconds (recorded, not used).
    pub timeout: u32,
}

/// The `SSLHandle` fields a program sees.
#[derive(Clone, Debug)]
pub struct SslHandle {
    /// `fd`.
    pub fd: i32,
    /// `createFlags`.
    pub create_flags: i32,
    /// `protocol` (the `*_VERSION_*` value; after the handshake, the one used).
    pub protocol: u32,
    /// `timeout`.
    pub timeout: u32,
    /// `cipherKind` (SSL Version 2; always zero).
    pub cipher_kind: [u8; 3],
    /// `cipherSuite`: the negotiated IANA code.
    pub cipher_suite: u16,
    /// `cipherSuiteList`: the handle's own list, if set before the handshake.
    pub cipher_suite_list: Vec<u16>,
    /// `peerCert`: the peer's certificate, DER.
    pub peer_cert: Vec<u8>,
    /// `peerCertValidateRc`: 0 when the peer's certificate validated.
    pub peer_cert_validate_rc: i32,
    /// `sessionType` as registered (`SSL_REGISTERED_AS_*`).
    pub session_type: u32,
}

struct Init {
    store: CertStore,
    identity: Option<Identity>,
    roots: RootCertStore,
    policy: TlsPolicy,
    app_kind: Option<AppKind>,
    app_client_auth: Option<ClientAuthMode>,
    session_type: u32,
}

struct Live {
    public: SslHandle,
    transport: Option<Box<dyn Transport>>,
    engine: Option<Engine>,
    session: Option<Session>,
    closed: bool,
}

/// The API's state for one job.
pub struct Ssl {
    ctx: Context,
    init: Option<Init>,
    handles: BTreeMap<u32, Live>,
    next: u32,
}

/// `Ok(value)` or an `SSL_ERROR_*` code.
pub type SslResult<T> = Result<T, i32>;

impl Ssl {
    /// A fresh state.
    pub fn new(ctx: Context) -> Self {
        Self {
            ctx,
            init: None,
            handles: BTreeMap::new(),
            next: 1,
        }
    }

    fn policy_for(&self, codes: &[u16], protocol: u32) -> SslResult<TlsPolicy> {
        let mut p = self.ctx.policy.clone();
        match protocol {
            SSL_VERSION_CURRENT | TLS_VERSION_1 | TLSV1_SSLV3 => {}
            TLS_VERSION_1_2 => p = p.restricted_to(&[Protocol::Tls12]),
            TLS_VERSION_1_3 => p = p.restricted_to(&[Protocol::Tls13]),
            _ => return Err(SSL_ERROR_UNSUPPORTED),
        }
        if !codes.is_empty() {
            p = p.with_suites(codes);
            if p.usable_suites().is_empty() {
                return Err(SSL_ERROR_BAD_CIPHER_SUITE);
            }
        }
        if p.versions().is_err() {
            return Err(SSL_ERROR_UNSUPPORTED);
        }
        Ok(p)
    }

    /// `SSL_Init` (and `QlgSSL_Init`, whose path arrives already
    /// converted).
    pub fn init(&mut self, init: &SslInit) -> SslResult<()> {
        let files = self.ctx.files.as_ref();
        let path = if init.keyring_file.trim().is_empty() {
            SYSTEM_STORE.to_string()
        } else {
            resolve(&init.keyring_file).to_string()
        };
        let store = if init.keyring_password.is_empty() {
            CertStore::open_stashed(files, &path, self.ctx.sealer.as_ref())
        } else {
            CertStore::open(files, &path, &init.keyring_password)
        }
        .map_err(map_store_error)?;
        let now = (self.ctx.now)();
        let identity = match store.default_label() {
            Some(l) => Some(identity(&store, l, now)?),
            None => None,
        };
        let roots = config::trusted_roots(&store, &CaTrust::All).map_err(|_| SSL_ERROR_UNKNOWN)?;
        let policy = self.policy_for(&init.cipher_suite_list, SSL_VERSION_CURRENT)?;
        self.init = Some(Init {
            store,
            identity,
            roots,
            policy,
            app_kind: None,
            app_client_auth: None,
            session_type: SSL_REGISTERED_AS_NOT_SPECIFIED,
        });
        Ok(())
    }

    /// `SSL_Init_Application`.
    pub fn init_application(&mut self, init: &SslInitApp) -> SslResult<()> {
        let files = self.ctx.files.as_ref();
        let registry = AppRegistry::load(files).map_err(map_store_error)?;
        let def = registry
            .get(init.application_id.trim())
            .map_err(map_store_error)?;
        let store = CertStore::open_stashed(files, SYSTEM_STORE, self.ctx.sealer.as_ref())
            .map_err(map_store_error)?;
        let now = (self.ctx.now)();
        let label = match &init.local_certificate {
            Some(der) => {
                let info = x509::CertInfo::from_der(der).map_err(|_| SSL_ERROR_BAD_CERTIFICATE)?;
                let e = store
                    .find_by_sha256(&info.sha256)
                    .map_err(map_store_error)?
                    .ok_or(SSL_ERROR_NO_CERTIFICATE)?;
                Some(e.label.clone())
            }
            None => def
                .label
                .clone()
                .or_else(|| store.default_label().map(str::to_string)),
        };
        let identity = match (def.kind, label) {
            (AppKind::Server, None) => return Err(SSL_ERROR_NO_CERTIFICATE),
            (_, None) => None,
            (_, Some(l)) => Some(identity(&store, &l, now)?),
        };
        let roots = config::trusted_roots(&store, &def.ca_trust).map_err(|_| SSL_ERROR_UNKNOWN)?;
        let policy = self.policy_for(&init.cipher_suite_list, init.protocol)?;
        let session_type = match (def.kind, ClientAuthMode::from(def.client_auth)) {
            (AppKind::Client, _) => SSL_REGISTERED_AS_CLIENT,
            (AppKind::Server, ClientAuthMode::None) => SSL_REGISTERED_AS_SERVER,
            (AppKind::Server, ClientAuthMode::Optional) => {
                SSL_REGISTERED_AS_SERVER_WITH_OPTIONAL_CLIENT_AUTH
            }
            (AppKind::Server, ClientAuthMode::Required) => {
                SSL_REGISTERED_AS_SERVER_WITH_CLIENT_AUTH
            }
        };
        self.init = Some(Init {
            store,
            identity,
            roots,
            policy,
            app_kind: Some(def.kind),
            app_client_auth: Some(ClientAuthMode::from(def.client_auth)),
            session_type,
        });
        Ok(())
    }

    /// The `sessionType` output of `SSL_Init_Application`.
    pub fn session_type(&self) -> u32 {
        self.init
            .as_ref()
            .map_or(SSL_REGISTERED_AS_NOT_SPECIFIED, |i| i.session_type)
    }

    /// `SSL_Create`: a handle for descriptor `fd`. `flags` is
    /// `SSL_ENCRYPT` or `SSL_DONT_ENCRYPT`.
    pub fn create(&mut self, fd: i32, flags: i32, transport: Box<dyn Transport>) -> SslResult<u32> {
        let init = self.init.as_ref().ok_or(SSL_ERROR_NO_INIT)?;
        if fd < 0 {
            return Err(SSL_ERROR_IO);
        }
        let h = self.next;
        self.next += 1;
        self.handles.insert(
            h,
            Live {
                public: SslHandle {
                    fd,
                    create_flags: flags,
                    protocol: SSL_VERSION_CURRENT,
                    timeout: 0,
                    cipher_kind: [0; 3],
                    cipher_suite: 0,
                    cipher_suite_list: Vec::new(),
                    peer_cert: Vec::new(),
                    peer_cert_validate_rc: 0,
                    session_type: init.session_type,
                },
                transport: Some(transport),
                engine: None,
                session: None,
                closed: false,
            },
        );
        Ok(h)
    }

    /// The handle's structure.
    pub fn handle(&self, h: u32) -> SslResult<&SslHandle> {
        self.handles
            .get(&h)
            .map(|l| &l.public)
            .ok_or(SSL_ERROR_BAD_STATE)
    }

    /// Fields a program may change before the handshake: `protocol`,
    /// `timeout`, `cipherSuiteList`.
    pub fn set_before_handshake(
        &mut self,
        h: u32,
        protocol: u32,
        timeout: u32,
        cipher_suite_list: Vec<u16>,
    ) -> SslResult<()> {
        let live = self.handles.get_mut(&h).ok_or(SSL_ERROR_BAD_STATE)?;
        if live.engine.is_some() {
            return Err(SSL_ERROR_BAD_STATE);
        }
        live.public.protocol = protocol;
        live.public.timeout = timeout;
        live.public.cipher_suite_list = cipher_suite_list;
        Ok(())
    }

    /// `SSL_Handshake` with `how` (`SSL_HANDSHAKE_AS_*`).
    pub fn handshake(&mut self, h: u32, how: i32) -> SslResult<()> {
        let init = self.init.as_ref().ok_or(SSL_ERROR_NO_INIT)?;
        let live = self.handles.get(&h).ok_or(SSL_ERROR_BAD_STATE)?;
        if live.engine.is_some() {
            return Err(SSL_ERROR_BAD_STATE);
        }
        if live.public.create_flags & SSL_ENCRYPT_MASK == 0 {
            // SSL_DONT_ENCRYPT: the handle passes data through.
            let live = self.handles.get_mut(&h).expect("checked");
            live.public.protocol = 0;
            return Ok(());
        }
        let policy = {
            let p = self.policy_for(&live.public.cipher_suite_list, live.public.protocol)?;
            if live.public.cipher_suite_list.is_empty() {
                init.policy.restricted_to(&p.protocols)
            } else {
                p
            }
        };
        let engine = match how {
            SSL_HANDSHAKE_AS_CLIENT => {
                if init.app_kind == Some(AppKind::Server) {
                    return Err(SSL_ERROR_BAD_STATE);
                }
                if init.roots.is_empty() {
                    return Err(SSL_ERROR_NO_CERTIFICATE_AUTHORITIES);
                }
                let config = config::client_config(
                    &policy,
                    init.roots.clone(),
                    init.identity.as_ref(),
                    ServerAuthMode::ChainOnly,
                )
                .map_err(map_tls_error)?;
                let name = config::server_name("qs400.invalid").map_err(|_| SSL_ERROR_UNKNOWN)?;
                Engine::client(config, name).map_err(map_tls_error)?
            }
            SSL_HANDSHAKE_AS_SERVER
            | SSL_HANDSHAKE_AS_SERVER_WITH_CLIENT_AUTH
            | SSL_HANDSHAKE_AS_SERVER_WITH_OPTIONAL_CLIENT_AUTH => {
                if init.app_kind == Some(AppKind::Client) {
                    return Err(SSL_ERROR_BAD_STATE);
                }
                let identity = init.identity.as_ref().ok_or(SSL_ERROR_NO_CERTIFICATE)?;
                let mut auth = match how {
                    SSL_HANDSHAKE_AS_SERVER => ClientAuthMode::None,
                    SSL_HANDSHAKE_AS_SERVER_WITH_CLIENT_AUTH => ClientAuthMode::Required,
                    _ => ClientAuthMode::Optional,
                };
                if let Some(a) = init.app_client_auth
                    && a == ClientAuthMode::Required
                {
                    auth = ClientAuthMode::Required;
                }
                if auth != ClientAuthMode::None && init.roots.is_empty() {
                    return Err(SSL_ERROR_NO_CERTIFICATE_AUTHORITIES);
                }
                let config = config::server_config(identity, &policy, auth, init.roots.clone())
                    .map_err(map_tls_error)?;
                Engine::server(config).map_err(map_tls_error)?
            }
            _ => return Err(SSL_ERROR_BAD_STATE),
        };
        let live = self.handles.get_mut(&h).expect("checked");
        let transport = live.transport.as_mut().ok_or(SSL_ERROR_IO)?;
        let mut engine = engine;
        match engine.complete_io(transport) {
            Ok(()) => {
                let session = engine.session();
                if let Some(s) = &session {
                    live.public.cipher_suite = s.suite_code;
                    live.public.protocol = match s.protocol {
                        Protocol::Tls13 => TLS_VERSION_1_3,
                        Protocol::Tls12 => TLS_VERSION_1_2,
                        _ => TLS_VERSION_1,
                    };
                    live.public.peer_cert = s.peer_certs.first().cloned().unwrap_or_default();
                    live.public.peer_cert_validate_rc = 0;
                }
                live.session = session;
                live.engine = Some(engine);
                Ok(())
            }
            Err(e) => {
                let rc = map_handshake_error(&e);
                live.public.peer_cert_validate_rc = rc;
                Err(rc)
            }
        }
    }

    /// `SSL_Read`: bytes read, or a code.
    pub fn read(&mut self, h: u32, buf: &mut [u8]) -> SslResult<usize> {
        let live = self.handles.get_mut(&h).ok_or(SSL_ERROR_BAD_STATE)?;
        if live.closed {
            return Err(SSL_ERROR_CLOSED);
        }
        let transport = live.transport.as_mut().ok_or(SSL_ERROR_IO)?;
        let Some(engine) = live.engine.as_mut() else {
            if live.public.create_flags & SSL_ENCRYPT_MASK == 0 && live.public.protocol == 0 {
                return transport.read(buf).map_err(|e| map_io_error(&e));
            }
            return Err(SSL_ERROR_BAD_STATE);
        };
        loop {
            match engine.read_plain(buf) {
                Ok(0) => {
                    live.closed = true;
                    return Err(SSL_ERROR_CLOSED);
                }
                Ok(n) => return Ok(n),
                Err(TlsError::Io(e)) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(TlsError::Closed) => return Err(SSL_ERROR_SOCKET_CLOSED),
                Err(_) => return Err(SSL_ERROR_BAD_MESSAGE),
            }
            match engine.connection().read_tls(transport) {
                Ok(0) => return Err(SSL_ERROR_SOCKET_CLOSED),
                Ok(_) => {}
                Err(e) => return Err(map_io_error(&e)),
            }
            if engine.process().is_err() {
                return Err(SSL_ERROR_BAD_MESSAGE);
            }
        }
    }

    /// `SSL_Write`: bytes written, or a code.
    pub fn write(&mut self, h: u32, data: &[u8]) -> SslResult<usize> {
        let live = self.handles.get_mut(&h).ok_or(SSL_ERROR_BAD_STATE)?;
        if live.closed {
            return Err(SSL_ERROR_CLOSED);
        }
        let transport = live.transport.as_mut().ok_or(SSL_ERROR_IO)?;
        let Some(engine) = live.engine.as_mut() else {
            if live.public.create_flags & SSL_ENCRYPT_MASK == 0 && live.public.protocol == 0 {
                return transport
                    .write(data)
                    .and_then(|n| transport.flush().map(|()| n))
                    .map_err(|e| map_io_error(&e));
            }
            return Err(SSL_ERROR_BAD_STATE);
        };
        let n = engine.write_plain(data).map_err(|_| SSL_ERROR_IO)?;
        while engine.wants_write() {
            engine
                .connection()
                .write_tls(transport)
                .map_err(|e| map_io_error(&e))?;
        }
        transport.flush().map_err(|e| map_io_error(&e))?;
        Ok(n)
    }

    /// `SSL_Destroy`: `close_notify` when a session is up, then the
    /// handle is freed. Returns the transport for the binding to close.
    pub fn destroy(&mut self, h: u32) -> SslResult<Option<Box<dyn Transport>>> {
        let mut live = self.handles.remove(&h).ok_or(SSL_ERROR_BAD_STATE)?;
        if let (Some(engine), Some(transport), false) =
            (live.engine.as_mut(), live.transport.as_mut(), live.closed)
        {
            engine.close();
            while engine.wants_write() {
                if engine.connection().write_tls(transport).is_err() {
                    break;
                }
            }
        }
        Ok(live.transport.take())
    }

    /// The session of a handshaken handle.
    pub fn session(&self, h: u32) -> SslResult<&Session> {
        self.handles
            .get(&h)
            .and_then(|l| l.session.as_ref())
            .ok_or(SSL_ERROR_BAD_STATE)
    }

    /// Whether `SSL_Init` or `SSL_Init_Application` has run.
    pub fn is_initialized(&self) -> bool {
        self.init.is_some()
    }

    /// The store the initialization opened (for diagnostics).
    pub fn store(&self) -> Option<&CertStore> {
        self.init.as_ref().map(|i| &i.store)
    }

    /// Keep the shared context reachable for the binding.
    pub fn context(&self) -> &Context {
        &self.ctx
    }
}

/// `SSL_Strerror`: the text, plus the message identifier and file a
/// program could show; QS/400 has none for these yet, so both are blank.
pub fn strerror(rc: i32) -> (&'static str, SslErrorMsg) {
    (codes::text(rc), SslErrorMsg::default())
}

/// `SSLErrorMsg`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SslErrorMsg {
    /// `messageID` (7 characters).
    pub message_id: String,
    /// `messageFile` (20 characters).
    pub message_file: String,
}

/// `SSL_Perror`: the line a program's standard error would show.
pub fn perror(rc: i32, prefix: &str) -> String {
    if prefix.is_empty() {
        codes::text(rc).to_string()
    } else {
        format!("{prefix}: {}", codes::text(rc))
    }
}

fn identity(store: &CertStore, label: &str, now: u64) -> SslResult<Identity> {
    match Identity::from_store(store, label, now) {
        Ok(i) => Ok(i),
        Err(TlsError::Expired(_)) => Err(SSL_ERROR_CERT_EXPIRED),
        Err(TlsError::NotAnIdentity(_) | TlsError::Certs(CertError::LabelNotFound(_))) => {
            Err(SSL_ERROR_NO_CERTIFICATE)
        }
        Err(e) => Err(map_tls_error(e)),
    }
}

#[allow(clippy::needless_pass_by_value)] // for map_err
fn map_store_error(e: CertError) -> i32 {
    match e {
        CertError::NotRegistered(_) => SSL_ERROR_NOT_REGISTERED,
        CertError::NotFound(_) => SSL_ERROR_NO_KEYRING,
        CertError::BadPassword | CertError::Corrupt(_) | CertError::Unseal => SSL_ERROR_NOT_KEYRING,
        CertError::LabelNotFound(_) | CertError::NoPrivateKey(_) => SSL_ERROR_NO_CERTIFICATE,
        CertError::Io(_) => SSL_ERROR_IO,
        CertError::Malformed(_) => SSL_ERROR_BAD_CERTIFICATE,
        _ => SSL_ERROR_UNKNOWN,
    }
}

fn map_tls_error(e: TlsError) -> i32 {
    match e {
        TlsError::Certs(c) => map_store_error(c),
        TlsError::NothingEnabled(_) => SSL_ERROR_NO_CIPHERS,
        TlsError::NoTrustedCas => SSL_ERROR_NO_CERTIFICATE_AUTHORITIES,
        TlsError::NoCertificate(_) | TlsError::NotAnIdentity(_) => SSL_ERROR_NO_CERTIFICATE,
        TlsError::Expired(_) => SSL_ERROR_CERT_EXPIRED,
        TlsError::Closed => SSL_ERROR_SOCKET_CLOSED,
        TlsError::Io(e) => map_io_error(&e),
        TlsError::ServerName(_) | TlsError::Rustls(_) => SSL_ERROR_UNKNOWN,
    }
}

fn map_io_error(e: &io::Error) -> i32 {
    match e.kind() {
        io::ErrorKind::UnexpectedEof
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::BrokenPipe => SSL_ERROR_SOCKET_CLOSED,
        _ => SSL_ERROR_IO,
    }
}

fn map_handshake_error(e: &TlsError) -> i32 {
    use rustls::{AlertDescription, CertificateError as C, Error as R};
    match e {
        TlsError::Io(io) => map_io_error(io),
        TlsError::Closed => SSL_ERROR_SOCKET_CLOSED,
        TlsError::Rustls(r) => match r {
            R::InvalidCertificate(C::UnknownIssuer) => SSL_ERROR_NOT_TRUSTED_ROOT,
            R::InvalidCertificate(C::Expired) => SSL_ERROR_CERT_EXPIRED,
            R::InvalidCertificate(C::NotValidYet) => SSL_ERROR_BAD_DATE,
            R::InvalidCertificate(C::BadSignature) => SSL_ERROR_BAD_CERT_SIG,
            R::InvalidCertificate(C::BadEncoding) => SSL_ERROR_BAD_CERT,
            R::InvalidCertificate(_) => SSL_ERROR_BAD_CERTIFICATE,
            R::NoCertificatesPresented => SSL_ERROR_NO_CERTIFICATE,
            R::AlertReceived(
                AlertDescription::HandshakeFailure
                | AlertDescription::ProtocolVersion
                | AlertDescription::InsufficientSecurity,
            )
            | R::PeerIncompatible(_) => SSL_ERROR_NO_CIPHERS,
            R::AlertReceived(
                AlertDescription::BadCertificate
                | AlertDescription::UnknownCA
                | AlertDescription::CertificateUnknown
                | AlertDescription::CertificateExpired
                | AlertDescription::UnsupportedCertificate,
            ) => SSL_ERROR_CERTIFICATE_REJECTED,
            R::AlertReceived(AlertDescription::CloseNotify) => SSL_ERROR_CLOSED,
            R::DecryptError => SSL_ERROR_BAD_MAC,
            _ => SSL_ERROR_BAD_MESSAGE,
        },
        TlsError::NothingEnabled(_) => SSL_ERROR_NO_CIPHERS,
        TlsError::NoTrustedCas => SSL_ERROR_NO_CERTIFICATE_AUTHORITIES,
        _ => SSL_ERROR_UNKNOWN,
    }
}
