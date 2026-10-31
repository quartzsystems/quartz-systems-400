// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The catalog codec and its signature, and order resolution.

use ed25519_dalek::SigningKey;
use ptf_update::catalog::{Catalog, CatalogGroup, CatalogPtf, GroupKind, PtfKind};
use ptf_update::order::{self, OrderMode, OrderRequest, OrderResult, Why};

fn key() -> SigningKey {
    SigningKey::from_bytes(&[5u8; 32])
}

fn ptf(id: &str, prereqs: &[&str], coreqs: &[&str], supersedes: &[&str]) -> CatalogPtf {
    let v = |x: &[&str]| x.iter().map(|s| (*s).to_string()).collect();
    CatalogPtf {
        id: id.into(),
        product: "5770SS1".into(),
        option: "0000".into(),
        release: "V1R0M0".into(),
        kind: PtfKind::Immediate,
        hiper: false,
        prereqs: v(prereqs),
        coreqs: v(coreqs),
        supersedes: v(supersedes),
        size: 10,
        sha256: "00".into(),
        text: format!("Fix \"{id}\""),
    }
}

fn group(name: &str, kind: GroupKind, level: u32, ptfs: &[&str]) -> CatalogGroup {
    CatalogGroup {
        name: name.into(),
        kind,
        level,
        release: "V1R0M0".into(),
        text: format!("{name} text"),
        ptfs: ptfs.iter().map(|s| (*s).to_string()).collect(),
        related: Vec::new(),
    }
}

fn catalog() -> Catalog {
    Catalog {
        format: 1,
        sequence: 3,
        generated: 1_793_318_400,
        products: Vec::new(),
        ptfs: vec![
            ptf("QS00001", &[], &[], &[]),
            ptf("QS00002", &["QS00001"], &[], &[]),
            ptf("QS00003", &[], &[], &[]),
            ptf("QS00004", &["QS00002"], &["QS00005"], &["QS00003"]),
            ptf("QS00005", &[], &["QS00004"], &[]),
            ptf("QS00006", &[], &[], &["QS00004"]),
            ptf("QS00007", &[], &[], &[]),
        ],
        groups: vec![
            group("QS99001", GroupKind::Group, 2, &["QS00002", "QS00007"]),
            group("C26100110", GroupKind::Cumulative, 1, &["QS00001"]),
            group("C26300110", GroupKind::Cumulative, 1, &["QS00001", "QS00007"]),
            group("QS99100", GroupKind::Hiper, 4, &["QS00007"]),
        ],
    }
}

#[test]
fn the_catalog_round_trips_signed() {
    let c = catalog();
    let text = c.sign(&key());
    let trusted = [key().verifying_key().to_bytes()];
    let back = Catalog::parse(&text, &trusted).unwrap();
    // The canonical form sorts; encoding again gives the same bytes.
    assert_eq!(back.encode(), c.encode());
    assert_eq!(back.ptf("QS00004").unwrap().coreqs, ["QS00005"]);
    assert_eq!(back.ptf("QS00001").unwrap().text, "Fix \"QS00001\"");
    // Another key, or one changed byte, does not verify.
    let other = [SigningKey::from_bytes(&[6; 32]).verifying_key().to_bytes()];
    assert_eq!(Catalog::parse(&text, &other).unwrap_err().msgid(), "CPF8C32");
    let tampered = text.replace("sequence = 3", "sequence = 9");
    assert!(Catalog::parse(&tampered, &trusted).is_err());
    assert!(Catalog::parse(&c.encode(), &trusted).is_err());
    assert_eq!(c.cumulative("V1R0M0").unwrap().name, "C26300110");
    assert_eq!(c.hiper("V1R0M0").unwrap().name, "QS99100");
}

fn req(items: &[&str], order: OrderMode, have: &[&str], reorder: bool) -> OrderRequest {
    OrderRequest {
        release: "V1R0M0".into(),
        order,
        reorder,
        cover_only: false,
        items: items.iter().map(|s| (*s).to_string()).collect(),
        have: have.iter().map(|s| (*s).to_string()).collect(),
    }
}

fn ids(r: &OrderResult) -> Vec<&str> {
    r.ptfs.iter().map(|(p, _)| p.as_str()).collect()
}

#[test]
fn required_orders_bring_requisites_first() {
    let c = catalog();
    let r = order::resolve(&c, &req(&["QS00002"], OrderMode::Required, &[], false));
    assert_eq!(ids(&r), ["QS00001", "QS00002"]);
    assert_eq!(r.ptfs[0].1, Why::Requisite);
    assert_eq!(r.ptfs[1].1, Why::Requested);
    // QS00003 is superseded by QS00004, itself by QS00006.
    let r = order::resolve(&c, &req(&["QS00003"], OrderMode::Required, &[], false));
    assert_eq!(ids(&r), ["QS00006"]);
    assert_eq!(r.ptfs[0].1, Why::Supersedes);
    // QS00004 asked for by *PTFID stays itself, without requisites.
    let r = order::resolve(&c, &req(&["QS00004"], OrderMode::PtfId, &[], false));
    assert_eq!(ids(&r), ["QS00004"]);
    // Co-requisites come together, prerequisites first.
    let mut c2 = c.clone();
    c2.ptfs.retain(|p| p.id != "QS00006");
    let r = order::resolve(&c2, &req(&["QS00005"], OrderMode::Required, &[], false));
    assert_eq!(ids(&r), ["QS00001", "QS00002", "QS00004", "QS00005"]);
}

#[test]
fn groups_cumulative_packages_and_hiper() {
    let c = catalog();
    let r = order::resolve(&c, &req(&["QS99001"], OrderMode::Required, &[], false));
    assert_eq!(ids(&r), ["QS00001", "QS00002", "QS00007"]);
    assert_eq!(r.groups, [("QS99001".to_string(), 2)]);
    let r = order::resolve(&c, &req(&["*CUMPKG", "*HIPER"], OrderMode::PtfId, &[], false));
    assert_eq!(ids(&r), ["QS00001", "QS00007"]);
    assert_eq!(r.groups.len(), 2);
    assert_eq!(r.groups[0].0, "C26300110");
    let r = order::resolve(&c, &req(&["QS00999", "QS99999"], OrderMode::Required, &[], false));
    assert!(r.ptfs.is_empty());
    assert_eq!(r.missing, ["QS00999", "QS99999"]);
}

#[test]
fn reorder_leaves_out_what_the_system_has() {
    let c = catalog();
    let r = order::resolve(&c, &req(&["QS00002"], OrderMode::Required, &["QS00001"], false));
    assert_eq!(ids(&r), ["QS00002"]);
    let r = order::resolve(&c, &req(&["QS00002"], OrderMode::Required, &["QS00001"], true));
    assert_eq!(ids(&r), ["QS00001", "QS00002"]);
    // A PTF superseded by one the system has is not ordered.
    let r = order::resolve(&c, &req(&["QS00003"], OrderMode::PtfId, &["QS00006"], false));
    assert!(r.ptfs.is_empty());
}

#[test]
fn requests_and_answers_round_trip() {
    let q = req(&["QS00002", "*CUMPKG"], OrderMode::PtfId, &["QS00001"], true);
    assert_eq!(OrderRequest::decode(&q.encode()).unwrap(), q);
    let mut r = order::resolve(&catalog(), &req(&["QS99001", "QS00999"], OrderMode::Required, &[], false));
    r.order = "0000042".into();
    assert_eq!(OrderResult::decode(&r.encode()).unwrap(), r);
    assert!(OrderRequest::decode("hello\n").is_err());
    assert!(OrderResult::decode("QS400ORDERED 1\nptf QS00001 maybe\n").is_err());
}
