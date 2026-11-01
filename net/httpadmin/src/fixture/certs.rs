// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sample system's certificates: real stores made by `net/certs`
//! (`certs::admin`, the local CA, a second CA whose key the system
//! holds) in memory, with a test sealer and cheap key derivation. The
//! stores are built once per process and copied into each fixture.

use std::sync::{Arc, Mutex, OnceLock};

use certs::admin::entries::{self, Signer};
use certs::admin::{Ctx, Keystore, appids, keystores};
use certs::apps::AppKind;
use certs::ca;
use certs::crypto::kdf::Argon2Params;
use certs::crypto::keys::{KeyAlgorithm, PrivateKey};
use certs::files::{MemFiles, StoreFiles};
use certs::paths::{LOCAL_CA_STORE, OBJECT_SIGNING_STORE, SYSTEM_STORE};
use certs::stash::KeySealer;
use certs::store::{CertStore, EntryKind, StoreKind};
use certs::x509::CertInfo;
use certs::x509::build::{self, CertKind, CertSpec, Subject};
use certs::{CertError, pem};

use crate::env::{
    AppInfo, CertDetail, CertFailure, CertRow, Certs, EntryType, Import, ImportFormat, ImportSource, KeySpec, NewCert,
    Signing, StoreInfo, StoreType,
};
use crate::util::{days, secs};

/// The `*SIGNATUREVERIFICATION` store.
pub const SIGNATURE_VERIFICATION_STORE: &str = "/QIBM/UserData/ICSS/Cert/Signing/SIGNVER.KDB";
/// The second CA's store: a CA whose key this system holds.
pub const INTERNAL_CA_STORE: &str = "/QIBM/UserData/ICSS/Cert/CertAuth/QUARTZCA.KDB";
/// The password of every sample store (stashed; never shown).
const STORE_PASSWORD: &str = "fixture-store-1";

/// Key derivation cheap enough for tests.
const FAST: Argon2Params = Argon2Params {
    m_cost: 64,
    t_cost: 1,
    p_cost: 1,
};

/// The application IDs the sample system shows, in its order.
const SHOWN_APPS: &[&str] = &[
    "QIBM_HTTP_SERVER_ADMIN",
    "QIBM_HTTP_SERVER_NAVIGATOR",
    "QIBM_HTTP_SERVER_ACCESSWEB",
    "QIBM_HTTP_SERVER_WEBSTORE",
    "QIBM_HTTP_SERVER_API",
    "QIBM_HTTP_SERVER_INTRANET",
    "QIBM_QTV_TELNET_SERVER",
    "QIBM_QTMF_FTP_SERVER",
    "QIBM_QRW_SERVER_DDM_DRDA",
    "QIBM_OS400_QZBS_SVR_CENTRAL",
    "QS400_UPDATE_CLIENT",
];

/// The sample certificate manager.
pub struct FixtureCerts {
    /// The store files.
    pub files: MemFiles,
    sealer: KeySealer,
    /// Stores made by someone: name and path, in the order made.
    others: Mutex<Vec<(String, String)>>,
    /// CAs that sign: name (their label in `*SYSTEM`) and store path.
    signers: Vec<(String, String)>,
    /// The IFS, for imports from a path.
    ifs: Arc<dyn httpd::files::Files>,
}

fn subject(cn: &str) -> Subject {
    Subject {
        common_name: cn.into(),
        organization: Some("Quartz Systems".into()),
        country: Some("US".into()),
        ..Subject::default()
    }
}

/// A server certificate valid from `from` (y, m, d) to `to`, at the
/// fixture clock's time of day.
fn spec(cn: &str, kind: CertKind, from: (i64, u32, u32), to: (i64, u32, u32)) -> CertSpec {
    let n = days(to.0, to.1, to.2) - days(from.0, from.1, from.2);
    CertSpec {
        subject: subject(cn),
        kind,
        san: if kind == CertKind::Server { vec![cn.to_string()] } else { Vec::new() },
        not_before: secs(from.0, from.1, from.2, 14, 11, 2),
        days: u32::try_from(n).unwrap_or(1),
    }
}

/// Build the sample stores into `files`.
#[allow(clippy::too_many_lines)]
fn build_stores(files: &MemFiles, sealer: &KeySealer) -> Result<(), CertError> {
    let at = |now: u64| Ctx {
        files,
        sealer: Some(sealer),
        now,
        params: FAST,
    };
    let t0 = secs(2024, 1, 2, 9, 0, 0);
    let c = at(t0);
    let sys = Keystore::new(SYSTEM_STORE, STORE_PASSWORD);
    keystores::create(&c, &sys, true, false)?;
    keystores::create(&c, &Keystore::new(OBJECT_SIGNING_STORE, STORE_PASSWORD), true, false)?;
    keystores::create(&c, &Keystore::new(SIGNATURE_VERIFICATION_STORE, STORE_PASSWORD), true, false)?;
    keystores::create(&c, &Keystore::new("/www/webstore/conf/webstore.kdb", STORE_PASSWORD), true, false)?;

    // The local CA, trusted by *SYSTEM.
    keystores::create_local_ca(&c, STORE_PASSWORD, &subject("QSHQ400 Local CA"), KeyAlgorithm::EcdsaP256, 3650, true)?;
    let sys = Keystore::new(SYSTEM_STORE, "");
    let rename = |from: &str, to: &str| {
        entries::change(
            &c,
            &sys,
            from,
            &entries::Change {
                new_label: Some(to.into()),
                ..entries::Change::default()
            },
        )
    };
    rename(ca::CA_LABEL, "QSHQ400 Local CA")?;

    // A second CA whose key the system holds.
    let mut internal = CertStore::create_with(files, INTERNAL_CA_STORE, STORE_PASSWORD, StoreKind::CertAuth, t0, FAST)?;
    let ca_spec = CertSpec {
        subject: subject("Quartz Internal Certificate Authority"),
        kind: CertKind::Ca,
        san: Vec::new(),
        not_before: t0,
        days: 3650,
    };
    internal.create_self_signed(ca::CA_LABEL, &ca_spec, KeyAlgorithm::EcdsaP256)?;
    internal.save(files)?;
    internal.stash(files, STORE_PASSWORD, sealer)?;

    // *SYSTEM's six certificates, in the order they were added.
    let issue = |label: &str, s: &CertSpec, alg: KeyAlgorithm| -> Result<(), CertError> {
        let ca_store = CertStore::open_stashed(files, INTERNAL_CA_STORE, sealer)?;
        let mut target = c.open(&sys)?;
        ca::issue(&ca_store, &mut target, label, s, alg)?;
        target.save(files)
    };
    let server = CertKind::Server;
    issue("*.quartz.example", &spec("*.quartz.example", server, (2025, 1, 2), (2028, 1, 2)), KeyAlgorithm::Rsa2048)?;
    rename(ca::CA_LABEL, "Quartz Internal CA")?;
    issue(
        "webstore.quartz.example",
        &spec("webstore.quartz.example", server, (2025, 3, 14), (2027, 3, 14)),
        KeyAlgorithm::Rsa2048,
    )?;
    issue(
        "api.quartz.example",
        &spec("api.quartz.example", server, (2025, 5, 1), (2026, 11, 30)),
        KeyAlgorithm::EcdsaP256,
    )?;
    entries::create(
        &c,
        &sys,
        "QSHQ400 self-signed",
        &spec("QSHQ400", server, (2025, 6, 1), (2031, 10, 29)),
        KeyAlgorithm::Rsa2048,
        &Signer::SelfSigned,
    )?;
    issue(
        "navigator-2025",
        &spec("qshq400.quartz.example", server, (2025, 7, 1), (2026, 9, 1)),
        KeyAlgorithm::Rsa2048,
    )?;
    issue(
        "old-webstore",
        &spec("webstore.quartz.example", server, (2025, 8, 1), (2026, 3, 14)),
        KeyAlgorithm::Rsa2048,
    )?;
    let dft = entries::Change {
        default: true,
        ..entries::Change::default()
    };
    entries::change(&c, &sys, "webstore.quartz.example", &dft)?;

    // The signing stores.
    entries::create(
        &c,
        &Keystore::new(OBJECT_SIGNING_STORE, ""),
        "QSHQ400 code signing",
        &spec("QSHQ400 code signing", CertKind::Client, (2025, 2, 1), (2028, 2, 1)),
        KeyAlgorithm::EcdsaP256,
        &Signer::SelfSigned,
    )?;
    let mut ver = c.open(&Keystore::new(SIGNATURE_VERIFICATION_STORE, ""))?;
    for (i, cn) in ["Quartz Systems code signing", "QS/400 release signing", "Partner code signing"].iter().enumerate() {
        let key = PrivateKey::generate(KeyAlgorithm::EcdsaP256)?;
        let mut s = spec(cn, CertKind::Ca, (2025, 2, 1), (2030, 2, 1));
        s.not_before += i as u64;
        ver.add_ca(cn, build::self_signed(&s, &key)?, true, s.not_before)?;
    }
    ver.save(files)?;

    // WEBSTORE's own store: a development certificate and a request.
    let web = Keystore::new("/www/webstore/conf/webstore.kdb", "");
    entries::create(
        &c,
        &web,
        "webstore-dev",
        &spec("webstore-dev.quartz.example", server, (2026, 1, 5), (2027, 1, 5)),
        KeyAlgorithm::EcdsaP256,
        &Signer::SelfSigned,
    )?;
    entries::create(
        &c,
        &web,
        "webstore-2027-request",
        &spec("webstore.quartz.example", server, (2026, 10, 20), (2027, 10, 20)),
        KeyAlgorithm::EcdsaP256,
        &Signer::Request,
    )?;

    // Application IDs: the HTTP instances, the update client, the
    // descriptions and the assignments.
    for (id, kind, text) in [
        ("QIBM_HTTP_SERVER_NAVIGATOR", AppKind::Server, "Navigator (port 2005)"),
        ("QIBM_HTTP_SERVER_ACCESSWEB", AppKind::Server, "Access for Web (port 2006)"),
        ("QIBM_HTTP_SERVER_WEBSTORE", AppKind::Server, "Instance WEBSTORE"),
        ("QIBM_HTTP_SERVER_API", AppKind::Server, "Instance API"),
        ("QIBM_HTTP_SERVER_INTRANET", AppKind::Server, "Instance INTRANET"),
        ("QS400_UPDATE_CLIENT", AppKind::Client, "Fix download from Quartz update server"),
    ] {
        appids::add(&c, id, kind, text)?;
    }
    for (id, text) in [
        ("QIBM_HTTP_SERVER_ADMIN", "HTTP administration (port 2001)"),
        ("QIBM_QTV_TELNET_SERVER", "Telnet server (TLS port 992)"),
        ("QIBM_QTMF_FTP_SERVER", "FTP server"),
        ("QIBM_QRW_SERVER_DDM_DRDA", "DDM/DRDA server"),
        ("QIBM_OS400_QZBS_SVR_CENTRAL", "Host servers (central, database, signon)"),
    ] {
        let ch = appids::Change {
            text: Some(text.into()),
            ..appids::Change::default()
        };
        appids::change(&c, id, &ch)?;
    }
    for (id, label) in [
        ("QIBM_HTTP_SERVER_ADMIN", "QSHQ400 self-signed"),
        ("QIBM_HTTP_SERVER_NAVIGATOR", "*.quartz.example"),
        ("QIBM_HTTP_SERVER_ACCESSWEB", "*.quartz.example"),
        ("QIBM_HTTP_SERVER_WEBSTORE", "webstore.quartz.example"),
        ("QIBM_HTTP_SERVER_API", "api.quartz.example"),
        ("QIBM_HTTP_SERVER_INTRANET", "*.quartz.example"),
        ("QIBM_QTV_TELNET_SERVER", "*.quartz.example"),
        ("QIBM_QRW_SERVER_DDM_DRDA", "*.quartz.example"),
        ("QIBM_OS400_QZBS_SVR_CENTRAL", "*.quartz.example"),
        ("QS400_UPDATE_CLIENT", "QSHQ400 self-signed"),
    ] {
        appids::assign(&c, id, Some(label))?;
    }
    Ok(())
}

/// The sample stores' files, built once.
fn snapshot() -> &'static Vec<(String, Vec<u8>)> {
    static SNAP: OnceLock<Vec<(String, Vec<u8>)>> = OnceLock::new();
    SNAP.get_or_init(|| {
        let files = MemFiles::new();
        build_stores(&files, &KeySealer::for_tests()).expect("fixture certificate stores");
        files
            .paths()
            .into_iter()
            .map(|p| {
                let b = files.read(&p).ok().flatten().unwrap_or_default();
                (p, b)
            })
            .collect()
    })
}

fn fail(e: CertError, store: &str) -> CertFailure {
    match e {
        CertError::NotFound(_) => CertFailure::NoStore(store.into()),
        CertError::Exists(_) => CertFailure::StoreExists(store.into()),
        CertError::BadPassword => CertFailure::BadPassword(store.into()),
        CertError::LabelNotFound(l) => CertFailure::NoLabel(l),
        CertError::LabelExists(l) => CertFailure::LabelInUse(l),
        CertError::NotRegistered(a) => CertFailure::NoApp(a),
        CertError::Invalid(t) => CertFailure::Invalid(t),
        other => CertFailure::Invalid(other.to_string()),
    }
}

/// `RSA 2048`, `ECDSA P-256`.
pub fn key_name(a: KeyAlgorithm) -> String {
    if a.is_rsa() {
        format!("RSA {}", a.bits())
    } else {
        format!("ECDSA P-{}", a.bits())
    }
}

fn kind_of(k: EntryKind) -> EntryType {
    match k {
        EntryKind::Server => EntryType::Server,
        EntryKind::Client => EntryType::Client,
        EntryKind::Ca => EntryType::Ca,
        EntryKind::Request => EntryType::Request,
    }
}

impl FixtureCerts {
    /// The sample stores; imports from a path read `ifs`.
    pub fn new(ifs: Arc<dyn httpd::files::Files>) -> Self {
        let files = MemFiles::new();
        for (p, b) in snapshot() {
            files.write(p, b).expect("memory");
        }
        Self {
            files,
            sealer: KeySealer::for_tests(),
            others: Mutex::new(vec![("WEBSTORE".into(), "/www/webstore/conf/webstore.kdb".into())]),
            signers: vec![
                ("Quartz Internal CA".into(), INTERNAL_CA_STORE.into()),
                ("QSHQ400 Local CA".into(), LOCAL_CA_STORE.into()),
            ],
            ifs,
        }
    }

    fn ctx(&self) -> Ctx<'_> {
        Ctx {
            files: &self.files,
            sealer: Some(&self.sealer),
            now: super::now(),
            params: FAST,
        }
    }

    /// A store's type and path by name.
    fn find(&self, name: &str) -> Result<(StoreType, String), CertFailure> {
        let n = name.trim();
        let fixed = [
            ("*SYSTEM", StoreType::System, SYSTEM_STORE),
            ("*OBJECTSIGNING", StoreType::ObjectSigning, OBJECT_SIGNING_STORE),
            ("*SIGNATUREVERIFICATION", StoreType::SignatureVerification, SIGNATURE_VERIFICATION_STORE),
        ];
        if let Some((_, t, p)) = fixed.iter().find(|f| f.0.eq_ignore_ascii_case(n)) {
            return Ok((*t, (*p).to_string()));
        }
        self.others
            .lock()
            .expect("stores")
            .iter()
            .find(|(o, _)| o.eq_ignore_ascii_case(n))
            .map(|(_, p)| (StoreType::Other, p.clone()))
            .ok_or_else(|| CertFailure::NoStore(n.to_string()))
    }

    fn keystore(&self, name: &str) -> Result<Keystore, CertFailure> {
        let (_, path) = self.find(name)?;
        Ok(Keystore::new(&path, ""))
    }

    fn open(&self, name: &str) -> Result<CertStore, CertFailure> {
        let ks = self.keystore(name)?;
        self.ctx().open(&ks).map_err(|e| fail(e, name))
    }

    fn row(store: &CertStore, label: &str) -> Result<(CertRow, Option<CertInfo>), CertError> {
        let e = store.get(label)?;
        let info = e.info()?;
        let is_default = store.default_label() == Some(label);
        let row = if let Some(i) = &info {
            CertRow {
                label: e.label.clone(),
                kind: kind_of(e.kind),
                subject: i.subject.clone(),
                issuer: i.issuer.clone(),
                self_signed: i.self_signed,
                not_before: i.not_before,
                not_after: i.not_after,
                key: i.key_algorithm.map_or_else(|| format!("{} bits", i.key_bits), key_name),
                trusted: e.trusted,
                is_default,
                added: e.added,
            }
        } else {
            let subject = e
                .csr
                .as_deref()
                .map(build::request_info)
                .transpose()?
                .map(|r| r.subject)
                .unwrap_or_default();
            CertRow {
                label: e.label.clone(),
                kind: kind_of(e.kind),
                subject,
                issuer: String::new(),
                self_signed: false,
                not_before: 0,
                not_after: 0,
                key: key_name(e.private_key()?.algorithm()),
                trusted: false,
                is_default,
                added: e.added,
            }
        };
        Ok((row, info))
    }

    fn store_info(&self, name: &str, kind: StoreType, path: &str) -> Option<StoreInfo> {
        let store = self.ctx().open(&Keystore::new(path, "")).ok()?;
        let certificates = store
            .entries()
            .iter()
            .filter(|e| (e.kind == EntryKind::Ca) == (kind == StoreType::SignatureVerification))
            .count();
        Some(StoreInfo {
            name: name.into(),
            kind,
            path: path.into(),
            certificates,
            default: store.default_label().map(ToString::to_string),
        })
    }

    /// The common name of the certificate in an import, for its label.
    fn common_name(data: &[u8], password: Option<&str>) -> Option<String> {
        if let Some(t) = std::str::from_utf8(data).ok().filter(|t| t.contains("-----BEGIN ")) {
            let first = pem::blocks(t).ok()?.into_iter().find(|b| b.label == "CERTIFICATE")?;
            return CertInfo::from_der(&first.der).ok()?.common_name;
        }
        if let Ok(i) = CertInfo::from_der(data) {
            return i.common_name;
        }
        let c = certs::pkcs12::read(data, password.unwrap_or("")).ok()?;
        let der = match c.identity().ok()? {
            Some(id) => id.cert,
            None => c.certs.first()?.der.clone(),
        };
        CertInfo::from_der(&der).ok()?.common_name
    }
}

impl Certs for FixtureCerts {
    fn stores(&self) -> Vec<StoreInfo> {
        let mut out: Vec<StoreInfo> = [
            ("*SYSTEM", StoreType::System, SYSTEM_STORE),
            ("*OBJECTSIGNING", StoreType::ObjectSigning, OBJECT_SIGNING_STORE),
            ("*SIGNATUREVERIFICATION", StoreType::SignatureVerification, SIGNATURE_VERIFICATION_STORE),
        ]
        .iter()
        .filter_map(|(n, k, p)| self.store_info(n, *k, p))
        .collect();
        let others = self.others.lock().expect("stores").clone();
        out.extend(others.iter().filter_map(|(n, p)| self.store_info(n, StoreType::Other, p)));
        out
    }

    fn entries(&self, store: &str) -> Result<Vec<CertRow>, CertFailure> {
        let s = self.open(store)?;
        s.entries()
            .iter()
            .map(|e| Self::row(&s, &e.label).map(|r| r.0))
            .collect::<Result<_, _>>()
            .map_err(|e| fail(e, store))
    }

    fn detail(&self, store: &str, label: &str) -> Result<CertDetail, CertFailure> {
        let s = self.open(store)?;
        let (row, info) = Self::row(&s, label).map_err(|e| fail(e, store))?;
        let e = s.get(label).map_err(|e| fail(e, store))?;
        let pem = match (&e.cert, &e.csr) {
            (Some(der), _) => pem::encode("CERTIFICATE", der),
            (None, Some(csr)) => pem::encode("CERTIFICATE REQUEST", csr),
            _ => String::new(),
        };
        Ok(CertDetail {
            row,
            serial: info.as_ref().map(|i| i.serial.clone()).unwrap_or_default(),
            fingerprint: info.as_ref().map(CertInfo::sha256_colons).unwrap_or_default(),
            san: info.map(|i| i.san).unwrap_or_default(),
            pem,
        })
    }

    fn export(&self, store: &str, label: &str, chain: bool) -> Result<Vec<u8>, CertFailure> {
        let s = self.open(store)?;
        let e = s.get(label).map_err(|e| fail(e, store))?;
        if e.kind == EntryKind::Request {
            let ks = self.keystore(store)?;
            return entries::export(&self.ctx(), &ks, label, &entries::ExportFormat::Request).map_err(|e| fail(e, store));
        }
        let ders = if chain {
            let mut c = s.chain(label).map_err(|e| fail(e, store))?;
            // The root that issued the last one, when the store has it.
            let last = c.last().and_then(|d| CertInfo::from_der(d).ok());
            if let Some(l) = last.filter(|l| !l.self_signed) {
                let root = s.entries().iter().find_map(|e| {
                    let i = e.info().ok().flatten()?;
                    (e.kind == EntryKind::Ca && i.self_signed && i.subject == l.issuer).then(|| e.cert.clone()).flatten()
                });
                c.extend(root);
            }
            c
        } else {
            e.cert.clone().into_iter().collect()
        };
        Ok(ders.iter().map(|d| pem::encode("CERTIFICATE", d)).collect::<String>().into_bytes())
    }

    fn create_store(&self, _user: &str, name: &str, path: &str, password: &str) -> Result<(), CertFailure> {
        if self.find(name).is_ok() || self.stores().iter().any(|s| s.path == path) {
            return Err(CertFailure::StoreExists(name.into()));
        }
        keystores::create(&self.ctx(), &Keystore::new(path, password), true, false).map_err(|e| fail(e, name))?;
        self.others.lock().expect("stores").push((name.into(), path.into()));
        Ok(())
    }

    fn change_password(&self, _user: &str, store: &str, old: &str, new: &str) -> Result<(), CertFailure> {
        let (_, path) = self.find(store)?;
        if old.is_empty() {
            return Err(CertFailure::BadPassword(store.into()));
        }
        keystores::change_password(&self.ctx(), &Keystore::new(&path, old), new).map_err(|e| fail(e, store))?;
        Ok(())
    }

    fn delete_store(&self, _user: &str, store: &str) -> Result<(), CertFailure> {
        let (kind, path) = self.find(store)?;
        if kind != StoreType::Other {
            return Err(CertFailure::Invalid(format!("{store} is one of the system's own stores and cannot be deleted")));
        }
        keystores::delete(&self.ctx(), &Keystore::new(&path, "")).map_err(|e| fail(e, store))?;
        self.others.lock().expect("stores").retain(|(n, _)| !n.eq_ignore_ascii_case(store));
        Ok(())
    }

    fn signers(&self) -> Vec<String> {
        self.signers.iter().map(|s| s.0.clone()).collect()
    }

    fn create(&self, _user: &str, new: &NewCert) -> Result<CertDetail, CertFailure> {
        let ks = self.keystore(&new.store)?;
        let alg = match new.key {
            KeySpec::Rsa2048 => KeyAlgorithm::Rsa2048,
            KeySpec::Rsa3072 => KeyAlgorithm::Rsa3072,
            KeySpec::Rsa4096 => KeyAlgorithm::Rsa4096,
            KeySpec::P256 => KeyAlgorithm::EcdsaP256,
            KeySpec::P384 => KeyAlgorithm::EcdsaP384,
        };
        let opt = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());
        let d = &new.subject;
        let spec = CertSpec {
            subject: Subject {
                common_name: d.cn.trim().into(),
                organization: opt(&d.o),
                organizational_unit: opt(&d.ou),
                locality: opt(&d.l),
                state: opt(&d.st),
                country: opt(&d.c),
            },
            kind: if new.server { CertKind::Server } else { CertKind::Client },
            san: new.san.clone(),
            not_before: super::now(),
            days: new.days,
        };
        let c = self.ctx();
        let store = &new.store;
        match &new.signing {
            Signing::SelfSigned => {
                entries::create(&c, &ks, &new.label, &spec, alg, &Signer::SelfSigned).map_err(|e| fail(e, store))?;
            }
            Signing::Request => {
                entries::create(&c, &ks, &new.label, &spec, alg, &Signer::Request).map_err(|e| fail(e, store))?;
            }
            Signing::Ca(name) => {
                let (_, path) = self
                    .signers
                    .iter()
                    .find(|s| &s.0 == name)
                    .ok_or_else(|| CertFailure::Invalid(format!("{name} is not a certificate authority of this system")))?;
                if path == LOCAL_CA_STORE {
                    entries::create(&c, &ks, &new.label, &spec, alg, &Signer::LocalCa(None)).map_err(|e| fail(e, store))?;
                } else {
                    let ca_store = CertStore::open_stashed(&self.files, path, &self.sealer).map_err(|e| fail(e, name))?;
                    let mut target = c.open(&ks).map_err(|e| fail(e, store))?;
                    ca::issue(&ca_store, &mut target, &new.label, &spec, alg).map_err(|e| fail(e, store))?;
                    target.save(&self.files).map_err(|e| fail(e, store))?;
                }
            }
        }
        if new.make_default && new.signing != Signing::Request {
            let ch = entries::Change {
                default: true,
                ..entries::Change::default()
            };
            entries::change(&c, &ks, &new.label, &ch).map_err(|e| fail(e, store))?;
        }
        self.detail(store, &new.label)
    }

    fn import(&self, _user: &str, import: &Import) -> Result<Vec<String>, CertFailure> {
        let store = &import.store;
        let ks = self.keystore(store)?;
        let mut data = match &import.source {
            ImportSource::Data(d) => d.clone(),
            ImportSource::Path(p) => self
                .ifs
                .read(p)
                .map_err(|e| CertFailure::Invalid(format!("stream file {p} could not be read: {e}")))?,
        };
        if data.is_empty() {
            return Err(CertFailure::Invalid("the file is empty".into()));
        }
        let pw = import.password.as_deref().filter(|p| !p.is_empty());
        let mut format = match import.format {
            ImportFormat::Auto => entries::ImportFormat::Auto,
            ImportFormat::Pem => entries::ImportFormat::Pem,
            ImportFormat::Pkcs12 => entries::ImportFormat::Pkcs12,
        };
        // One DER certificate goes in as PEM.
        if import.format == ImportFormat::Auto && CertInfo::from_der(&data).is_ok() {
            data = pem::encode("CERTIFICATE", &data).into_bytes();
            format = entries::ImportFormat::Pem;
        }
        let label = match import.label.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
            Some(l) => l.to_string(),
            None => Self::common_name(&data, pw)
                .ok_or_else(|| CertFailure::Invalid("the file holds no certificate with a common name; give a label".into()))?,
        };
        let c = self.ctx();
        let notes = entries::import(&c, &ks, &label, &data, format, pw, true).map_err(|e| fail(e, store))?;
        let labels: Vec<String> = notes.into_iter().filter_map(|n| n.data.into_iter().next()).collect();
        if import.make_default && !import.ca && labels.iter().any(|l| l == &label) {
            let ch = entries::Change {
                default: true,
                ..entries::Change::default()
            };
            entries::change(&c, &ks, &label, &ch).map_err(|e| fail(e, store))?;
        }
        Ok(labels)
    }

    fn set_default(&self, _user: &str, store: &str, label: &str) -> Result<(), CertFailure> {
        let ks = self.keystore(store)?;
        let ch = entries::Change {
            default: true,
            ..entries::Change::default()
        };
        entries::change(&self.ctx(), &ks, label, &ch).map_err(|e| fail(e, store))?;
        Ok(())
    }

    fn remove(&self, _user: &str, store: &str, label: &str) -> Result<(), CertFailure> {
        let ks = self.keystore(store)?;
        entries::remove(&self.ctx(), &ks, label).map_err(|e| fail(e, store))?;
        Ok(())
    }

    fn apps(&self) -> Vec<AppInfo> {
        let all = appids::list(&self.ctx()).unwrap_or_default();
        let info = |a: &certs::apps::AppDef| AppInfo {
            id: a.id.clone(),
            server: a.kind == AppKind::Server,
            description: a.text.clone(),
            label: a.label.clone(),
        };
        let mut out: Vec<AppInfo> = SHOWN_APPS
            .iter()
            .filter_map(|id| all.iter().find(|a| a.id == *id).map(info))
            .collect();
        out.extend(all.iter().filter(|a| !a.shipped && !SHOWN_APPS.contains(&a.id.as_str())).map(info));
        out
    }

    fn assign(&self, _user: &str, app: &str, label: Option<&str>) -> Result<(), CertFailure> {
        appids::assign(&self.ctx(), app, label).map_err(|e| fail(e, "*SYSTEM"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_stores() {
        let f = FixtureCerts::new(Arc::new(httpd::files::MemFiles::new()));
        let stores = f.stores();
        let counts: Vec<(&str, usize, Option<&str>)> =
            stores.iter().map(|s| (s.name.as_str(), s.certificates, s.default.as_deref())).collect();
        assert_eq!(
            counts,
            [
                ("*SYSTEM", 6, Some("webstore.quartz.example")),
                ("*OBJECTSIGNING", 1, Some("QSHQ400 code signing")),
                ("*SIGNATUREVERIFICATION", 3, None),
                ("WEBSTORE", 2, Some("webstore-dev")),
            ]
        );
        let rows = f.entries("*SYSTEM").unwrap();
        let api = rows.iter().find(|r| r.label == "api.quartz.example").unwrap();
        assert_eq!(api.key, "ECDSA P-256");
        assert_eq!(api.subject, "CN=api.quartz.example,O=Quartz Systems,C=US");
        assert_eq!(api.issuer, "CN=Quartz Internal Certificate Authority,O=Quartz Systems,C=US");
        assert_eq!((api.not_after - super::super::now()) / 86_400, 32);
        let cas: Vec<&str> = rows.iter().filter(|r| r.kind == EntryType::Ca).map(|r| r.label.as_str()).collect();
        assert_eq!(cas, ["QSHQ400 Local CA", "Quartz Internal CA"]);
        assert_eq!(f.app_label("QIBM_HTTP_SERVER_API").as_deref(), Some("api.quartz.example"));
        assert_eq!(f.apps().len(), 11);
    }
}
