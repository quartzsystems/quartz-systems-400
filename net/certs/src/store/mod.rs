// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A certificate store in memory: opened or created from its file,
//! changed, saved back. One store is one file; the IFS path names it.

pub mod entry;
pub mod format;
mod io;

use zeroize::Zeroizing;

use crate::crypto::kdf::Argon2Params;
use crate::crypto::keys::{KeyAlgorithm, PrivateKey};
use crate::error::{CertError, Result};
use crate::files::StoreFiles;
use crate::stash::{self, Sealer};
use crate::x509::CertInfo;
use crate::x509::build::{self, CertKind, CertSpec};

pub use entry::{Entry, EntryKind};
pub use format::{Body, Header, StoreKind};
pub use io::Imported;

/// An open store.
pub struct CertStore {
    path: String,
    header: Header,
    body: Body,
    key: Zeroizing<[u8; 32]>,
    dirty: bool,
}

/// Minimum store password length (IBM's DCM asks for at least this).
pub const MIN_PASSWORD: usize = 6;

/// An entry's facts for listings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    /// The label.
    pub label: String,
    /// The kind.
    pub kind: EntryKind,
    /// The certificate's facts, when it has one.
    pub info: Option<CertInfo>,
    /// Trusted (CAs).
    pub trusted: bool,
    /// This is the store's default certificate.
    pub is_default: bool,
    /// Free text.
    pub text: String,
}

/// Where an entry stands relative to expiry at a given time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expiry {
    /// The label.
    pub label: String,
    /// The subject.
    pub subject: String,
    /// Not valid after, Unix seconds.
    pub not_after: u64,
    /// Whole days left; negative once expired.
    pub days_left: i64,
}

impl Expiry {
    /// The warning thresholds in days: a warning goes to `QSYSOPR` the
    /// first time the days left fall to each.
    pub const THRESHOLDS: [i64; 3] = [30, 7, 1];

    /// The threshold this entry has crossed, if any (`Some(0)` once expired).
    pub fn crossed(&self) -> Option<i64> {
        if self.days_left < 0 {
            return Some(0);
        }
        Self::THRESHOLDS
            .iter()
            .rev()
            .find(|t| self.days_left <= **t)
            .copied()
    }
}

impl CertStore {
    /// Create a new store file. Fails if one exists at `path`.
    pub fn create(
        files: &dyn StoreFiles,
        path: &str,
        password: &str,
        kind: StoreKind,
        now: u64,
    ) -> Result<Self> {
        Self::create_with(files, path, password, kind, now, Argon2Params::default())
    }

    /// [`CertStore::create`] with explicit key derivation costs (tests).
    pub fn create_with(
        files: &dyn StoreFiles,
        path: &str,
        password: &str,
        kind: StoreKind,
        now: u64,
        params: Argon2Params,
    ) -> Result<Self> {
        check_password(password)?;
        if files.exists(path)? {
            return Err(CertError::Exists(path.to_string()));
        }
        let header = format::new_header(kind, now, params)?;
        let key = format::derive(&header, password)?;
        let mut store = Self {
            path: path.to_string(),
            header,
            body: Body {
                entries: Vec::new(),
                default_label: None,
                changed: now,
            },
            key,
            dirty: true,
        };
        store.save(files)?;
        Ok(store)
    }

    /// Open the store at `path` with its password.
    pub fn open(files: &dyn StoreFiles, path: &str, password: &str) -> Result<Self> {
        let bytes = files
            .read(path)?
            .ok_or_else(|| CertError::NotFound(path.to_string()))?;
        let d = format::decode(&bytes, password)?;
        Ok(Self {
            path: path.to_string(),
            header: d.header,
            body: d.body,
            key: d.key,
            dirty: false,
        })
    }

    /// Open the store with the password its stash holds.
    pub fn open_stashed(files: &dyn StoreFiles, path: &str, sealer: &dyn Sealer) -> Result<Self> {
        let password = stash::read(files, path, sealer)?;
        Self::open(files, path, &password)
    }

    /// Write the store back. A no-op when nothing changed.
    pub fn save(&mut self, files: &dyn StoreFiles) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let bytes = format::encode(&self.header, &self.body, &self.key)?;
        files.write(&self.path, &bytes)?;
        self.dirty = false;
        Ok(())
    }

    /// Write the stash for this store's password.
    pub fn stash(&self, files: &dyn StoreFiles, password: &str, sealer: &dyn Sealer) -> Result<()> {
        // The password must be the one that opens this store.
        let key = format::derive(&self.header, password)?;
        if *key != *self.key {
            return Err(CertError::BadPassword);
        }
        stash::write(files, &self.path, password, sealer)
    }

    /// Change the password: a new salt, the body resealed on the next save.
    pub fn change_password(&mut self, old: &str, new: &str, now: u64) -> Result<()> {
        check_password(new)?;
        if *format::derive(&self.header, old)? != *self.key {
            return Err(CertError::BadPassword);
        }
        let params = Argon2Params {
            m_cost: self.header.m_cost,
            t_cost: self.header.t_cost,
            p_cost: self.header.p_cost,
        };
        self.header = format::new_header(self.header.kind, self.header.created, params)?;
        self.key = format::derive(&self.header, new)?;
        self.touch(now);
        Ok(())
    }

    /// The path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The kind.
    pub fn kind(&self) -> StoreKind {
        self.header.kind
    }

    /// Unix time of the last change.
    pub fn changed(&self) -> u64 {
        self.body.changed
    }

    /// Whether unsaved changes exist.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The entries, in label order.
    pub fn entries(&self) -> &[Entry] {
        &self.body.entries
    }

    /// Listings of every entry.
    pub fn list(&self) -> Result<Vec<Listing>> {
        self.body
            .entries
            .iter()
            .map(|e| {
                Ok(Listing {
                    label: e.label.clone(),
                    kind: e.kind,
                    info: e.info()?,
                    trusted: e.trusted,
                    is_default: self.body.default_label.as_deref() == Some(&e.label),
                    text: e.text.clone(),
                })
            })
            .collect()
    }

    /// The entry with `label`.
    pub fn get(&self, label: &str) -> Result<&Entry> {
        self.body
            .entries
            .iter()
            .find(|e| e.label == label)
            .ok_or_else(|| CertError::LabelNotFound(label.to_string()))
    }

    fn get_mut(&mut self, label: &str) -> Result<&mut Entry> {
        self.body
            .entries
            .iter_mut()
            .find(|e| e.label == label)
            .ok_or_else(|| CertError::LabelNotFound(label.to_string()))
    }

    /// The default certificate's label.
    pub fn default_label(&self) -> Option<&str> {
        self.body.default_label.as_deref()
    }

    /// Make `label` the default certificate.
    pub fn set_default(&mut self, label: &str, now: u64) -> Result<()> {
        let e = self.get(label)?;
        if !e.has_identity() {
            return Err(CertError::Invalid(format!(
                "{label} has no private key and cannot be the default"
            )));
        }
        self.body.default_label = Some(label.to_string());
        self.touch(now);
        Ok(())
    }

    /// The entry whose certificate has this SHA-256 fingerprint.
    pub fn find_by_sha256(&self, sha256_hex: &str) -> Result<Option<&Entry>> {
        for e in &self.body.entries {
            if let Some(i) = e.info()?
                && i.sha256.eq_ignore_ascii_case(sha256_hex)
            {
                return Ok(Some(e));
            }
        }
        Ok(None)
    }

    fn insert(&mut self, entry: Entry, now: u64) -> Result<()> {
        check_label(&entry.label)?;
        if self.body.entries.iter().any(|e| e.label == entry.label) {
            return Err(CertError::LabelExists(entry.label));
        }
        let at = self
            .body
            .entries
            .partition_point(|e| e.label.as_str() < entry.label.as_str());
        self.body.entries.insert(at, entry);
        self.touch(now);
        Ok(())
    }

    /// Add a certificate with its key as a server or client entry.
    /// `chain` holds its intermediates, leaf's issuer first.
    pub fn add_identity(
        &mut self,
        label: &str,
        kind: EntryKind,
        cert: Vec<u8>,
        chain: Vec<Vec<u8>>,
        key: &PrivateKey,
        now: u64,
    ) -> Result<()> {
        if !matches!(kind, EntryKind::Server | EntryKind::Client) {
            return Err(CertError::Invalid(
                "an identity is a server or client entry".into(),
            ));
        }
        let info = CertInfo::from_der(&cert)?;
        let spki = key.spki_der()?;
        let cert_spki = x509_cert::Certificate::from_der_spki(&cert)?;
        if spki != cert_spki {
            return Err(CertError::Invalid(
                "the private key does not match the certificate".into(),
            ));
        }
        for c in &chain {
            CertInfo::from_der(c)?;
        }
        let _ = info;
        self.insert(
            Entry {
                label: label.to_string(),
                kind,
                cert: Some(cert),
                chain,
                key: Some(key.to_pkcs8_der()?.to_vec()),
                csr: None,
                trusted: false,
                text: String::new(),
                added: now,
            },
            now,
        )?;
        if self.body.default_label.is_none() {
            self.body.default_label = Some(label.to_string());
        }
        Ok(())
    }

    /// Add a CA certificate.
    pub fn add_ca(&mut self, label: &str, cert: Vec<u8>, trusted: bool, now: u64) -> Result<()> {
        let info = CertInfo::from_der(&cert)?;
        if !info.is_ca && !info.self_signed {
            return Err(CertError::Invalid(format!(
                "{} is not a certificate authority",
                info.subject
            )));
        }
        self.insert(
            Entry {
                label: label.to_string(),
                kind: EntryKind::Ca,
                cert: Some(cert),
                chain: Vec::new(),
                key: None,
                csr: None,
                trusted,
                text: String::new(),
                added: now,
            },
            now,
        )
    }

    /// Make a self-signed certificate with a new key.
    pub fn create_self_signed(
        &mut self,
        label: &str,
        spec: &CertSpec,
        alg: KeyAlgorithm,
    ) -> Result<CertInfo> {
        let key = PrivateKey::generate(alg)?;
        let cert = build::self_signed(spec, &key)?;
        let info = CertInfo::from_der(&cert)?;
        let kind = match spec.kind {
            CertKind::Ca => EntryKind::Ca,
            CertKind::Server => EntryKind::Server,
            CertKind::Client => EntryKind::Client,
        };
        let now = spec.not_before;
        if kind == EntryKind::Ca {
            // A CA's own entry keeps its key: it signs with it.
            self.insert(
                Entry {
                    label: label.to_string(),
                    kind,
                    cert: Some(cert),
                    chain: Vec::new(),
                    key: Some(key.to_pkcs8_der()?.to_vec()),
                    csr: None,
                    trusted: true,
                    text: String::new(),
                    added: now,
                },
                now,
            )?;
        } else {
            self.add_identity(label, kind, cert, Vec::new(), &key, now)?;
        }
        Ok(info)
    }

    /// Make a key and a request for it; the entry waits for the
    /// certificate ([`CertStore::receive_certificate`]). Returns the CSR DER.
    pub fn create_request(
        &mut self,
        label: &str,
        spec: &CertSpec,
        alg: KeyAlgorithm,
    ) -> Result<Vec<u8>> {
        let key = PrivateKey::generate(alg)?;
        let csr = build::request(spec, &key)?;
        self.insert(
            Entry {
                label: label.to_string(),
                kind: EntryKind::Request,
                cert: None,
                chain: Vec::new(),
                key: Some(key.to_pkcs8_der()?.to_vec()),
                csr: Some(csr.clone()),
                trusted: false,
                text: String::new(),
                added: spec.not_before,
            },
            spec.not_before,
        )?;
        Ok(csr)
    }

    /// Give a request entry its certificate (and intermediates).
    pub fn receive_certificate(
        &mut self,
        label: &str,
        cert: Vec<u8>,
        chain: Vec<Vec<u8>>,
        now: u64,
    ) -> Result<CertInfo> {
        let e = self.get(label)?;
        if e.kind != EntryKind::Request {
            return Err(CertError::Invalid(format!(
                "{label} is not a pending request"
            )));
        }
        let key = e.private_key()?;
        let info = CertInfo::from_der(&cert)?;
        if key.spki_der()? != x509_cert::Certificate::from_der_spki(&cert)? {
            return Err(CertError::Invalid(
                "the certificate is not for this request's key".into(),
            ));
        }
        for c in &chain {
            CertInfo::from_der(c)?;
        }
        let kind = if info.server_auth {
            EntryKind::Server
        } else {
            EntryKind::Client
        };
        let e = self.get_mut(label)?;
        e.kind = kind;
        e.cert = Some(cert);
        e.chain = chain;
        e.csr = None;
        if self.body.default_label.is_none() {
            self.body.default_label = Some(label.to_string());
        }
        self.touch(now);
        Ok(info)
    }

    /// Remove an entry.
    pub fn remove(&mut self, label: &str, now: u64) -> Result<Entry> {
        let at = self
            .body
            .entries
            .iter()
            .position(|e| e.label == label)
            .ok_or_else(|| CertError::LabelNotFound(label.to_string()))?;
        let e = self.body.entries.remove(at);
        if self.body.default_label.as_deref() == Some(label) {
            self.body.default_label = None;
        }
        self.touch(now);
        Ok(e)
    }

    /// Rename an entry.
    pub fn rename(&mut self, label: &str, new_label: &str, now: u64) -> Result<()> {
        check_label(new_label)?;
        if label == new_label {
            return Ok(());
        }
        if self.body.entries.iter().any(|e| e.label == new_label) {
            return Err(CertError::LabelExists(new_label.to_string()));
        }
        let mut e = self.remove(label, now)?;
        let was_default = self.body.default_label.is_none() && e.has_identity();
        e.label = new_label.to_string();
        self.insert(e, now)?;
        if was_default {
            self.body.default_label = Some(new_label.to_string());
        }
        Ok(())
    }

    /// Set a CA's trust.
    pub fn set_trusted(&mut self, label: &str, trusted: bool, now: u64) -> Result<()> {
        let e = self.get_mut(label)?;
        if e.kind != EntryKind::Ca {
            return Err(CertError::Invalid(format!("{label} is not a CA")));
        }
        e.trusted = trusted;
        self.touch(now);
        Ok(())
    }

    /// Set an entry's text.
    pub fn set_text(&mut self, label: &str, text: &str, now: u64) -> Result<()> {
        self.get_mut(label)?.text = text.to_string();
        self.touch(now);
        Ok(())
    }

    /// The private key of `label`.
    pub fn private_key(&self, label: &str) -> Result<PrivateKey> {
        self.get(label)?.private_key()
    }

    /// The trusted CA certificates, DER.
    pub fn trusted_cas(&self) -> Vec<Vec<u8>> {
        self.body
            .entries
            .iter()
            .filter(|e| e.kind == EntryKind::Ca && e.trusted)
            .filter_map(|e| e.cert.clone())
            .collect()
    }

    /// The labels of trusted CAs.
    pub fn trusted_ca_labels(&self) -> Vec<String> {
        self.body
            .entries
            .iter()
            .filter(|e| e.kind == EntryKind::Ca && e.trusted)
            .map(|e| e.label.clone())
            .collect()
    }

    /// The certificate chain to send for `label`: its certificate, its
    /// stored intermediates, then CA entries of the store up to (not
    /// including) a self-signed root.
    pub fn chain(&self, label: &str) -> Result<Vec<Vec<u8>>> {
        let e = self.get(label)?;
        let leaf = e
            .cert
            .clone()
            .ok_or_else(|| CertError::Invalid(format!("{label} has no certificate")))?;
        let mut out = vec![leaf];
        out.extend(e.chain.iter().cloned());
        let mut last = CertInfo::from_der(out.last().expect("leaf"))?;
        let mut guard = 0;
        while !last.self_signed && guard < 16 {
            guard += 1;
            let Some(next) = self.issuer_of(&last)? else {
                break;
            };
            let info = CertInfo::from_der(&next)?;
            if info.self_signed {
                break;
            }
            out.push(next);
            last = info;
        }
        Ok(out)
    }

    fn issuer_of(&self, info: &CertInfo) -> Result<Option<Vec<u8>>> {
        for e in &self.body.entries {
            if e.kind != EntryKind::Ca {
                continue;
            }
            if let (Some(der), Some(i)) = (&e.cert, e.info()?)
                && i.subject == info.issuer
                && i.sha256 != info.sha256
            {
                return Ok(Some(der.clone()));
            }
        }
        Ok(None)
    }

    /// Every entry with a certificate and its days left at `now`, soonest
    /// first. Trusted and untrusted CAs included; expired ones first.
    pub fn expiry(&self, now: u64) -> Result<Vec<Expiry>> {
        let mut out = Vec::new();
        for e in &self.body.entries {
            if let Some(i) = e.info()? {
                out.push(Expiry {
                    label: e.label.clone(),
                    subject: i.subject.clone(),
                    not_after: i.not_after,
                    days_left: i.days_left(now),
                });
            }
        }
        out.sort_by_key(|x| x.days_left);
        Ok(out)
    }

    /// The entries that have crossed a warning threshold at `now`.
    pub fn expiring(&self, now: u64) -> Result<Vec<Expiry>> {
        Ok(self
            .expiry(now)?
            .into_iter()
            .filter(|x| x.crossed().is_some())
            .collect())
    }

    fn touch(&mut self, now: u64) {
        self.body.changed = now;
        self.dirty = true;
    }
}

/// Labels: 1 to 64 characters, printable, no leading or trailing blanks.
pub fn check_label(label: &str) -> Result<()> {
    let ok = !label.is_empty()
        && label.chars().count() <= 64
        && label.trim() == label
        && label.chars().all(|c| !c.is_control());
    if ok {
        Ok(())
    } else {
        Err(CertError::Invalid(format!("label '{label}' is not valid")))
    }
}

fn check_password(password: &str) -> Result<()> {
    if password.chars().count() < MIN_PASSWORD {
        return Err(CertError::Invalid(format!(
            "the store password needs at least {MIN_PASSWORD} characters"
        )));
    }
    Ok(())
}

/// The SPKI DER of a certificate.
trait SpkiOf {
    fn from_der_spki(der: &[u8]) -> Result<Vec<u8>>;
}

impl SpkiOf for x509_cert::Certificate {
    fn from_der_spki(der: &[u8]) -> Result<Vec<u8>> {
        use der::{Decode, Encode};
        let c = x509_cert::Certificate::from_der(der)?;
        Ok(c.tbs_certificate().subject_public_key_info().to_der()?)
    }
}
