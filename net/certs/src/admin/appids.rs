// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSASNCRT`, `QSADDAPPID`, `QSCHGAPPID`, `QSRMVAPPID` and the rows of
//! `QSWRKAPPID`. Assignments name labels of the `*SYSTEM` store.

use super::{Ctx, Keystore, Note, msgs};
use crate::apps::{AppDef, AppKind, AppRegistry, CaTrust, ClientAuth};
use crate::error::{CertError, Result};
use crate::store::EntryKind;

/// Assign certificate `label` of `*SYSTEM` to application `id`, or take
/// its assignment away with `None`. A server needs a server or client
/// certificate with its key; a client may have a client one.
pub fn assign(ctx: &Ctx<'_>, id: &str, label: Option<&str>) -> Result<Vec<Note>> {
    let mut reg = AppRegistry::load(ctx.files)?;
    let def = reg.get(id)?.clone();
    let Some(label) = label else {
        reg.assign(id, None)?;
        reg.save(ctx.files)?;
        return Ok(vec![Note::new(msgs::UNASSIGNED, &[id])]);
    };
    let store = ctx.open(&Keystore::system())?;
    let e = store.get(label)?;
    let fits = match def.kind {
        AppKind::Server => matches!(e.kind, EntryKind::Server | EntryKind::Client),
        AppKind::Client => matches!(e.kind, EntryKind::Client | EntryKind::Server),
    };
    if !fits || !e.has_identity() {
        return Err(CertError::Invalid(format!(
            "{label} is a {} entry; {id} needs a certificate with its private key",
            e.kind.name()
        )));
    }
    reg.assign(id, Some(label))?;
    reg.save(ctx.files)?;
    Ok(vec![Note::new(msgs::ASSIGNED, &[label, id])])
}

/// Register a new application ID.
pub fn add(ctx: &Ctx<'_>, id: &str, kind: AppKind, text: &str) -> Result<Vec<Note>> {
    let mut reg = AppRegistry::load(ctx.files)?;
    reg.register(id, kind, text)?;
    reg.save(ctx.files)?;
    Ok(vec![Note::new(msgs::APP_ADDED, &[id])])
}

/// Remove an application ID a user registered.
pub fn remove(ctx: &Ctx<'_>, id: &str) -> Result<Vec<Note>> {
    let mut reg = AppRegistry::load(ctx.files)?;
    reg.deregister(id)?;
    reg.save(ctx.files)?;
    Ok(vec![Note::new(msgs::APP_REMOVED, &[id])])
}

/// What `QSCHGAPPID` changes; `None` keeps the value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Change {
    /// Client authentication.
    pub client_auth: Option<ClientAuth>,
    /// The CA trust list.
    pub ca_trust: Option<CaTrust>,
    /// The description.
    pub text: Option<String>,
}

/// Change an application ID. A CA trust list names trusted CA entries
/// of `*SYSTEM`.
pub fn change(ctx: &Ctx<'_>, id: &str, c: &Change) -> Result<Vec<Note>> {
    let mut reg = AppRegistry::load(ctx.files)?;
    reg.get(id)?;
    if let Some(a) = c.client_auth {
        reg.set_client_auth(id, a)?;
    }
    if let Some(t) = &c.ca_trust {
        if let CaTrust::Only(labels) = t {
            let store = ctx.open(&Keystore::system())?;
            for l in labels {
                let e = store.get(l)?;
                if e.kind != EntryKind::Ca {
                    return Err(CertError::Invalid(format!("{l} is not a CA certificate")));
                }
            }
        }
        reg.set_ca_trust(id, t.clone())?;
    }
    if let Some(t) = &c.text
        && let Some(a) = reg.apps.iter_mut().find(|a| a.id == id)
    {
        a.text.clone_from(t);
    }
    reg.save(ctx.files)?;
    Ok(vec![Note::new(msgs::APP_CHANGED, &[id])])
}

/// Every application ID, in ID order.
pub fn list(ctx: &Ctx<'_>) -> Result<Vec<AppDef>> {
    Ok(AppRegistry::load(ctx.files)?.apps)
}

/// The names commands use for client authentication.
pub fn client_auth_name(a: ClientAuth) -> &'static str {
    match a {
        ClientAuth::None => "*NONE",
        ClientAuth::Optional => "*OPTIONAL",
        ClientAuth::Required => "*REQUIRED",
    }
}

/// Client authentication from its command name.
pub fn client_auth_from(s: &str) -> Option<ClientAuth> {
    match s.trim().to_ascii_uppercase().as_str() {
        "*NONE" => Some(ClientAuth::None),
        "*OPTIONAL" => Some(ClientAuth::Optional),
        "*REQUIRED" => Some(ClientAuth::Required),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::entries::{self, Signer};
    use super::super::testing::*;
    use super::*;
    use crate::crypto::keys::KeyAlgorithm;
    use crate::x509::build::CertSpec;

    const ADMIN: &str = "QIBM_HTTP_SERVER_ADMIN";

    fn with_cert(w: &World, label: &str) {
        entries::create(
            &w.ctx(),
            &Keystore::system(),
            label,
            &CertSpec::server("h.example", NOW, 30),
            KeyAlgorithm::EcdsaP256,
            &Signer::SelfSigned,
        )
        .unwrap();
    }

    #[test]
    fn assigns_and_unassigns() {
        let w = World::with_system();
        with_cert(&w, "ADM");
        assert_eq!(assign(&w.ctx(), ADMIN, Some("ADM")).unwrap(), [Note::new(msgs::ASSIGNED, &["ADM", ADMIN])]);
        assert_eq!(list(&w.ctx()).unwrap().iter().find(|a| a.id == ADMIN).unwrap().label.as_deref(), Some("ADM"));
        assert!(matches!(assign(&w.ctx(), ADMIN, Some("NONE")), Err(CertError::LabelNotFound(_))));
        assert!(matches!(assign(&w.ctx(), "NOT_AN_APP", Some("ADM")), Err(CertError::NotRegistered(_))));
        assign(&w.ctx(), ADMIN, None).unwrap();
        assert_eq!(list(&w.ctx()).unwrap().iter().find(|a| a.id == ADMIN).unwrap().label, None);
    }

    #[test]
    fn a_ca_is_not_an_identity() {
        let w = World::with_system();
        let ca = crate::roots::roots()[0].der.clone();
        let mut s = w.ctx().open(&Keystore::system()).unwrap();
        s.add_ca("ROOT", ca, true, NOW).unwrap();
        s.save(&w.files).unwrap();
        assert!(matches!(assign(&w.ctx(), ADMIN, Some("ROOT")), Err(CertError::Invalid(_))));
        // ... but it may be on a trust list.
        let c = Change {
            client_auth: Some(ClientAuth::Required),
            ca_trust: Some(CaTrust::Only(vec!["ROOT".into()])),
            text: Some("Admin".into()),
        };
        change(&w.ctx(), ADMIN, &c).unwrap();
        let a = list(&w.ctx()).unwrap().into_iter().find(|a| a.id == ADMIN).unwrap();
        assert_eq!((a.client_auth, a.text.as_str()), (ClientAuth::Required, "Admin"));
        with_cert(&w, "LEAF");
        let bad = Change {
            ca_trust: Some(CaTrust::Only(vec!["LEAF".into()])),
            ..Change::default()
        };
        assert!(change(&w.ctx(), ADMIN, &bad).is_err());
    }

    #[test]
    fn user_ids_come_and_go_shipped_ones_stay() {
        let w = World::with_system();
        add(&w.ctx(), "ACME_ORDERS", AppKind::Client, "Orders").unwrap();
        assert!(matches!(add(&w.ctx(), "ACME_ORDERS", AppKind::Client, ""), Err(CertError::AlreadyRegistered(_))));
        let c = Change {
            client_auth: Some(ClientAuth::Optional),
            ..Change::default()
        };
        assert!(change(&w.ctx(), "ACME_ORDERS", &c).is_err());
        remove(&w.ctx(), "ACME_ORDERS").unwrap();
        assert!(remove(&w.ctx(), ADMIN).is_err());
        assert_eq!(client_auth_from("*optional"), Some(ClientAuth::Optional));
        assert_eq!(client_auth_name(ClientAuth::Required), "*REQUIRED");
    }
}
