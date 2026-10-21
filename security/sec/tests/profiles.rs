// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped profiles in a store: created once, objects re-owned, the
//! indexes built.

mod common;

use common::{NOW, create, lib, open_new};
use sec::profile::index::{self, Kind};
use sec::profile::{SpecialAuth, UserClass, objects, shipped};
use store::obj::header::{Authority, text_of};
use store::obj::{ops, types};

#[test]
fn install_reowns_and_indexes() {
    let (mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    let qgpl = lib(&mut t, qsys, "QGPL");
    let app = lib(&mut t, qsys, "APPLIB");
    let usrsys = lib(&mut t, qsys, "QUSRSYS");
    let jobd = create(&mut t, qgpl, "QDFTJOBD", types::JOBD, "QSYS");
    let mine = create(&mut t, app, "MYAREA", types::DTAARA, "QSECOFR");
    let orphan = create(&mut t, app, "ORPHAN", types::DTAARA, "GONE");
    let q = create(&mut t, usrsys, "QSECOFR", types::MSGQ, "QSYS");
    t.commit().unwrap();

    let mut t = store.begin().unwrap();
    let done = shipped::install(&mut t, qsys, 40, 3, NOW).unwrap();
    t.commit().unwrap();
    assert_eq!(done.profiles, shipped::PROFILES.len());
    assert!(done.reowned > 8);

    let mut t = store.begin().unwrap();
    let (_, sec) = objects::read(&mut t, qsys, "QSECOFR").unwrap();
    assert_eq!(sec.usrcls, UserClass::SecOfr);
    assert_eq!(sec.spcaut, SpecialAuth::ALL);
    assert!(sec.pwdexp && !sec.pwd_none);
    assert!(sec::password::matches(&sec, "QSECOFR", 3));
    let (_, opr) = objects::read(&mut t, qsys, "QSYSOPR").unwrap();
    assert!(opr.pwd_none);
    assert_eq!(opr.spcaut, SpecialAuth::JOBCTL.union(SpecialAuth::SAVSYS));

    let owner =
        |t: &mut store::txn::Txn<'_>, id| text_of(&ops::read_header(t, id).unwrap().1.owner);
    assert_eq!(owner(&mut t, jobd), "QSYS");
    assert_eq!(owner(&mut t, qgpl), "QSYS");
    assert_eq!(owner(&mut t, app), "QSECOFR");
    assert_eq!(owner(&mut t, mine), "QSECOFR");
    assert_eq!(owner(&mut t, orphan), "QDFTOWN");
    assert_eq!(owner(&mut t, q), "QSECOFR");
    assert_eq!(
        ops::read_header(&mut t, jobd).unwrap().1.public_authority,
        Authority::USE
    );
    assert_eq!(
        ops::read_header(&mut t, qgpl).unwrap().1.public_authority,
        Authority::CHANGE
    );

    let owned_list = index::list(&mut t, qsys, "QSECOFR", Kind::Owned).unwrap();
    let ids: Vec<_> = owned_list.iter().map(|(id, _)| *id).collect();
    assert!(ids.contains(&mine) && ids.contains(&q));
    assert_eq!(
        objects::read(&mut t, qsys, "QSECOFR")
            .unwrap()
            .1
            .owned_count,
        ids.len() as u64
    );
    let qsys_owned = index::list(&mut t, qsys, "QSYS", Kind::Owned).unwrap();
    assert!(qsys_owned.iter().any(|(id, _)| *id == jobd));
    drop(t);

    // A second install changes nothing.
    let mut t = store.begin().unwrap();
    let again = shipped::install(&mut t, qsys, 40, 3, NOW).unwrap();
    assert_eq!(again, shipped::Installed::default());
}
