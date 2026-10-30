// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Keys: public keys and signatures, private keys, `authorized_keys`,
//! and the server's host key set, made at first start and kept beside
//! the configuration.

pub mod authorized;
pub mod host;
pub mod public;

use ifs::client::IfsClient;

pub use host::{KeyError, KeyKind, PrivateKey};
pub use public::PublicKey;

use crate::io::files;

/// The server's host keys, in the order they were configured.
#[derive(Debug, Default)]
pub struct HostKeys(pub Vec<PrivateKey>);

/// A host key file could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HostKeyError {
    /// The file exists and does not hold a key the server reads; it is
    /// left alone rather than replaced.
    #[error("host key {0} exists but cannot be read")]
    Unreadable(String),
    /// The key could not be made.
    #[error("host key {0} could not be generated")]
    Generate(String),
    /// The IFS refused.
    #[error("host key {path}: {errno}")]
    Ifs {
        /// The file.
        path: String,
        /// Why.
        errno: ifs::Errno,
    },
}

/// The kind a host key file is for, from its name.
#[must_use]
pub fn kind_of_path(path: &str) -> Option<KeyKind> {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.contains("ed25519") {
        Some(KeyKind::Ed25519)
    } else if name.contains("ecdsa") {
        Some(KeyKind::EcdsaP256)
    } else if name.contains("rsa") {
        Some(KeyKind::Rsa)
    } else {
        None
    }
}

impl HostKeys {
    /// Load the keys at `paths`, making each that does not exist yet
    /// (with its `.pub` beside it, `comment` in it). A key that exists
    /// is never replaced. A path whose kind cannot be told from its
    /// name and that does not exist is skipped.
    pub fn load_or_generate(
        ifs: &dyn IfsClient,
        paths: &[String],
        comment: &str,
    ) -> Result<Self, HostKeyError> {
        let mut keys = Vec::new();
        for path in paths {
            let err = |errno| HostKeyError::Ifs {
                path: path.clone(),
                errno,
            };
            if files::exists(ifs, path) {
                let text = files::read_text(ifs, path).map_err(err)?;
                let key = PrivateKey::from_pem(&text)
                    .map_err(|_| HostKeyError::Unreadable(path.clone()))?;
                keys.push(key);
                continue;
            }
            let Some(kind) = kind_of_path(path) else {
                tracing::warn!(path, "host key file missing and its type unknown; skipped");
                continue;
            };
            let key =
                PrivateKey::generate(kind).map_err(|_| HostKeyError::Generate(path.clone()))?;
            if let Some((dir, _)) = path.rsplit_once('/')
                && !dir.is_empty()
            {
                files::mkdir_p(ifs, dir, 0o755).map_err(err)?;
            }
            files::write(ifs, path, key.to_pem().as_bytes(), 0o600).map_err(err)?;
            let line = format!("{} {comment}\n", key.public().openssh());
            files::write(ifs, &format!("{path}.pub"), line.as_bytes(), 0o644).map_err(err)?;
            tracing::info!(path, fingerprint = %key.public().fingerprint(), "host key generated");
            keys.push(key);
        }
        Ok(Self(keys))
    }

    /// The host key signature algorithms, preferred first.
    #[must_use]
    pub fn algorithms(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        for k in &self.0 {
            for a in k.sig_algs() {
                if !v.contains(a) {
                    v.push(*a);
                }
            }
        }
        v
    }

    /// The key that signs with `alg`.
    #[must_use]
    pub fn for_alg(&self, alg: &str) -> Option<&PrivateKey> {
        self.0.iter().find(|k| k.sig_algs().contains(&alg))
    }
}
