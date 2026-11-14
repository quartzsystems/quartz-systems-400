// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One job's client state: its `LDAP *` handles, the message chains
//! `ldap_result` handed out, the `BerElement`s, and the storage every
//! returned structure lives in until the program frees it.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ldap::client::{Client, ClientError, Options};
use ldap::control::Control;
use ldap::msg::{DerefAliases, LdapMessage, ProtocolOp};
use mi::{FatPtr, Job};
use tls::rustls::ClientConfig;

use crate::consts::*;
use crate::mem::{Codec, Image};

/// What a secure connection is made with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TlsWant {
    /// The `*SYSTEM` certificate store's trusted CAs; a client
    /// certificate of the label, if one is named.
    System {
        /// The client certificate's label.
        label: Option<String>,
    },
    /// A key ring file (`ldap_ssl_client_init`): the store at the path.
    KeyRing {
        /// The store's path.
        path: String,
        /// Its password, if given.
        password: Option<String>,
        /// The client certificate's label.
        label: Option<String>,
    },
    /// A DCM application ID's definition.
    App(String),
}

/// The job's source of TLS client configurations.
pub type TlsSource = Rc<dyn Fn(&TlsWant) -> Result<Arc<ClientConfig>, String>>;

thread_local! {
    static TLS: RefCell<Option<TlsSource>> = const { RefCell::new(None) };
    static STATES: RefCell<HashMap<usize, Rc<RefCell<State>>>> = RefCell::new(HashMap::new());
}

/// Sets where secure connections get their configuration.
pub fn set_tls_source(src: Option<TlsSource>) {
    TLS.with(|t| *t.borrow_mut() = src);
}

/// A configuration for `want`; the text of why not.
pub fn tls_config(want: &TlsWant) -> Result<Arc<ClientConfig>, String> {
    let src = TLS.with(|t| t.borrow().clone());
    match src {
        Some(f) => f(want),
        None => Err("this job has no certificate store".into()),
    }
}

/// The options of a handle (or the defaults new handles take).
#[derive(Debug, Clone)]
pub struct Opts {
    /// `LDAP_OPT_DEREF`.
    pub deref: i32,
    /// `LDAP_OPT_SIZELIMIT`.
    pub sizelimit: i32,
    /// `LDAP_OPT_TIMELIMIT`.
    pub timelimit: i32,
    /// `LDAP_OPT_REFHOPLIMIT`.
    pub refhoplimit: i32,
    /// `LDAP_OPT_REFERRALS`.
    pub referrals: bool,
    /// `LDAP_OPT_PROTOCOL_VERSION`.
    pub version: i32,
    /// `LDAP_OPT_UTF8_IO`.
    pub utf8: bool,
    /// `LDAP_OPT_SSL_TIMEOUT`, seconds.
    pub ssl_timeout: i32,
    /// `LDAP_OPT_SERVER_CONTROLS`.
    pub sctrls: Vec<Control>,
    /// `LDAP_OPT_CLIENT_CONTROLS`.
    pub cctrls: Vec<Control>,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            deref: LDAP_DEREF_NEVER,
            sizelimit: LDAP_NO_LIMIT,
            timelimit: LDAP_NO_LIMIT,
            refhoplimit: 10,
            referrals: false,
            version: LDAP_VERSION3,
            utf8: false,
            ssl_timeout: 30,
            sctrls: Vec::new(),
            cctrls: Vec::new(),
        }
    }
}

impl Opts {
    /// The client options these give.
    #[must_use]
    pub fn client(&self) -> Options {
        Options {
            connect_timeout: Duration::from_secs(u64::from(self.ssl_timeout.max(1).unsigned_abs())),
            size_limit: self.sizelimit,
            time_limit: self.timelimit,
            deref: deref(self.deref),
            referrals: self.referrals,
            hop_limit: usize::try_from(self.refhoplimit.max(0)).unwrap_or(10),
            version: self.version,
            ..Options::default()
        }
    }
}

/// The dereferencing of an `LDAP_DEREF_*` number.
#[must_use]
pub fn deref(n: i32) -> DerefAliases {
    DerefAliases::from_number(i64::from(n)).unwrap_or(DerefAliases::Never)
}

/// One `LDAP *`.
pub struct Handle {
    /// The servers to try, in order.
    pub hosts: Vec<(String, u16)>,
    /// The options.
    pub opts: Opts,
    /// LDAPS: connect with this.
    pub tls: Option<TlsWant>,
    /// The connection, once made.
    pub client: Option<Client>,
    /// The host connected to.
    pub host_used: Option<String>,
    /// The last result code.
    pub errno: i32,
    /// The last matched DN.
    pub matched: Option<String>,
    /// The last error message.
    pub errmsg: Option<String>,
    /// A secure connection's reason code.
    pub ext_error: i32,
    /// `ldap_default_dn_set`.
    pub default_dn: Option<String>,
    /// `ldap_password_set`.
    pub default_pw: Option<String>,
    /// The controls of the last bind response.
    pub bind_controls: Vec<Control>,
    /// Messages received and not yet returned, by message ID.
    pub pending: BTreeMap<i32, Vec<LdapMessage>>,
    /// Outstanding requests (message IDs).
    pub outstanding: Vec<i32>,
}

impl Handle {
    /// A handle for `hosts`, not connected.
    #[must_use]
    pub fn new(hosts: Vec<(String, u16)>, opts: Opts) -> Self {
        Self {
            hosts,
            opts,
            tls: None,
            client: None,
            host_used: None,
            errno: LDAP_SUCCESS,
            matched: None,
            errmsg: None,
            ext_error: 0,
            default_dn: None,
            default_pw: None,
            bind_controls: Vec::new(),
            pending: BTreeMap::new(),
            outstanding: Vec::new(),
        }
    }

    /// How strings cross for this handle.
    #[must_use]
    pub fn codec(&self, ccsid: u16) -> Codec {
        Codec {
            ccsid,
            utf8: self.opts.utf8,
        }
    }

    /// Records a result; returns its code.
    pub fn set_error(&mut self, code: i32, matched: Option<String>, msg: Option<String>) -> i32 {
        self.errno = code;
        self.matched = matched.filter(|s| !s.is_empty());
        self.errmsg = msg.filter(|s| !s.is_empty());
        code
    }

    /// Records a client error; returns its code.
    pub fn fail(&mut self, e: &ClientError) -> i32 {
        let (code, msg) = client_error(e);
        if let ClientError::Result(r) = e {
            return self.set_error(code, Some(r.matched_dn.clone()), Some(r.message.clone()));
        }
        if matches!(
            e,
            ClientError::Io(_) | ClientError::Closed | ClientError::Disconnected(_)
        ) {
            self.client = None;
        }
        self.set_error(code, None, Some(msg))
    }

    /// Applies the options to the connection.
    pub fn sync(&mut self) {
        let o = self.opts.client();
        if let Some(c) = &mut self.client {
            c.options.size_limit = o.size_limit;
            c.options.time_limit = o.time_limit;
            c.options.deref = o.deref;
            c.options.referrals = o.referrals;
            c.options.hop_limit = o.hop_limit;
            c.options.version = o.version;
        }
    }

    /// The connection, made on first use: each host in turn, LDAPS when
    /// the handle asks for it.
    pub fn connect(&mut self) -> Result<&mut Client, i32> {
        if self.client.is_none() {
            let mut opts = self.opts.client();
            let cfg = match &self.tls {
                Some(want) => match tls_config(want) {
                    Ok(c) => Some(c),
                    Err(e) => {
                        return Err(self.set_error(LDAP_SSL_INITIALIZE_FAILED, None, Some(e)));
                    }
                },
                None => None,
            };
            opts.tls.clone_from(&cfg);
            let mut last = None;
            for (h, p) in self.hosts.clone() {
                let r = match &cfg {
                    Some(c) => Client::connect_tls(&h, p, c.clone(), opts.clone()),
                    None => Client::connect(&h, p, opts.clone()),
                };
                match r {
                    Ok(c) => {
                        self.client = Some(c);
                        self.host_used = Some(h);
                        break;
                    }
                    Err(e) => last = Some(e),
                }
            }
            if self.client.is_none() {
                let e = last.unwrap_or(ClientError::Closed);
                let code = match e {
                    ClientError::Tls(_) => LDAP_SSL_HANDSHAKE_FAILED,
                    _ => LDAP_SERVER_DOWN,
                };
                return Err(self.set_error(code, None, Some(e.to_string())));
            }
        }
        Ok(self.client.as_mut().expect("connected"))
    }

    /// Sends a request with the handle's server controls when the call
    /// gives none; the message ID.
    pub fn send(&mut self, op: ProtocolOp, controls: Option<Vec<Control>>) -> Result<i32, i32> {
        let controls = controls.unwrap_or_else(|| self.opts.sctrls.clone());
        let c = self.connect()?;
        match c.send(op, controls) {
            Ok(id) => {
                self.outstanding.push(id);
                Ok(id)
            }
            Err(e) => Err(self.fail(&e)),
        }
    }
}

/// The result code and text of a client error.
#[must_use]
pub fn client_error(e: &ClientError) -> (i32, String) {
    let code = match e {
        ClientError::Io(_) | ClientError::Closed | ClientError::Disconnected(_) => LDAP_SERVER_DOWN,
        ClientError::Tls(_) => LDAP_SSL_HANDSHAKE_FAILED,
        ClientError::Decode(_) | ClientError::Ber(_) | ClientError::Unexpected(_) => {
            LDAP_DECODING_ERROR
        }
        ClientError::Timeout => LDAP_TIMEOUT,
        ClientError::Result(r) => i32::try_from(r.code.number()).unwrap_or(LDAP_OTHER),
        ClientError::Url(_) => LDAP_PARAM_ERROR,
        ClientError::Unsupported(_) => LDAP_NOT_SUPPORTED,
        ClientError::HopLimit => LDAP_REFERRAL_LIMIT_EXCEEDED,
    };
    (code, e.to_string())
}

/// A `BerElement`.
#[derive(Debug, Clone)]
pub enum Ber {
    /// `ldap_first_attribute`'s cursor: the next attribute's index.
    Attrs(usize),
    /// Decoding (`ber_init`).
    Read(crate::api::lber::Reading),
    /// Encoding (`ber_alloc_t`).
    Write(crate::api::lber::Writing),
}

/// The storage returned structures live in, by native address.
#[derive(Default)]
pub struct Heap {
    blocks: HashMap<u64, Box<[u8]>>,
}

impl Heap {
    /// `n` zero bytes the program can address; the pointer and the
    /// native address.
    pub fn alloc(&mut self, j: &mut Job, n: usize) -> (FatPtr, u64) {
        let data = vec![0u8; n.max(1)].into_boxed_slice();
        let native = data.as_ptr() as u64;
        let p = j.spaces.add_region(native, data.len() as u64);
        self.blocks.insert(native, data);
        (p, native)
    }

    /// An image placed in a new block; its pointer.
    pub fn place(&mut self, j: &mut Job, im: &Image) -> FatPtr {
        let (p, native) = self.alloc(j, im.bytes.len());
        let b = self.blocks.get_mut(&native).expect("just made");
        b[..im.bytes.len()].copy_from_slice(&im.bytes);
        for &(slot, target) in &im.ptrs {
            if let Ok(t) = j.spaces.add(p, i64::try_from(target).unwrap_or(0)) {
                b[slot..slot + 16].copy_from_slice(&t.to_bytes());
            }
        }
        p
    }

    /// Frees the block starting at `native`; false if it is not one.
    pub fn free(&mut self, j: &mut Job, native: u64) -> bool {
        if self.blocks.remove(&native).is_some() {
            j.spaces.kill_at(native);
            true
        } else {
            false
        }
    }

    /// Whether `native` starts a block.
    #[must_use]
    pub fn owns(&self, native: u64) -> bool {
        self.blocks.contains_key(&native)
    }

    /// Blocks held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Whether no block is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    fn drain(&mut self, j: &mut Job) {
        for (n, _) in self.blocks.drain() {
            j.spaces.kill_at(n);
        }
    }
}

/// One chain of messages from `ldap_result` (or a synchronous call).
#[derive(Debug, Clone)]
pub struct Chain {
    /// The messages; message `i` is at offset `16 * i`.
    pub msgs: Vec<LdapMessage>,
    /// The handle's key it came from.
    pub handle: u64,
    /// The pointer to the first message.
    pub ptr: FatPtr,
}

/// A job's client state.
pub struct State {
    /// The local CCSID (the job's, or `ldap_set_iconv_local_codepage`'s).
    pub ccsid: u16,
    /// Options new handles take (`ldap_set_option(NULL, ...)`).
    pub defaults: Opts,
    /// Handles by the native address of their block.
    pub handles: HashMap<u64, Handle>,
    /// Message chains by the native address of their block.
    pub chains: BTreeMap<u64, Chain>,
    /// `BerElement`s by the native address of their block.
    pub bers: HashMap<u64, Ber>,
    /// Returned storage.
    pub heap: Heap,
    /// The error of calls with no handle.
    pub errno: i32,
    /// The environment `ldap_ssl_client_init` set up.
    pub ssl: Option<TlsWant>,
    /// Texts handed out by `ldap_err2string`, kept for good.
    pub texts: HashMap<(u8, i32), FatPtr>,
    /// Strings `ldap_get_lderrno` handed out, by handle and slot, freed
    /// at the next call.
    pub held: HashMap<(u64, u8), u64>,
}

impl State {
    fn new(ccsid: u16) -> Self {
        Self {
            ccsid,
            defaults: Opts::default(),
            handles: HashMap::new(),
            chains: BTreeMap::new(),
            bers: HashMap::new(),
            heap: Heap::default(),
            errno: LDAP_SUCCESS,
            ssl: None,
            texts: HashMap::new(),
            held: HashMap::new(),
        }
    }

    /// The local codec, for calls with no handle.
    #[must_use]
    pub fn codec(&self) -> Codec {
        Codec {
            ccsid: self.ccsid,
            utf8: false,
        }
    }

    /// The codec of a handle, or the local one.
    #[must_use]
    pub fn codec_of(&self, key: Option<u64>) -> Codec {
        key.and_then(|k| self.handles.get(&k))
            .map_or(self.codec(), |h| h.codec(self.ccsid))
    }

    /// A new handle; its pointer.
    pub fn new_handle(&mut self, j: &mut Job, h: Handle) -> FatPtr {
        let (p, native) = self.heap.alloc(j, 16);
        self.handles.insert(native, h);
        p
    }

    /// Drops a handle: its connection and storage.
    pub fn drop_handle(&mut self, j: &mut Job, key: u64) -> Option<Handle> {
        let h = self.handles.remove(&key)?;
        self.heap.free(j, key);
        Some(h)
    }

    /// A chain of `msgs`; its first message's pointer.
    pub fn new_chain(&mut self, j: &mut Job, handle: u64, msgs: Vec<LdapMessage>) -> FatPtr {
        let (p, native) = self.heap.alloc(j, msgs.len().max(1) * 16);
        self.chains.insert(
            native,
            Chain {
                msgs,
                handle,
                ptr: p,
            },
        );
        p
    }

    /// The chain and message index a message pointer reaches.
    pub fn message(&self, j: &Job, p: u128) -> Option<(u64, usize)> {
        let at = crate::mem::native(j, p)?;
        let (&base, c) = self.chains.range(..=at).next_back()?;
        let off = usize::try_from(at - base).ok()?;
        (off % 16 == 0 && off / 16 < c.msgs.len()).then_some((base, off / 16))
    }

    /// The pointer of message `i` of the chain at `base`.
    pub fn message_ptr(&self, j: &Job, base: u64, i: usize) -> FatPtr {
        let Some(c) = self.chains.get(&base) else {
            return FatPtr::null();
        };
        if i >= c.msgs.len() {
            return FatPtr::null();
        }
        j.spaces
            .add(c.ptr, i64::try_from(i * 16).unwrap_or(0))
            .unwrap_or_else(|_| FatPtr::null())
    }

    /// A new `BerElement`; its pointer.
    pub fn new_ber(&mut self, j: &mut Job, b: Ber) -> FatPtr {
        let (p, native) = self.heap.alloc(j, 16);
        self.bers.insert(native, b);
        p
    }

    /// Frees a `BerElement`.
    pub fn free_ber(&mut self, j: &mut Job, native: u64) {
        if self.bers.remove(&native).is_some() {
            self.heap.free(j, native);
        }
    }

    /// Frees everything a job holds.
    pub fn clear(&mut self, j: &mut Job) {
        for (_, h) in self.handles.drain() {
            if let Some(c) = h.client {
                let _ = c.unbind();
            }
        }
        self.chains.clear();
        self.bers.clear();
        self.texts.clear();
        self.heap.drain(j);
    }
}

fn key(j: &Job) -> usize {
    std::ptr::from_ref(j) as usize
}

/// The job's state, made at first use with CCSID 37.
pub fn of(j: &Job) -> Rc<RefCell<State>> {
    STATES.with(|m| {
        Rc::clone(
            m.borrow_mut()
                .entry(key(j))
                .or_insert_with(|| Rc::new(RefCell::new(State::new(37)))),
        )
    })
}

/// Sets the job's CCSID (65535 and 0 mean 37).
pub fn set_ccsid(j: &Job, ccsid: u16) {
    let c = if ccsid == 0 || ccsid == 65535 {
        37
    } else {
        ccsid
    };
    of(j).borrow_mut().ccsid = c;
}

/// Forgets a job's state, closing its connections.
pub fn forget(j: &mut Job) {
    let st = STATES.with(|m| m.borrow_mut().remove(&key(j)));
    if let Some(s) = st
        && let Ok(mut s) = s.try_borrow_mut()
    {
        s.clear(j);
    }
}
