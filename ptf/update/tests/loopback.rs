// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The reference server over loopback with the client: a published
//! directory, the catalog, an order, the downloads verified, a group
//! manifest, a service request; plain HTTP, a proxy and TLS.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use certs::ca;
use certs::crypto::kdf::Argon2Params;
use certs::crypto::keys::KeyAlgorithm;
use certs::files::MemFiles;
use certs::paths::{LOCAL_CA_STORE, SYSTEM_STORE};
use certs::store::{CertStore, StoreKind};
use certs::x509::build::{CertSpec, Subject};
use ed25519_dalek::SigningKey;
use ptf::package::{self, Build};
use ptf::{PtfHeader, PtfKind, PtfRef};
use ptf_update::catalog::GroupKind;
use ptf_update::order::{OrderMode, OrderRequest, Why};
use ptf_update::publish::{self, GroupDef, Publication};
use ptf_update::{Client, Server};
use qsname::Name;
use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
use store::obj::{CreateParams, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn key() -> SigningKey {
    SigningKey::from_bytes(&[9u8; 32])
}

fn trusted() -> [[u8; 32]; 1] {
    [key().verifying_key().to_bytes()]
}

fn system(value: &str) -> (Store, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, 3));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
        now: NOW,
    };
    let mut s = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &fp, StoreConfig::default()).unwrap();
    let mut t = s.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    let p = |lib, n: &str, ty| CreateParams {
        library: lib,
        name: Name::parse(n).unwrap(),
        type_code: ty,
        attribute: String::new(),
        text: String::new(),
        owner: "QSYS".into(),
        system: "QS400".into(),
        now: NOW,
        by_system: true,
    };
    let lib = ops::create(&mut t, &p(qsys, "QSVCLIB", types::LIB)).unwrap();
    dtaara::create(
        &mut t,
        &p(lib, "LEVEL", types::DTAARA),
        DataAreaType::Char { len: 8 },
        &DataAreaValue::Char(value.into()),
    )
    .unwrap();
    t.commit().unwrap();
    (s, qsys)
}

fn make(id: &str, prereqs: &[&str]) -> Vec<u8> {
    let (mut s, q) = system(id);
    let k = key();
    let header = PtfHeader {
        ptf: id.into(),
        product: "5770SS1".into(),
        option: "0000".into(),
        release: "V1R0M0".into(),
        load: "5050".into(),
        target_release: "V1R0M0".into(),
        kind: PtfKind::Immediate,
        lic: false,
        migration: false,
        cover_only: false,
        created: NOW,
        prereqs: prereqs
            .iter()
            .map(|p| PtfRef {
                ptf: (*p).into(),
                product: "5770SS1".into(),
            })
            .collect(),
        coreqs: Vec::new(),
        supersedes: Vec::new(),
        exits: Vec::new(),
        object_count: 0,
        store_format: 0,
    };
    let b = Build {
        header,
        cover: format!("{id}: sets the level.\nMore text."),
        objects: vec![("QSVCLIB".into(), "LEVEL".into(), types::DTAARA)],
        lic: None,
        lic_b: None,
        key: &k,
        system: "QSBUILD".into(),
    };
    package::build(&mut s, q, &b, NOW).unwrap()
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("qs400-upd-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// A published directory: QS00001, QS00002 (needs QS00001), QS00003;
/// a group, a cumulative package and the HIPER group.
fn published(name: &str) -> PathBuf {
    let dir = tmp(name);
    let ptfs = vec![make("QS00001", &[]), make("QS00002", &["QS00001"]), make("QS00003", &[])];
    let g = |n: &str, kind, ptfs: &[&str]| GroupDef {
        name: n.into(),
        kind,
        level: 3,
        release: "V1R0M0".into(),
        text: format!("{n} group"),
        ptfs: ptfs.iter().map(|s| (*s).to_string()).collect(),
    };
    let groups = vec![
        g("QS99001", GroupKind::Group, &["QS00002"]),
        g("C26303110", GroupKind::Cumulative, &["QS00001", "QS00002", "QS00003"]),
        g("QS99100", GroupKind::Hiper, &["QS00003"]),
    ];
    let p = Publication {
        ptfs: &ptfs,
        hiper: &["QS00003".to_string()],
        groups: &groups,
        sequence: 7,
        now: NOW,
    };
    let cat = publish::publish(&dir, &p, &key()).unwrap();
    assert_eq!(cat.ptfs.len(), 3);
    assert!(cat.ptf("QS00003").unwrap().hiper);
    assert_eq!(cat.ptf("QS00001").unwrap().text, "QS00001: sets the level.");
    dir
}

fn order(items: &[&str]) -> OrderRequest {
    OrderRequest {
        release: "V1R0M0".into(),
        order: OrderMode::Required,
        reorder: false,
        cover_only: false,
        items: items.iter().map(|s| (*s).to_string()).collect(),
        have: Vec::new(),
    }
}

#[test]
fn an_order_over_plain_http() {
    let dir = published("plain");
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let run = Server::new(&dir).spawn(l, "/qs400").unwrap();
    let c = Client::new(&format!("http://{}/qs400", run.addr), "", None).unwrap();
    let cat = c.catalog(&trusted(), 0).unwrap();
    assert_eq!(cat.sequence, 7);
    // A catalog older than one seen is refused.
    assert_eq!(c.catalog(&trusted(), 8).unwrap_err().msgid(), "CPF8C32");
    let r = c.order(&order(&["QS99001"])).unwrap();
    assert_eq!(r.order, "0000001");
    assert_eq!(
        r.ptfs,
        [("QS00001".to_string(), Why::Requisite), ("QS00002".to_string(), Why::Group)]
    );
    assert_eq!(r.groups, [("QS99001".to_string(), 3)]);
    // The answer agrees with the client's own resolution.
    assert_eq!(ptf_update::order::resolve(&cat, &order(&["QS99001"])).ptfs, r.ptfs);
    for (id, _) in &r.ptfs {
        let bytes = c.ptf(cat.ptf(id).unwrap()).unwrap();
        let pkg = package::open(&bytes, &trusted()).unwrap();
        assert_eq!(&pkg.header.ptf, id);
    }
    let manifest = c.group("QS99001", &trusted()).unwrap();
    let (g, ok) = ptf::group::parse(&manifest, &trusted()).unwrap();
    assert!(ok);
    assert_eq!(g.level, 3);
    assert!(c.service("*TEST", "Cody").unwrap().starts_with("ok T"));
    let r = c.order(&order(&["*CUMPKG"])).unwrap();
    assert_eq!(r.order, "0000002");
    assert_eq!(r.ptfs.len(), 3);
    // Unknown things.
    assert_eq!(c.group("QS99999", &trusted()).unwrap_err().msgid(), "CPF8C24");
    let mut bad = cat.ptf("QS00001").unwrap().clone();
    bad.sha256 = "00".repeat(32);
    assert_eq!(c.ptf(&bad).unwrap_err().msgid(), "CPF8C32");
    run.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A forwarding proxy for absolute-form requests: one connection.
fn proxy(target: std::net::SocketAddr) -> (std::net::SocketAddr, std::thread::JoinHandle<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let t = std::thread::spawn(move || {
        let (mut c, _) = l.accept().unwrap();
        let req = ptf_update::http::read_request(&mut c).unwrap();
        let mut up = TcpStream::connect(target).unwrap();
        ptf_update::http::write_request(&mut up, &req, &target.to_string()).unwrap();
        let mut all = Vec::new();
        up.read_to_end(&mut all).unwrap();
        c.write_all(&all).unwrap();
        req.target
    });
    (addr, t)
}

#[test]
fn through_a_proxy() {
    let dir = published("proxy");
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let run = Server::new(&dir).spawn(l, "").unwrap();
    let (paddr, t) = proxy(run.addr);
    let c = Client::new(&format!("http://{}", run.addr), &paddr.to_string(), None).unwrap();
    assert_eq!(c.catalog(&trusted(), 0).unwrap().sequence, 7);
    assert_eq!(t.join().unwrap(), format!("http://{}/v1/catalog", run.addr));
    run.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn over_tls() {
    let files = MemFiles::new();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 120;
    let fast = Argon2Params {
        m_cost: 64,
        t_cost: 1,
        p_cost: 1,
    };
    ca::create(&files, "capass1", &Subject::cn("Update CA"), KeyAlgorithm::EcdsaP256, now, 3650, fast).unwrap();
    let cas = CertStore::open(&files, LOCAL_CA_STORE, "capass1").unwrap();
    let mut sys = CertStore::create_with(&files, SYSTEM_STORE, "system1", StoreKind::System, now, fast).unwrap();
    ca::issue(&cas, &mut sys, "UPD", &CertSpec::server("localhost", now, 30), KeyAlgorithm::EcdsaP256).unwrap();
    let policy = tls::policy::TlsPolicy::opsys();
    let roots = tls::config::trusted_roots(&sys, &certs::apps::CaTrust::All).unwrap();
    let id = tls::config::Identity::from_store(&sys, "UPD", now).unwrap();
    let server = tls::config::server_config(&id, &policy, tls::config::ClientAuthMode::None, roots.clone()).unwrap();
    let client = tls::config::client_config(&policy, roots, None, tls::config::ServerAuthMode::Full).unwrap();

    let dir = published("tls");
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let run = Server::new(&dir).with_tls(server).spawn(l, "").unwrap();
    let url = format!("https://localhost:{}", run.addr.port());
    let c = Client::new(&url, "", Some(client)).unwrap();
    let cat = c.catalog(&trusted(), 0).unwrap();
    let bytes = c.ptf(cat.ptf("QS00003").unwrap()).unwrap();
    assert_eq!(package::open(&bytes, &trusted()).unwrap().header.ptf, "QS00003");
    // Without a TLS configuration an https URL is refused.
    let none = Client::new(&url, "", None).unwrap();
    assert!(none.catalog(&trusted(), 0).is_err());
    run.stop();
    let _ = std::fs::remove_dir_all(&dir);
}
