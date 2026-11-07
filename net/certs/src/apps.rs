// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Application IDs: the names TLS servers and clients give when they ask
//! for a certificate (`GSK_OS400_APPLICATION_ID`, `SSL_Init_Application`),
//! each with its assigned certificate, client authentication and CA trust
//! settings. IBM keeps these in the registration facility and DCM; here
//! they are one JSON file at [`crate::paths::APP_DEFINITIONS`].
//!
//! The IDs are `QS400_*`. The `QIBM_*` IDs programs written for IBM i
//! give are aliases ([`ALIASES`], [`canonical`]): accepted wherever an
//! ID is, and a definition stored under one is renamed when read.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::error::{CertError, Result};
use crate::files::StoreFiles;
use crate::paths::APP_DEFINITIONS;

/// Server or client application.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppKind {
    /// Accepts connections; needs a certificate.
    Server,
    /// Makes connections; a certificate is optional.
    Client,
}

/// Whether a server asks its clients for certificates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientAuth {
    /// Never.
    #[default]
    None,
    /// Asks; a client without one is still accepted.
    Optional,
    /// Asks and requires one from a trusted CA.
    Required,
}

/// Which CAs an application trusts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaTrust {
    /// Every trusted CA of the store.
    #[default]
    All,
    /// Only these labels (IBM's "define the CA trust list").
    Only(Vec<String>),
}

/// One application definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppDef {
    /// The ID.
    pub id: String,
    /// Server or client.
    pub kind: AppKind,
    /// Description.
    pub text: String,
    /// Shipped with the system (cannot be removed).
    pub shipped: bool,
    /// The assigned certificate's label in the `*SYSTEM` store.
    pub label: Option<String>,
    /// Client authentication (servers).
    pub client_auth: ClientAuth,
    /// Which CAs to trust for the peer's certificate.
    pub ca_trust: CaTrust,
}

/// The shipped application IDs.
pub const SHIPPED: &[(&str, AppKind, &str)] = &[
    ("QS400_TELNET_SERVER", AppKind::Server, "Telnet server"),
    ("QS400_FTP_SERVER", AppKind::Server, "FTP server"),
    ("QS400_FTP_CLIENT", AppKind::Client, "FTP client"),
    ("QS400_SMTP_CLIENT", AppKind::Client, "SMTP client"),
    ("QS400_CENTRAL_SERVER", AppKind::Server, "Central host server"),
    ("QS400_DATABASE_SERVER", AppKind::Server, "Database host server"),
    ("QS400_DTAQ_SERVER", AppKind::Server, "Data queue host server"),
    ("QS400_NETPRT_SERVER", AppKind::Server, "Network print host server"),
    ("QS400_RMTCMD_SERVER", AppKind::Server, "Remote command host server"),
    ("QS400_SIGNON_SERVER", AppKind::Server, "Sign-on host server"),
    ("QS400_FILE_SERVER", AppKind::Server, "File host server"),
    ("QS400_DDM_DRDA_SERVER", AppKind::Server, "DDM and DRDA server"),
    ("QS400_DDM_DRDA_CLIENT", AppKind::Client, "DDM and DRDA client"),
    ("QS400_HTTP_SERVER_ADMIN", AppKind::Server, "HTTP administration server"),
    ("QS400_HTTP_SERVER_NAVIGATOR", AppKind::Server, "Navigator for QS/400"),
    ("QS400_HTTP_SERVER_ACCESSWEB", AppKind::Server, "QS/400 Access for Web"),
    ("QS400_DIRECTORY_SERVER", AppKind::Server, "Directory server"),
    ("QS400_DIRECTORY_CLIENT", AppKind::Client, "Directory client"),
    ("QS400_VPN_IKE_SERVER", AppKind::Server, "VPN key manager"),
    ("QS400_LPD_SERVER", AppKind::Server, "LPD server"),
    ("QS400_NTP_CLIENT", AppKind::Client, "NTP client"),
    ("QS400_ACCESS_SERVER", AppKind::Server, "QAP server (QS/400 Access)"),
];

/// The IBM i IDs (old) and the IDs they name here (new).
pub const ALIASES: &[(&str, &str)] = &[
    ("QIBM_QTV_TELNET_SERVER", "QS400_TELNET_SERVER"),
    ("QIBM_QTMF_FTP_SERVER", "QS400_FTP_SERVER"),
    ("QIBM_QTMF_FTP_CLIENT", "QS400_FTP_CLIENT"),
    ("QIBM_QTMS_SMTP_CLIENT", "QS400_SMTP_CLIENT"),
    ("QIBM_OS400_QZBS_SVR_CENTRAL", "QS400_CENTRAL_SERVER"),
    ("QIBM_OS400_QZBS_SVR_DATABASE", "QS400_DATABASE_SERVER"),
    ("QIBM_OS400_QZBS_SVR_DTAQ", "QS400_DTAQ_SERVER"),
    ("QIBM_OS400_QZBS_SVR_NETPRT", "QS400_NETPRT_SERVER"),
    ("QIBM_OS400_QZBS_SVR_RMTCMD", "QS400_RMTCMD_SERVER"),
    ("QIBM_OS400_QZBS_SVR_SIGNON", "QS400_SIGNON_SERVER"),
    ("QIBM_OS400_QZBS_SVR_FILE", "QS400_FILE_SERVER"),
    ("QIBM_QRW_SERVER_DDM_DRDA", "QS400_DDM_DRDA_SERVER"),
    ("QIBM_QRW_CLIENT_DDM_DRDA", "QS400_DDM_DRDA_CLIENT"),
    ("QIBM_GLD_DIRSRV_SERVER", "QS400_DIRECTORY_SERVER"),
    ("QIBM_GLD_DIRSRV_CLIENT", "QS400_DIRECTORY_CLIENT"),
    ("QIBM_QTOB_VPN_IKE", "QS400_VPN_IKE_SERVER"),
    ("QIBM_QSO_LPD_SERVER", "QS400_LPD_SERVER"),
    ("QIBM_QTOC_NTP_CLIENT", "QS400_NTP_CLIENT"),
];

/// The prefix of an HTTP instance's ID; the instance name follows.
pub const HTTP_SERVER_PREFIX: &str = "QS400_HTTP_SERVER_";

/// The prefix IBM i gives HTTP instances' IDs, an alias.
const OLD_HTTP_SERVER_PREFIX: &str = "QIBM_HTTP_SERVER_";

/// The ID of HTTP instance `instance`.
pub fn http_server(instance: &str) -> String {
    format!("{HTTP_SERVER_PREFIX}{}", instance.trim().to_ascii_uppercase())
}

/// The ID `id` names: the new one for an old one, else `id` itself.
pub fn canonical(id: &str) -> Cow<'_, str> {
    if let Some((_, new)) = ALIASES.iter().find(|(old, _)| *old == id) {
        return Cow::Borrowed(new);
    }
    match id.strip_prefix(OLD_HTTP_SERVER_PREFIX) {
        Some(rest) if !rest.is_empty() => Cow::Owned(format!("{HTTP_SERVER_PREFIX}{rest}")),
        _ => Cow::Borrowed(id),
    }
}

/// The definitions file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRegistry {
    /// Definitions, in ID order.
    pub apps: Vec<AppDef>,
}

impl AppRegistry {
    /// The shipped definitions, nothing assigned.
    pub fn shipped() -> Self {
        let mut r = Self::default();
        for (id, kind, text) in SHIPPED {
            r.apps.push(AppDef {
                id: (*id).to_string(),
                kind: *kind,
                text: (*text).to_string(),
                shipped: true,
                label: None,
                client_auth: ClientAuth::None,
                ca_trust: CaTrust::All,
            });
        }
        r.apps.sort_by(|a, b| a.id.cmp(&b.id));
        r
    }

    /// Read the file, or the shipped set when it does not exist.
    /// Definitions under an old ID are renamed (one already under the
    /// new ID wins unless it has no certificate); shipped IDs missing
    /// from the file are added back.
    pub fn load(files: &dyn StoreFiles) -> Result<Self> {
        let mut r = match files.read(APP_DEFINITIONS)? {
            Some(bytes) => serde_json::from_slice::<Self>(&bytes)
                .map_err(|e| CertError::Corrupt(format!("application definitions: {e}")))?,
            None => Self::shipped(),
        };
        r.migrate();
        for s in Self::shipped().apps {
            if r.get(&s.id).is_err() {
                r.apps.push(s);
            }
        }
        r.apps.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(r)
    }

    /// Rename definitions stored under an old ID.
    fn migrate(&mut self) {
        let old: Vec<AppDef> = self
            .apps
            .iter()
            .filter(|a| canonical(&a.id) != a.id.as_str())
            .cloned()
            .collect();
        for mut a in old {
            let new = canonical(&a.id).into_owned();
            self.apps.retain(|x| x.id != a.id);
            match self.apps.iter_mut().find(|x| x.id == new) {
                Some(x) if x.label.is_none() => {
                    a.id = new;
                    *x = a;
                }
                Some(_) => {}
                None => {
                    a.id = new;
                    self.apps.push(a);
                }
            }
        }
    }

    /// Write the file.
    pub fn save(&self, files: &dyn StoreFiles) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| CertError::Corrupt(format!("application definitions: {e}")))?;
        files.write(APP_DEFINITIONS, &bytes)?;
        Ok(())
    }

    /// The definition of `id` or its alias (case-sensitive, as IBM's
    /// are).
    pub fn get(&self, id: &str) -> Result<&AppDef> {
        let want = canonical(id);
        self.apps
            .iter()
            .find(|a| a.id == want)
            .ok_or_else(|| CertError::NotRegistered(id.to_string()))
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut AppDef> {
        let want = canonical(id);
        self.apps
            .iter_mut()
            .find(|a| a.id == want)
            .ok_or_else(|| CertError::NotRegistered(id.to_string()))
    }

    /// Register a new application; an old ID registers its new one.
    pub fn register(&mut self, id: &str, kind: AppKind, text: &str) -> Result<()> {
        check_id(id)?;
        let id = canonical(id).into_owned();
        if self.get(&id).is_ok() {
            return Err(CertError::AlreadyRegistered(id));
        }
        self.apps.push(AppDef {
            id,
            kind,
            text: text.to_string(),
            shipped: false,
            label: None,
            client_auth: ClientAuth::None,
            ca_trust: CaTrust::All,
        });
        self.apps.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(())
    }

    /// Remove an application the user registered.
    pub fn deregister(&mut self, id: &str) -> Result<()> {
        let a = self.get(id)?;
        if a.shipped {
            return Err(CertError::Invalid(format!(
                "{} is shipped with the system",
                a.id
            )));
        }
        let gone = a.id.clone();
        self.apps.retain(|a| a.id != gone);
        Ok(())
    }

    /// Assign a certificate label (or none).
    pub fn assign(&mut self, id: &str, label: Option<&str>) -> Result<()> {
        self.get_mut(id)?.label = label.map(str::to_string);
        Ok(())
    }

    /// Set client authentication.
    pub fn set_client_auth(&mut self, id: &str, auth: ClientAuth) -> Result<()> {
        let a = self.get_mut(id)?;
        if a.kind != AppKind::Server && auth != ClientAuth::None {
            return Err(CertError::Invalid(format!(
                "{} is a client application; client authentication applies to servers",
                a.id
            )));
        }
        a.client_auth = auth;
        Ok(())
    }

    /// Set the CA trust.
    pub fn set_ca_trust(&mut self, id: &str, trust: CaTrust) -> Result<()> {
        self.get_mut(id)?.ca_trust = trust;
        Ok(())
    }

    /// Set the description.
    pub fn set_text(&mut self, id: &str, text: &str) -> Result<()> {
        self.get_mut(id)?.text = text.to_string();
        Ok(())
    }

    /// Applications using `label`.
    pub fn users_of(&self, label: &str) -> Vec<&AppDef> {
        self.apps
            .iter()
            .filter(|a| a.label.as_deref() == Some(label))
            .collect()
    }

    /// Forget a label everywhere (after the certificate is removed).
    pub fn unassign_label(&mut self, label: &str) {
        for a in &mut self.apps {
            if a.label.as_deref() == Some(label) {
                a.label = None;
            }
            if let CaTrust::Only(list) = &mut a.ca_trust {
                list.retain(|l| l != label);
            }
        }
    }
}

/// IDs: 1 to 100 characters, no blanks or control characters.
pub fn check_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id.len() <= 100
        && id
            .chars()
            .all(|c| c.is_ascii_graphic() && c != '(' && c != ')');
    if ok {
        Ok(())
    } else {
        Err(CertError::Invalid(format!(
            "application ID '{id}' is not valid"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::MemFiles;

    #[test]
    fn shipped_ids_load_assign_and_persist() {
        let files = MemFiles::new();
        let mut r = AppRegistry::load(&files).unwrap();
        assert_eq!(r.apps.len(), SHIPPED.len());
        assert!(r.get("QS400_TELNET_SERVER").is_ok());
        assert!(matches!(r.get("NOPE"), Err(CertError::NotRegistered(_))));
        r.assign("QS400_TELNET_SERVER", Some("QS400")).unwrap();
        r.set_client_auth("QS400_TELNET_SERVER", ClientAuth::Optional)
            .unwrap();
        assert!(
            r.set_client_auth("QS400_FTP_CLIENT", ClientAuth::Required)
                .is_err()
        );
        r.register("MYAPP_SERVER", AppKind::Server, "mine").unwrap();
        assert!(matches!(
            r.register("MYAPP_SERVER", AppKind::Server, ""),
            Err(CertError::AlreadyRegistered(_))
        ));
        assert!(r.deregister("QS400_TELNET_SERVER").is_err());
        r.save(&files).unwrap();

        let back = AppRegistry::load(&files).unwrap();
        assert_eq!(back, r);
        assert_eq!(back.users_of("QS400").len(), 1);
        let mut back = back;
        back.unassign_label("QS400");
        assert!(back.users_of("QS400").is_empty());
        back.deregister("MYAPP_SERVER").unwrap();
        assert_eq!(back.apps.len(), SHIPPED.len());
    }

    #[test]
    fn a_file_missing_shipped_ids_gets_them_back() {
        let files = MemFiles::new();
        files.write(APP_DEFINITIONS, br#"{"apps":[]}"#).unwrap();
        let r = AppRegistry::load(&files).unwrap();
        assert_eq!(r.apps.len(), SHIPPED.len());
    }

    #[test]
    fn every_old_id_names_a_shipped_one() {
        for (old, new) in ALIASES {
            assert!(SHIPPED.iter().any(|s| s.0 == *new), "{old}");
            assert_eq!(canonical(old), *new);
        }
        assert_eq!(canonical("QIBM_HTTP_SERVER_ADMIN"), "QS400_HTTP_SERVER_ADMIN");
        assert_eq!(canonical("QIBM_HTTP_SERVER_SHOP"), "QS400_HTTP_SERVER_SHOP");
        assert_eq!(canonical("QIBM_HTTP_SERVER_"), "QIBM_HTTP_SERVER_");
        assert_eq!(canonical("QIBM_MYAPP"), "QIBM_MYAPP");
        assert_eq!(http_server("shop"), "QS400_HTTP_SERVER_SHOP");
    }

    #[test]
    fn old_ids_are_accepted_everywhere() {
        let mut r = AppRegistry::shipped();
        r.assign("QIBM_QTV_TELNET_SERVER", Some("T")).unwrap();
        assert_eq!(r.get("QS400_TELNET_SERVER").unwrap().label.as_deref(), Some("T"));
        assert_eq!(r.get("QIBM_QTV_TELNET_SERVER").unwrap().id, "QS400_TELNET_SERVER");
        r.register("QIBM_HTTP_SERVER_SHOP", AppKind::Server, "Shop").unwrap();
        assert!(r.get("QS400_HTTP_SERVER_SHOP").is_ok());
        assert!(matches!(
            r.register("QS400_HTTP_SERVER_SHOP", AppKind::Server, ""),
            Err(CertError::AlreadyRegistered(_))
        ));
        r.deregister("QIBM_HTTP_SERVER_SHOP").unwrap();
        assert!(r.get("QS400_HTTP_SERVER_SHOP").is_err());
    }

    /// A file written before the rename keeps its assignments.
    #[test]
    fn stored_old_ids_are_renamed() {
        let files = MemFiles::new();
        let old = |id: &str, shipped: bool, label: Option<&str>| AppDef {
            id: id.into(),
            kind: AppKind::Server,
            text: "t".into(),
            shipped,
            label: label.map(str::to_string),
            client_auth: ClientAuth::Optional,
            ca_trust: CaTrust::All,
        };
        let file = AppRegistry {
            apps: vec![
                old("QIBM_HTTP_SERVER_ADMIN", true, Some("ADMIN-CERT01")),
                old("QIBM_HTTP_SERVER_WEB1", false, Some("WEB")),
                old("QIBM_QTV_TELNET_SERVER", true, None),
                old("QS400_TELNET_SERVER", true, Some("TN")),
                old("QIBM_MYAPP", false, None),
            ],
        };
        file.save(&files).unwrap();
        let r = AppRegistry::load(&files).unwrap();
        assert_eq!(r.apps.len(), SHIPPED.len() + 2);
        assert!(r.apps.iter().all(|a| !a.id.starts_with("QIBM_") || a.id == "QIBM_MYAPP"));
        let admin = r.get("QS400_HTTP_SERVER_ADMIN").unwrap();
        assert_eq!((admin.label.as_deref(), admin.client_auth), (Some("ADMIN-CERT01"), ClientAuth::Optional));
        let web = r.get("QS400_HTTP_SERVER_WEB1").unwrap();
        assert_eq!((web.label.as_deref(), web.shipped), (Some("WEB"), false));
        assert_eq!(r.get("QS400_TELNET_SERVER").unwrap().label.as_deref(), Some("TN"));
        r.save(&files).unwrap();
        assert_eq!(AppRegistry::load(&files).unwrap(), r);
    }
}
