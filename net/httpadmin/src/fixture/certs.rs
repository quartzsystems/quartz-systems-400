// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sample system's certificates: real stores made by `net/certs`
//! (`certs::admin`, the local CA, a second CA whose key the system
//! holds) in memory, with a test sealer and cheap key derivation. The
//! stores are built once per process and copied into each fixture.

use std::sync::{Arc, OnceLock};

use certs::admin::entries::{self, Signer};
use certs::admin::{Ctx, Keystore, appids, keystores};
use certs::apps::AppKind;
use certs::ca;
use certs::crypto::kdf::Argon2Params;
use certs::crypto::keys::{KeyAlgorithm, PrivateKey};
use certs::files::{MemFiles, StoreFiles};
pub use certs::paths::SIGNATURE_VERIFICATION_STORE;
use certs::paths::{LOCAL_CA_STORE, OBJECT_SIGNING_STORE, SYSTEM_STORE};
use certs::stash::KeySealer;
use certs::CertError;
use certs::store::{CertStore, StoreKind};
use certs::x509::build::{self, CertKind, CertSpec, Subject};

use crate::certmgr::{Manager, save_user_stores};
use crate::env::{AppInfo, CertDetail, CertFailure, CertRow, Certs, Import, ImportSource, NewCert, StoreInfo};
use crate::util::{days, secs};

/// The second CA's store: a CA whose key this system holds.
pub const INTERNAL_CA_STORE: &str = "/QIBM/UserData/ICSS/Cert/CertAuth/QUARTZCA.KDB";
/// The password of every sample store (stashed; never shown).
const STORE_PASSWORD: &str = "fixture-store-1";

/// Key derivation cheap enough for tests.
const FAST: Argon2Params = Argon2Params {
    m_cost: 64,
    t_cost: 1,
    p_cost: 1,
};

/// The application IDs the sample system shows, in its order.
const SHOWN_APPS: &[&str] = &[
    "QIBM_HTTP_SERVER_ADMIN",
    "QIBM_HTTP_SERVER_NAVIGATOR",
    "QIBM_HTTP_SERVER_ACCESSWEB",
    "QIBM_HTTP_SERVER_WEBSTORE",
    "QIBM_HTTP_SERVER_API",
    "QIBM_HTTP_SERVER_INTRANET",
    "QIBM_QTV_TELNET_SERVER",
    "QIBM_QTMF_FTP_SERVER",
    "QIBM_QRW_SERVER_DDM_DRDA",
    "QIBM_OS400_QZBS_SVR_CENTRAL",
    "QS400_UPDATE_CLIENT",
];

/// The sample certificate manager.
pub struct FixtureCerts {
    /// The store files.
    pub files: MemFiles,
    sealer: KeySealer,
    /// CAs that sign: name (their label in `*SYSTEM`) and store path.
    signers: Vec<(String, String)>,
    /// The IFS, for imports from a path.
    ifs: Arc<dyn httpd::files::Files>,
}

fn subject(cn: &str) -> Subject {
    Subject {
        common_name: cn.into(),
        organization: Some("Quartz Systems".into()),
        country: Some("US".into()),
        ..Subject::default()
    }
}

/// A server certificate valid from `from` (y, m, d) to `to`, at the
/// fixture clock's time of day.
fn spec(cn: &str, kind: CertKind, from: (i64, u32, u32), to: (i64, u32, u32)) -> CertSpec {
    let n = days(to.0, to.1, to.2) - days(from.0, from.1, from.2);
    CertSpec {
        subject: subject(cn),
        kind,
        san: if kind == CertKind::Server { vec![cn.to_string()] } else { Vec::new() },
        not_before: secs(from.0, from.1, from.2, 14, 11, 2),
        days: u32::try_from(n).unwrap_or(1),
    }
}

/// Build the sample stores into `files`.
#[allow(clippy::too_many_lines)]
fn build_stores(files: &MemFiles, sealer: &KeySealer) -> Result<(), CertError> {
    let at = |now: u64| Ctx {
        files,
        sealer: Some(sealer),
        now,
        params: FAST,
    };
    let t0 = secs(2024, 1, 2, 9, 0, 0);
    let c = at(t0);
    let sys = Keystore::new(SYSTEM_STORE, STORE_PASSWORD);
    keystores::create(&c, &sys, true, false)?;
    keystores::create(&c, &Keystore::new(OBJECT_SIGNING_STORE, STORE_PASSWORD), true, false)?;
    keystores::create(&c, &Keystore::new(SIGNATURE_VERIFICATION_STORE, STORE_PASSWORD), true, false)?;
    keystores::create(&c, &Keystore::new("/www/webstore/conf/webstore.kdb", STORE_PASSWORD), true, false)?;

    // The local CA, trusted by *SYSTEM.
    keystores::create_local_ca(&c, STORE_PASSWORD, &subject("QSHQ400 Local CA"), KeyAlgorithm::EcdsaP256, 3650, true)?;
    let sys = Keystore::new(SYSTEM_STORE, "");
    let rename = |from: &str, to: &str| {
        entries::change(
            &c,
            &sys,
            from,
            &entries::Change {
                new_label: Some(to.into()),
                ..entries::Change::default()
            },
        )
    };
    rename(ca::CA_LABEL, "QSHQ400 Local CA")?;

    // A second CA whose key the system holds.
    let mut internal = CertStore::create_with(files, INTERNAL_CA_STORE, STORE_PASSWORD, StoreKind::CertAuth, t0, FAST)?;
    let ca_spec = CertSpec {
        subject: subject("Quartz Internal Certificate Authority"),
        kind: CertKind::Ca,
        san: Vec::new(),
        not_before: t0,
        days: 3650,
    };
    internal.create_self_signed(ca::CA_LABEL, &ca_spec, KeyAlgorithm::EcdsaP256)?;
    internal.save(files)?;
    internal.stash(files, STORE_PASSWORD, sealer)?;

    // *SYSTEM's six certificates, in the order they were added.
    let issue = |label: &str, s: &CertSpec, alg: KeyAlgorithm| -> Result<(), CertError> {
        let ca_store = CertStore::open_stashed(files, INTERNAL_CA_STORE, sealer)?;
        let mut target = c.open(&sys)?;
        ca::issue(&ca_store, &mut target, label, s, alg)?;
        target.save(files)
    };
    let server = CertKind::Server;
    issue("*.quartz.example", &spec("*.quartz.example", server, (2025, 1, 2), (2028, 1, 2)), KeyAlgorithm::Rsa2048)?;
    rename(ca::CA_LABEL, "Quartz Internal CA")?;
    issue(
        "webstore.quartz.example",
        &spec("webstore.quartz.example", server, (2025, 3, 14), (2027, 3, 14)),
        KeyAlgorithm::Rsa2048,
    )?;
    issue(
        "api.quartz.example",
        &spec("api.quartz.example", server, (2025, 5, 1), (2026, 11, 30)),
        KeyAlgorithm::EcdsaP256,
    )?;
    entries::create(
        &c,
        &sys,
        "QSHQ400 self-signed",
        &spec("QSHQ400", server, (2025, 6, 1), (2031, 10, 29)),
        KeyAlgorithm::Rsa2048,
        &Signer::SelfSigned,
    )?;
    issue(
        "navigator-2025",
        &spec("qshq400.quartz.example", server, (2025, 7, 1), (2026, 9, 1)),
        KeyAlgorithm::Rsa2048,
    )?;
    issue(
        "old-webstore",
        &spec("webstore.quartz.example", server, (2025, 8, 1), (2026, 3, 14)),
        KeyAlgorithm::Rsa2048,
    )?;
    let dft = entries::Change {
        default: true,
        ..entries::Change::default()
    };
    entries::change(&c, &sys, "webstore.quartz.example", &dft)?;

    // The signing stores.
    entries::create(
        &c,
        &Keystore::new(OBJECT_SIGNING_STORE, ""),
        "QSHQ400 code signing",
        &spec("QSHQ400 code signing", CertKind::Client, (2025, 2, 1), (2028, 2, 1)),
        KeyAlgorithm::EcdsaP256,
        &Signer::SelfSigned,
    )?;
    let mut ver = c.open(&Keystore::new(SIGNATURE_VERIFICATION_STORE, ""))?;
    for (i, cn) in ["Quartz Systems code signing", "QS/400 release signing", "Partner code signing"].iter().enumerate() {
        let key = PrivateKey::generate(KeyAlgorithm::EcdsaP256)?;
        let mut s = spec(cn, CertKind::Ca, (2025, 2, 1), (2030, 2, 1));
        s.not_before += i as u64;
        ver.add_ca(cn, build::self_signed(&s, &key)?, true, s.not_before)?;
    }
    ver.save(files)?;

    // WEBSTORE's own store: a development certificate and a request.
    let web = Keystore::new("/www/webstore/conf/webstore.kdb", "");
    entries::create(
        &c,
        &web,
        "webstore-dev",
        &spec("webstore-dev.quartz.example", server, (2026, 1, 5), (2027, 1, 5)),
        KeyAlgorithm::EcdsaP256,
        &Signer::SelfSigned,
    )?;
    entries::create(
        &c,
        &web,
        "webstore-2027-request",
        &spec("webstore.quartz.example", server, (2026, 10, 20), (2027, 10, 20)),
        KeyAlgorithm::EcdsaP256,
        &Signer::Request,
    )?;

    // Application IDs: the HTTP instances, the update client, the
    // descriptions and the assignments.
    for (id, kind, text) in [
        ("QIBM_HTTP_SERVER_NAVIGATOR", AppKind::Server, "Navigator (port 2005)"),
        ("QIBM_HTTP_SERVER_ACCESSWEB", AppKind::Server, "Access for Web (port 2006)"),
        ("QIBM_HTTP_SERVER_WEBSTORE", AppKind::Server, "Instance WEBSTORE"),
        ("QIBM_HTTP_SERVER_API", AppKind::Server, "Instance API"),
        ("QIBM_HTTP_SERVER_INTRANET", AppKind::Server, "Instance INTRANET"),
        ("QS400_UPDATE_CLIENT", AppKind::Client, "Fix download from Quartz update server"),
    ] {
        appids::add(&c, id, kind, text)?;
    }
    for (id, text) in [
        ("QIBM_HTTP_SERVER_ADMIN", "HTTP administration (port 2001)"),
        ("QIBM_QTV_TELNET_SERVER", "Telnet server (TLS port 992)"),
        ("QIBM_QTMF_FTP_SERVER", "FTP server"),
        ("QIBM_QRW_SERVER_DDM_DRDA", "DDM/DRDA server"),
        ("QIBM_OS400_QZBS_SVR_CENTRAL", "Host servers (central, database, signon)"),
    ] {
        let ch = appids::Change {
            text: Some(text.into()),
            ..appids::Change::default()
        };
        appids::change(&c, id, &ch)?;
    }
    for (id, label) in [
        ("QIBM_HTTP_SERVER_ADMIN", "QSHQ400 self-signed"),
        ("QIBM_HTTP_SERVER_NAVIGATOR", "*.quartz.example"),
        ("QIBM_HTTP_SERVER_ACCESSWEB", "*.quartz.example"),
        ("QIBM_HTTP_SERVER_WEBSTORE", "webstore.quartz.example"),
        ("QIBM_HTTP_SERVER_API", "api.quartz.example"),
        ("QIBM_HTTP_SERVER_INTRANET", "*.quartz.example"),
        ("QIBM_QTV_TELNET_SERVER", "*.quartz.example"),
        ("QIBM_QRW_SERVER_DDM_DRDA", "*.quartz.example"),
        ("QIBM_OS400_QZBS_SVR_CENTRAL", "*.quartz.example"),
        ("QS400_UPDATE_CLIENT", "QSHQ400 self-signed"),
    ] {
        appids::assign(&c, id, Some(label))?;
    }
    save_user_stores(&c, &[("WEBSTORE".into(), "/www/webstore/conf/webstore.kdb".into())])
        .map_err(|e| CertError::Invalid(e.to_string()))?;
    Ok(())
}

/// The sample stores' files, built once.
fn snapshot() -> &'static Vec<(String, Vec<u8>)> {
    static SNAP: OnceLock<Vec<(String, Vec<u8>)>> = OnceLock::new();
    SNAP.get_or_init(|| {
        let files = MemFiles::new();
        build_stores(&files, &KeySealer::for_tests()).expect("fixture certificate stores");
        files
            .paths()
            .into_iter()
            .map(|p| {
                let b = files.read(&p).ok().flatten().unwrap_or_default();
                (p, b)
            })
            .collect()
    })
}
impl FixtureCerts {
    /// The sample stores; imports from a path read `ifs`.
    pub fn new(ifs: Arc<dyn httpd::files::Files>) -> Self {
        let files = MemFiles::new();
        for (p, b) in snapshot() {
            files.write(p, b).expect("memory");
        }
        Self {
            files,
            sealer: KeySealer::for_tests(),
            signers: vec![
                ("Quartz Internal CA".into(), INTERNAL_CA_STORE.into()),
                ("QSHQ400 Local CA".into(), LOCAL_CA_STORE.into()),
            ],
            ifs,
        }
    }

    fn ctx(&self) -> Ctx<'_> {
        Ctx {
            files: &self.files,
            sealer: Some(&self.sealer),
            now: super::now(),
            params: FAST,
        }
    }

    /// The manager over the sample stores.
    pub fn manager(&self) -> Manager<'_> {
        Manager {
            ctx: self.ctx(),
            signers: self.signers.clone(),
            shown: Some(SHOWN_APPS),
        }
    }
}

impl Certs for FixtureCerts {
    fn stores(&self) -> Vec<StoreInfo> {
        self.manager().stores()
    }

    fn entries(&self, store: &str) -> Result<Vec<CertRow>, CertFailure> {
        self.manager().entries(store)
    }

    fn detail(&self, store: &str, label: &str) -> Result<CertDetail, CertFailure> {
        self.manager().detail(store, label)
    }

    fn export(&self, store: &str, label: &str, chain: bool) -> Result<Vec<u8>, CertFailure> {
        self.manager().export(store, label, chain)
    }

    fn create_store(&self, _user: &str, name: &str, path: &str, password: &str) -> Result<(), CertFailure> {
        self.manager().create_store(name, path, password)
    }

    fn change_password(&self, _user: &str, store: &str, old: &str, new: &str) -> Result<(), CertFailure> {
        self.manager().change_password(store, old, new)
    }

    fn delete_store(&self, _user: &str, store: &str) -> Result<(), CertFailure> {
        self.manager().delete_store(store)
    }

    fn signers(&self) -> Vec<String> {
        self.manager().signer_names()
    }

    fn create(&self, _user: &str, new: &NewCert) -> Result<CertDetail, CertFailure> {
        self.manager().create(new)
    }

    fn import(&self, _user: &str, import: &Import) -> Result<Vec<String>, CertFailure> {
        let mut i = import.clone();
        if let ImportSource::Path(p) = &import.source {
            let data = self
                .ifs
                .read(p)
                .map_err(|e| CertFailure::Invalid(format!("stream file {p} could not be read: {e}")))?;
            i.source = ImportSource::Data(data);
        }
        self.manager().import(&i)
    }

    fn set_default(&self, _user: &str, store: &str, label: &str) -> Result<(), CertFailure> {
        self.manager().set_default(store, label)
    }

    fn remove(&self, _user: &str, store: &str, label: &str) -> Result<(), CertFailure> {
        self.manager().remove(store, label)
    }

    fn apps(&self) -> Vec<AppInfo> {
        self.manager().apps()
    }

    fn assign(&self, _user: &str, app: &str, label: Option<&str>) -> Result<(), CertFailure> {
        self.manager().assign(app, label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::certmgr::wire::{self, CertCall, Remote, Transport};
    use crate::env::{EntryType, StoreType};

    /// The service's side in the process.
    struct Here(FixtureCerts);

    impl Transport for Here {
        fn call(&self, user: Option<&str>, call: Vec<u8>) -> Option<Vec<u8>> {
            let c = CertCall::decode(&call)?;
            assert_eq!(c.changes(), user.is_some());
            Some(wire::answer(&self.0.manager(), c).encode())
        }

        fn read_as(&self, _user: &str, path: &str) -> Result<Vec<u8>, String> {
            Err(format!("{path} not here"))
        }
    }

    #[test]
    fn through_the_wire() {
        let local = FixtureCerts::new(Arc::new(httpd::files::MemFiles::new()));
        let names: Vec<String> = local.stores().into_iter().map(|s| s.name).collect();
        let r = Remote(Here(FixtureCerts::new(Arc::new(httpd::files::MemFiles::new()))));
        assert_eq!(r.stores().into_iter().map(|s| s.name).collect::<Vec<_>>(), names);
        r.create_store("BOB", "SHOP", "/www/shop/shop.kdb", "pass-word-1").unwrap();
        let shop = r.stores().into_iter().find(|s| s.name == "SHOP").unwrap();
        assert_eq!((shop.kind, shop.path.as_str()), (StoreType::Other, "/www/shop/shop.kdb"));
        assert_eq!(
            r.change_password("BOB", "SHOP", "wrong-one", "x").unwrap_err(),
            CertFailure::BadPassword("SHOP".into())
        );
        assert!(matches!(r.delete_store("BOB", "*SYSTEM"), Err(CertFailure::Invalid(_))));
        r.delete_store("BOB", "SHOP").unwrap();
        assert!(r.stores().iter().all(|s| s.name != "SHOP"));
        assert_eq!(r.signers(), local.signers());
        assert_eq!(r.apps(), local.apps());
        assert_eq!(r.app_label("QIBM_HTTP_SERVER_API").as_deref(), Some("api.quartz.example"));
        let i = Import {
            store: "*SYSTEM".into(),
            source: ImportSource::Path("/tmp/x.pem".into()),
            format: crate::env::ImportFormat::Auto,
            password: None,
            label: None,
            ca: false,
            make_default: false,
        };
        assert!(matches!(r.import("BOB", &i), Err(CertFailure::Invalid(t)) if t.contains("could not be read")));
    }

    #[test]
    fn sample_stores() {
        let f = FixtureCerts::new(Arc::new(httpd::files::MemFiles::new()));
        let stores = f.stores();
        let counts: Vec<(&str, usize, Option<&str>)> =
            stores.iter().map(|s| (s.name.as_str(), s.certificates, s.default.as_deref())).collect();
        assert_eq!(
            counts,
            [
                ("*SYSTEM", 6, Some("webstore.quartz.example")),
                ("*OBJECTSIGNING", 1, Some("QSHQ400 code signing")),
                ("*SIGNATUREVERIFICATION", 3, None),
                ("WEBSTORE", 2, Some("webstore-dev")),
            ]
        );
        let rows = f.entries("*SYSTEM").unwrap();
        let api = rows.iter().find(|r| r.label == "api.quartz.example").unwrap();
        assert_eq!(api.key, "ECDSA P-256");
        assert_eq!(api.subject, "CN=api.quartz.example,O=Quartz Systems,C=US");
        assert_eq!(api.issuer, "CN=Quartz Internal Certificate Authority,O=Quartz Systems,C=US");
        assert_eq!((api.not_after - super::super::now()) / 86_400, 32);
        let cas: Vec<&str> = rows.iter().filter(|r| r.kind == EntryType::Ca).map(|r| r.label.as_str()).collect();
        assert_eq!(cas, ["QSHQ400 Local CA", "Quartz Internal CA"]);
        assert_eq!(f.app_label("QIBM_HTTP_SERVER_API").as_deref(), Some("api.quartz.example"));
        assert_eq!(f.apps().len(), 11);
    }
}
