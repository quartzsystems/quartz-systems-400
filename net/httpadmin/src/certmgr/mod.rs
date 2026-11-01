// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The certificate manager over `net/certs`: what [`Certs`] asks, done
//! on store files through a `certs::admin::Ctx`. The storage service
//! runs it on the system's files (its sealer is the LIC partition's
//! key); the fixture on its sample stores. The stores someone made are
//! listed in `certs::paths::USER_STORES`. [`wire`] carries the calls
//! from the `ADMIN` job to the service.
//!
//! [`Certs`]: crate::env::Certs

pub mod wire;

use certs::admin::entries::{self, Signer};
use certs::admin::{Ctx, Keystore, appids, keystores};
use certs::apps::AppKind;
use certs::ca;
use certs::crypto::keys::KeyAlgorithm;
use certs::paths::{
    LOCAL_CA_STORE, OBJECT_SIGNING_STORE, SIGNATURE_VERIFICATION_STORE, SYSTEM_STORE, USER_STORES,
};
use certs::store::{CertStore, EntryKind};
use certs::x509::CertInfo;
use certs::x509::build::{self, CertKind, CertSpec, Subject};
use certs::{CertError, pem};

use crate::env::{
    AppInfo, CertDetail, CertFailure, CertRow, EntryType, Import, ImportFormat, ImportSource, KeySpec, NewCert,
    Signing, StoreInfo, StoreType,
};

/// The system's own stores: name, type, path.
pub const SYSTEM_STORES: &[(&str, StoreType, &str)] = &[
    ("*SYSTEM", StoreType::System, SYSTEM_STORE),
    ("*OBJECTSIGNING", StoreType::ObjectSigning, OBJECT_SIGNING_STORE),
    ("*SIGNATUREVERIFICATION", StoreType::SignatureVerification, SIGNATURE_VERIFICATION_STORE),
];

/// A `net/certs` error as the product's failure for `store`.
pub fn fail(e: CertError, store: &str) -> CertFailure {
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

fn io_fail(e: &std::io::Error) -> CertFailure {
    CertFailure::Invalid(format!("the list of certificate stores could not be used: {e}"))
}

/// The stores someone made: name and path, in the order made.
pub fn user_stores(ctx: &Ctx<'_>) -> Vec<(String, String)> {
    ctx.files
        .read(USER_STORES)
        .ok()
        .flatten()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Write the list of stores someone made.
pub fn save_user_stores(ctx: &Ctx<'_>, list: &[(String, String)]) -> Result<(), CertFailure> {
    let mut t = serde_json::to_vec_pretty(list).map_err(|e| CertFailure::Invalid(e.to_string()))?;
    t.push(b'\n');
    ctx.files.write(USER_STORES, &t).map_err(|e| io_fail(&e))
}

/// The local CA as a signer, when the system has one: named by the
/// label its certificate has in `*SYSTEM`.
pub fn local_signer(ctx: &Ctx<'_>) -> Option<(String, String)> {
    if !ctx.files.exists(LOCAL_CA_STORE).unwrap_or(false) {
        return None;
    }
    let sealer = ctx.sealer?;
    let local = CertStore::open_stashed(ctx.files, LOCAL_CA_STORE, sealer).ok()?;
    let ca_der = local.get(ca::CA_LABEL).ok()?.cert.clone()?;
    let sys = ctx.open(&Keystore::new(SYSTEM_STORE, "")).ok();
    let label = sys
        .and_then(|s| {
            s.entries()
                .iter()
                .find(|e| e.kind == EntryKind::Ca && e.cert.as_ref() == Some(&ca_der))
                .map(|e| e.label.clone())
        })
        .unwrap_or_else(|| ca::CA_LABEL.to_string());
    Some((label, LOCAL_CA_STORE.to_string()))
}

/// The certificate manager over one context.
pub struct Manager<'a> {
    /// The store files and the sealer.
    pub ctx: Ctx<'a>,
    /// CAs whose keys the system holds: name and store path.
    pub signers: Vec<(String, String)>,
    /// The application IDs shown, in order (the others not shipped
    /// follow); `None` shows every one in the system's order.
    pub shown: Option<&'static [&'static str]>,
}

impl Manager<'_> {
    /// A store's type and path by name.
    pub fn find(&self, name: &str) -> Result<(StoreType, String), CertFailure> {
        let n = name.trim();
        if let Some((_, t, p)) = SYSTEM_STORES.iter().find(|f| f.0.eq_ignore_ascii_case(n)) {
            return Ok((*t, (*p).to_string()));
        }
        user_stores(&self.ctx)
            .into_iter()
            .find(|(o, _)| o.eq_ignore_ascii_case(n))
            .map(|(_, p)| (StoreType::Other, p))
            .ok_or_else(|| CertFailure::NoStore(n.to_string()))
    }

    fn keystore(&self, name: &str) -> Result<Keystore, CertFailure> {
        let (_, path) = self.find(name)?;
        Ok(Keystore::new(&path, ""))
    }

    fn open(&self, name: &str) -> Result<CertStore, CertFailure> {
        let ks = self.keystore(name)?;
        self.ctx.open(&ks).map_err(|e| fail(e, name))
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
        let store = self.ctx.open(&Keystore::new(path, "")).ok()?;
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

    /// [`Certs::stores`](crate::env::Certs::stores).
    pub fn stores(&self) -> Vec<StoreInfo> {
        let mut out: Vec<StoreInfo> = SYSTEM_STORES
            .iter()
            .filter_map(|(n, k, p)| self.store_info(n, *k, p))
            .collect();
        out.extend(
            user_stores(&self.ctx)
                .iter()
                .filter_map(|(n, p)| self.store_info(n, StoreType::Other, p)),
        );
        out
    }

    /// [`Certs::entries`](crate::env::Certs::entries).
    pub fn entries(&self, store: &str) -> Result<Vec<CertRow>, CertFailure> {
        let s = self.open(store)?;
        s.entries()
            .iter()
            .map(|e| Self::row(&s, &e.label).map(|r| r.0))
            .collect::<Result<_, _>>()
            .map_err(|e| fail(e, store))
    }

    /// [`Certs::detail`](crate::env::Certs::detail).
    pub fn detail(&self, store: &str, label: &str) -> Result<CertDetail, CertFailure> {
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

    /// [`Certs::export`](crate::env::Certs::export).
    pub fn export(&self, store: &str, label: &str, chain: bool) -> Result<Vec<u8>, CertFailure> {
        let s = self.open(store)?;
        let e = s.get(label).map_err(|e| fail(e, store))?;
        if e.kind == EntryKind::Request {
            let ks = self.keystore(store)?;
            return entries::export(&self.ctx, &ks, label, &entries::ExportFormat::Request).map_err(|e| fail(e, store));
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

    /// [`Certs::create_store`](crate::env::Certs::create_store).
    pub fn create_store(&self, name: &str, path: &str, password: &str) -> Result<(), CertFailure> {
        let name = name.trim();
        if name.is_empty() || name.starts_with('*') {
            return Err(CertFailure::Invalid(format!("{name} is not a name a store can be given")));
        }
        if self.find(name).is_ok() || self.stores().iter().any(|s| s.path == path) {
            return Err(CertFailure::StoreExists(name.into()));
        }
        keystores::create(&self.ctx, &Keystore::new(path, password), true, false).map_err(|e| fail(e, name))?;
        let mut list = user_stores(&self.ctx);
        list.push((name.into(), path.into()));
        save_user_stores(&self.ctx, &list)
    }

    /// [`Certs::change_password`](crate::env::Certs::change_password).
    pub fn change_password(&self, store: &str, old: &str, new: &str) -> Result<(), CertFailure> {
        let (_, path) = self.find(store)?;
        if old.is_empty() {
            return Err(CertFailure::BadPassword(store.into()));
        }
        keystores::change_password(&self.ctx, &Keystore::new(&path, old), new).map_err(|e| fail(e, store))?;
        Ok(())
    }

    /// [`Certs::delete_store`](crate::env::Certs::delete_store).
    pub fn delete_store(&self, store: &str) -> Result<(), CertFailure> {
        let (kind, path) = self.find(store)?;
        if kind != StoreType::Other {
            return Err(CertFailure::Invalid(format!("{store} is one of the system's own stores and cannot be deleted")));
        }
        keystores::delete(&self.ctx, &Keystore::new(&path, "")).map_err(|e| fail(e, store))?;
        let mut list = user_stores(&self.ctx);
        list.retain(|(n, _)| !n.eq_ignore_ascii_case(store));
        save_user_stores(&self.ctx, &list)
    }

    /// [`Certs::signers`](crate::env::Certs::signers).
    pub fn signer_names(&self) -> Vec<String> {
        self.signers.iter().map(|s| s.0.clone()).collect()
    }

    /// [`Certs::create`](crate::env::Certs::create).
    pub fn create(&self, new: &NewCert) -> Result<CertDetail, CertFailure> {
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
            not_before: self.ctx.now,
            days: new.days,
        };
        let c = &self.ctx;
        let store = &new.store;
        match &new.signing {
            Signing::SelfSigned => {
                entries::create(c, &ks, &new.label, &spec, alg, &Signer::SelfSigned).map_err(|e| fail(e, store))?;
            }
            Signing::Request => {
                entries::create(c, &ks, &new.label, &spec, alg, &Signer::Request).map_err(|e| fail(e, store))?;
            }
            Signing::Ca(name) => {
                let (_, path) = self
                    .signers
                    .iter()
                    .find(|s| &s.0 == name)
                    .ok_or_else(|| CertFailure::Invalid(format!("{name} is not a certificate authority of this system")))?;
                if path == LOCAL_CA_STORE {
                    entries::create(c, &ks, &new.label, &spec, alg, &Signer::LocalCa(None)).map_err(|e| fail(e, store))?;
                } else {
                    let sealer = c
                        .sealer
                        .ok_or_else(|| CertFailure::Invalid("the system has no key to open stashed stores".into()))?;
                    let ca_store = CertStore::open_stashed(c.files, path, sealer).map_err(|e| fail(e, name))?;
                    let mut target = c.open(&ks).map_err(|e| fail(e, store))?;
                    ca::issue(&ca_store, &mut target, &new.label, &spec, alg).map_err(|e| fail(e, store))?;
                    target.save(c.files).map_err(|e| fail(e, store))?;
                }
            }
        }
        if new.make_default && new.signing != Signing::Request {
            let ch = entries::Change {
                default: true,
                ..entries::Change::default()
            };
            entries::change(c, &ks, &new.label, &ch).map_err(|e| fail(e, store))?;
        }
        self.detail(store, &new.label)
    }

    /// [`Certs::import`](crate::env::Certs::import), the file's bytes
    /// already read: an [`ImportSource::Path`] is refused here.
    pub fn import(&self, import: &Import) -> Result<Vec<String>, CertFailure> {
        let store = &import.store;
        let ks = self.keystore(store)?;
        let mut data = match &import.source {
            ImportSource::Data(d) => d.clone(),
            ImportSource::Path(p) => {
                return Err(CertFailure::Invalid(format!("stream file {p} was not read")));
            }
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
                .ok_or_else(|| CertFailure::Invalid("no common name could be read from the file (a PKCS#12 file needs its password); give a label".into()))?,
        };
        let c = &self.ctx;
        let notes = entries::import(c, &ks, &label, &data, format, pw, true).map_err(|e| match e {
            // The password is the file's, not the store's.
            CertError::BadPassword => CertFailure::Invalid("the password does not open the file".into()),
            e => fail(e, store),
        })?;
        let labels: Vec<String> = notes.into_iter().filter_map(|n| n.data.into_iter().next()).collect();
        if import.make_default && !import.ca && labels.iter().any(|l| l == &label) {
            let ch = entries::Change {
                default: true,
                ..entries::Change::default()
            };
            entries::change(c, &ks, &label, &ch).map_err(|e| fail(e, store))?;
        }
        Ok(labels)
    }

    /// [`Certs::set_default`](crate::env::Certs::set_default).
    pub fn set_default(&self, store: &str, label: &str) -> Result<(), CertFailure> {
        let ks = self.keystore(store)?;
        let ch = entries::Change {
            default: true,
            ..entries::Change::default()
        };
        entries::change(&self.ctx, &ks, label, &ch).map_err(|e| fail(e, store))?;
        Ok(())
    }

    /// [`Certs::remove`](crate::env::Certs::remove).
    pub fn remove(&self, store: &str, label: &str) -> Result<(), CertFailure> {
        let ks = self.keystore(store)?;
        entries::remove(&self.ctx, &ks, label).map_err(|e| fail(e, store))?;
        Ok(())
    }

    /// [`Certs::apps`](crate::env::Certs::apps).
    pub fn apps(&self) -> Vec<AppInfo> {
        let all = appids::list(&self.ctx).unwrap_or_default();
        let info = |a: &certs::apps::AppDef| AppInfo {
            id: a.id.clone(),
            server: a.kind == AppKind::Server,
            description: a.text.clone(),
            label: a.label.clone(),
        };
        let Some(shown) = self.shown else {
            return all.iter().map(info).collect();
        };
        let mut out: Vec<AppInfo> = shown
            .iter()
            .filter_map(|id| all.iter().find(|a| a.id == *id).map(info))
            .collect();
        out.extend(all.iter().filter(|a| !a.shipped && !shown.contains(&a.id.as_str())).map(info));
        out
    }

    /// [`Certs::assign`](crate::env::Certs::assign).
    pub fn assign(&self, app: &str, label: Option<&str>) -> Result<(), CertFailure> {
        appids::assign(&self.ctx, app, label).map_err(|e| fail(e, "*SYSTEM"))?;
        Ok(())
    }
}
