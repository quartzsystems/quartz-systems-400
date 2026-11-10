// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `NetServer` security: the NT hash form and where it is set and dropped,
//! the V1R1M0 payload read under version 2, the `NTLMv2` sign-on with its
//! refusals, the guest, disabling for `NetServer` and its `VP` entries.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

mod common;

use std::sync::Arc;

use common::{NOW, open_new};
use sec::audit::Kind;
use sec::auth::principal::Principal;
use sec::cmd::{Env, Keywords, usrprf};
use sec::netsvr::{Logon, NetUsers, Refusal, sign_on};
use sec::password::nthash::{self, TestSealer};
use sec::profile::payload::{PwdForm, PwdHash, UsrPrf};
use sec::profile::{objects, shipped};
use sec::signon::Values;
use store::obj::ObjectId;
use store::txn::Txn;

const CHALLENGE: [u8; 8] = *b"chalnge1";

fn sysval(level: u8) -> impl Fn(&str) -> String {
    move |n: &str| match n {
        "QSECURITY" => "40".to_string(),
        "QPWDLVL" => level.to_string(),
        "QPWDMINLEN" => "1".into(),
        "QPWDMAXLEN" => "128".into(),
        "QPWDRULES" => "*PWDSYSVAL".into(),
        "QPWDEXPITV" => "*NOMAX".into(),
        "QMAXSIGN" => "3".into(),
        _ => String::new(),
    }
}

fn install() {
    nthash::install(Some(Arc::new(TestSealer)));
}

/// A store with the shipped profiles and `BOB` (password `pw` at `level`).
fn setup(level: u8, pw: &str) -> (store::Store, ObjectId) {
    install();
    let (mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    shipped::install(&mut t, qsys, 40, level, NOW).unwrap();
    let secofr = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
    let sv = sysval(level);
    let env = Env {
        qsys,
        actor: &secofr,
        now: NOW,
        sysval: &sv,
    };
    usrprf::crtusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "BOB"), ("PASSWORD", pw)]),
    )
    .unwrap();
    t.commit().unwrap();
    (store, qsys)
}

fn bob(t: &mut Txn<'_>, qsys: ObjectId) -> UsrPrf {
    objects::read(t, qsys, "BOB").unwrap().1
}

fn nt(p: &UsrPrf) -> Vec<[u8; 16]> {
    nthash::open_with(Some(&TestSealer), p)
}

/// A client's `NTLMv2` response for `password`.
fn response(user: &str, domain: &str, password: &str) -> (Vec<u8>, [u8; 16]) {
    let mut blob = vec![1, 1, 0, 0, 0, 0, 0, 0];
    blob.extend(0x01dc_0000_0000_0000u64.to_le_bytes());
    blob.extend(*b"clientch");
    blob.extend([0, 0, 0, 0]);
    blob.extend([2, 0, 4, 0, b'Q', 0, b'S', 0, 0, 0, 0, 0]);
    ntlm::ntlmv2_response(&ntlm::nt_hash(password), user, domain, &CHALLENGE, &blob)
}

fn logon(user: &str, password: &str) -> Logon {
    Logon {
        user: user.into(),
        domain: "WORKGROUP".into(),
        server_challenge: CHALLENGE,
        nt_response: response(user, "WORKGROUP", password).0,
        guest: None,
        client: "10.0.0.9".into(),
    }
}

fn values() -> Values {
    Values::from_sysvals(&sysval(3))
}

#[test]
fn v1r1m0_payload_reads_under_version_2() {
    let b = include_bytes!("fixtures/usrprf-v1r1m0.bin");
    assert_eq!(&b[8..10], &[1, 0], "the fixture is a version 1 payload");
    let p = UsrPrf::decode(b).unwrap();
    assert_eq!(p.name, "BOB");
    assert_eq!(p.text, "Bob at V1R1M0");
    assert_eq!(p.supgrpprf, vec!["DEVS"]);
    assert_eq!(p.hashes.len(), 3);
    assert!(p.hash(PwdForm::Long).is_some());
    assert!(p.hash(PwdForm::Short).is_some());
    assert!(p.hash(PwdForm::ShaToken).is_some());
    assert!(!nthash::has(&p), "no NT hash before V1R2M0");
    assert_eq!(p.history.len(), 1);
    // Written back it is version 2 and reads the same.
    let again = p.encode();
    assert_eq!(&again[8..10], &[2, 0]);
    assert_eq!(UsrPrf::decode(&again).unwrap(), p);
}

#[test]
fn unknown_forms_are_kept_and_never_match() {
    let mut p = UsrPrf::new("BOB", 40);
    p.pwd_none = false;
    p.hashes = sec::password::hashes_for("Secret1", 3);
    p.hashes.push(PwdHash {
        form: PwdForm::Unknown(9),
        phc: p.hash(PwdForm::Long).unwrap().phc.clone(),
    });
    let back = UsrPrf::decode(&p.encode()).unwrap();
    assert_eq!(back.hashes.last().unwrap().form, PwdForm::Unknown(9));
    // Taken as the long form before; now it is matched by nothing.
    assert_eq!(
        back.hashes
            .iter()
            .filter(|h| h.form == PwdForm::Long)
            .count(),
        1
    );
}

#[test]
fn nt_hash_set_at_each_level_and_dropped() {
    for (level, pw, want) in [
        (0u8, "Secret1", vec!["SECRET1", "secret1"]),
        (1, "SECRET1", vec!["SECRET1", "secret1"]),
        (2, "Mixed Case 2", vec!["Mixed Case 2"]),
        (3, "Mixed Case 3", vec!["Mixed Case 3"]),
        (4, "Mixed Case 4", vec!["Mixed Case 4"]),
    ] {
        let (mut store, qsys) = setup(level, pw);
        let mut t = store.begin().unwrap();
        let p = bob(&mut t, qsys);
        let want: Vec<[u8; 16]> = want.iter().map(|w| ntlm::nt_hash(w)).collect();
        assert_eq!(nt(&p), want, "CRTUSRPRF at level {level}");
        assert!(p.history.is_empty());

        // CHGUSRPRF PASSWORD: a new hash, the old one not in the history.
        let secofr = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
        let sv = sysval(level);
        let env = Env {
            qsys,
            actor: &secofr,
            now: NOW,
            sysval: &sv,
        };
        usrprf::chgusrprf(
            &mut t,
            &env,
            &Keywords::new(&[("USRPRF", "BOB"), ("PASSWORD", "NEXT2")]),
        )
        .unwrap();
        let p = bob(&mut t, qsys);
        let next = if level <= 1 {
            vec![ntlm::nt_hash("NEXT2"), ntlm::nt_hash("next2")]
        } else {
            vec![ntlm::nt_hash("NEXT2")]
        };
        assert_eq!(nt(&p), next, "CHGUSRPRF at level {level}");
        assert!(p.history.iter().all(|h| h.form.in_history()));

        // CHGPWD by the user.
        let me = Principal::load(&mut t, qsys, "BOB", 40).unwrap();
        let env = Env {
            qsys,
            actor: &me,
            now: NOW,
            sysval: &sv,
        };
        usrprf::chgpwd(&mut t, &env, "NEXT2", "THIRD3", &mut |_, _, _| Ok(())).unwrap();
        assert_eq!(nt(&bob(&mut t, qsys))[0], ntlm::nt_hash("THIRD3"));

        // QSYRUPWD never carries it; QSYSUPWD never takes one.
        let p = bob(&mut t, qsys);
        let sealed = p.hash(PwdForm::NtHash).unwrap().phc.clone();
        let blob = sec::api::password_blob(&p);
        assert!(!blob.windows(sealed.len()).any(|w| w == sealed.as_bytes()));

        // PASSWORD(*NONE) drops it.
        let env = Env {
            qsys,
            actor: &secofr,
            now: NOW,
            sysval: &sv,
        };
        usrprf::chgusrprf(
            &mut t,
            &env,
            &Keywords::new(&[("USRPRF", "BOB"), ("PASSWORD", "*NONE")]),
        )
        .unwrap();
        let p = bob(&mut t, qsys);
        assert!(!nthash::has(&p));
        assert!(p.history.iter().all(|h| h.form != PwdForm::NtHash));
    }
}

#[test]
fn no_sealer_no_nt_hash() {
    let mut p = UsrPrf::new("BOB", 40);
    p.hashes = nthash::hashes_with(None, "BOB", "Secret1", 3);
    assert!(!nthash::has(&p));
}

#[test]
fn sign_on_every_path() {
    let (mut store, qsys) = setup(3, "Right Pass 1");
    let mut t = store.begin().unwrap();
    let users = NetUsers::new();
    let v = values();
    let s: Option<&dyn nthash::NtSealer> = Some(&TestSealer);
    let _ = sec::audit::take();

    // The right password: the profile and the session base key.
    let l = logon("bob", "Right Pass 1");
    let ok = sign_on(&mut t, qsys, &l, &v, &users, s, NOW)
        .unwrap()
        .unwrap();
    assert_eq!(ok.user, "BOB");
    assert!(!ok.guest);
    assert_eq!(
        ok.session_base_key,
        Some(response("bob", "WORKGROUP", "Right Pass 1").1)
    );
    assert!(sec::audit::take().is_empty());

    // Without the key (another system's) the hash does not open.
    assert_eq!(
        sign_on(&mut t, qsys, &l, &v, &users, None, NOW).unwrap(),
        Err(Refusal::WrongPassword {
            disabled_now: false
        })
    );
    users.succeeded("BOB");
    let _ = sec::audit::take();

    // Unknown user, no guest.
    assert_eq!(
        sign_on(&mut t, qsys, &logon("NOBODY", "x"), &v, &users, s, NOW).unwrap(),
        Err(Refusal::NoSuchUser)
    );
    assert!(
        sec::audit::take().is_empty(),
        "no entry for an unknown user"
    );

    // NTLMv1 is refused and not counted.
    let mut v1 = logon("BOB", "Right Pass 1");
    v1.nt_response.truncate(24);
    assert_eq!(
        sign_on(&mut t, qsys, &v1, &v, &users, s, NOW).unwrap(),
        Err(Refusal::NotNtlmV2)
    );
    assert_eq!(users.failures("BOB"), 0);

    // Wrong passwords: QMAXSIGN 3 disables BOB for NetServer only.
    let bad = logon("BOB", "Wrong");
    for _ in 0..2 {
        assert_eq!(
            sign_on(&mut t, qsys, &bad, &v, &users, s, NOW).unwrap(),
            Err(Refusal::WrongPassword {
                disabled_now: false
            })
        );
    }
    assert_eq!(
        sign_on(&mut t, qsys, &bad, &v, &users, s, NOW).unwrap(),
        Err(Refusal::WrongPassword { disabled_now: true })
    );
    let events = sec::audit::take();
    let vp: Vec<_> = events.iter().filter(|e| e.kind == Kind::Vp).collect();
    assert_eq!(vp.len(), 4);
    assert_eq!(
        vp.iter().map(|e| e.get("TYPE")).collect::<Vec<_>>(),
        ["P", "P", "P", "D"]
    );
    assert!(vp.iter().all(|e| e.get("PROF") == "BOB"));
    assert!(vp.iter().all(|e| e.get("NWSD") == "*NETSERVER"));
    let data = vp[0].encode();
    assert_eq!(data.len(), 41);
    assert!(users.is_disabled("BOB"));
    assert!(!bob(&mut t, qsys).disabled, "the profile stays enabled");
    assert_eq!(
        sign_on(&mut t, qsys, &l, &v, &users, s, NOW).unwrap(),
        Err(Refusal::NetServerDisabled)
    );
    assert_eq!(Refusal::NetServerDisabled.msgid(), Some("CPIB68E"));
    // QZLSCHSI enables it again.
    assert!(users.enable("BOB"));
    assert!(
        sign_on(&mut t, qsys, &l, &v, &users, s, NOW)
            .unwrap()
            .is_ok()
    );

    // A good sign-on resets the count.
    let _ = sign_on(&mut t, qsys, &bad, &v, &users, s, NOW).unwrap();
    let _ = sign_on(&mut t, qsys, &bad, &v, &users, s, NOW).unwrap();
    assert!(
        sign_on(&mut t, qsys, &l, &v, &users, s, NOW)
            .unwrap()
            .is_ok()
    );
    assert_eq!(users.failures("BOB"), 0);

    // An expired password, the right one: refused after the check.
    let (id, mut p) = objects::read(&mut t, qsys, "BOB").unwrap();
    p.pwdexp = true;
    objects::write(&mut t, id, &p).unwrap();
    assert_eq!(
        sign_on(&mut t, qsys, &l, &v, &users, s, NOW).unwrap(),
        Err(Refusal::Expired)
    );
    // *DISABLED.
    p.pwdexp = false;
    p.disabled = true;
    objects::write(&mut t, id, &p).unwrap();
    assert_eq!(
        sign_on(&mut t, qsys, &l, &v, &users, s, NOW).unwrap(),
        Err(Refusal::Disabled)
    );
    // No NT hash (a V1R1M0 password).
    p.disabled = false;
    p.hashes.retain(|h| h.form != PwdForm::NtHash);
    objects::write(&mut t, id, &p).unwrap();
    assert_eq!(
        sign_on(&mut t, qsys, &l, &v, &users, s, NOW).unwrap(),
        Err(Refusal::NoNtHash)
    );
    // No password.
    p.pwd_none = true;
    objects::write(&mut t, id, &p).unwrap();
    assert_eq!(
        sign_on(&mut t, qsys, &l, &v, &users, s, NOW).unwrap(),
        Err(Refusal::NoPassword)
    );
}

#[test]
fn levels_zero_and_one_take_either_case() {
    let (mut store, qsys) = setup(0, "Secret1");
    let mut t = store.begin().unwrap();
    let users = NetUsers::new();
    let v = values();
    let s: Option<&dyn nthash::NtSealer> = Some(&TestSealer);
    for typed in ["SECRET1", "secret1"] {
        assert!(
            sign_on(&mut t, qsys, &logon("BOB", typed), &v, &users, s, NOW)
                .unwrap()
                .is_ok(),
            "{typed}"
        );
    }
    assert!(
        sign_on(&mut t, qsys, &logon("BOB", "Secret1"), &v, &users, s, NOW)
            .unwrap()
            .is_err(),
        "mixed case is not the password at level 0"
    );
}

#[test]
fn guest_for_an_unknown_user_only() {
    let (mut store, qsys) = setup(3, "Right Pass 1");
    let mut t = store.begin().unwrap();
    let secofr = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
    let sv = sysval(3);
    let env = Env {
        qsys,
        actor: &secofr,
        now: NOW,
        sysval: &sv,
    };
    usrprf::crtusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "NSGUEST"), ("PASSWORD", "*NONE")]),
    )
    .unwrap();
    let users = NetUsers::new();
    let v = values();
    let s: Option<&dyn nthash::NtSealer> = Some(&TestSealer);
    let mut l = logon("STRANGER", "anything");
    l.guest = Some("nsguest".into());
    let g = sign_on(&mut t, qsys, &l, &v, &users, s, NOW)
        .unwrap()
        .unwrap();
    assert_eq!(g.user, "NSGUEST");
    assert!(g.guest);
    assert_eq!(g.session_base_key, None);
    // A known user with a wrong password is not the guest.
    let mut bad = logon("BOB", "Wrong");
    bad.guest = Some("NSGUEST".into());
    assert!(matches!(
        sign_on(&mut t, qsys, &bad, &v, &users, s, NOW).unwrap(),
        Err(Refusal::WrongPassword { .. })
    ));
    // A guest with a special authority is not used.
    l.guest = Some("QSECOFR".into());
    assert_eq!(
        sign_on(&mut t, qsys, &l, &v, &users, s, NOW).unwrap(),
        Err(Refusal::NoSuchUser)
    );
}

#[test]
fn wire_round_trip() {
    use ipc::Wire;
    let l = logon("bob", "x");
    let mut b = Vec::new();
    l.put(&mut b);
    assert_eq!(Logon::get(&mut ipc::Dec::new(&b)).unwrap(), l);
    for r in [
        Refusal::NoSuchUser,
        Refusal::WrongPassword { disabled_now: true },
        Refusal::NotNtlmV2,
    ] {
        let mut b = Vec::new();
        r.put(&mut b);
        assert_eq!(Refusal::get(&mut ipc::Dec::new(&b)).unwrap(), r);
    }
}
