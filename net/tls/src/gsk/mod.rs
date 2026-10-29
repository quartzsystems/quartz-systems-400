// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `GSKit` API semantics: environments and secure sockets with the
//! attribute model of `gskssl.h`, the state rules, and IBM's return
//! codes. Plain Rust; the ILE binding layer maps `gsk_handle` values,
//! buffers and the socket descriptor onto these calls (docs/CERTS.md).
//!
//! The socket is a [`Transport`] the binding attaches to a secure
//! socket handle; `GSK_FD` records the descriptor number it stands for.

pub mod certinfo;
pub mod codes;
mod strerror;

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::sync::Arc;
use std::time::Duration;

use certs::apps::{AppKind, AppRegistry, CaTrust};
use certs::files::StoreFiles;
use certs::paths::{SYSTEM_STORE, resolve};
use certs::stash::Sealer;
use certs::store::CertStore;
use certs::{CertError, x509};
use rustls::RootCertStore;

#[allow(clippy::wildcard_imports)] // the header's names
use self::codes::*;
use crate::config::{self, ClientAuthMode, Identity, ServerAuthMode};
use crate::engine::{Engine, Session};
use crate::error::TlsError;
use crate::policy::{Protocol, TlsPolicy};
use crate::suites;

pub use self::certinfo::CertDataElem;
pub use self::strerror::strerror;

/// A `gsk_handle`.
pub type Handle = u32;

/// What a secure socket reads and writes: a socket, once the binding
/// attaches one.
pub trait Transport: Read + Write + Send {
    /// Apply a read timeout (`GSK_IBMI_READ_TIMEOUT`, `GSK_HANDSHAKE_TIMEOUT`).
    fn set_read_timeout(&mut self, _timeout: Option<Duration>) -> io::Result<()> {
        Ok(())
    }
}

impl Transport for std::net::TcpStream {
    fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        std::net::TcpStream::set_read_timeout(self, timeout)
    }
}

/// What the layer needs from the system: the files, the stash sealer,
/// the policy from the system values, and the clock.
pub struct Context {
    /// The IFS.
    pub files: Arc<dyn StoreFiles>,
    /// Opens stashed stores.
    pub sealer: Arc<dyn Sealer>,
    /// `QSSLPCL`, `QSSLCSL`, `QSSLCSLCTL`.
    pub policy: TlsPolicy,
    /// Unix time.
    pub now: Arc<dyn Fn() -> u64 + Send + Sync>,
}

/// The attributes a handle holds; a socket starts with its
/// environment's.
#[derive(Clone, Debug, Default)]
struct Attrs {
    buffers: BTreeMap<i32, Vec<u8>>,
    enums: BTreeMap<i32, i32>,
    numbers: BTreeMap<i32, i32>,
}

impl Attrs {
    fn text(&self, id: i32) -> Option<String> {
        self.buffers.get(&id).map(|b| {
            String::from_utf8_lossy(b)
                .trim_end_matches('\0')
                .to_string()
        })
    }

    fn enum_or(&self, id: i32, default: i32) -> i32 {
        self.enums.get(&id).copied().unwrap_or(default)
    }
}

/// What an initialized environment resolved from the store.
struct Material {
    identity: Option<Identity>,
    roots: RootCertStore,
    /// The application's client authentication, when it has one.
    app_client_auth: Option<ClientAuthMode>,
    app_kind: Option<AppKind>,
}

struct Environment {
    attrs: Attrs,
    material: Option<Material>,
    sockets: usize,
    closing: bool,
}

struct Socket {
    env: Handle,
    attrs: Attrs,
    transport: Option<Box<dyn Transport>>,
    engine: Option<Engine>,
    session: Option<Session>,
    validation: i32,
    closed: bool,
}

/// The API's state: every open environment and secure socket.
pub struct Gsk {
    ctx: Context,
    envs: BTreeMap<Handle, Environment>,
    /// Environments closed while sockets still use them: gone for the
    /// program, kept until their last socket closes.
    retired: BTreeMap<Handle, Environment>,
    socs: BTreeMap<Handle, Socket>,
    next: Handle,
}

/// `Ok(value)` or a GSK return code.
pub type GskResult<T> = Result<T, Rc>;

impl Gsk {
    /// A fresh table.
    pub fn new(ctx: Context) -> Self {
        Self {
            ctx,
            envs: BTreeMap::new(),
            retired: BTreeMap::new(),
            socs: BTreeMap::new(),
            next: 1,
        }
    }

    fn fresh(&mut self) -> Handle {
        let h = self.next;
        self.next += 1;
        h
    }

    fn env(&self, h: Handle) -> GskResult<&Environment> {
        self.envs.get(&h).ok_or(GSK_INVALID_HANDLE)
    }

    fn env_mut(&mut self, h: Handle) -> GskResult<&mut Environment> {
        self.envs.get_mut(&h).ok_or(GSK_INVALID_HANDLE)
    }

    /// A socket's environment, closed or not.
    fn env_of(&self, h: Handle) -> GskResult<&Environment> {
        self.envs
            .get(&h)
            .or_else(|| self.retired.get(&h))
            .ok_or(GSK_INVALID_STATE)
    }

    fn soc(&self, h: Handle) -> GskResult<&Socket> {
        self.socs.get(&h).ok_or(GSK_INVALID_HANDLE)
    }

    fn soc_mut(&mut self, h: Handle) -> GskResult<&mut Socket> {
        self.socs.get_mut(&h).ok_or(GSK_INVALID_HANDLE)
    }

    /// Whether `h` is an environment (else a socket, or nothing).
    pub fn is_environment(&self, h: Handle) -> bool {
        self.envs.contains_key(&h)
    }

    // ---- environments ----

    /// `gsk_environment_open`.
    pub fn environment_open(&mut self) -> GskResult<Handle> {
        let h = self.fresh();
        let mut attrs = Attrs::default();
        attrs.enums.insert(GSK_SESSION_TYPE, GSK_CLIENT_SESSION);
        attrs
            .enums
            .insert(GSK_CLIENT_AUTH_TYPE, GSK_CLIENT_AUTH_FULL);
        attrs
            .enums
            .insert(GSK_SERVER_AUTH_TYPE, GSK_SERVER_AUTH_FULL);
        attrs
            .enums
            .insert(GSK_ENVIRONMENT_CLOSE_OPTIONS, GSK_NORMAL_ENVIRONMENT_CLOSE);
        self.envs.insert(
            h,
            Environment {
                attrs,
                material: None,
                sockets: 0,
                closing: false,
            },
        );
        Ok(h)
    }

    /// `gsk_environment_init`: open the store the attributes name and
    /// pick the certificate.
    pub fn environment_init(&mut self, h: Handle) -> GskResult<()> {
        let env = self.env(h)?;
        if env.material.is_some() {
            return Err(GSK_INVALID_STATE);
        }
        let attrs = env.attrs.clone();
        let material = self.resolve(&attrs)?;
        self.env_mut(h)?.material = Some(material);
        Ok(())
    }

    fn resolve(&self, attrs: &Attrs) -> GskResult<Material> {
        let now = (self.ctx.now)();
        let files = self.ctx.files.as_ref();
        if let Some(app) = attrs.text(GSK_IBMI_APPLICATION_ID) {
            let registry = AppRegistry::load(files).map_err(map_store_error)?;
            let def = registry.get(&app).map_err(map_store_error)?;
            let store = CertStore::open_stashed(files, SYSTEM_STORE, self.ctx.sealer.as_ref())
                .map_err(map_store_error)?;
            let label = def
                .label
                .clone()
                .or_else(|| store.default_label().map(str::to_string));
            let identity = match (def.kind, label) {
                (AppKind::Server, None) => return Err(GSK_ERROR_NO_CERTIFICATE),
                (_, None) => None,
                (_, Some(l)) => Some(identity(&store, &l, now)?),
            };
            let roots = config::trusted_roots(&store, &def.ca_trust).map_err(map_tls_error)?;
            return Ok(Material {
                identity,
                roots,
                app_client_auth: Some(ClientAuthMode::from(def.client_auth)),
                app_kind: Some(def.kind),
            });
        }
        let path = attrs
            .text(GSK_KEYRING_FILE)
            .map_or_else(|| SYSTEM_STORE.to_string(), |p| resolve(&p).to_string());
        let store = match attrs.text(GSK_KEYRING_PW) {
            Some(pw) => CertStore::open(files, &path, &pw).map_err(map_store_error)?,
            None => match CertStore::open_stashed(files, &path, self.ctx.sealer.as_ref()) {
                Ok(s) => s,
                Err(CertError::NotFound(p)) if p != path => return Err(GSK_NO_KEYFILE_PASSWORD),
                Err(e) => return Err(map_store_error(e)),
            },
        };
        let label = attrs
            .text(GSK_KEYRING_LABEL)
            .or_else(|| attrs.text(GSK_KEYRING_LABEL_EX))
            .or_else(|| store.default_label().map(str::to_string));
        let identity = match label {
            Some(l) => Some(identity(&store, &l, now)?),
            None => None,
        };
        let roots = config::trusted_roots(&store, &CaTrust::All).map_err(map_tls_error)?;
        Ok(Material {
            identity,
            roots,
            app_client_auth: None,
            app_kind: None,
        })
    }

    /// `gsk_environment_close`. With sockets still open the handle goes
    /// at once (`GSK_NORMAL_ENVIRONMENT_CLOSE`) or stays until the last
    /// socket closes, refusing new sockets with `GSK_PENDING_CLOSE_ERROR`
    /// (`GSK_DELAYED_ENVIRONMENT_CLOSE`). Open sockets keep working.
    pub fn environment_close(&mut self, h: Handle) -> GskResult<()> {
        let env = self.env_mut(h)?;
        if env.closing {
            return Err(GSK_INVALID_STATE);
        }
        if env.sockets > 0 {
            if env
                .attrs
                .enum_or(GSK_ENVIRONMENT_CLOSE_OPTIONS, GSK_NORMAL_ENVIRONMENT_CLOSE)
                == GSK_DELAYED_ENVIRONMENT_CLOSE
            {
                env.closing = true;
            } else {
                let mut env = self.envs.remove(&h).expect("checked");
                env.closing = true;
                self.retired.insert(h, env);
            }
            return Ok(());
        }
        self.envs.remove(&h);
        Ok(())
    }

    // ---- attributes ----

    fn attrs_mut(&mut self, h: Handle) -> GskResult<&mut Attrs> {
        if let Some(env) = self.envs.get_mut(&h) {
            if env.material.is_some() {
                return Err(GSK_INVALID_STATE);
            }
            return Ok(&mut env.attrs);
        }
        let soc = self.socs.get_mut(&h).ok_or(GSK_INVALID_HANDLE)?;
        if soc.engine.is_some() {
            return Err(GSK_INVALID_STATE);
        }
        Ok(&mut soc.attrs)
    }

    fn attrs(&self, h: Handle) -> GskResult<&Attrs> {
        if let Some(env) = self.envs.get(&h) {
            return Ok(&env.attrs);
        }
        Ok(&self.soc(h)?.attrs)
    }

    /// `gsk_attribute_set_buffer`.
    pub fn attribute_set_buffer(&mut self, h: Handle, id: i32, value: &[u8]) -> GskResult<()> {
        if !BUF_IDS.contains(&id) {
            return Err(GSK_ATTRIBUTE_INVALID_ID);
        }
        let max = match id {
            GSK_IBMI_APPLICATION_ID => 100,
            GSK_KEYRING_LABEL | GSK_KEYRING_LABEL_EX => 64,
            GSK_KEYRING_PW => 128,
            _ => 4096,
        };
        if value.is_empty() || value.len() > max {
            return Err(GSK_ATTRIBUTE_INVALID_LENGTH);
        }
        if matches!(
            id,
            GSK_CONNECT_CIPHER_SPEC
                | GSK_CONNECT_CIPHER_SPEC_EX
                | GSK_SSL_EXTN_SERVERNAME
                | GSK_SSL_EXTN_SELECTED_ALPN_PROTOCOL
                | GSK_CONNECT_COMPRESSION
        ) {
            return Err(GSK_ATTRIBUTE_INVALID_ID);
        }
        let is_env = self.is_environment(h);
        if !is_env
            && matches!(
                id,
                GSK_IBMI_APPLICATION_ID
                    | GSK_KEYRING_FILE
                    | GSK_KEYRING_PW
                    | GSK_KEYRING_STASH_FILE
            )
        {
            return Err(GSK_ATTRIBUTE_INVALID_ID);
        }
        if matches!(
            id,
            GSK_V3_CIPHER_SPECS
                | GSK_V2_CIPHER_SPECS
                | GSK_TLSV10_CIPHER_SPECS
                | GSK_TLSV11_CIPHER_SPECS
                | GSK_TLSV12_CIPHER_SPECS
        ) {
            // Two hex digits per suite.
            if !value.len().is_multiple_of(2) || !value.iter().all(u8::is_ascii_hexdigit) {
                return Err(GSK_ATTRIBUTE_INVALID_LENGTH);
            }
        }
        self.attrs_mut(h)?.buffers.insert(id, value.to_vec());
        Ok(())
    }

    /// `gsk_attribute_set_enum`.
    pub fn attribute_set_enum(&mut self, h: Handle, id: i32, value: i32) -> GskResult<()> {
        if !ENUM_IDS.contains(&id) {
            return Err(GSK_ATTRIBUTE_INVALID_ID);
        }
        let valid = match id {
            GSK_PROTOCOL_USED => return Err(GSK_ATTRIBUTE_INVALID_ID),
            GSK_SESSION_TYPE => matches!(
                value,
                GSK_CLIENT_SESSION
                    | GSK_SERVER_SESSION
                    | GSK_SERVER_SESSION_WITH_CL_AUTH
                    | GSK_SERVER_SESSION_WITH_CL_AUTH_CRITICAL
            ),
            GSK_CLIENT_AUTH_TYPE => matches!(
                value,
                GSK_CLIENT_AUTH_FULL | GSK_CLIENT_AUTH_PASSTHRU | GSK_IBMI_CLIENT_AUTH_REQUIRED
            ),
            GSK_SERVER_AUTH_TYPE => matches!(
                value,
                GSK_SERVER_AUTH_FULL | GSK_SERVER_AUTH_PASSTHRU | GSK_IBMI_SERVER_AUTH_REQUIRED
            ),
            GSK_PROTOCOL_SSLV2 => matches!(value, GSK_PROTOCOL_SSLV2_ON | GSK_PROTOCOL_SSLV2_OFF),
            GSK_PROTOCOL_SSLV3 => matches!(value, GSK_PROTOCOL_SSLV3_ON | GSK_PROTOCOL_SSLV3_OFF),
            GSK_PROTOCOL_TLSV1 => matches!(value, GSK_PROTOCOL_TLSV1_ON | GSK_PROTOCOL_TLSV1_OFF),
            GSK_ENVIRONMENT_CLOSE_OPTIONS => {
                matches!(
                    value,
                    GSK_DELAYED_ENVIRONMENT_CLOSE | GSK_NORMAL_ENVIRONMENT_CLOSE
                )
            }
            GSK_SID_FIRST => matches!(value, GSK_SID_IS_FIRST | GSK_SID_NOT_FIRST),
            GSK_SSL_FIPS_MODE_PROCESSING => {
                matches!(
                    value,
                    GSK_SSL_FIPS_MODE_PROCESSING_ON | GSK_SSL_FIPS_MODE_PROCESSING_OFF
                )
            }
            GSK_ALLOW_UNAUTHENTICATED_RESUME => matches!(
                value,
                GSK_ALLOW_UNAUTHENTICATED_RESUME_ON | GSK_ALLOW_UNAUTHENTICATED_RESUME_OFF
            ),
            GSK_CC_MODE_CONTROL => matches!(
                value,
                GSK_CC_MODE_STRICT_BASIC_CONST_ON | GSK_CC_MODE_STRICT_BASIC_CONST_OFF
            ),
            _ => matches!(value, GSK_TRUE | GSK_FALSE),
        };
        if !valid {
            return Err(GSK_ATTRIBUTE_INVALID_ENUMERATION);
        }
        if id == GSK_ENVIRONMENT_CLOSE_OPTIONS && !self.is_environment(h) {
            return Err(GSK_ATTRIBUTE_INVALID_ID);
        }
        self.attrs_mut(h)?.enums.insert(id, value);
        Ok(())
    }

    /// `gsk_attribute_set_numeric_value`.
    pub fn attribute_set_numeric_value(&mut self, h: Handle, id: i32, value: i32) -> GskResult<()> {
        if !NUM_IDS.contains(&id) {
            return Err(GSK_ATTRIBUTE_INVALID_ID);
        }
        if matches!(
            id,
            GSK_LAST_VALIDATION_ERROR | GSK_CERTIFICATE_VALIDATION_CODE
        ) {
            return Err(GSK_ATTRIBUTE_INVALID_ID);
        }
        if id == GSK_FD {
            if self.is_environment(h) {
                return Err(GSK_ATTRIBUTE_INVALID_ID);
            }
            if value < 0 {
                return Err(GSK_ATTRIBUTE_INVALID_NUMERIC_VALUE);
            }
        } else if value < 0 {
            return Err(GSK_ATTRIBUTE_INVALID_NUMERIC_VALUE);
        }
        self.attrs_mut(h)?.numbers.insert(id, value);
        Ok(())
    }

    /// `gsk_attribute_set_callback`: the binding layer owns the C
    /// function pointers; none is supported here.
    pub fn attribute_set_callback(&mut self, h: Handle, id: i32) -> GskResult<()> {
        self.attrs(h)?;
        match id {
            GSK_IO_CALLBACK
            | GSK_SID_CACHE_CALLBACK
            | GSK_CLIENT_CERT_CALLBACK
            | GSK_PKCS11_CALLBACK
            | GSK_ENVIRONMENT_CLOSE_CALLBACK
            | GSK_CERT_VALIDATION_CALLBACK
            | GSK_CIPHER_SESSION_RESET => Err(GSK_API_NOT_AVAILABLE),
            _ => Err(GSK_ATTRIBUTE_INVALID_ID),
        }
    }

    /// `gsk_attribute_get_buffer`.
    pub fn attribute_get_buffer(&self, h: Handle, id: i32) -> GskResult<Vec<u8>> {
        if !BUF_IDS.contains(&id) || id == GSK_KEYRING_PW {
            return Err(GSK_ATTRIBUTE_INVALID_ID);
        }
        if let Some(soc) = self.socs.get(&h) {
            match id {
                GSK_CONNECT_CIPHER_SPEC_EX => {
                    let s = soc.session.as_ref().ok_or(GSK_INVALID_STATE)?;
                    return Ok(suites::by_code(s.suite_code)
                        .map_or_else(
                            || format!("{:04X}", s.suite_code),
                            suites::IbmSuite::iana_name,
                        )
                        .into_bytes());
                }
                GSK_CONNECT_CIPHER_SPEC => {
                    let s = soc.session.as_ref().ok_or(GSK_INVALID_STATE)?;
                    return Ok(format!("{:02X}", s.suite_code & 0xFF).into_bytes());
                }
                GSK_SSL_EXTN_SERVERNAME => {
                    let s = soc.session.as_ref().ok_or(GSK_INVALID_STATE)?;
                    return s
                        .server_name
                        .clone()
                        .map(String::into_bytes)
                        .ok_or(GSK_ATTRIBUTE_INVALID_ID);
                }
                GSK_SSL_EXTN_SELECTED_ALPN_PROTOCOL => {
                    let s = soc.session.as_ref().ok_or(GSK_INVALID_STATE)?;
                    return s.alpn.clone().ok_or(GSK_ERROR_NO_APPLICATION_PROTOCOL);
                }
                GSK_CONNECT_COMPRESSION => return Ok(b"NONE".to_vec()),
                _ => {}
            }
        }
        self.attrs(h)?
            .buffers
            .get(&id)
            .cloned()
            .ok_or(GSK_ATTRIBUTE_INVALID_ID)
    }

    /// `gsk_attribute_get_enum`.
    pub fn attribute_get_enum(&self, h: Handle, id: i32) -> GskResult<i32> {
        if !ENUM_IDS.contains(&id) {
            return Err(GSK_ATTRIBUTE_INVALID_ID);
        }
        if id == GSK_PROTOCOL_USED {
            let soc = self.soc(h)?;
            let s = soc.session.as_ref().ok_or(GSK_INVALID_STATE)?;
            return Ok(match s.protocol {
                Protocol::Tls13 => GSK_PROTOCOL_USED_TLSV13,
                Protocol::Tls12 => GSK_PROTOCOL_USED_TLSV12,
                Protocol::Tls11 => GSK_PROTOCOL_USED_TLSV11,
                Protocol::Tls10 => GSK_PROTOCOL_USED_TLSV1,
                Protocol::Ssl3 => GSK_PROTOCOL_USED_SSLV3,
                Protocol::Ssl2 => GSK_PROTOCOL_USED_SSLV2,
            });
        }
        let attrs = self.attrs(h)?;
        if let Some(v) = attrs.enums.get(&id) {
            return Ok(*v);
        }
        // Defaults for what was never set.
        Ok(match id {
            GSK_PROTOCOL_TLSV13 => bool_enum(self.ctx.policy.protocols.contains(&Protocol::Tls13)),
            GSK_PROTOCOL_TLSV12 => bool_enum(self.ctx.policy.protocols.contains(&Protocol::Tls12)),
            GSK_PROTOCOL_SSLV2 => GSK_PROTOCOL_SSLV2_OFF,
            GSK_PROTOCOL_SSLV3 => GSK_PROTOCOL_SSLV3_OFF,
            GSK_PROTOCOL_TLSV1 => GSK_PROTOCOL_TLSV1_OFF,
            GSK_SID_FIRST => GSK_SID_IS_FIRST,
            GSK_SSL_FIPS_MODE_PROCESSING => GSK_SSL_FIPS_MODE_PROCESSING_OFF,
            GSK_ALLOW_UNAUTHENTICATED_RESUME => GSK_ALLOW_UNAUTHENTICATED_RESUME_OFF,
            _ => GSK_FALSE,
        })
    }

    /// `gsk_attribute_get_numeric_value`.
    pub fn attribute_get_numeric_value(&self, h: Handle, id: i32) -> GskResult<i32> {
        if !NUM_IDS.contains(&id) {
            return Err(GSK_ATTRIBUTE_INVALID_ID);
        }
        if matches!(
            id,
            GSK_LAST_VALIDATION_ERROR | GSK_CERTIFICATE_VALIDATION_CODE
        ) {
            return Ok(self.socs.get(&h).map_or(GSKVAL_OK, |s| s.validation));
        }
        Ok(self.attrs(h)?.numbers.get(&id).copied().unwrap_or(0))
    }

    /// `gsk_attribute_get_cert_info`.
    pub fn attribute_get_cert_info(&self, h: Handle, which: i32) -> GskResult<Vec<CertDataElem>> {
        let soc = self.soc(h)?;
        let session = soc.session.as_ref().ok_or(GSK_INVALID_STATE)?;
        match which {
            GSK_PARTNER_CERT_INFO => {
                let der = session
                    .peer_certs
                    .first()
                    .ok_or(GSK_CERTIFICATE_NOT_AVAILABLE)?;
                certinfo::elements(der)
            }
            GSK_LOCAL_CERT_INFO => {
                let env = self.env_of(soc.env)?;
                let m = env.material.as_ref().ok_or(GSK_INVALID_STATE)?;
                let id = m.identity.as_ref().ok_or(GSK_CERTIFICATE_NOT_AVAILABLE)?;
                certinfo::elements(id.chain[0].as_ref())
            }
            _ => Err(GSK_ATTRIBUTE_INVALID_ID),
        }
    }

    // ---- secure sockets ----

    /// `gsk_secure_soc_open`: a socket handle from an initialized
    /// environment, with the environment's attributes.
    pub fn secure_soc_open(&mut self, env_handle: Handle) -> GskResult<Handle> {
        let env = self.env_mut(env_handle)?;
        if env.material.is_none() {
            return Err(GSK_INVALID_STATE);
        }
        if env.closing {
            return Err(GSK_PENDING_CLOSE_ERROR);
        }
        env.sockets += 1;
        let attrs = env.attrs.clone();
        let h = self.fresh();
        self.socs.insert(
            h,
            Socket {
                env: env_handle,
                attrs,
                transport: None,
                engine: None,
                session: None,
                validation: GSKVAL_OK,
                closed: false,
            },
        );
        Ok(h)
    }

    /// Attach the socket the descriptor in `GSK_FD` stands for. The
    /// binding calls this when it sets `GSK_FD`.
    pub fn attach(&mut self, h: Handle, transport: Box<dyn Transport>) -> GskResult<()> {
        let soc = self.soc_mut(h)?;
        if soc.engine.is_some() {
            return Err(GSK_IBMI_ERROR_ALREADY_SECURE);
        }
        soc.transport = Some(transport);
        Ok(())
    }

    /// Take the transport back (after close, or to hand the socket on).
    pub fn detach(&mut self, h: Handle) -> GskResult<Option<Box<dyn Transport>>> {
        Ok(self.soc_mut(h)?.transport.take())
    }

    /// `gsk_secure_soc_init`: the handshake, blocking on the transport.
    pub fn secure_soc_init(&mut self, h: Handle) -> GskResult<()> {
        let (env_handle, attrs) = {
            let soc = self.soc(h)?;
            if soc.engine.is_some() {
                return Err(GSK_INVALID_STATE);
            }
            if !soc.attrs.numbers.contains_key(&GSK_FD) || soc.transport.is_none() {
                return Err(GSK_IBMI_ERROR_BAD_SOCKET_DESCRIPTOR);
            }
            (soc.env, soc.attrs.clone())
        };
        let engine = {
            let env = self.env_of(env_handle)?;
            let m = env.material.as_ref().ok_or(GSK_INVALID_STATE)?;
            self.build_engine(m, &attrs)?
        };
        let soc = self.soc_mut(h)?;
        let transport = soc.transport.as_mut().expect("checked");
        let timeout = attrs
            .numbers
            .get(&GSK_HANDSHAKE_TIMEOUT)
            .copied()
            .unwrap_or(0);
        let _ = transport.set_read_timeout(seconds(timeout));
        let mut engine = engine;
        let result = engine.complete_io(transport);
        let read_timeout = attrs
            .numbers
            .get(&GSK_IBMI_READ_TIMEOUT)
            .copied()
            .unwrap_or(0);
        let _ = transport.set_read_timeout(seconds(read_timeout));
        match result {
            Ok(()) => {
                soc.session = engine.session();
                soc.engine = Some(engine);
                soc.validation = GSKVAL_OK;
                Ok(())
            }
            Err(e) => {
                let (rc, code) = map_handshake_error(&e);
                soc.validation = code;
                Err(rc)
            }
        }
    }

    fn build_engine(&self, m: &Material, attrs: &Attrs) -> GskResult<Engine> {
        let policy = effective_policy(&self.ctx.policy, attrs);
        let session_type = attrs.enum_or(GSK_SESSION_TYPE, GSK_CLIENT_SESSION);
        if m.app_kind == Some(AppKind::Client) && session_type != GSK_CLIENT_SESSION {
            return Err(GSK_ERROR_INCORRECT_SESSION_TYPE);
        }
        if session_type == GSK_CLIENT_SESSION {
            let mode = match attrs.enum_or(GSK_SERVER_AUTH_TYPE, GSK_SERVER_AUTH_FULL) {
                GSK_SERVER_AUTH_PASSTHRU => ServerAuthMode::Passthru,
                _ => ServerAuthMode::ChainOnly,
            };
            if mode == ServerAuthMode::ChainOnly && m.roots.is_empty() {
                return Err(GSK_IBMI_ERROR_NO_CERTIFICATE_AUTHORITIES);
            }
            let config = config::client_config(&policy, m.roots.clone(), m.identity.as_ref(), mode)
                .map_err(map_tls_error)?;
            let name = attrs
                .text(GSK_SSL_EXTN_SERVERNAME_REQUEST)
                .unwrap_or_else(|| "qs400.invalid".to_string());
            let name = config::server_name(&name).map_err(|_| GSK_ATTRIBUTE_INVALID_LENGTH)?;
            Engine::client(config, name).map_err(map_tls_error)
        } else {
            let identity = m.identity.as_ref().ok_or(GSK_ERROR_NO_CERTIFICATE)?;
            let client_auth = match session_type {
                GSK_SERVER_SESSION => ClientAuthMode::None,
                GSK_SERVER_SESSION_WITH_CL_AUTH_CRITICAL => ClientAuthMode::Required,
                _ => match attrs.enum_or(GSK_CLIENT_AUTH_TYPE, GSK_CLIENT_AUTH_FULL) {
                    GSK_IBMI_CLIENT_AUTH_REQUIRED => ClientAuthMode::Required,
                    _ => ClientAuthMode::Optional,
                },
            };
            // The application's registered setting wins over the
            // program's when it is stricter.
            let client_auth = match (m.app_client_auth, client_auth) {
                (Some(ClientAuthMode::Required), _) => ClientAuthMode::Required,
                (Some(ClientAuthMode::Optional), ClientAuthMode::None) => ClientAuthMode::Optional,
                (_, c) => c,
            };
            if client_auth != ClientAuthMode::None && m.roots.is_empty() {
                return Err(GSK_IBMI_ERROR_NO_CERTIFICATE_AUTHORITIES);
            }
            let config = config::server_config(identity, &policy, client_auth, m.roots.clone())
                .map_err(map_tls_error)?;
            Engine::server(config).map_err(map_tls_error)
        }
    }

    /// `gsk_secure_soc_read`.
    pub fn secure_soc_read(&mut self, h: Handle, buf: &mut [u8]) -> GskResult<usize> {
        if buf.is_empty() {
            return Err(GSK_INVALID_BUFFER_SIZE);
        }
        let soc = self.soc_mut(h)?;
        if soc.closed {
            return Err(GSK_ERROR_SSL_CLOSED);
        }
        let engine = soc.engine.as_mut().ok_or(GSK_INVALID_STATE)?;
        let transport = soc
            .transport
            .as_mut()
            .ok_or(GSK_IBMI_ERROR_BAD_SOCKET_DESCRIPTOR)?;
        loop {
            match engine.read_plain(buf) {
                Ok(0) => {
                    soc.closed = true;
                    return Err(GSK_ERROR_SSL_CLOSED);
                }
                Ok(n) => return Ok(n),
                Err(TlsError::Io(e)) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(TlsError::Closed) => return Err(GSK_ERROR_SOCKET_CLOSED),
                Err(_) => return Err(GSK_ERROR_BAD_MESSAGE),
            }
            // Nothing buffered: pull one record from the transport.
            match engine.connection().read_tls(transport) {
                Ok(0) => return Err(GSK_ERROR_SOCKET_CLOSED),
                Ok(_) => {}
                Err(e) => return Err(map_io_error(&e)),
            }
            if let Err(e) = engine.process() {
                return Err(map_handshake_error(&e).0);
            }
        }
    }

    /// `gsk_secure_soc_write`: encrypts and sends all of `data`.
    pub fn secure_soc_write(&mut self, h: Handle, data: &[u8]) -> GskResult<usize> {
        if data.is_empty() {
            return Err(GSK_INVALID_BUFFER_SIZE);
        }
        let soc = self.soc_mut(h)?;
        if soc.closed {
            return Err(GSK_ERROR_SSL_CLOSED);
        }
        let engine = soc.engine.as_mut().ok_or(GSK_INVALID_STATE)?;
        let transport = soc
            .transport
            .as_mut()
            .ok_or(GSK_IBMI_ERROR_BAD_SOCKET_DESCRIPTOR)?;
        let n = engine.write_plain(data).map_err(|_| GSK_ERROR_IO)?;
        while engine.wants_write() {
            engine
                .connection()
                .write_tls(transport)
                .map_err(|e| map_io_error(&e))?;
        }
        transport.flush().map_err(|e| map_io_error(&e))?;
        Ok(n)
    }

    /// `gsk_secure_soc_misc`.
    pub fn secure_soc_misc(&mut self, h: Handle, id: i32) -> GskResult<()> {
        let soc = self.soc_mut(h)?;
        let engine = soc.engine.as_mut().ok_or(GSK_INVALID_STATE)?;
        match id {
            GSK_CLOSE_NOTIFY => {
                let transport = soc
                    .transport
                    .as_mut()
                    .ok_or(GSK_IBMI_ERROR_BAD_SOCKET_DESCRIPTOR)?;
                engine.close();
                while engine.wants_write() {
                    engine
                        .connection()
                        .write_tls(transport)
                        .map_err(|e| map_io_error(&e))?;
                }
                soc.closed = true;
                Ok(())
            }
            GSK_RESET_CIPHER | GSK_RESET_SESSION | GSK_RESET_CIPHER_WITHNEWSESSION => {
                Err(GSK_ERROR_UNSUPPORTED)
            }
            _ => Err(GSK_MISC_INVALID_ID),
        }
    }

    /// `gsk_secure_soc_close`: sends `close_notify` when the session is
    /// up, then frees the handle. The transport is dropped unless the
    /// binding detached it first.
    pub fn secure_soc_close(&mut self, h: Handle) -> GskResult<()> {
        let mut soc = self.socs.remove(&h).ok_or(GSK_INVALID_HANDLE)?;
        if let (Some(engine), Some(transport), false) =
            (soc.engine.as_mut(), soc.transport.as_mut(), soc.closed)
        {
            engine.close();
            while engine.wants_write() {
                if engine.connection().write_tls(transport).is_err() {
                    break;
                }
            }
        }
        for table in [&mut self.envs, &mut self.retired] {
            if let Some(env) = table.get_mut(&soc.env) {
                env.sockets = env.sockets.saturating_sub(1);
                if env.closing && env.sockets == 0 {
                    table.remove(&soc.env);
                }
            }
        }
        Ok(())
    }

    /// `gsk_secure_soc_startInit`, `startRecv`, `startSend`: the
    /// asynchronous forms need the completion-port layer of the sockets
    /// crate; until then they are refused.
    pub fn secure_soc_start(&self, h: Handle) -> GskResult<()> {
        self.soc(h)?;
        Err(GSK_IBMI_ERROR_INVALID_IOCOMPLETIONPORT)
    }

    /// The session of an initialized socket.
    pub fn session(&self, h: Handle) -> GskResult<&Session> {
        self.soc(h)?.session.as_ref().ok_or(GSK_INVALID_STATE)
    }

    /// Environments (open or closed but still in use) and sockets, for
    /// tests and diagnostics.
    pub fn counts(&self) -> (usize, usize) {
        (self.envs.len() + self.retired.len(), self.socs.len())
    }
}

/// `gsk_strerror_msg`: the text plus the message identifier and file
/// a program could show. QS/400 has no message for these yet, so the
/// identifier and file are blank.
pub fn strerror_msg(rc: Rc) -> (&'static str, &'static str, &'static str) {
    (strerror(rc), "", "")
}

fn bool_enum(b: bool) -> i32 {
    if b { GSK_TRUE } else { GSK_FALSE }
}

fn seconds(n: i32) -> Option<Duration> {
    (n > 0).then(|| Duration::from_secs(u64::try_from(n).unwrap_or(0)))
}

fn identity(store: &CertStore, label: &str, now: u64) -> GskResult<Identity> {
    match Identity::from_store(store, label, now) {
        Ok(i) => Ok(i),
        Err(TlsError::Expired(_)) => Err(GSK_KEYFILE_CERT_EXPIRED),
        Err(TlsError::NotAnIdentity(_)) => Err(GSK_ERROR_NO_PRIVATE_KEY),
        Err(TlsError::Certs(CertError::LabelNotFound(_))) => Err(GSK_KEY_LABEL_NOT_FOUND),
        Err(e) => Err(map_tls_error(e)),
    }
}

/// The policy after the handle's own protocol and cipher attributes.
fn effective_policy(system: &TlsPolicy, attrs: &Attrs) -> TlsPolicy {
    let mut protocols = system.protocols.clone();
    for (id, p) in [
        (GSK_PROTOCOL_TLSV13, Protocol::Tls13),
        (GSK_PROTOCOL_TLSV12, Protocol::Tls12),
    ] {
        if let Some(v) = attrs.enums.get(&id)
            && *v == GSK_FALSE
        {
            protocols.retain(|x| *x != p);
        }
    }
    let mut policy = system.restricted_to(&protocols);
    let mut codes: Vec<u16> = Vec::new();
    let mut any = false;
    for (id, tls13) in [
        (GSK_TLSV13_CIPHER_SPECS_EX, true),
        (GSK_TLSV12_CIPHER_SPECS_EX, false),
        (GSK_V3_CIPHER_SPECS_EX, false),
    ] {
        if let Some(spec) = attrs.text(id) {
            any = true;
            let (ok, _) = suites::parse_spec_string(&spec);
            codes.extend(ok.iter().filter(|s| s.tls13 == tls13).map(|s| s.code));
        }
    }
    for id in [GSK_TLSV12_CIPHER_SPECS, GSK_V3_CIPHER_SPECS] {
        if let Some(spec) = attrs.text(id) {
            any = true;
            codes.extend(suites::parse_short_specs(&spec));
        }
    }
    if any {
        policy = policy.with_suites(&codes);
    }
    policy
}

#[allow(clippy::needless_pass_by_value)] // for map_err
fn map_store_error(e: CertError) -> Rc {
    match e {
        CertError::NotRegistered(_) => GSK_IBMI_ERROR_NOT_REGISTERED,
        CertError::NotFound(_) => GSK_KEYRING_OPEN_ERROR,
        CertError::BadPassword => GSK_BAD_FORMAT_OR_INVALID_PASSWORD,
        CertError::Corrupt(_) => GSK_KEYFILE_INVALID_FORMAT,
        CertError::Unseal => GSK_NO_KEYFILE_PASSWORD,
        CertError::LabelNotFound(_) => GSK_KEY_LABEL_NOT_FOUND,
        CertError::NoPrivateKey(_) => GSK_ERROR_NO_PRIVATE_KEY,
        CertError::Io(_) => GSK_KEYFILE_IO_ERROR,
        CertError::Malformed(_) => GSK_ERROR_ASN,
        _ => GSK_ERROR_UNKNOWN_ERROR,
    }
}

fn map_tls_error(e: TlsError) -> Rc {
    match e {
        TlsError::Certs(c) => map_store_error(c),
        TlsError::NothingEnabled(_) => GSK_ERROR_NO_CIPHERS,
        TlsError::NoTrustedCas => GSK_IBMI_ERROR_NO_CERTIFICATE_AUTHORITIES,
        TlsError::NoCertificate(_) => GSK_ERROR_NO_CERTIFICATE,
        TlsError::NotAnIdentity(_) => GSK_ERROR_NO_PRIVATE_KEY,
        TlsError::Expired(_) => GSK_KEYFILE_CERT_EXPIRED,
        TlsError::ServerName(_) => GSK_ATTRIBUTE_INVALID_LENGTH,
        TlsError::Closed => GSK_ERROR_SOCKET_CLOSED,
        TlsError::Io(e) => map_io_error(&e),
        TlsError::Rustls(_) => GSK_ERROR_BAD_MESSAGE,
    }
}

fn map_io_error(e: &io::Error) -> Rc {
    match e.kind() {
        io::ErrorKind::WouldBlock => GSK_WOULD_BLOCK,
        io::ErrorKind::TimedOut => GSK_IBMI_ERROR_TIMED_OUT,
        io::ErrorKind::UnexpectedEof
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::BrokenPipe => GSK_ERROR_SOCKET_CLOSED,
        _ => GSK_ERROR_IO,
    }
}

/// A handshake failure as a return code and a validation code.
fn map_handshake_error(e: &TlsError) -> (Rc, i32) {
    use rustls::{AlertDescription, CertificateError as C, Error as R};
    match e {
        TlsError::Io(io) => (map_io_error(io), GSKVAL_OK),
        TlsError::Closed => (GSK_ERROR_SOCKET_CLOSED, GSKVAL_OK),
        TlsError::Rustls(r) => match r {
            R::InvalidCertificate(C::UnknownIssuer) => (
                GSK_IBMI_ERROR_NOT_TRUSTED_ROOT,
                GSKVAL_ERROR_ROOT_NOT_TRUSTED,
            ),
            R::InvalidCertificate(C::Expired) => (GSK_ERROR_BAD_DATE, GSKVAL_ERROR_CERT_EXPIRED),
            R::InvalidCertificate(C::NotValidYet) => {
                (GSK_ERROR_BAD_DATE, GSKVAL_ERROR_CERT_NOT_YET_VALID)
            }
            R::InvalidCertificate(C::Revoked) => (
                GSK_IBMI_ERROR_CERTIFICATE_REVOKED,
                GSKVAL_ERROR_REVOKED_CERT,
            ),
            R::InvalidCertificate(C::BadSignature) => {
                (GSK_ERROR_BAD_CERT_SIG, GSKVAL_ERROR_SIGNATURE_MISMATCH)
            }
            R::InvalidCertificate(C::NotValidForName) => (
                GSK_ERROR_CERT_VALIDATION,
                GSKVAL_ERROR_UNRECOGNIZED_SUBJECTALTNAME,
            ),
            R::InvalidCertificate(C::BadEncoding) => (GSK_ERROR_BAD_CERT, GSKVAL_ERROR_FAILED),
            R::InvalidCertificate(_) => (GSK_ERROR_CERT_VALIDATION, GSKVAL_ERROR_FAILED),
            R::NoCertificatesPresented => (GSK_ERROR_NO_CERTIFICATE, GSKVAL_OK),
            R::AlertReceived(
                AlertDescription::HandshakeFailure
                | AlertDescription::ProtocolVersion
                | AlertDescription::InsufficientSecurity,
            )
            | R::PeerIncompatible(_) => (GSK_ERROR_NO_CIPHERS, GSKVAL_OK),
            R::AlertReceived(
                AlertDescription::BadCertificate
                | AlertDescription::UnknownCA
                | AlertDescription::CertificateUnknown
                | AlertDescription::CertificateExpired
                | AlertDescription::CertificateRevoked
                | AlertDescription::UnsupportedCertificate,
            ) => (GSK_ERROR_BAD_PEER, GSKVAL_OK),
            R::AlertReceived(AlertDescription::CloseNotify) => (GSK_ERROR_SSL_CLOSED, GSKVAL_OK),
            R::DecryptError => (GSK_ERROR_BAD_MAC, GSKVAL_OK),
            _ => (GSK_ERROR_BAD_MESSAGE, GSKVAL_OK),
        },
        other => (map_tls_error_ref(other), GSKVAL_OK),
    }
}

fn map_tls_error_ref(e: &TlsError) -> Rc {
    match e {
        TlsError::NothingEnabled(_) => GSK_ERROR_NO_CIPHERS,
        TlsError::NoTrustedCas => GSK_IBMI_ERROR_NO_CERTIFICATE_AUTHORITIES,
        TlsError::NoCertificate(_) => GSK_ERROR_NO_CERTIFICATE,
        TlsError::Expired(_) => GSK_KEYFILE_CERT_EXPIRED,
        _ => GSK_ERROR_UNKNOWN_ERROR,
    }
}

/// The facts of a peer certificate, for programs that want more than
/// the element array.
pub fn peer_info(session: &Session) -> Option<x509::CertInfo> {
    session
        .peer_certs
        .first()
        .and_then(|d| x509::CertInfo::from_der(d).ok())
}
