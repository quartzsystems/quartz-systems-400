// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Store round trips: create, the local CA, application IDs, PKCS#12 and
//! PEM in and out, the wrong password, expiry queries, and the stash.

use certs::apps::{AppKind, AppRegistry, ClientAuth};
use certs::ca;
use certs::crypto::kdf::Argon2Params;
use certs::crypto::keys::KeyAlgorithm;
use certs::files::{DirFiles, MemFiles, StoreFiles};
use certs::paths::{LOCAL_CA_CERT, SYSTEM_STORE};
use certs::stash::{self, KeySealer};
use certs::store::{CertStore, EntryKind, Expiry, StoreKind};
use certs::x509::CertInfo;
use certs::x509::build::{CertKind, CertSpec, Subject};
use certs::{CertError, roots};

const NOW: u64 = 1_800_000_000;
const DAY: u64 = 86_400;

fn fast() -> Argon2Params {
    Argon2Params {
        m_cost: 64,
        t_cost: 1,
        p_cost: 1,
    }
}

fn system(files: &dyn StoreFiles) -> CertStore {
    CertStore::create_with(
        files,
        SYSTEM_STORE,
        "system1",
        StoreKind::System,
        NOW,
        fast(),
    )
    .unwrap()
}

#[test]
fn create_open_and_wrong_password() {
    let files = MemFiles::new();
    let mut s = system(&files);
    assert!(matches!(
        CertStore::create_with(
            &files,
            SYSTEM_STORE,
            "system1",
            StoreKind::System,
            NOW,
            fast()
        ),
        Err(CertError::Exists(_))
    ));
    assert!(CertStore::create_with(&files, "/x", "short", StoreKind::System, NOW, fast()).is_err());
    s.create_self_signed(
        "QS400",
        &CertSpec::server("qs400.local", NOW, 365),
        KeyAlgorithm::EcdsaP256,
    )
    .unwrap();
    s.save(&files).unwrap();
    assert!(matches!(
        CertStore::open(&files, SYSTEM_STORE, "wrong!"),
        Err(CertError::BadPassword)
    ));
    assert!(matches!(
        CertStore::open(&files, "/nowhere", "system1"),
        Err(CertError::NotFound(_))
    ));
    let back = CertStore::open(&files, SYSTEM_STORE, "system1").unwrap();
    assert_eq!(back.default_label(), Some("QS400"));
    let e = back.get("QS400").unwrap();
    assert_eq!(e.kind, EntryKind::Server);
    assert!(e.has_identity());
    assert_eq!(back.list().unwrap().len(), 1);

    // The key is not in the file in the clear.
    let bytes = files.read(SYSTEM_STORE).unwrap().unwrap();
    let key = e.key.as_ref().unwrap();
    assert!(!bytes.windows(32).any(|w| w == &key[..32]));
    let hex = hex::encode(&key[..32]);
    assert!(!String::from_utf8_lossy(&bytes).contains(&hex));

    // A new password reseals; the old one stops working.
    let mut back = back;
    assert!(matches!(
        back.change_password("nope", "system2", NOW),
        Err(CertError::BadPassword)
    ));
    back.change_password("system1", "system2", NOW).unwrap();
    back.save(&files).unwrap();
    assert!(CertStore::open(&files, SYSTEM_STORE, "system1").is_err());
    assert_eq!(
        CertStore::open(&files, SYSTEM_STORE, "system2")
            .unwrap()
            .default_label(),
        Some("QS400")
    );
}

#[test]
fn local_ca_issues_and_apps_assign() {
    let files = MemFiles::new();
    let info = ca::create(
        &files,
        "capass1",
        &Subject {
            common_name: "QS400 Local CA".into(),
            organization: Some("Quartz Systems".into()),
            ..Default::default()
        },
        KeyAlgorithm::EcdsaP384,
        NOW,
        3650,
        fast(),
    )
    .unwrap();
    assert!(info.is_ca);
    let pem = String::from_utf8(files.read(LOCAL_CA_CERT).unwrap().unwrap()).unwrap();
    assert!(pem.starts_with("-----BEGIN CERTIFICATE-----"));

    let ca_store = CertStore::open(&files, certs::paths::LOCAL_CA_STORE, "capass1").unwrap();
    let mut sys = system(&files);
    let spec = CertSpec {
        san: vec!["qs400.local".into(), "192.168.1.9".into()],
        ..CertSpec::server("qs400.local", NOW, 365)
    };
    let leaf = ca::issue(&ca_store, &mut sys, "TELNET", &spec, KeyAlgorithm::Rsa2048).unwrap();
    assert_eq!(leaf.issuer, "CN=QS400 Local CA,O=Quartz Systems");
    assert_eq!(leaf.key_algorithm, Some(KeyAlgorithm::Rsa2048));
    // The CA went in as a trusted CA, once.
    assert!(sys.get(ca::CA_LABEL).unwrap().trusted);
    assert!(!ca::trust(&ca_store, &mut sys, NOW).unwrap());
    let chain = sys.chain("TELNET").unwrap();
    assert_eq!(chain.len(), 1, "a self-signed root is not sent");
    assert_eq!(sys.trusted_cas().len(), 1);

    // Too long for the CA.
    let long = CertSpec::server("x", NOW, 4000);
    assert!(ca::issue(&ca_store, &mut sys, "LONG", &long, KeyAlgorithm::EcdsaP256).is_err());

    // A request signed by the CA, received into the store.
    let csr = sys
        .create_request(
            "FTP",
            &CertSpec::server("ftp.local", NOW, 30),
            KeyAlgorithm::EcdsaP256,
        )
        .unwrap();
    assert_eq!(sys.get("FTP").unwrap().kind, EntryKind::Request);
    let cert = ca::sign_request(&ca_store, &csr, CertKind::Server, NOW, 30).unwrap();
    let got = sys.receive_certificate("FTP", cert, vec![], NOW).unwrap();
    assert_eq!(got.common_name.as_deref(), Some("ftp.local"));
    assert_eq!(sys.get("FTP").unwrap().kind, EntryKind::Server);
    sys.save(&files).unwrap();

    // Application IDs.
    let mut apps = AppRegistry::load(&files).unwrap();
    apps.assign("QIBM_QTV_TELNET_SERVER", Some("TELNET"))
        .unwrap();
    apps.assign("QIBM_QTMF_FTP_SERVER", Some("FTP")).unwrap();
    apps.set_client_auth("QIBM_QTMF_FTP_SERVER", ClientAuth::Required)
        .unwrap();
    apps.register("MY_SERVER", AppKind::Server, "mine").unwrap();
    apps.save(&files).unwrap();
    let apps = AppRegistry::load(&files).unwrap();
    assert_eq!(
        apps.get("QIBM_QTV_TELNET_SERVER").unwrap().label.as_deref(),
        Some("TELNET")
    );
    assert_eq!(
        apps.get("QIBM_QTMF_FTP_SERVER").unwrap().client_auth,
        ClientAuth::Required
    );
    let def = apps.get("QIBM_QTV_TELNET_SERVER").unwrap();
    let sys = CertStore::open(&files, SYSTEM_STORE, "system1").unwrap();
    assert!(
        sys.get(def.label.as_deref().unwrap())
            .unwrap()
            .has_identity()
    );
}

#[test]
fn pkcs12_and_pem_round_trip_between_stores() {
    let files = MemFiles::new();
    let mut a = system(&files);
    a.create_self_signed(
        "ROOT",
        &CertSpec {
            subject: Subject::cn("Root"),
            kind: CertKind::Ca,
            san: vec![],
            not_before: NOW,
            days: 1000,
        },
        KeyAlgorithm::EcdsaP256,
    )
    .unwrap();
    let root = a.get("ROOT").unwrap();
    let (root_der, root_key) = (root.cert.clone().unwrap(), root.private_key().unwrap());
    // An intermediate signed by the root, a leaf signed by the intermediate.
    let int_key = certs::crypto::keys::PrivateKey::generate(KeyAlgorithm::EcdsaP256).unwrap();
    let int_spec = CertSpec {
        subject: Subject::cn("Intermediate"),
        kind: CertKind::Ca,
        san: vec![],
        not_before: NOW,
        days: 500,
    };
    let int_der = certs::x509::build::issue(&int_spec, &int_key, &root_der, &root_key).unwrap();
    let leaf_key = certs::crypto::keys::PrivateKey::generate(KeyAlgorithm::EcdsaP256).unwrap();
    let leaf_der = certs::x509::build::issue(
        &CertSpec::server("leaf.local", NOW, 90),
        &leaf_key,
        &int_der,
        &int_key,
    )
    .unwrap();
    a.add_identity(
        "LEAF",
        EntryKind::Server,
        leaf_der.clone(),
        vec![int_der.clone()],
        &leaf_key,
        NOW,
    )
    .unwrap();
    assert_eq!(a.chain("LEAF").unwrap().len(), 2);

    // Out as PKCS#12, in to another store: identity plus the root as a CA.
    let p12 = a.export_pkcs12("LEAF", "p12pass").unwrap();
    assert!(matches!(
        CertStore::create_with(&files, "/B.KDB", "store2", StoreKind::System, NOW, fast())
            .unwrap()
            .import_pkcs12("X", &p12, "wrong", NOW),
        Err(CertError::BadPassword)
    ));
    let mut b =
        CertStore::create_with(&files, "/C.KDB", "store3", StoreKind::System, NOW, fast()).unwrap();
    let done = b.import_pkcs12("LEAF2", &p12, "p12pass", NOW).unwrap();
    assert_eq!(done.identity.as_deref(), Some("LEAF2"));
    assert!(
        done.cas.is_empty(),
        "no root travels in the chain: {:?}",
        done.cas
    );
    let e = b.get("LEAF2").unwrap();
    assert_eq!(e.cert.as_deref(), Some(&leaf_der[..]));
    assert_eq!(e.chain, vec![int_der.clone()]);
    assert_eq!(
        b.private_key("LEAF2").unwrap().spki_der().unwrap(),
        leaf_key.spki_der().unwrap()
    );

    // PEM with the key, then the root alone as a CA.
    let pem = a.export_pem("LEAF", true).unwrap();
    assert_eq!(pem.matches("BEGIN CERTIFICATE").count(), 2);
    assert!(pem.contains("BEGIN PRIVATE KEY"));
    let mut c =
        CertStore::create_with(&files, "/D.KDB", "store4", StoreKind::System, NOW, fast()).unwrap();
    let done = c.import_pem("LEAF3", &pem, None, NOW).unwrap();
    assert_eq!(done.identity.as_deref(), Some("LEAF3"));
    assert_eq!(c.get("LEAF3").unwrap().chain.len(), 1);
    let root_pem = a.export_pem("ROOT", false).unwrap();
    let done = c.import_pem("ROOT", &root_pem, None, NOW).unwrap();
    assert_eq!(done.cas, vec!["ROOT".to_string()]);
    assert!(c.get("ROOT").unwrap().trusted);
    // Importing it again changes nothing.
    assert!(c.import_pem("ROOT", &root_pem, None, NOW).is_err());
    assert_eq!(c.chain("LEAF3").unwrap().len(), 2);
    // Key without matching certificate.
    let bad = format!(
        "{}{}",
        certs::pem::encode("CERTIFICATE", &root_der),
        certs::pem::encode("PRIVATE KEY", &leaf_key.to_pkcs8_der().unwrap())
    );
    assert!(c.import_pem("BAD", &bad, None, NOW).is_err());

    // Rename, trust, remove.
    c.rename("LEAF3", "LEAF4", NOW).unwrap();
    assert!(c.get("LEAF3").is_err());
    assert_eq!(c.default_label(), Some("LEAF4"));
    c.set_trusted("ROOT", false, NOW).unwrap();
    assert!(c.trusted_cas().is_empty());
    assert!(c.set_trusted("LEAF4", true, NOW).is_err());
    c.remove("LEAF4", NOW).unwrap();
    assert_eq!(c.default_label(), None);
    assert!(matches!(
        c.remove("LEAF4", NOW),
        Err(CertError::LabelNotFound(_))
    ));
}

#[test]
fn expiry_thresholds() {
    let files = MemFiles::new();
    let mut s = system(&files);
    for (label, days) in [("A", 400u32), ("B", 30), ("C", 7), ("D", 1), ("E", 2)] {
        s.create_self_signed(
            label,
            &CertSpec::server(label, NOW, days),
            KeyAlgorithm::EcdsaP256,
        )
        .unwrap();
    }
    let all = s.expiry(NOW).unwrap();
    assert_eq!(
        all.iter().map(|x| x.label.as_str()).collect::<Vec<_>>(),
        vec!["D", "E", "C", "B", "A"]
    );
    let warn = s.expiring(NOW).unwrap();
    assert_eq!(
        warn.iter()
            .map(|x| (x.label.as_str(), x.crossed()))
            .collect::<Vec<_>>(),
        vec![
            ("D", Some(1)),
            ("E", Some(7)),
            ("C", Some(7)),
            ("B", Some(30))
        ]
    );
    // A day later D has expired; A and E keep their status.
    let later = s.expiring(NOW + DAY + 1).unwrap();
    let d = later.iter().find(|x| x.label == "D").unwrap();
    assert_eq!(d.days_left, -1);
    assert_eq!(d.crossed(), Some(0));
    assert_eq!(Expiry::THRESHOLDS, [30, 7, 1]);
    assert!(
        !CertInfo::from_der(s.get("D").unwrap().cert.as_ref().unwrap())
            .unwrap()
            .is_valid_at(NOW + 2 * DAY)
    );
}

#[test]
fn stash_opens_the_store_and_roots_seed_it() {
    let files = MemFiles::new();
    let mut s = system(&files);
    let sealer = KeySealer::for_tests();
    assert!(matches!(
        s.stash(&files, "other1", &sealer),
        Err(CertError::BadPassword)
    ));
    s.stash(&files, "system1", &sealer).unwrap();
    let n = roots::seed(&mut s, NOW).unwrap();
    assert!(n > 100);
    assert_eq!(roots::seed(&mut s, NOW).unwrap(), 0);
    s.save(&files).unwrap();
    let back = CertStore::open_stashed(&files, SYSTEM_STORE, &sealer).unwrap();
    assert_eq!(back.trusted_cas().len(), n);
    assert!(roots::holds(&back, &roots::roots()[0]).unwrap());
    stash::remove(&files, SYSTEM_STORE).unwrap();
    assert!(CertStore::open_stashed(&files, SYSTEM_STORE, &sealer).is_err());
}

#[test]
fn stores_live_under_ibm_paths_on_a_directory() {
    let dir = std::env::temp_dir().join(format!("qs400-certs-store-{}", std::process::id()));
    let files = DirFiles::new(&dir);
    let mut s = system(&files);
    s.create_self_signed(
        "QS400",
        &CertSpec::server("qs400", NOW, 10),
        KeyAlgorithm::EcdsaP256,
    )
    .unwrap();
    s.save(&files).unwrap();
    assert!(
        dir.join("QIBM/UserData/ICSS/Cert/Server/DEFAULT.KDB")
            .is_file()
    );
    let back = CertStore::open(&files, "*SYSTEM".pipe(certs::paths::resolve), "system1").unwrap();
    assert!(back.get("QS400").is_ok());
    let _ = std::fs::remove_dir_all(dir);
}

trait Pipe: Sized {
    fn pipe<R>(self, f: impl FnOnce(Self) -> R) -> R {
        f(self)
    }
}
impl<T> Pipe for T {}

// ---- host tool checks: openssl, when present ----

fn openssl() -> Option<std::path::PathBuf> {
    let name = if cfg!(windows) {
        "openssl.exe"
    } else {
        "openssl"
    };
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join(name))
            .find(|f| f.is_file())
    })
}

fn tmpdir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("qs400-certs-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Our PKCS#12 opens in openssl, and openssl's (modern and legacy
/// ciphers) opens in ours. Skipped, with a note, when openssl is absent.
#[test]
fn openssl_reads_our_pkcs12_and_we_read_its() {
    let Some(ssl) = openssl() else {
        eprintln!("openssl not on PATH; skipping the interop check");
        return;
    };
    let dir = tmpdir("p12");
    let files = MemFiles::new();
    let mut s = system(&files);
    s.create_self_signed(
        "QS400",
        &CertSpec::server("qs400.local", NOW, 30),
        KeyAlgorithm::Rsa2048,
    )
    .unwrap();
    let p12 = s.export_pkcs12("QS400", "exp0rt").unwrap();
    let ours = dir.join("ours.p12");
    std::fs::write(&ours, &p12).unwrap();
    let out = std::process::Command::new(&ssl)
        .args(["pkcs12", "-in"])
        .arg(&ours)
        .args(["-passin", "pass:exp0rt", "-nodes", "-info"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout) + String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "openssl pkcs12 failed: {text}");
    assert!(text.contains("BEGIN PRIVATE KEY"), "{text}");
    assert!(text.contains("BEGIN CERTIFICATE"), "{text}");
    assert!(text.contains("friendlyName: QS400"), "{text}");

    // openssl makes one from our PEM, modern defaults and legacy ciphers.
    let pem = s.export_pem("QS400", true).unwrap();
    let pem_path = dir.join("ours.pem");
    std::fs::write(&pem_path, pem.as_bytes()).unwrap();
    for (tag, extra) in [
        ("modern", vec![]),
        ("legacy", vec!["-legacy"]),
        (
            "sha1",
            vec![
                "-macalg",
                "sha1",
                "-keypbe",
                "PBE-SHA1-3DES",
                "-certpbe",
                "PBE-SHA1-RC2-40",
            ],
        ),
    ] {
        let theirs = dir.join(format!("theirs-{tag}.p12"));
        let out = std::process::Command::new(&ssl)
            .args(["pkcs12", "-export", "-in"])
            .arg(&pem_path)
            .arg("-out")
            .arg(&theirs)
            .args(["-passout", "pass:imp0rt", "-name", "from openssl"])
            .args(&extra)
            .output()
            .unwrap();
        if !out.status.success() {
            eprintln!(
                "openssl pkcs12 -export ({tag}) failed, skipping: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            continue;
        }
        let data = std::fs::read(&theirs).unwrap();
        let mut t = CertStore::create_with(
            &files,
            &format!("/T-{tag}.KDB"),
            "store9",
            StoreKind::System,
            NOW,
            fast(),
        )
        .unwrap();
        assert!(
            matches!(
                t.import_pkcs12("IN", &data, "wrong", NOW),
                Err(CertError::BadPassword)
            ),
            "{tag}"
        );
        let done = t.import_pkcs12("IN", &data, "imp0rt", NOW).unwrap();
        assert_eq!(done.identity.as_deref(), Some("IN"), "{tag}");
        assert_eq!(
            t.private_key("IN").unwrap().spki_der().unwrap(),
            s.private_key("QS400").unwrap().spki_der().unwrap(),
            "{tag}"
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// openssl verifies a certificate the local CA issued, with the chain
/// our store builds.
#[test]
fn openssl_verifies_a_local_ca_chain() {
    let Some(ssl) = openssl() else {
        eprintln!("openssl not on PATH; skipping the verify check");
        return;
    };
    let dir = tmpdir("verify");
    let files = MemFiles::new();
    // openssl checks against the real clock, so everything is dated from it.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 60;
    ca::create(
        &files,
        "capass1",
        &Subject::cn("QS400 CA"),
        KeyAlgorithm::EcdsaP256,
        now,
        3650,
        fast(),
    )
    .unwrap();
    let ca_store = CertStore::open(&files, certs::paths::LOCAL_CA_STORE, "capass1").unwrap();
    let mut sys = system(&files);
    let spec = CertSpec::server("verify.local", now, 30);
    ca::issue(&ca_store, &mut sys, "V", &spec, KeyAlgorithm::EcdsaP256).unwrap();
    let leaf = dir.join("leaf.pem");
    let root = dir.join("ca.pem");
    std::fs::write(&leaf, sys.export_pem("V", false).unwrap().as_bytes()).unwrap();
    std::fs::write(&root, files.read(LOCAL_CA_CERT).unwrap().unwrap()).unwrap();
    let out = std::process::Command::new(&ssl)
        .args(["verify", "-CAfile"])
        .arg(&root)
        .arg(&leaf)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout) + String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success() && text.contains("OK"), "{text}");
    let _ = std::fs::remove_dir_all(dir);
}
