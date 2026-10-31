// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSPKGPRD manifests: good ones build, bad ones say where and why.

#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines
)]

mod common;

use common::{VENDOR, vendor, vendor_manifest};
use licpgm::model::definition::{Compliance, LngSel};
use licpgm::payload::{prddfn, prdlod};
use licpgm::pkg::{self, Options};
use licpgm::{LoadType, OptionNo, QualName};

#[test]
fn a_good_manifest_builds_its_objects() {
    let p = vendor();
    let d = &p.definition;
    assert_eq!(p.definition_name, QualName::new("P1AB1234", "INVPRD"));
    assert_eq!(d.options.len(), 2);
    assert_eq!(d.options[1].requires[0].product.as_str(), "1AB1234");
    assert_eq!(d.release_date.unwrap().to_string(), "2026-10-30");
    assert!(d.licensed);
    assert!(d.license_key.is_some());
    assert_eq!(d.licenses[0].compliance, Compliance::Keyed);
    assert_eq!(d.languages[1].load, LngSel::BaseOpt);
    let names: Vec<&str> = p.loads.iter().map(|(q, _)| q.name.as_str()).collect();
    assert_eq!(
        names,
        ["P3400C5001", "P3400L2924", "P3401C5101", "P3401L2924"]
    );
    let (_, base) = &p.loads[0];
    assert_eq!(base.libraries.len(), 2);
    assert_eq!(base.libraries[0].post_exit.as_deref(), Some("INVPOST"));
    assert_eq!(base.pre_exits[0], QualName::new("INVPRE", "INVPRD"));
    assert_eq!(base.objects.len(), 2);
    assert_eq!(base.objects[1].lib, "INVDTA");
    assert_eq!(base.directories[0].home, "INV");
    assert_eq!(
        base.agreements,
        ["/QIBM/ProdData/LicenseDoc/1AB1234_V1R0M0/2924.txt"]
    );
    assert_eq!(p.agreement.as_deref(), Some("Use it well."));
    let (_, lng) = &p.loads[1];
    assert_eq!(lng.key.load_type, LoadType::Lng);
    assert_eq!(lng.objects[0].name, "INVMSG");
    // Payloads decode to the same objects.
    assert_eq!(
        prddfn::decode(&p.definition_payload().unwrap()).unwrap(),
        *d
    );
    for ((q, l), (q2, b)) in p.loads.iter().zip(p.load_payloads().unwrap()) {
        assert_eq!(q, &q2);
        assert_eq!(&prdlod::decode(&b).unwrap(), l);
    }
    assert_eq!(d.option(OptionNo(1)).unwrap().feature.as_str(), "5101");
}

fn errors(text: &str) -> Vec<String> {
    pkg::parse(text, Options::default())
        .unwrap_err()
        .into_iter()
        .map(|e| e.to_string())
        .collect()
}

fn has(errs: &[String], at: &str) -> bool {
    errs.iter().any(|e| e.starts_with(at))
}

#[test]
fn bad_manifests_say_where() {
    let good = vendor_manifest();
    // Syntax.
    let e = errors("[product\nid=");
    assert!(e[0].starts_with("manifest: not a valid manifest"));
    // Unknown keys are refused.
    assert!(
        !errors(&good.replace("text = \"Inventory control\"", "text = \"x\"\ncolour = 1"))
            .is_empty()
    );
    // Many problems at once.
    let bad = good
        .replace("id = \"1AB1234\"", "id = \"QS00XYZ\"")
        .replace("library = \"INVPRD\"", "library = \"1BAD\"")
        .replace(
            "release_date = \"2026-10-30\"",
            "release_date = \"2026-13-01\"",
        )
        .replace("first_copyright = 2024", "first_copyright = 2030")
        .replace(
            "feature = \"5101\"\nrequires",
            "feature = \"4000\"\nrequires",
        )
        .replace(
            "{ name = \"INVFILE\", type = \"*FILE\", lib = \"INVDTA\" }",
            "{ name = \"INVFILE\", type = \"*FILE\", lib = \"ELSEWHERE\" }",
        )
        .replace(
            "paths = [\"/QIBM/ProdData/INV/bin\"]",
            "paths = [\"/tmp/x\"]",
        )
        .replace("usage_limit = 2", "usage_limit = 1000000");
    let e = errors(&bad);
    for at in [
        "product.id",
        "product.library",
        "product.release_date",
        "product.first_copyright",
        "option[1].feature",
        "option[0].objects[1].lib",
        "option[0].paths[0]",
        "license[1].usage_limit",
    ] {
        assert!(has(&e, at), "{at} missing from {e:#?}");
    }
    // The first option must be *BASE, options are unique.
    let swapped = good.replace("option = \"*BASE\"", "option = 2");
    assert!(has(&errors(&swapped), "option[0].option"));
    let dup = good.replace("option = 1\n", "option = \"*BASE\"\n");
    assert!(errors(&dup).iter().any(|e| e.contains("appears twice")));
    // *KEYED without a key; a key that is not a key.
    let nokey = VENDOR.replace("license_key = \"{KEY}\"\n", "");
    assert!(has(&errors(&nokey), "license[0].compliance"));
    let badkey = VENDOR.replace("{KEY}", "zz");
    assert!(has(&errors(&badkey), "product.license_key"));
    // Agreement: one of text or document.
    let both = good.replace("text = \"Use it well.\"", "text = \"a\"\ndocument = \"/x\"");
    assert!(has(&errors(&both), "agreement"));
    // A catalog-only field.
    let st = good.replace(
        "text = \"Inventory control\"",
        "text = \"Inventory control\"\nstatus = \"reserved\"",
    );
    assert!(has(&errors(&st), "product.status"));
    // Prerequisite form and languages.
    let pre = good.replace("requires = [\"QS00SS1:*BASE\"]", "requires = [\"QS00SS1\"]");
    assert!(has(&errors(&pre), "option[0].requires[0]"));
    let lng = good.replace(
        "text = \"Inventory reports\"",
        "text = \"Inventory reports\"\nlanguages = [\"5001\"]",
    );
    assert!(has(&errors(&lng), "option[1].languages[0]"));
}

#[test]
fn quartz_ids_need_the_pipeline() {
    let q = vendor_manifest().replace("1AB1234", "QS00INV");
    assert!(has(&errors(&q), "product.id"));
    let p = pkg::parse(
        &q,
        Options {
            allow_quartz: true,
            catalog: false,
        },
    )
    .unwrap();
    assert_eq!(p.loads[0].0.name, "INV00C5001");
}
