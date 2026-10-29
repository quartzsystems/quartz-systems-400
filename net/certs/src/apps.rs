// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Application IDs: the names TLS servers and clients give when they ask
//! for a certificate (`GSK_OS400_APPLICATION_ID`, `SSL_Init_Application`),
//! each with its assigned certificate, client authentication and CA trust
//! settings. IBM keeps these in the registration facility and DCM; here
//! they are one JSON file at [`crate::paths::APP_DEFINITIONS`].

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
    /// The ID, IBM's name where one exists.
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

/// The shipped application IDs. Names are IBM's; doubts are listed in
/// `docs/NET_OPEN_QUESTIONS.md` under "Certificates".
pub const SHIPPED: &[(&str, AppKind, &str)] = &[
    ("QIBM_QTV_TELNET_SERVER", AppKind::Server, "Telnet server"),
    ("QIBM_QTMF_FTP_SERVER", AppKind::Server, "FTP server"),
    ("QIBM_QTMF_FTP_CLIENT", AppKind::Client, "FTP client"),
    ("QIBM_QTMS_SMTP_CLIENT", AppKind::Client, "SMTP client"),
    (
        "QIBM_OS400_QZBS_SVR_CENTRAL",
        AppKind::Server,
        "Central host server",
    ),
    (
        "QIBM_OS400_QZBS_SVR_DATABASE",
        AppKind::Server,
        "Database host server",
    ),
    (
        "QIBM_OS400_QZBS_SVR_DTAQ",
        AppKind::Server,
        "Data queue host server",
    ),
    (
        "QIBM_OS400_QZBS_SVR_NETPRT",
        AppKind::Server,
        "Network print host server",
    ),
    (
        "QIBM_OS400_QZBS_SVR_RMTCMD",
        AppKind::Server,
        "Remote command host server",
    ),
    (
        "QIBM_OS400_QZBS_SVR_SIGNON",
        AppKind::Server,
        "Sign-on host server",
    ),
    (
        "QIBM_OS400_QZBS_SVR_FILE",
        AppKind::Server,
        "File host server",
    ),
    (
        "QIBM_QRW_SERVER_DDM_DRDA",
        AppKind::Server,
        "DDM and DRDA server",
    ),
    (
        "QIBM_QRW_CLIENT_DDM_DRDA",
        AppKind::Client,
        "DDM and DRDA client",
    ),
    (
        "QIBM_HTTP_SERVER_ADMIN",
        AppKind::Server,
        "HTTP administration server",
    ),
    (
        "QIBM_GLD_DIRSRV_SERVER",
        AppKind::Server,
        "Directory server",
    ),
    (
        "QIBM_GLD_DIRSRV_CLIENT",
        AppKind::Client,
        "Directory client",
    ),
    ("QIBM_QTOB_VPN_IKE", AppKind::Server, "VPN key manager"),
    ("QIBM_QSO_LPD_SERVER", AppKind::Server, "LPD server"),
    ("QIBM_QTOC_NTP_CLIENT", AppKind::Client, "NTP client"),
];

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

    /// Read the file, or the shipped set when it does not exist. Shipped
    /// IDs missing from the file are added back.
    pub fn load(files: &dyn StoreFiles) -> Result<Self> {
        let mut r = match files.read(APP_DEFINITIONS)? {
            Some(bytes) => serde_json::from_slice::<Self>(&bytes)
                .map_err(|e| CertError::Corrupt(format!("application definitions: {e}")))?,
            None => Self::shipped(),
        };
        for s in Self::shipped().apps {
            if r.get(&s.id).is_err() {
                r.apps.push(s);
            }
        }
        r.apps.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(r)
    }

    /// Write the file.
    pub fn save(&self, files: &dyn StoreFiles) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| CertError::Corrupt(format!("application definitions: {e}")))?;
        files.write(APP_DEFINITIONS, &bytes)?;
        Ok(())
    }

    /// The definition of `id` (case-sensitive, as IBM's are).
    pub fn get(&self, id: &str) -> Result<&AppDef> {
        self.apps
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| CertError::NotRegistered(id.to_string()))
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut AppDef> {
        self.apps
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| CertError::NotRegistered(id.to_string()))
    }

    /// Register a new application.
    pub fn register(&mut self, id: &str, kind: AppKind, text: &str) -> Result<()> {
        check_id(id)?;
        if self.get(id).is_ok() {
            return Err(CertError::AlreadyRegistered(id.to_string()));
        }
        self.apps.push(AppDef {
            id: id.to_string(),
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
                "{id} is shipped with the system"
            )));
        }
        self.apps.retain(|a| a.id != id);
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
                "{id} is a client application; client authentication applies to servers"
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
        assert!(r.get("QIBM_QTV_TELNET_SERVER").is_ok());
        assert!(matches!(r.get("NOPE"), Err(CertError::NotRegistered(_))));
        r.assign("QIBM_QTV_TELNET_SERVER", Some("QS400")).unwrap();
        r.set_client_auth("QIBM_QTV_TELNET_SERVER", ClientAuth::Optional)
            .unwrap();
        assert!(
            r.set_client_auth("QIBM_QTMF_FTP_CLIENT", ClientAuth::Required)
                .is_err()
        );
        r.register("MYAPP_SERVER", AppKind::Server, "mine").unwrap();
        assert!(matches!(
            r.register("MYAPP_SERVER", AppKind::Server, ""),
            Err(CertError::AlreadyRegistered(_))
        ));
        assert!(r.deregister("QIBM_QTV_TELNET_SERVER").is_err());
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
}
