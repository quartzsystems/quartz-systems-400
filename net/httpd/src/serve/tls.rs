// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A listener's TLS from its [`TlsSpec`]: the certificate assigned to
//! the application ID (the job fetches it from `*SYSTEM`), the
//! system's policy without the `SSLProtocolDisable` versions, and
//! `SSLClientAuth` over the application's own setting.

use std::sync::Arc;

use tls::config::{ClientAuthMode, Identity, server_config};
use tls::policy::{Protocol, TlsPolicy};
use tls::rustls::{RootCertStore, ServerConfig};

use super::site::TlsSpec;

/// The protocol an `SSLProtocolDisable` value names.
pub fn protocol(name: &str) -> Option<Protocol> {
    Protocol::from_name(&format!("*{}", name.trim().to_ascii_uppercase()))
}

/// The client authentication a spec asks for, `app` when it says
/// nothing.
pub fn client_auth(spec: &TlsSpec, app: ClientAuthMode) -> ClientAuthMode {
    match spec.client_auth {
        Some(1) => ClientAuthMode::Optional,
        Some(2) => ClientAuthMode::Required,
        Some(_) => ClientAuthMode::None,
        None => app,
    }
}

/// The server configuration of a listener.
pub fn config(
    spec: &TlsSpec,
    identity: &Identity,
    policy: &TlsPolicy,
    roots: RootCertStore,
    app: ClientAuthMode,
) -> Result<Arc<ServerConfig>, String> {
    let off: Vec<Protocol> = spec.disabled.iter().filter_map(|d| protocol(d)).collect();
    let keep: Vec<Protocol> = policy
        .protocols
        .iter()
        .copied()
        .filter(|p| !off.contains(p))
        .collect();
    let p = policy.restricted_to(&keep);
    server_config(identity, &p, client_auth(spec, app), roots)
        .map_err(|e| format!("{}: {e}", spec.app_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(protocol("TLSv1.2"), Some(Protocol::Tls12));
        assert_eq!(protocol("SSLv3"), Some(Protocol::Ssl3));
        let s = TlsSpec {
            app_id: "A".into(),
            disabled: Vec::new(),
            client_auth: Some(2),
        };
        assert_eq!(
            client_auth(&s, ClientAuthMode::None),
            ClientAuthMode::Required
        );
        let s = TlsSpec {
            client_auth: None,
            ..s
        };
        assert_eq!(
            client_auth(&s, ClientAuthMode::Optional),
            ClientAuthMode::Optional
        );
    }
}
