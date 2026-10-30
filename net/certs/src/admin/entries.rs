// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSCRTCRT`, `QSIMPCRT`, `QSEXPCRT`, `QSCHGCRT`, `QSDLTCRT` and the
//! rows of `QSWRKCRT`.

use super::{Ctx, Keystore, Note, msgs};
use crate::apps::{AppRegistry, CaTrust};
use crate::ca;
use crate::crypto::keys::KeyAlgorithm;
use crate::error::{CertError, Result};
use crate::pem;
use crate::store::{CertStore, EntryKind};
use crate::x509::CertInfo;
use crate::x509::build::CertSpec;

/// Who signs a new certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Signer {
    /// The certificate signs itself.
    SelfSigned,
    /// The local CA, opened with this password or its stash.
    LocalCa(Option<String>),
    /// Nobody yet: a key and a request to send to a CA.
    Request,
}

/// What `QSCRTCRT` made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Created {
    /// The completion messages.
    pub notes: Vec<Note>,
    /// For a request: the PKCS#10 request as PEM, for `CSR(path)`.
    pub csr_pem: Option<String>,
}

/// Make a certificate (or a request) with a new key as entry `label`.
pub fn create(
    ctx: &Ctx<'_>,
    ks: &Keystore,
    label: &str,
    spec: &CertSpec,
    alg: KeyAlgorithm,
    signer: &Signer,
) -> Result<Created> {
    let mut store = ctx.open(ks)?;
    let shown = ks.shown();
    let out = match signer {
        Signer::SelfSigned => {
            store.create_self_signed(label, spec, alg)?;
            Created {
                notes: vec![Note::new(msgs::CREATED, &[label, shown])],
                csr_pem: None,
            }
        }
        Signer::LocalCa(password) => {
            let ca_ks = Keystore::new(super::LOCAL_CA_NAME, password.as_deref().unwrap_or(""));
            let ca_store = ctx.open(&ca_ks)?;
            ca::issue(&ca_store, &mut store, label, spec, alg)?;
            Created {
                notes: vec![Note::new(msgs::CREATED, &[label, shown])],
                csr_pem: None,
            }
        }
        Signer::Request => {
            let csr = store.create_request(label, spec, alg)?;
            Created {
                notes: vec![Note::new(msgs::REQUEST_CREATED, &[label, shown])],
                csr_pem: Some(pem::encode("CERTIFICATE REQUEST", &csr)),
            }
        }
    };
    store.save(ctx.files)?;
    Ok(out)
}

/// The format of a file to import.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportFormat {
    /// PEM when the data is text with a `-----BEGIN` line, else PKCS#12.
    Auto,
    /// PEM.
    Pem,
    /// PKCS#12.
    Pkcs12,
}

/// Import `data`. A certificate with its key becomes entry `label`; CA
/// certificates without a key become CA entries (trusted with `trust`);
/// a certificate for a waiting request named `label` completes it.
pub fn import(
    ctx: &Ctx<'_>,
    ks: &Keystore,
    label: &str,
    data: &[u8],
    format: ImportFormat,
    password: Option<&str>,
    trust: bool,
) -> Result<Vec<Note>> {
    let mut store = ctx.open(ks)?;
    let pem_text = match format {
        ImportFormat::Pkcs12 => None,
        ImportFormat::Pem => Some(
            std::str::from_utf8(data).map_err(|_| CertError::Invalid("the PEM file is not text".into()))?,
        ),
        ImportFormat::Auto => std::str::from_utf8(data).ok().filter(|t| t.contains("-----BEGIN ")),
    };
    let shown = ks.shown();
    let mut notes = Vec::new();
    if let Some(text) = pem_text {
        let waiting = store.get(label).is_ok_and(|e| e.kind == EntryKind::Request);
        let blocks = pem::blocks(text)?;
        let has_key = blocks.iter().any(|b| b.label.contains("PRIVATE KEY"));
        if waiting && !has_key {
            let mut certs: Vec<Vec<u8>> = blocks
                .into_iter()
                .filter(|b| b.label == "CERTIFICATE")
                .map(|b| b.der)
                .collect();
            if certs.is_empty() {
                return Err(CertError::Invalid("no certificate in the PEM text".into()));
            }
            let leaf = certs.remove(0);
            store.receive_certificate(label, leaf, certs, ctx.now)?;
            notes.push(Note::new(msgs::IMPORTED, &[label, shown]));
        } else {
            let done = store.import_pem(label, text, password, ctx.now)?;
            notes.extend(imported(&mut store, &done, trust, shown, ctx.now)?);
        }
    } else {
        let done = store.import_pkcs12(label, data, password.unwrap_or(""), ctx.now)?;
        notes.extend(imported(&mut store, &done, trust, shown, ctx.now)?);
    }
    store.save(ctx.files)?;
    Ok(notes)
}

fn imported(
    store: &mut CertStore,
    done: &crate::store::Imported,
    trust: bool,
    shown: &str,
    now: u64,
) -> Result<Vec<Note>> {
    let mut notes = Vec::new();
    if let Some(l) = &done.identity {
        notes.push(Note::new(msgs::IMPORTED, &[l, shown]));
    }
    for l in &done.cas {
        if !trust {
            store.set_trusted(l, false, now)?;
        }
        notes.push(Note::new(msgs::IMPORTED, &[l, shown]));
    }
    Ok(notes)
}

/// The format of an export.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    /// PKCS#12 sealed with this password.
    Pkcs12(String),
    /// PEM: the certificate and its chain, with the key when asked.
    Pem {
        /// Add the private key, unencrypted.
        with_key: bool,
    },
    /// A waiting request's PKCS#10, PEM.
    Request,
}

/// Export entry `label`; the bytes for the stream file.
pub fn export(ctx: &Ctx<'_>, ks: &Keystore, label: &str, format: &ExportFormat) -> Result<Vec<u8>> {
    let store = ctx.open(ks)?;
    let e = store.get(label)?;
    match format {
        ExportFormat::Request => {
            let csr = e
                .csr
                .as_ref()
                .ok_or_else(|| CertError::Invalid(format!("{label} is not a waiting request")))?;
            Ok(pem::encode("CERTIFICATE REQUEST", csr).into_bytes())
        }
        _ if e.kind == EntryKind::Request => Err(CertError::Invalid(format!(
            "{label} is a request still waiting for its certificate"
        ))),
        ExportFormat::Pkcs12(pw) => {
            if pw.is_empty() {
                return Err(CertError::Invalid("a PKCS#12 file needs a password".into()));
            }
            store.export_pkcs12(label, pw)
        }
        ExportFormat::Pem { with_key } => Ok(store.export_pem(label, *with_key)?.as_bytes().to_vec()),
    }
}

/// What `QSCHGCRT` changes; `None` keeps the value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Change {
    /// A new label.
    pub new_label: Option<String>,
    /// A CA's trust.
    pub trusted: Option<bool>,
    /// Make it the store's default certificate.
    pub default: bool,
    /// New text.
    pub text: Option<String>,
}

/// Change entry `label`. In `*SYSTEM` a new label follows into the
/// application IDs that name it.
pub fn change(ctx: &Ctx<'_>, ks: &Keystore, label: &str, c: &Change) -> Result<Vec<Note>> {
    let mut store = ctx.open(ks)?;
    store.get(label)?;
    let mut now_label = label.to_string();
    if let Some(n) = &c.new_label
        && n != label
    {
        let was_default = store.default_label() == Some(label);
        store.rename(label, n, ctx.now)?;
        if was_default {
            store.set_default(n, ctx.now)?;
        }
        if ks.is_system() {
            let mut reg = AppRegistry::load(ctx.files)?;
            for a in &mut reg.apps {
                if a.label.as_deref() == Some(label) {
                    a.label = Some(n.clone());
                }
                if let CaTrust::Only(list) = &mut a.ca_trust {
                    for l in list.iter_mut().filter(|l| *l == label) {
                        l.clone_from(n);
                    }
                }
            }
            reg.save(ctx.files)?;
        }
        now_label.clone_from(n);
    }
    if let Some(t) = c.trusted {
        store.set_trusted(&now_label, t, ctx.now)?;
    }
    if c.default {
        store.set_default(&now_label, ctx.now)?;
    }
    if let Some(t) = &c.text {
        store.set_text(&now_label, t, ctx.now)?;
    }
    store.save(ctx.files)?;
    Ok(vec![Note::new(msgs::CHANGED, &[&now_label])])
}

/// Remove entry `label`; in `*SYSTEM` the application IDs that named it
/// lose it.
pub fn remove(ctx: &Ctx<'_>, ks: &Keystore, label: &str) -> Result<Vec<Note>> {
    let mut store = ctx.open(ks)?;
    store.remove(label, ctx.now)?;
    store.save(ctx.files)?;
    if ks.is_system() {
        let mut reg = AppRegistry::load(ctx.files)?;
        reg.unassign_label(label);
        reg.save(ctx.files)?;
    }
    Ok(vec![Note::new(msgs::REMOVED, &[label, ks.shown()])])
}

/// One certificate as `QSWRKCRT` lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The label.
    pub label: String,
    /// The kind.
    pub kind: EntryKind,
    /// The certificate's facts; none for a waiting request.
    pub info: Option<CertInfo>,
    /// Whole days left at the time of the listing; negative once expired.
    pub days_left: Option<i64>,
    /// A trusted CA.
    pub trusted: bool,
    /// The store's default certificate.
    pub is_default: bool,
    /// The entry's text.
    pub text: String,
    /// Application IDs assigned this certificate (`*SYSTEM` only).
    pub apps: Vec<String>,
}

impl Row {
    /// The status column: `Expired`, `Expires`(within 30 days),
    /// `Valid`, `Request`.
    pub fn status(&self) -> &'static str {
        match self.days_left {
            None => "Request",
            Some(d) if d < 0 => "Expired",
            Some(d) if d <= 30 => "Expiring",
            Some(_) => "Valid",
        }
    }
}

/// Every entry of the store, in label order.
pub fn list(ctx: &Ctx<'_>, ks: &Keystore) -> Result<Vec<Row>> {
    let store = ctx.open(ks)?;
    let reg = if ks.is_system() {
        Some(AppRegistry::load(ctx.files)?)
    } else {
        None
    };
    let mut out = Vec::new();
    for l in store.list()? {
        let apps = reg
            .as_ref()
            .map(|r| r.users_of(&l.label).iter().map(|a| a.id.clone()).collect())
            .unwrap_or_default();
        out.push(Row {
            days_left: l.info.as_ref().map(|i| i.days_left(ctx.now)),
            label: l.label,
            kind: l.kind,
            info: l.info,
            trusted: l.trusted,
            is_default: l.is_default,
            text: l.text,
            apps,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::testing::*;
    use super::super::{appids, keystores};
    use super::*;
    use crate::x509::build::{CertKind, Subject};

    fn server(cn: &str, days: u32) -> CertSpec {
        CertSpec::server(cn, NOW, days)
    }

    #[test]
    fn self_signed_listed_with_days_left() {
        let w = World::with_system();
        let ks = Keystore::system();
        let c = create(&w.ctx(), &ks, "WEB", &server("web.example", 90), KeyAlgorithm::EcdsaP256, &Signer::SelfSigned)
            .unwrap();
        assert_eq!(c.notes, [Note::new(msgs::CREATED, &["WEB", "*SYSTEM"])]);
        let rows = list(&w.ctx(), &ks).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].days_left, Some(90));
        assert!(rows[0].is_default);
        assert_eq!(rows[0].status(), "Valid");
        let later = list(&w.at(NOW + 70 * DAY), &ks).unwrap();
        assert_eq!(later[0].status(), "Expiring");
        let gone = list(&w.at(NOW + 91 * DAY), &ks).unwrap();
        assert_eq!(gone[0].status(), "Expired");
        assert!(matches!(
            create(&w.ctx(), &ks, "WEB", &server("x", 1), KeyAlgorithm::EcdsaP256, &Signer::SelfSigned),
            Err(CertError::LabelExists(_))
        ));
    }

    #[test]
    fn the_local_ca_signs_and_the_store_trusts_it() {
        let w = World::with_system();
        keystores::create_local_ca(&w.ctx(), "cacert1", &Subject::cn("Local CA"), KeyAlgorithm::EcdsaP256, 3650, true)
            .unwrap();
        let ks = Keystore::system();
        create(&w.ctx(), &ks, "SRV", &server("srv.example", 365), KeyAlgorithm::EcdsaP256, &Signer::LocalCa(None))
            .unwrap();
        let rows = list(&w.ctx(), &ks).unwrap();
        let srv = rows.iter().find(|r| r.label == "SRV").unwrap();
        assert_eq!(srv.info.as_ref().unwrap().issuer, "CN=Local CA");
        assert!(rows.iter().any(|r| r.kind == EntryKind::Ca && r.trusted));
    }

    #[test]
    fn a_request_goes_out_and_its_certificate_comes_back() {
        let w = World::with_system();
        keystores::create_local_ca(&w.ctx(), "cacert1", &Subject::cn("Local CA"), KeyAlgorithm::EcdsaP256, 3650, true)
            .unwrap();
        let ks = Keystore::system();
        let made = create(&w.ctx(), &ks, "REQ", &server("req.example", 30), KeyAlgorithm::Rsa2048, &Signer::Request)
            .unwrap();
        let csr_pem = made.csr_pem.unwrap();
        assert!(csr_pem.starts_with("-----BEGIN CERTIFICATE REQUEST-----"));
        assert_eq!(export(&w.ctx(), &ks, "REQ", &ExportFormat::Request).unwrap(), csr_pem.as_bytes());
        assert!(export(&w.ctx(), &ks, "REQ", &ExportFormat::Pem { with_key: false }).is_err());
        let csr = pem::blocks(&csr_pem).unwrap().remove(0).der;
        let ca_store = w.ctx().open(&Keystore::new("*LCLCA", "")).unwrap();
        let cert = ca::sign_request(&ca_store, &csr, CertKind::Server, NOW, 30).unwrap();
        let notes = import(&w.ctx(), &ks, "REQ", pem::encode("CERTIFICATE", &cert).as_bytes(), ImportFormat::Auto, None, true)
            .unwrap();
        assert_eq!(notes[0].msgid, msgs::IMPORTED);
        let rows = list(&w.ctx(), &ks).unwrap();
        assert_eq!(rows.iter().find(|r| r.label == "REQ").unwrap().kind, EntryKind::Server);
    }

    #[test]
    fn export_and_import_round_trip() {
        let w = World::with_system();
        let ks = Keystore::system();
        create(&w.ctx(), &ks, "A", &server("a.example", 30), KeyAlgorithm::EcdsaP256, &Signer::SelfSigned).unwrap();
        let p12 = export(&w.ctx(), &ks, "A", &ExportFormat::Pkcs12("pw1234".into())).unwrap();
        let pem_key = export(&w.ctx(), &ks, "A", &ExportFormat::Pem { with_key: true }).unwrap();
        assert!(String::from_utf8_lossy(&pem_key).contains("PRIVATE KEY"));
        assert!(export(&w.ctx(), &ks, "A", &ExportFormat::Pkcs12(String::new())).is_err());
        let other = Keystore::new("/home/o.kdb", "other1");
        keystores::create(&w.ctx(), &other, false, false).unwrap();
        import(&w.ctx(), &other, "FROMP12", &p12, ImportFormat::Auto, Some("pw1234"), true).unwrap();
        import(&w.ctx(), &other, "FROMPEM", &pem_key, ImportFormat::Pem, None, true).unwrap();
        let rows = list(&w.ctx(), &other).unwrap();
        let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["FROMP12", "FROMPEM"]);
        assert!(import(&w.ctx(), &other, "BAD", &p12, ImportFormat::Pkcs12, Some("wrong"), true).is_err());
        // A CA certificate alone, untrusted on request.
        let ca_der = crate::roots::roots()[0].der.clone();
        let notes = import(&w.ctx(), &other, "ROOT", pem::encode("CERTIFICATE", &ca_der).as_bytes(), ImportFormat::Auto, None, false)
            .unwrap();
        assert_eq!(notes[0].data[0], "ROOT");
        let rows = list(&w.ctx(), &other).unwrap();
        let root = rows.iter().find(|r| r.label == "ROOT").unwrap();
        assert!(!root.trusted);
    }

    #[test]
    fn rename_follows_into_the_assignments_and_remove_clears_them() {
        let w = World::with_system();
        let ks = Keystore::system();
        create(&w.ctx(), &ks, "OLD", &server("o.example", 30), KeyAlgorithm::EcdsaP256, &Signer::SelfSigned).unwrap();
        appids::assign(&w.ctx(), "QIBM_HTTP_SERVER_ADMIN", Some("OLD")).unwrap();
        let c = Change {
            new_label: Some("NEW".into()),
            text: Some("admin".into()),
            ..Change::default()
        };
        change(&w.ctx(), &ks, "OLD", &c).unwrap();
        let rows = list(&w.ctx(), &ks).unwrap();
        assert_eq!(rows[0].label, "NEW");
        assert_eq!(rows[0].text, "admin");
        assert!(rows[0].is_default);
        assert_eq!(rows[0].apps, ["QIBM_HTTP_SERVER_ADMIN"]);
        assert!(change(&w.ctx(), &ks, "NEW", &Change { trusted: Some(true), ..Change::default() }).is_err());
        remove(&w.ctx(), &ks, "NEW").unwrap();
        let reg = AppRegistry::load(&w.files).unwrap();
        assert_eq!(reg.get("QIBM_HTTP_SERVER_ADMIN").unwrap().label, None);
        assert!(matches!(remove(&w.ctx(), &ks, "NEW"), Err(CertError::LabelNotFound(_))));
    }
}
