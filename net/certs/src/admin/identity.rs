// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a server job gets for its application ID: the certificate chain
//! and key of the label assigned in `*SYSTEM` (the store's default when
//! none is), the client authentication setting and the CAs it trusts.
//! The storage service answers this (`cert_app`) because only it holds
//! the LIC key that opens the `*SYSTEM` stash.

use zeroize::Zeroizing;

use super::{Ctx, Keystore};
use crate::apps::{AppKind, AppRegistry, CaTrust, ClientAuth};
use crate::error::{CertError, Result};
use crate::x509::CertInfo;

/// An application's certificate and trust.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppIdentity {
    /// The label used.
    pub label: String,
    /// The chain to send, leaf first, DER.
    pub chain: Vec<Vec<u8>>,
    /// The private key, PKCS#8 DER.
    pub key: Zeroizing<Vec<u8>>,
    /// Client authentication.
    pub client_auth: ClientAuth,
}

/// The answer for one application ID.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppTrust {
    /// The certificate, when one is assigned (or the store has a
    /// default) and still valid.
    pub identity: Option<AppIdentity>,
    /// The CAs the application trusts, DER.
    pub roots: Vec<Vec<u8>>,
}

/// The identity and trust of application `id`; with an empty `id`, the
/// trusted CAs of `*SYSTEM` only. A missing `*SYSTEM` store answers
/// nothing, so callers keep their own certificate.
pub fn for_app(ctx: &Ctx<'_>, id: &str) -> Result<AppTrust> {
    let ks = Keystore::system();
    if !ctx.files.exists(&ks.path)? {
        if !id.is_empty() {
            AppRegistry::load(ctx.files)?.get(id)?;
        }
        return Ok(AppTrust::default());
    }
    let store = ctx.open(&ks)?;
    if id.is_empty() {
        return Ok(AppTrust {
            identity: None,
            roots: store.trusted_cas(),
        });
    }
    let reg = AppRegistry::load(ctx.files)?;
    let def = reg.get(id)?;
    let roots = match &def.ca_trust {
        CaTrust::All => store.trusted_cas(),
        CaTrust::Only(labels) => labels
            .iter()
            .filter_map(|l| store.get(l).ok())
            .filter(|e| e.trusted)
            .filter_map(|e| e.cert.clone())
            .collect(),
    };
    let label = match (&def.label, def.kind) {
        (Some(l), _) => Some(l.clone()),
        (None, AppKind::Server) => store.default_label().map(str::to_string),
        (None, AppKind::Client) => None,
    };
    let identity = match label {
        Some(label) => {
            let e = store.get(&label)?;
            if !e.has_identity() {
                return Err(CertError::NoPrivateKey(label));
            }
            let chain = store.chain(&label)?;
            let info = CertInfo::from_der(&chain[0])?;
            if !info.is_valid_at(ctx.now) {
                return Err(CertError::Invalid(format!(
                    "certificate {label} is not valid now (it is valid to {})",
                    crate::x509::iso_date(info.not_after)
                )));
            }
            Some(AppIdentity {
                key: Zeroizing::new(e.private_key()?.to_pkcs8_der()?.to_vec()),
                label,
                chain,
                client_auth: def.client_auth,
            })
        }
        None => None,
    };
    Ok(AppTrust { identity, roots })
}

#[cfg(test)]
mod tests {
    use super::super::entries::{self, Signer};
    use super::super::testing::*;
    use super::super::appids;
    use super::*;
    use crate::crypto::keys::KeyAlgorithm;
    use crate::x509::build::CertSpec;

    const ADMIN: &str = "QIBM_HTTP_SERVER_ADMIN";

    fn cert(w: &World, label: &str, days: u32) {
        entries::create(
            &w.ctx(),
            &Keystore::system(),
            label,
            &CertSpec::server("a.example", NOW, days),
            KeyAlgorithm::EcdsaP256,
            &Signer::SelfSigned,
        )
        .unwrap();
    }

    #[test]
    fn nothing_without_a_system_store() {
        let w = World::new();
        assert_eq!(for_app(&w.ctx(), ADMIN).unwrap(), AppTrust::default());
        assert!(matches!(for_app(&w.ctx(), "NOPE"), Err(CertError::NotRegistered(_))));
    }

    #[test]
    fn the_assigned_label_then_the_default() {
        let w = World::with_system();
        assert_eq!(for_app(&w.ctx(), ADMIN).unwrap().identity, None);
        cert(&w, "FIRST", 30);
        cert(&w, "ADMIN", 30);
        let t = for_app(&w.ctx(), ADMIN).unwrap();
        assert_eq!(t.identity.unwrap().label, "FIRST");
        appids::assign(&w.ctx(), ADMIN, Some("ADMIN")).unwrap();
        let id = for_app(&w.ctx(), ADMIN).unwrap().identity.unwrap();
        assert_eq!(id.label, "ADMIN");
        assert_eq!(id.chain.len(), 1);
        let key = crate::crypto::keys::PrivateKey::from_pkcs8_der(&id.key).unwrap();
        assert_eq!(key.algorithm(), KeyAlgorithm::EcdsaP256);
        // A client ID gets no default certificate.
        assert_eq!(for_app(&w.ctx(), "QIBM_QTMF_FTP_CLIENT").unwrap().identity, None);
        // Expired: refused, so the server says why and keeps its own.
        assert!(for_app(&w.at(NOW + 31 * DAY), ADMIN).is_err());
    }

    #[test]
    fn trust_follows_the_list() {
        let w = World::with_system();
        let mut s = w.ctx().open(&Keystore::system()).unwrap();
        let r = crate::roots::roots();
        s.add_ca("R0", r[0].der.clone(), true, NOW).unwrap();
        s.add_ca("R1", r[1].der.clone(), true, NOW).unwrap();
        s.add_ca("R2", r[2].der.clone(), false, NOW).unwrap();
        s.save(&w.files).unwrap();
        assert_eq!(for_app(&w.ctx(), "").unwrap().roots.len(), 2);
        assert_eq!(for_app(&w.ctx(), ADMIN).unwrap().roots.len(), 2);
        let c = appids::Change {
            ca_trust: Some(CaTrust::Only(vec!["R1".into(), "R2".into()])),
            ..appids::Change::default()
        };
        appids::change(&w.ctx(), ADMIN, &c).unwrap();
        assert_eq!(for_app(&w.ctx(), ADMIN).unwrap().roots, [r[1].der.clone()]);
    }
}
