// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The registry's operations with IBM's messages, the `*EXITRG`
//! payload codec, the repository in a store, and the queries.

#![allow(clippy::many_single_char_names)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use regfac::{
    AddOpts, Answer, Desc, ExitPoint, ExitProgram, HIGH, LOW, MtAction, Prep, Query, Registry,
    ThreadSafe, Which, answer, payload,
};
use store::obj::{CreateParams, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn pgm(name: &str, number: i32) -> ExitProgram {
    ExitProgram {
        number,
        pgm: name.into(),
        lib: "QGPL".into(),
        ..ExitProgram::default()
    }
}

fn add(r: &mut Registry, point: &str, format: &str, p: ExitProgram) -> Result<i32, String> {
    r.add_program(point, format, p, &AddOpts::default())
        .map_err(|e| e.msgid)
}

#[test]
fn the_shipped_points_are_ibms() {
    let r = Registry::shipped();
    for (n, f) in [
        ("QIBM_QZSO_SIGNONSRV", "ZSOY0100"),
        ("QIBM_QZSC_LM", "ZSCL0100"),
        ("QIBM_QZSC_NLS", "ZSCN0100"),
        ("QIBM_QZSC_SM", "ZSCS0100"),
        ("QIBM_QZDA_INIT", "ZDAI0100"),
        ("QIBM_QZDA_SQL1", "ZDAQ0100"),
        ("QIBM_QZDA_SQL2", "ZDAQ0200"),
        ("QIBM_QZDA_NDB1", "ZDAD0100"),
        ("QIBM_QZDA_NDB1", "ZDAD0200"),
        ("QIBM_QZDA_ROI1", "ZDAR0100"),
        ("QIBM_QZDA_ROI1", "ZDAR0200"),
        ("QIBM_QZHQ_DATA_QUEUE", "ZHQ00100"),
        ("QIBM_QPWFS_FILE_SERV", "PWFS0100"),
        ("QIBM_QPWFS_FILE_SERV", "PWFS0200"),
        ("QIBM_QNPS_ENTRY", "ENTR0100"),
        ("QIBM_QNPS_SPLF", "SPLF0100"),
        ("QIBM_QZRC_RMT", "CZRC0100"),
        ("QIBM_QTMF_SERVER_REQ", "VLRQ0100"),
        ("QIBM_QTMF_CLIENT_REQ", "VLRQ0100"),
        ("QIBM_QTMF_SVR_LOGON", "TCPL0100"),
        ("QIBM_QTMF_SVR_LOGON", "TCPL0200"),
        ("QIBM_QTMF_SVR_LOGON", "TCPL0300"),
        ("QIBM_QTG_DEVINIT", "INIT0100"),
        ("QIBM_QTG_DEVTERM", "TERM0100"),
    ] {
        let p = r
            .point(n, f)
            .unwrap_or_else(|| panic!("{n} {f} is not shipped"));
        assert!(p.registered);
        assert!(p.desc.as_text().len() <= 50);
    }
    // Every shipped format can be called.
    let known = regfac::formats::known();
    for p in &r.points {
        assert!(
            known.contains(&p.format.as_str()),
            "{} {}",
            p.name,
            p.format
        );
    }
    // The flags and preprocessing programs the oracle shows.
    let ftp = r.point("QIBM_QTMF_SERVER_REQ", "VLRQ0100").unwrap();
    assert!(ftp.allow_dereg && !ftp.allow_change);
    assert_eq!(ftp.max, Some(1));
    assert_eq!(
        ftp.prep_add,
        Some(Prep::new("QTCP", "QTMFADDP", "ADDP0100"))
    );
    assert_eq!(
        ftp.prep_rmv,
        Some(Prep::new("QTCP", "QTMFRMVP", "RMVP0100"))
    );
    assert_eq!(ftp.prep_rtv, None);
    let rmt = r.point("QIBM_QZRC_RMT", "CZRC0100").unwrap();
    assert!(!rmt.allow_dereg && rmt.allow_change);
    assert_eq!(r.point("QIBM_QNPS_SPLF", "SPLF0100").unwrap().max, None);
    // Sorted by name then format.
    let names: Vec<_> = r
        .points
        .iter()
        .map(|p| (p.name.clone(), p.format.clone()))
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}

#[test]
fn programs_are_added_numbered_and_removed() {
    let mut r = Registry::shipped();
    let p = "QIBM_QNPS_ENTRY";
    assert_eq!(add(&mut r, p, "ENTR0100", pgm("A", LOW)), Ok(1));
    assert_eq!(add(&mut r, p, "ENTR0100", pgm("B", HIGH)), Ok(2));
    assert_eq!(add(&mut r, p, "ENTR0100", pgm("C", 10)), Ok(10));
    assert_eq!(add(&mut r, p, "ENTR0100", pgm("D", HIGH)), Ok(11));
    assert_eq!(
        add(&mut r, p, "ENTR0100", pgm("E", 2)),
        Err("CPF3CDF".into())
    );
    assert_eq!(
        add(&mut r, p, "ENTR0100", pgm("E", 0)),
        Err("CPF3CE1".into())
    );
    assert_eq!(
        add(&mut r, p, "ENTR0100", pgm("1BAD", 3)),
        Err("CPF3CDE".into())
    );
    let big = ExitProgram {
        data: vec![0x40; 2049],
        ..pgm("E", 3)
    };
    assert_eq!(add(&mut r, p, "ENTR0100", big), Err("CPF3CD6".into()));
    let replaced = r
        .add_program(
            p,
            "ENTR0100",
            pgm("E", 2),
            &AddOpts {
                replace: true,
                create_point: false,
            },
        )
        .unwrap();
    assert_eq!(replaced, 2);
    let names: Vec<_> = r
        .programs(p, "ENTR0100")
        .into_iter()
        .map(|g| g.pgm)
        .collect();
    assert_eq!(names, ["A", "E", "C", "D"]);
    assert_eq!(r.remove_program(p, "ENTR0100", 10).unwrap().pgm, "C");
    assert_eq!(
        r.remove_program(p, "ENTR0100", 10).unwrap_err().msgid,
        "CPF3CDD"
    );
    assert_eq!(add(&mut r, p, "ENTR0100", pgm("F", LOW)), Ok(3));

    // One program at most on the remote command point.
    assert_eq!(add(&mut r, "QIBM_QZRC_RMT", "CZRC0100", pgm("X", 1)), Ok(1));
    assert_eq!(
        add(&mut r, "QIBM_QZRC_RMT", "CZRC0100", pgm("Y", 2)),
        Err("CPF3CD4".into())
    );
    // No such point, unless asked to make it.
    assert_eq!(
        add(&mut r, "MY_POINT", "MYFM0100", pgm("Z", 1)),
        Err("CPF3CDB".into())
    );
    let made = r.add_program(
        "MY_POINT",
        "MYFM0100",
        pgm("Z", 1),
        &AddOpts {
            replace: false,
            create_point: true,
        },
    );
    assert_eq!(made, Ok(1));
    let mine = r.point("MY_POINT", "MYFM0100").unwrap();
    assert!(!mine.registered);
    assert_eq!(r.select("MY*", "*ALL", Which::Unregistered).len(), 1);
    assert_eq!(r.select("MY*", "*ALL", Which::Registered).len(), 0);
}

#[test]
fn points_are_registered_and_deregistered() {
    let mut r = Registry::default();
    let mut p = ExitPoint::new("ACME_EXIT", "ACME0100");
    p.desc = Desc::text("Acme exit point");
    p.max = Some(2);
    r.register(p.clone()).unwrap();
    assert_eq!(r.register(p.clone()).unwrap_err().msgid, "CPF3CD1");
    assert_eq!(
        r.register(ExitPoint::new("9BAD", "ACME0100"))
            .unwrap_err()
            .msgid,
        "CPF3CD2"
    );
    assert_eq!(
        r.register(ExitPoint::new("ACME_EXIT", "TOO-LONG1"))
            .unwrap_err()
            .msgid,
        "CPF3CD3"
    );
    r.register(ExitPoint::new("ACME_EXIT", "ACME0200")).unwrap();
    assert_eq!(r.deregister("ACME_EXIT", "*ALL").unwrap(), 2);
    assert_eq!(
        r.deregister("ACME_EXIT", "*ALL").unwrap_err().msgid,
        "CPF3CDB"
    );
    // A shipped host server point cannot go.
    let mut s = Registry::shipped();
    let e = s.deregister("QIBM_QZRC_RMT", "CZRC0100").unwrap_err();
    assert_eq!(e.msgid, "CPF3CDC");
    assert_eq!(&e.data[4..8], &1i32.to_be_bytes(), "one not deregistered");
    // An unregistered point with programs becomes registered with them.
    let mut u = Registry::default();
    u.add_program(
        "LATE_EXIT",
        "LATE0100",
        pgm("P", 1),
        &AddOpts {
            replace: false,
            create_point: true,
        },
    )
    .unwrap();
    u.register(ExitPoint::new("LATE_EXIT", "LATE0100")).unwrap();
    let l = u.point("LATE_EXIT", "LATE0100").unwrap();
    assert!(l.registered);
    assert_eq!(l.programs.len(), 1);
}

#[test]
fn the_payload_round_trips() {
    let mut r = Registry::shipped();
    let g = ExitProgram {
        number: 7,
        pgm: "EXITPGM".into(),
        lib: "QSTEST".into(),
        data: b"\x01\x02program data".to_vec(),
        ccsid: 37,
        threadsafe: ThreadSafe::Yes,
        mt_action: MtAction::NoRun,
        desc: Desc::Msg {
            msgf: "QCPFMSG".into(),
            lib: "QSYS".into(),
            msgid: "CPF9898".into(),
        },
    };
    r.add_program("QIBM_QNPS_SPLF", "SPLF0100", g.clone(), &AddOpts::default())
        .unwrap();
    r.add_program(
        "USER_POINT",
        "USER0100",
        pgm("U", 1),
        &AddOpts {
            replace: false,
            create_point: true,
        },
    )
    .unwrap();
    let b = payload::encode(&r);
    assert_eq!(&b[..4], b"QREG");
    let back = payload::decode(&b).unwrap();
    assert_eq!(back, r);
    assert_eq!(back.programs("QIBM_QNPS_SPLF", "SPLF0100"), vec![g]);
    // Damage is the repository unavailable.
    assert_eq!(
        payload::decode(&b[..b.len() - 1]).unwrap_err().msgid,
        "CPF3CDA"
    );
    let mut bad = b.clone();
    bad[0] = b'X';
    assert_eq!(payload::decode(&bad).unwrap_err().msgid, "CPF3CDA");
    let mut longer = b;
    longer.push(0);
    assert!(payload::decode(&longer).is_err());
}

fn open_new() -> (Store, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, 7));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
        now: NOW,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    ops::create(
        &mut t,
        &CreateParams {
            library: qsys,
            name: Name::parse("QUSRSYS").unwrap(),
            type_code: types::LIB,
            attribute: String::new(),
            text: String::new(),
            owner: "QSYS".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: true,
        },
    )
    .unwrap();
    t.commit().unwrap();
    (store, qsys)
}

fn stamp() -> wm::obj::Stamp {
    wm::obj::Stamp {
        owner: "QSYS".into(),
        system: "QS400".into(),
        now: NOW,
    }
}

#[test]
fn the_repository_is_made_once_and_kept() {
    let (mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    assert_eq!(
        regfac::store::load(&mut t, qsys).unwrap_err().msgid,
        "CPF3CDA"
    );
    let made = regfac::store::install(&mut t, qsys, &stamp()).unwrap();
    assert_eq!(made, Registry::shipped().points.len());
    t.commit().unwrap();

    let mut t = store.begin().unwrap();
    let mut r = regfac::store::load(&mut t, qsys).unwrap();
    assert_eq!(r, Registry::shipped());
    r.add_program(
        "QIBM_QZRC_RMT",
        "CZRC0100",
        pgm("RMTEXIT", 1),
        &AddOpts::default(),
    )
    .unwrap();
    // A shipped point the user deregistered is not one this release can
    // drop; drop a user's own instead and see the next install keep it gone.
    r.register(ExitPoint::new("ACME_EXIT", "ACME0100")).unwrap();
    regfac::store::save(&mut t, qsys, &r).unwrap();
    t.commit().unwrap();

    let mut t = store.begin().unwrap();
    assert_eq!(regfac::store::install(&mut t, qsys, &stamp()).unwrap(), 0);
    let back = regfac::store::load(&mut t, qsys).unwrap();
    assert_eq!(back, r, "the next IPL keeps programs and user points");
    let id = regfac::store::find(&mut t, qsys).unwrap().unwrap();
    assert_eq!(id.type_code, types::EXITRG);
    t.commit().unwrap();
}

#[test]
fn queries_answer_and_audit() {
    let mut r = Registry::shipped();
    let out = answer(
        &mut r,
        Query::Add {
            name: "QIBM_QZRC_RMT".into(),
            format: "CZRC0100".into(),
            program: pgm("RMTEXIT", LOW),
            opts: AddOpts::default(),
        },
    )
    .unwrap();
    assert_eq!(out.answer, Answer::Number { n: 1 });
    assert!(out.changed);
    assert_eq!(out.audit.len(), 1);
    assert_eq!(out.audit[0].get("ETYP"), "A");
    assert_eq!(out.audit[0].get("EXPT"), "QIBM_QZRC_RMT");
    assert_eq!(out.audit[0].get("EXPN"), "RMTEXIT");
    let list = answer(
        &mut r,
        Query::List {
            name: "QIBM_QZ*".into(),
            format: "*ALL".into(),
            which: 0,
        },
    )
    .unwrap();
    let Answer::Points { points } = list.answer else {
        panic!("points");
    };
    assert!(points.iter().all(|p| p.name.starts_with("QIBM_QZ")));
    assert!(!list.changed);
    let rm = answer(
        &mut r,
        Query::Remove {
            name: "QIBM_QZRC_RMT".into(),
            format: "CZRC0100".into(),
            number: 1,
        },
    )
    .unwrap();
    assert_eq!(rm.audit[0].get("ETYP"), "D");
    let e = answer(
        &mut r,
        Query::Remove {
            name: "QIBM_QZRC_RMT".into(),
            format: "CZRC0100".into(),
            number: 1,
        },
    )
    .unwrap_err();
    assert_eq!(e.msgid, "CPF3CDD");
    // On the wire.
    let q = Query::Register {
        point: ExitPoint::new("WIRE_EXIT", "WIRE0100"),
    };
    let back: Query = ipc::decode(&ipc::encode(&q)).unwrap();
    assert_eq!(back, q);
}
