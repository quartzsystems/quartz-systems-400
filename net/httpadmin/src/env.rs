// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the product needs from the system, as traits: the running
//! system answers them in the `ADMIN` job (`work/qsjob`), the fixture
//! ([`crate::fixture`]) in tests and the demo server.

use std::sync::Arc;

use httpd::files::Files;
use qsweb::msg::{Message, SecondLevel};
use serde::{Deserialize, Serialize};

/// A special authority or other right an action needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// `*IOSYSCFG`: instances, ports, TLS.
    IoSysCfg,
    /// `*JOBCTL`: starting and ending server jobs.
    JobCtl,
    /// `*SECADM`.
    SecAdm,
    /// `*ALLOBJ`: certificate stores and their passwords.
    AllObj,
}

impl Need {
    /// The special authority's name.
    pub fn name(self) -> &'static str {
        match self {
            Need::IoSysCfg => "*IOSYSCFG",
            Need::JobCtl => "*JOBCTL",
            Need::SecAdm => "*SECADM",
            Need::AllObj => "*ALLOBJ",
        }
    }
}

/// An audit journal entry the product writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Audit {
    /// `PW`: a refused sign-on (user, the reason code).
    Password {
        /// The user named.
        user: String,
        /// `P` wrong password, `U` no such user, `D` disabled, ...
        code: char,
    },
    /// `AF`: a refused action.
    Refused {
        /// The user.
        user: String,
        /// The object or function.
        object: String,
        /// What was missing.
        need: String,
    },
    /// `ZC`: a change made through the product (Global settings' audit
    /// flag on).
    Change {
        /// The user.
        user: String,
        /// The object changed.
        object: String,
        /// What changed.
        what: String,
    },
}

/// Why a sign-on was refused: the message for the sign-on page.
pub type SignOnRefused = Message;

/// The system: its name and clock, users, authority, audit, messages.
pub trait System: Send + Sync {
    /// The system name (`QSHQ400`).
    fn name(&self) -> String;
    /// Seconds since 1970.
    fn now(&self) -> u64;
    /// `secs` as the date and time the user's job shows
    /// (`10/29/26`, `14:02:17`).
    fn stamp(&self, secs: u64) -> (String, String);
    /// Check a user and password with the 5250 sign-on rules
    /// (`QMAXSIGN`, disabled profiles, expired passwords). `from` is
    /// the client's address. Gives the profile name, upper case.
    fn sign_on(&self, user: &str, password: &str, from: &str) -> Result<String, SignOnRefused>;
    /// The user's special authorities (`*ALLOBJ`, `*IOSYSCFG`, ...).
    fn special(&self, user: &str) -> Vec<String>;
    /// The user's group profiles.
    fn groups(&self, user: &str) -> Vec<String>;
    /// The user's class (`*SECOFR`, `*USER`, ...).
    fn user_class(&self, user: &str) -> String;
    /// Whether a user profile exists.
    fn profile_exists(&self, user: &str) -> bool;
    /// Write an audit journal entry.
    fn audit(&self, entry: Audit);
    /// A message's text from its message file, `data` substituted.
    fn message(&self, id: &str, data: &[&str]) -> Option<SecondLevel>;
    /// This job, qualified (`123457/QTMHHTTP/ADMIN`).
    fn job(&self) -> String;
    /// The addresses of this system's TCP/IP interfaces.
    fn interfaces(&self) -> Vec<String>;
    /// Whether licensed program `product`'s `*BASE` is installed.
    fn product_installed(&self, product: &str) -> bool {
        let _ = product;
        true
    }
}

/// How an instance ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndHow {
    /// `OPTION(*CNTRLD) DELAY(n)`: requests in progress get `n` seconds.
    Controlled(u32),
    /// `OPTION(*IMMED)`.
    Immediate,
}

/// An instance's active job in `QHTTPSVR`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpJob {
    /// The instance (the job name).
    pub instance: String,
    /// The job number, six digits.
    pub number: String,
    /// The job's user (`QTMHHTTP`).
    pub user: String,
    /// Seconds since 1970 when it started.
    pub started: u64,
    /// Client connections open now.
    pub connections: u32,
}

impl HttpJob {
    /// `number/user/name`.
    pub fn qualified(&self) -> String {
        format!("{}/{}/{}", self.number, self.user, self.instance)
    }
}

/// The instances' jobs: which run, and starting and ending them.
pub trait Jobs: Send + Sync {
    /// Every active job of subsystem `QHTTPSVR` that runs an instance.
    fn active(&self) -> Vec<HttpJob>;
    /// `STRTCPSVR SERVER(*HTTP) HTTPSVR(instance)` as `user`. Gives the
    /// completion message.
    fn start(&self, user: &str, instance: &str) -> Result<Message, Message>;
    /// `ENDTCPSVR SERVER(*HTTP) HTTPSVR(instance)` as `user`.
    fn end(&self, user: &str, instance: &str, how: EndHow) -> Result<Message, Message>;
    /// Give server user profile `profile` what an instance rooted at
    /// `root` needs: read on its tree, write on its logs and on the
    /// registry. Done as the files' owner (the `ADMIN` job); the page
    /// has checked the user's authority already.
    fn authorize(&self, root: &str, profile: &str) -> Result<(), Message> {
        let _ = (root, profile);
        Ok(())
    }
}

/// What a certificate store is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoreType {
    /// `*SYSTEM`: the certificates TCP servers present.
    System,
    /// `*OBJECTSIGNING`: certificates that sign objects.
    ObjectSigning,
    /// `*SIGNATUREVERIFICATION`: certificates that verify signed objects.
    SignatureVerification,
    /// A store someone made, named by its path.
    Other,
}

impl StoreType {
    /// The Type column.
    pub fn label(self) -> &'static str {
        match self {
            StoreType::System => "System",
            StoreType::ObjectSigning => "Object signing",
            StoreType::SignatureVerification => "Signature verification",
            StoreType::Other => "Other system store",
        }
    }
}

/// A certificate store as the stores page lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreInfo {
    /// `*SYSTEM`, `*OBJECTSIGNING`, `*SIGNATUREVERIFICATION`, or the
    /// name given when the store was made (`WEBSTORE`). Routes and every
    /// other method name the store by this.
    pub name: String,
    /// What it is for.
    pub kind: StoreType,
    /// The store file in the IFS (`/QIBM/UserData/ICSS/Cert/Server/DEFAULT.KDB`).
    pub path: String,
    /// The certificates the store is for: its CA certificates in a
    /// signature verification store, every other entry elsewhere.
    pub certificates: usize,
    /// The default certificate's label.
    pub default: Option<String>,
    /// Whether the store has a stash that opens it. Without one its
    /// certificates are not counted and changing its password takes the
    /// current one.
    #[serde(default)]
    pub stashed: bool,
}

/// What a store entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryType {
    /// A server certificate with its private key.
    Server,
    /// A client certificate with its private key.
    Client,
    /// A certificate authority's certificate.
    Ca,
    /// A key waiting for its certificate (a request went out).
    Request,
}

/// One store entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertRow {
    /// The label, unique in the store.
    pub label: String,
    /// What it is.
    pub kind: EntryType,
    /// The subject, RFC 4514 (`CN=a,O=b,C=US`); a request's from its CSR.
    pub subject: String,
    /// The issuer, RFC 4514 ("" for a request).
    pub issuer: String,
    /// The certificate signs itself.
    pub self_signed: bool,
    /// Valid from, seconds since 1970 (0 for a request).
    pub not_before: u64,
    /// Valid to, seconds since 1970 (0 for a request).
    pub not_after: u64,
    /// `RSA 2048`, `ECDSA P-256`.
    pub key: String,
    /// A trusted CA.
    pub trusted: bool,
    /// The store's default certificate.
    pub is_default: bool,
    /// When the entry was added, seconds since 1970.
    pub added: u64,
}

/// One entry with everything the certificate page shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertDetail {
    /// The entry.
    pub row: CertRow,
    /// The serial number, upper-case hex ("" for a request).
    pub serial: String,
    /// The SHA-256 fingerprint as `AB:CD:...` ("" for a request).
    pub fingerprint: String,
    /// Subject alternative names.
    pub san: Vec<String>,
    /// The certificate as PEM, or a request's CSR.
    pub pem: String,
}

/// A key type and size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeySpec {
    /// RSA, 2048 bits.
    Rsa2048,
    /// RSA, 3072 bits.
    Rsa3072,
    /// RSA, 4096 bits.
    Rsa4096,
    /// ECDSA on P-256 (signed with SHA-256).
    P256,
    /// ECDSA on P-384 (signed with SHA-384).
    P384,
}

/// A distinguished name as the create wizard asks for it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dn {
    /// Common name (required).
    pub cn: String,
    /// Organizational unit.
    pub ou: String,
    /// Organization.
    pub o: String,
    /// Locality.
    pub l: String,
    /// State or province.
    pub st: String,
    /// Two-letter country.
    pub c: String,
}

/// Who signs a new certificate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Signing {
    /// The certificate signs itself.
    SelfSigned,
    /// A certificate authority whose key this system holds: a name from
    /// [`Certs::signers`].
    Ca(String),
    /// Nobody yet: a key and a certificate signing request for an
    /// external CA.
    Request,
}

/// A new certificate (`QSCRTCRT`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewCert {
    /// The store, as [`StoreInfo::name`].
    pub store: String,
    /// The label.
    pub label: String,
    /// The key.
    pub key: KeySpec,
    /// Days of validity from now.
    pub days: u32,
    /// The subject.
    pub subject: Dn,
    /// Subject alternative names (DNS names or addresses).
    pub san: Vec<String>,
    /// For servers (server and client authentication); `false` makes a
    /// client certificate.
    pub server: bool,
    /// Who signs it.
    pub signing: Signing,
    /// Make it the store's default certificate.
    pub make_default: bool,
}

/// Where an imported certificate comes from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportSource {
    /// An uploaded file's bytes.
    Data(Vec<u8>),
    /// A stream file on this system, read with the user's authority.
    Path(String),
}

/// The format of an imported file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportFormat {
    /// PEM when the data is text with a `-----BEGIN` line, DER when it
    /// is one certificate, else PKCS#12.
    Auto,
    /// PEM.
    Pem,
    /// PKCS#12.
    Pkcs12,
}

/// An import (`QSIMPCRT`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    /// The store, as [`StoreInfo::name`].
    pub store: String,
    /// The file.
    pub source: ImportSource,
    /// Its format.
    pub format: ImportFormat,
    /// The PKCS#12 (or encrypted key) password; used for this call only.
    pub password: Option<String>,
    /// The label; `None` uses the certificate's common name.
    pub label: Option<String>,
    /// Import certificate authority certificates (trusted) rather than a
    /// certificate with its private key.
    pub ca: bool,
    /// Make the imported certificate the store's default.
    pub make_default: bool,
}

/// An application ID and its certificate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    /// `QS400_HTTP_SERVER_ADMIN`.
    pub id: String,
    /// A server (else a client).
    pub server: bool,
    /// The description.
    pub description: String,
    /// The assigned certificate's label in `*SYSTEM`.
    pub label: Option<String>,
}

/// Why a certificate request failed. The text never holds a password.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CertFailure {
    /// The password does not open the store (its name).
    BadPassword(String),
    /// No such store.
    NoStore(String),
    /// A store exists there already.
    StoreExists(String),
    /// No entry has this label.
    NoLabel(String),
    /// The label is in use.
    LabelInUse(String),
    /// No such application ID.
    NoApp(String),
    /// Anything else, as a sentence.
    Invalid(String),
}

impl std::fmt::Display for CertFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CertFailure::BadPassword(s) => write!(f, "the password is not correct for certificate store {s}"),
            CertFailure::NoStore(s) => write!(f, "certificate store {s} does not exist"),
            CertFailure::StoreExists(s) => write!(f, "certificate store {s} exists already"),
            CertFailure::NoLabel(l) => write!(f, "no certificate has label {l}"),
            CertFailure::LabelInUse(l) => write!(f, "label {l} is in use"),
            CertFailure::NoApp(a) => write!(f, "application ID {a} is not registered"),
            CertFailure::Invalid(t) => f.write_str(t),
        }
    }
}

/// The `CHGAUT` commands [`Jobs::authorize`] runs: `/www` (to reach the
/// tree and replace the registry), the tree, the logs, the registry.
pub fn authorize_commands(root: &str, profile: &str) -> Vec<String> {
    let root = root.trim_end_matches('/');
    let reg = httpd::registry::PATH;
    vec![
        format!("CHGAUT OBJ('/www') USER({profile}) DTAAUT(*RWX) OBJAUT(*NONE)"),
        format!("CHGAUT OBJ('{root}') USER({profile}) DTAAUT(*RX) OBJAUT(*NONE) SUBTREE(*ALL)"),
        format!("CHGAUT OBJ('{root}/logs') USER({profile}) DTAAUT(*RWX) OBJAUT(*NONE) SUBTREE(*ALL)"),
        format!("CHGAUT OBJ('{reg}') USER({profile}) DTAAUT(*RW) OBJAUT(*NONE)"),
    ]
}

/// The certificate manager (`net/certs` through the storage service:
/// `QSCRTCRTS`, `QSCRTCRT`, `QSIMPCRT`, ...). Pages check `*ALLOBJ` and
/// `*SECADM` before calling a method that changes anything; `user` is
/// the signed-on user, for the implementation's own check and its
/// audit. Stores open through their stash; store passwords given to a
/// method are used for that call only and never logged, sent in a
/// message or kept.
pub trait Certs: Send + Sync {
    /// Every store: `*SYSTEM`, `*OBJECTSIGNING`,
    /// `*SIGNATUREVERIFICATION` (those that exist), then the others the
    /// system knows, in the order they were made.
    fn stores(&self) -> Vec<StoreInfo>;
    /// Every entry of `store`, in label order.
    fn entries(&self, store: &str) -> Result<Vec<CertRow>, CertFailure>;
    /// One entry with its PEM and fingerprint.
    fn detail(&self, store: &str, label: &str) -> Result<CertDetail, CertFailure>;
    /// The entry as a PEM file: the certificate, with `chain` its
    /// issuers up to a root the store holds; for a request, its CSR.
    fn export(&self, store: &str, label: &str, chain: bool) -> Result<Vec<u8>, CertFailure>;
    /// Make an empty store `name` at `path` with `password`, its
    /// password stashed (`QSCRTCRTS STASH(*YES)`).
    fn create_store(&self, user: &str, name: &str, path: &str, password: &str) -> Result<(), CertFailure>;
    /// Change a store's password; `old` must open it, or is empty when
    /// the store's stash opens it (the user manages certificates, so
    /// has `*SECADM` and `*ALLOBJ`). The stash follows.
    fn change_password(&self, user: &str, store: &str, old: &str, new: &str) -> Result<(), CertFailure>;
    /// Delete a store made by someone (`QSDLTCRTS`); the system's own
    /// stores are refused.
    fn delete_store(&self, user: &str, store: &str) -> Result<(), CertFailure>;
    /// The certificate authorities whose keys this system holds, by the
    /// name [`Signing::Ca`] takes (the label of their CA entry in
    /// `*SYSTEM`).
    fn signers(&self) -> Vec<String>;
    /// Make a certificate or a request (`QSCRTCRT`).
    fn create(&self, user: &str, new: &NewCert) -> Result<CertDetail, CertFailure>;
    /// Import (`QSIMPCRT`); gives the labels added.
    fn import(&self, user: &str, import: &Import) -> Result<Vec<String>, CertFailure>;
    /// Make `label` the store's default certificate.
    fn set_default(&self, user: &str, store: &str, label: &str) -> Result<(), CertFailure>;
    /// Remove an entry (`QSDLTCRT`); application IDs naming it lose it.
    fn remove(&self, user: &str, store: &str, label: &str) -> Result<(), CertFailure>;
    /// The application IDs of the system's TLS servers and clients and
    /// of the HTTP instances, in the order the system lists them.
    fn apps(&self) -> Vec<AppInfo>;
    /// Assign a `*SYSTEM` certificate to an application ID, or with
    /// `None` remove its assignment.
    fn assign(&self, user: &str, app: &str, label: Option<&str>) -> Result<(), CertFailure>;
    /// Copy entry `label` of `from`, with its private key, into `to`
    /// under the same label.
    fn copy(&self, user: &str, from: &str, label: &str, to: &str) -> Result<(), CertFailure>;
    /// The entry with its private key and issuers as a PKCS#12 file
    /// sealed with `password`.
    fn export_pkcs12(&self, user: &str, store: &str, label: &str, password: &str) -> Result<Vec<u8>, CertFailure>;
    /// The label assigned to application ID `app` (or its old ID).
    fn app_label(&self, app: &str) -> Option<String> {
        let want = certs::apps::canonical(app);
        self.apps().into_iter().find(|a| a.id == want).and_then(|a| a.label)
    }
    /// The `*SYSTEM` server certificates application `app` may present,
    /// the one assigned to it first: label and expiry (seconds since
    /// 1970). The TLS page of an instance shows them.
    fn for_app(&self, app: &str) -> Vec<(String, u64)> {
        let assigned = self.app_label(app);
        let mut out: Vec<(String, u64)> = self
            .entries("*SYSTEM")
            .unwrap_or_default()
            .into_iter()
            .filter(|e| e.kind == EntryType::Server)
            .map(|e| (e.label, e.not_after))
            .collect();
        if let Some(a) = assigned
            && let Some(i) = out.iter().position(|(l, _)| *l == a)
        {
            let first = out.remove(i);
            out.insert(0, first);
        }
        out
    }
}

/// Everything the product runs over.
#[derive(Clone)]
pub struct Env {
    /// The system.
    pub system: Arc<dyn System>,
    /// The instances' jobs.
    pub jobs: Arc<dyn Jobs>,
    /// The IFS, as the administration job sees it.
    pub files: Arc<dyn Files>,
    /// Certificates.
    pub certs: Arc<dyn Certs>,
}
