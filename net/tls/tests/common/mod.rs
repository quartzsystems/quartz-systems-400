// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A `*SYSTEM` store with a local CA, an RSA and an ECDSA server
//! identity and a client identity, dated from the real clock (rustls
//! validates against it).

#![allow(dead_code)]

use certs::ca;
use certs::crypto::kdf::Argon2Params;
use certs::crypto::keys::KeyAlgorithm;
use certs::files::MemFiles;
use certs::paths::{LOCAL_CA_STORE, SYSTEM_STORE};
use certs::store::{CertStore, StoreKind};
use certs::x509::build::{CertKind, CertSpec, Subject};
use tls::config::{
    ClientAuthMode, Identity, ServerAuthMode, client_config, server_config, trusted_roots,
};
use tls::policy::TlsPolicy;

pub const HOST: &str = "qs400.test";
pub const PASSWORD: &str = "system1";

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 120
}

pub fn fast() -> Argon2Params {
    Argon2Params {
        m_cost: 64,
        t_cost: 1,
        p_cost: 1,
    }
}

/// The files, the open `*SYSTEM` store and the CA store.
pub struct Fixture {
    pub files: MemFiles,
    pub system: CertStore,
    pub ca: CertStore,
    pub now: u64,
}

impl Fixture {
    pub fn new() -> Self {
        let files = MemFiles::new();
        let now = now();
        ca::create(
            &files,
            "capass1",
            &Subject::cn("QS400 Test CA"),
            KeyAlgorithm::EcdsaP256,
            now,
            3650,
            fast(),
        )
        .unwrap();
        let ca = CertStore::open(&files, LOCAL_CA_STORE, "capass1").unwrap();
        let mut system = CertStore::create_with(
            &files,
            SYSTEM_STORE,
            PASSWORD,
            StoreKind::System,
            now,
            fast(),
        )
        .unwrap();
        ca::issue(
            &ca,
            &mut system,
            "RSA",
            &CertSpec::server(HOST, now, 30),
            KeyAlgorithm::Rsa2048,
        )
        .unwrap();
        ca::issue(
            &ca,
            &mut system,
            "ECDSA",
            &CertSpec::server(HOST, now, 30),
            KeyAlgorithm::EcdsaP256,
        )
        .unwrap();
        let client_spec = CertSpec {
            subject: Subject::cn("client one"),
            kind: CertKind::Client,
            san: vec![],
            not_before: now,
            days: 30,
        };
        ca::issue(
            &ca,
            &mut system,
            "CLIENT",
            &client_spec,
            KeyAlgorithm::EcdsaP256,
        )
        .unwrap();
        system.save(&files).unwrap();
        Self {
            files,
            system,
            ca,
            now,
        }
    }

    pub fn identity(&self, label: &str) -> Identity {
        Identity::from_store(&self.system, label, self.now).unwrap()
    }

    pub fn roots(&self) -> rustls::RootCertStore {
        trusted_roots(&self.system, &certs::apps::CaTrust::All).unwrap()
    }

    pub fn server(
        &self,
        label: &str,
        policy: &TlsPolicy,
        auth: ClientAuthMode,
    ) -> std::sync::Arc<rustls::ServerConfig> {
        server_config(&self.identity(label), policy, auth, self.roots()).unwrap()
    }

    pub fn client(
        &self,
        policy: &TlsPolicy,
        identity: Option<&str>,
        mode: ServerAuthMode,
    ) -> std::sync::Arc<rustls::ClientConfig> {
        let id = identity.map(|l| self.identity(l));
        client_config(policy, self.roots(), id.as_ref(), mode).unwrap()
    }

    /// A store that trusts nothing, for the untrusted-CA cases.
    pub fn empty_roots() -> rustls::RootCertStore {
        rustls::RootCertStore::empty()
    }
}
