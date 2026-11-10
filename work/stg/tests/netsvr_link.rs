// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `netsvr_signon` end to end: a `QZLSFILE` job's request over a link
//! whose far end answers it as the storage service does
//! (`sec::netsvr::sign_on` over a store), both ways through the wire
//! codec. Only the profile and the session base key come back.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use sec::auth::principal::Principal;
use sec::cmd::{Env, Keywords, usrprf};
use sec::netsvr::{Logon, NetUsers, Refusal};
use sec::password::nthash::{self, TestSealer};
use sec::signon::Values;
use stg::{Client, Link, Request};
use store::obj::{ObjectId, ops};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn sysval(n: &str) -> String {
    match n {
        "QPWDLVL" => "3",
        "QPWDMINLEN" => "1",
        "QPWDMAXLEN" => "128",
        "QPWDRULES" => "*PWDSYSVAL",
        "QMAXSIGN" => "2",
        _ => "",
    }
    .into()
}

fn store() -> (Store, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(8192, 3));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [1; 16],
        unit_uuids: vec![[2; 16]],
        unit_serials: Vec::new(),
        wal_pages: 64,
        now: NOW,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &fp,
        StoreConfig::default(),
    )
    .unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    sec::profile::shipped::install(&mut t, qsys, 40, 3, NOW).unwrap();
    let secofr = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
    let env = Env {
        qsys,
        actor: &secofr,
        now: NOW,
        sysval: &sysval,
    };
    usrprf::crtusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "BOB"), ("PASSWORD", "Over The Wire")]),
    )
    .unwrap();
    t.commit().unwrap();
    (store, qsys)
}

fn logon(password: &str) -> (Logon, [u8; 16]) {
    let challenge = *b"servchal";
    let mut blob = vec![1, 1, 0, 0, 0, 0, 0, 0];
    blob.extend(7u64.to_le_bytes());
    blob.extend(*b"clientch");
    blob.extend([0; 8]);
    let (resp, key) =
        ntlm::ntlmv2_response(&ntlm::nt_hash(password), "bob", "QS400", &challenge, &blob);
    (
        Logon {
            user: "bob".into(),
            domain: "QS400".into(),
            server_challenge: challenge,
            nt_response: resp,
            guest: None,
            client: "192.0.2.7".into(),
        },
        key,
    )
}

#[test]
fn signon_over_the_link() {
    nthash::install(Some(Arc::new(TestSealer)));
    let (store, qsys) = store();
    let store = Rc::new(RefCell::new(store));
    let users = Rc::new(NetUsers::new());
    let (shared, table) = (Rc::clone(&store), Rc::clone(&users));
    let mut link = Link::answering(move |req| match req {
        Request::NetsvrSignon { logon, swap: _ } => {
            let mut st = shared.borrow_mut();
            let mut t = st.begin().unwrap();
            let v = Values::from_sysvals(&sysval);
            let sealer = nthash::sealer();
            let r = sec::netsvr::sign_on(&mut t, qsys, &logon, &v, &table, sealer.as_deref(), NOW)
                .unwrap();
            ipc::encode(&r)
        }
        other => panic!("not a NetServer request: {}", other.name()),
    });
    let (good, key) = logon("Over The Wire");
    let ok = link.netsvr_signon(good.clone(), true).unwrap().unwrap();
    assert_eq!(ok.user, "BOB");
    assert_eq!(ok.session_base_key, Some(key));
    let (bad, _) = logon("Under The Wire");
    assert_eq!(
        link.netsvr_signon(bad.clone(), true).unwrap(),
        Err(Refusal::WrongPassword {
            disabled_now: false
        })
    );
    assert_eq!(
        link.netsvr_signon(bad, true).unwrap(),
        Err(Refusal::WrongPassword { disabled_now: true })
    );
    assert!(users.is_disabled("BOB"));
    assert_eq!(
        link.netsvr_signon(good, true).unwrap(),
        Err(Refusal::NetServerDisabled)
    );
}
