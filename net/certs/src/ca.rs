// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The local certificate authority: its own store at
//! [`crate::paths::LOCAL_CA_STORE`], one CA entry with its key, and the
//! certificates it issues into the `*SYSTEM` store (or any other).

use crate::crypto::kdf::Argon2Params;
use crate::crypto::keys::{KeyAlgorithm, PrivateKey};
use crate::error::{CertError, Result};
use crate::files::StoreFiles;
use crate::paths::{LOCAL_CA_CERT, LOCAL_CA_STORE};
use crate::pem;
use crate::store::{CertStore, EntryKind, StoreKind};
use crate::x509::CertInfo;
use crate::x509::build::{self, CertKind, CertSpec, Subject};

/// The label of the CA's own entry.
pub const CA_LABEL: &str = "LOCAL_CERTIFICATE_AUTHORITY";

/// Create the local CA: its store, its self-signed certificate, and the
/// certificate as PEM at [`LOCAL_CA_CERT`] for clients to install.
pub fn create(
    files: &dyn StoreFiles,
    password: &str,
    subject: &Subject,
    alg: KeyAlgorithm,
    now: u64,
    days: u32,
    params: Argon2Params,
) -> Result<CertInfo> {
    let mut store = CertStore::create_with(
        files,
        LOCAL_CA_STORE,
        password,
        StoreKind::CertAuth,
        now,
        params,
    )?;
    let spec = CertSpec {
        subject: subject.clone(),
        kind: CertKind::Ca,
        san: Vec::new(),
        not_before: now,
        days,
    };
    let info = store.create_self_signed(CA_LABEL, &spec, alg)?;
    store.save(files)?;
    let der = store.get(CA_LABEL)?.cert.clone().expect("just made");
    files.write(LOCAL_CA_CERT, pem::encode("CERTIFICATE", &der).as_bytes())?;
    Ok(info)
}

/// The CA's certificate (DER) and key from its open store.
pub fn credentials(ca_store: &CertStore) -> Result<(Vec<u8>, PrivateKey)> {
    let e = ca_store.get(CA_LABEL)?;
    if e.kind != EntryKind::Ca {
        return Err(CertError::Invalid("the local CA entry is not a CA".into()));
    }
    let der = e
        .cert
        .clone()
        .ok_or_else(|| CertError::Invalid("the local CA has no certificate".into()))?;
    Ok((der, e.private_key()?))
}

/// Issue a certificate for a new key into `target`, as entry `label`,
/// and make sure `target` trusts the CA (adding it as `CA_LABEL` when
/// absent). Returns the new certificate's facts.
pub fn issue(
    ca_store: &CertStore,
    target: &mut CertStore,
    label: &str,
    spec: &CertSpec,
    alg: KeyAlgorithm,
) -> Result<CertInfo> {
    if spec.kind == CertKind::Ca {
        return Err(CertError::Invalid(
            "the local CA issues server and client certificates".into(),
        ));
    }
    let (ca_der, ca_key) = credentials(ca_store)?;
    let ca_info = CertInfo::from_der(&ca_der)?;
    if ca_info.not_after < spec.not_before + u64::from(spec.days) * 86_400 {
        return Err(CertError::Invalid(
            "the certificate would outlive the local CA's certificate".into(),
        ));
    }
    let key = PrivateKey::generate(alg)?;
    let cert = build::issue(spec, &key, &ca_der, &ca_key)?;
    let info = CertInfo::from_der(&cert)?;
    let kind = if spec.kind == CertKind::Server {
        EntryKind::Server
    } else {
        EntryKind::Client
    };
    target.add_identity(label, kind, cert, Vec::new(), &key, spec.not_before)?;
    trust(ca_store, target, spec.not_before)?;
    Ok(info)
}

/// Sign a request from elsewhere; the certificate is returned, nothing
/// is stored.
pub fn sign_request(
    ca_store: &CertStore,
    csr_der: &[u8],
    kind: CertKind,
    now: u64,
    days: u32,
) -> Result<Vec<u8>> {
    let (ca_der, ca_key) = credentials(ca_store)?;
    build::sign_request(csr_der, kind, now, days, &ca_der, &ca_key)
}

/// Make `target` trust the local CA. Returns whether an entry was added.
pub fn trust(ca_store: &CertStore, target: &mut CertStore, now: u64) -> Result<bool> {
    let (ca_der, _) = credentials(ca_store)?;
    let info = CertInfo::from_der(&ca_der)?;
    if target.find_by_sha256(&info.sha256)?.is_some() {
        return Ok(false);
    }
    let mut label = CA_LABEL.to_string();
    let mut n = 1;
    while target.get(&label).is_ok() {
        n += 1;
        label = format!("{CA_LABEL} ({n})");
    }
    target.add_ca(&label, ca_der, true, now)?;
    Ok(true)
}
