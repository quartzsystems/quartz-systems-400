// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! License keys (ADDLICKEY, RMVLICKEY, DSPLICKEY), license information
//! (WRKLICINF, CHGLICINF) and use (QLZAREQ, QLZARLS).

#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines
)]

mod common;

use common::{e, sys, vendor, vendor_signer};
use ed25519_dalek::SigningKey;
use licpgm::api::lic::{licl0200_extent, qlzareq, qlzarls};
use licpgm::license::LicError;
use licpgm::license::key::{KeyError, KeyFields, LicenseKey, generate};
use licpgm::license::repo::{AddLicKey, ChgLicInf, LicenseRepo};
use licpgm::license::usage::{Notice, Request};
use licpgm::{Date, LicTerm, LoadId, ProductDefinition, ProductId};

fn d(y: u16, m: u8, day: u8) -> Date {
    Date::new(y, m, day).unwrap()
}

fn today() -> Date {
    d(2026, 10, 30)
}

fn defs() -> Vec<ProductDefinition> {
    vec![vendor().definition]
}

fn fields(limit: Option<u32>, exp: Option<Date>) -> KeyFields {
    KeyFields {
        product: "1AB1234".parse().unwrap(),
        term: "V1".parse().unwrap(),
        feature: LoadId::feature("5001").unwrap(),
        serial: "QS40001".into(),
        processor_group: "*ANY".into(),
        usage_limit: limit,
        expiration: exp,
        vendor_data: "ACME".into(),
    }
}

fn repo() -> LicenseRepo {
    let mut r = LicenseRepo::default();
    r.sync_definition(&defs()[0]);
    r
}

fn add(r: &mut LicenseRepo, f: KeyFields, signer: &SigningKey) -> Result<(), LicError> {
    let key = generate(signer, &f).unwrap();
    r.add_key(&defs(), None, &sys(), AddLicKey { fields: f, key }, today())
}

fn req(user: &str) -> Request {
    Request {
        product: "1AB1234".parse().unwrap(),
        release: release::RELEASE,
        feature: LoadId::feature("5001").unwrap(),
        user: user.into(),
        handle: *b"HANDLE01",
        uses: 1,
    }
}

#[test]
fn add_key_checks_signature_serial_group_and_expiry() {
    let signer = vendor_signer();
    let mut r = repo();
    assert!(add(&mut r, fields(Some(5), None), &signer).is_ok());
    // *LOCAL is this system.
    let mut f = fields(Some(6), None);
    f.serial = "*LOCAL".into();
    let mut signed = f.clone();
    signed.serial = "QS40001".into();
    let key = generate(&signer, &signed).unwrap();
    r.add_key(&defs(), None, &sys(), AddLicKey { fields: f, key }, today())
        .unwrap();
    assert_eq!(r.keys.len(), 1, "the same fields replace the key");
    assert_eq!(r.keys[0].fields.usage_limit, Some(6));

    let mut f = fields(Some(5), None);
    f.serial = "QS40002".into();
    assert_eq!(add(&mut r, f, &signer), Err(LicError::SerialMismatch));
    let mut f = fields(Some(5), None);
    f.processor_group = "P10".into();
    assert_eq!(add(&mut r, f, &signer), Err(LicError::ProcessorGroup));
    assert_eq!(
        add(&mut r, fields(Some(5), Some(d(2026, 1, 1))), &signer),
        Err(LicError::Expired)
    );
    // A key signed by someone else, or for other fields.
    let other = SigningKey::from_bytes(&[1; 32]);
    assert_eq!(
        add(&mut r, fields(Some(5), None), &other),
        Err(LicError::Key(KeyError::Signature))
    );
    let key = generate(&signer, &fields(Some(500), None)).unwrap();
    let tampered = AddLicKey {
        fields: fields(Some(5000), None),
        key,
    };
    assert_eq!(
        r.add_key(&defs(), None, &sys(), tampered, today()),
        Err(LicError::Key(KeyError::Signature))
    );
    let mut f = fields(Some(5), None);
    f.feature = LoadId::feature("5999").unwrap();
    assert_eq!(add(&mut r, f, &signer), Err(LicError::NoInfo));
}

#[test]
fn key_text_survives_display_and_typing() {
    let signer = vendor_signer();
    let mut r = repo();
    add(&mut r, fields(Some(5), Some(d(2027, 12, 31))), &signer).unwrap();
    let rows = r.key_rows(None);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].usage_limit, "5");
    assert_eq!(rows[0].expiration, "2027-12-31");
    assert_eq!(rows[0].key.len(), 6);
    let typed = rows[0].key.join(" ").to_lowercase();
    assert_eq!(LicenseKey::parse(&typed).unwrap(), r.keys[0].key);
    // RMVLICKEY.
    let p: ProductId = "1AB1234".parse().unwrap();
    let t: LicTerm = "V1".parse().unwrap();
    assert_eq!(r.remove_keys(&p, Some(&t), None, None, None), Ok(1));
    assert_eq!(
        r.remove_keys(&p, None, None, None, None),
        Err(LicError::NoKey)
    );
}

#[test]
fn keyed_use_with_grace_period_and_expiry() {
    let signer = vendor_signer();
    let mut r = repo();
    // No key: granted during the grace period, refused after.
    let n = r.request(&req("ALICE"), &sys(), today()).unwrap();
    assert_eq!(
        n,
        [Notice::GraceActive {
            ends: today().plus(30)
        }]
    );
    assert_eq!(
        r.request(&req("BOB"), &sys(), today().plus(31)),
        Err(LicError::GraceExpired)
    );
    r.release(
        &"1AB1234".parse().unwrap(),
        release::RELEASE,
        &LoadId::feature("5001").unwrap(),
        "ALICE",
        *b"HANDLE01",
    )
    .unwrap();
    // A key for two users clears the grace period.
    add(&mut r, fields(Some(2), Some(d(2026, 11, 20))), &signer).unwrap();
    let n = r.request(&req("ALICE"), &sys(), today()).unwrap();
    assert_eq!(
        n,
        [Notice::ExpiresSoon {
            on: d(2026, 11, 20)
        }]
    );
    assert!(r.info[0].grace_start.is_none());
    // The threshold (2) is reached by the second user.
    let n = r.request(&req("BOB"), &sys(), today()).unwrap();
    assert!(n.contains(&Notice::Threshold { count: 2 }));
    // A third is past the key's limit: the grace period starts again.
    let n = r.request(&req("CAROL"), &sys(), today()).unwrap();
    assert_eq!(
        n,
        [Notice::GraceOverLimit {
            ends: today().plus(30)
        }]
    );
    assert_eq!(n[0].clone().msg_id(), "CPF9E72");
    // Once the key has expired, use is refused outright.
    assert_eq!(
        r.request(&req("DAVE"), &sys(), d(2026, 12, 1)),
        Err(LicError::Expired)
    );
    let row = &r.info_rows(&sys(), today())[0];
    assert_eq!(row.usage_count, 3);
    assert_eq!(row.peak, 3);
    assert_eq!(row.usage_limit, "2");
    assert_eq!(row.compliance, "*KEYED");
    assert_eq!(row.expiration, Some(d(2026, 11, 20)));
}

#[test]
fn opraction_and_warning_limits() {
    let mut r = repo();
    let mut q = req("ALICE");
    q.feature = LoadId::feature("5101").unwrap();
    q.uses = 2;
    assert_eq!(r.request(&q, &sys(), today()), Ok(vec![]));
    // The same user again with the same uses is granted; different is not.
    assert_eq!(r.request(&q, &sys(), today()), Ok(vec![]));
    let mut q3 = q.clone();
    q3.uses = 3;
    assert_eq!(
        r.request(&q3, &sys(), today()),
        Err(LicError::DifferentUses)
    );
    let mut b = q.clone();
    b.user = "BOB".into();
    b.uses = 1;
    assert_eq!(r.request(&b, &sys(), today()), Err(LicError::OverLimit));
    assert_eq!(LicError::OverLimit.msg_id(), "CPF9E18");
    // CHGLICINF to *WARNING-like behavior is not possible, but raising
    // the limit is.
    let p: ProductId = "1AB1234".parse().unwrap();
    let t: LicTerm = "V1".parse().unwrap();
    let f = LoadId::feature("5101").unwrap();
    let chg = ChgLicInf {
        usage_limit: Some(Some(3)),
        ..ChgLicInf::default()
    };
    r.change_info(&p, &t, &f, &chg).unwrap();
    assert_eq!(r.request(&b, &sys(), today()), Ok(vec![]));
    // A keyed limit is not changed by CHGLICINF.
    let keyed = LoadId::feature("5001").unwrap();
    assert_eq!(
        r.change_info(&p, &t, &keyed, &chg),
        Err(LicError::KeyedLimit)
    );

    // *WARNING over the limit: granted with CPF9E17.
    let i = r.info.iter_mut().find(|i| i.feature == f).unwrap();
    i.compliance = licpgm::model::definition::Compliance::Warning;
    let mut c = b.clone();
    c.user = "CAROL".into();
    let n = r.request(&c, &sys(), today()).unwrap();
    assert_eq!(n, [Notice::OverLimitAdded { limit: 3 }]);
    assert_eq!(n[0].msg_id(), "CPF9E17");
    // Release needs the handle.
    let rel = release::RELEASE;
    assert_eq!(
        r.release(&p, rel, &f, "CAROL", *b"WRONG..."),
        Err(LicError::User)
    );
    assert!(r.release(&p, rel, &f, "CAROL", *b"HANDLE01").is_ok());
}

#[test]
fn qlzareq_and_qlzarls_bytes() {
    let mut r = repo();
    let defs = defs();
    let mut prod = e("1AB1234", 7);
    prod.extend(e("*ONLY", 6));
    prod.extend(e("5101", 4));
    // LICL0100: a ten-character user.
    let n = qlzareq(
        &mut r,
        &defs,
        &sys(),
        today(),
        &prod,
        "LICP0100",
        &e("ALICE", 10),
        "LICL0100",
        "J",
    )
    .unwrap();
    assert!(n.is_empty());
    // LICL0200: user at 28, uses at 36 (two), handle.
    let mut u = Vec::new();
    u.extend(28i32.to_be_bytes());
    u.extend(5i32.to_be_bytes());
    u.extend(*b"HANDLE02");
    u.extend(36i32.to_be_bytes());
    u.extend(4i32.to_be_bytes());
    u.extend([0; 4]);
    u.extend(e("*JOB", 5));
    u.extend([0; 3]);
    u.extend(1i32.to_be_bytes());
    assert_eq!(licl0200_extent(&u[..28]), 40);
    qlzareq(
        &mut r,
        &defs,
        &sys(),
        today(),
        &prod,
        "LICP0100",
        &u,
        "LICL0200",
        "123456/QUSER/QPADEV0001",
    )
    .unwrap();
    let info = r
        .info
        .iter()
        .find(|i| i.feature.as_str() == "5101")
        .unwrap();
    assert_eq!(info.count(), 2);
    assert_eq!(info.users[1].name, "123456/QUSER/QPADEV0001");
    let e1 = qlzareq(
        &mut r,
        &defs,
        &sys(),
        today(),
        &prod,
        "LICP0100",
        &e("BOB", 10),
        "LICL0100",
        "J",
    )
    .unwrap_err();
    assert_eq!(e1.id, "CPF9E18");
    qlzarls(
        &mut r,
        &defs,
        &prod,
        "LICP0100",
        &u,
        "LICL0200",
        "123456/QUSER/QPADEV0001",
    )
    .unwrap();
    let e2 = qlzarls(
        &mut r,
        &defs,
        &prod,
        "LICP0100",
        &e("NOBODY", 10),
        "LICL0100",
        "J",
    )
    .unwrap_err();
    assert_eq!(e2.id, "CPF9E91");
    let e3 = qlzareq(
        &mut r,
        &defs,
        &sys(),
        today(),
        &prod,
        "LICP0200",
        &u,
        "LICL0200",
        "J",
    )
    .unwrap_err();
    assert_eq!(e3.id, "CPF3C21");
}

#[test]
fn repository_round_trips() {
    let mut r = repo();
    add(
        &mut r,
        fields(Some(2), Some(d(2027, 1, 1))),
        &vendor_signer(),
    )
    .unwrap();
    r.request(&req("ALICE"), &sys(), today()).unwrap();
    let back = LicenseRepo::decode(&r.encode().unwrap()).unwrap();
    assert_eq!(back, r);
}
