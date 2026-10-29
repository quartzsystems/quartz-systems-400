// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/QSYS.LIB` over a store in memory: listing, members as streams,
//! user spaces, authority, creation and deletion, and the whole thing
//! behind an `Ifs`.

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::CreateFileOpts;
use db::file::MemberOpt;
use ifs::auth::AuthEnv;
use ifs::backend::mem::MemBackend;
use ifs::meta::IdMap;
use ifs::ops::audit::NoAudit;
use ifs::ops::flags::{O_CCSID, O_RDONLY, O_TEXTDATA, O_TRUNC, O_WRONLY};
use ifs::ops::{Config, Ctx, Ifs, OpenArgs, Owner};
use ifs::vfs::qsys::{QsysNode, QsysView};
use qsname::{MemberSpec, Name};
use qsyslib::{MemAccess, StoreAccess, StoreEnv, StoreQsys};
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::profile::payload::UsrPrf;
use sec::profile::spcaut::SpecialAuth;
use store::obj::header::Authority;
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types, usrspc};
use store::txn::Txn;
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
}

fn params(lib: ObjectId, n: &str, type_code: u16, owner: &str) -> CreateParams {
    CreateParams {
        library: lib,
        name: name(n),
        type_code,
        attribute: String::new(),
        text: "made for the view".into(),
        owner: owner.into(),
        system: "QS400".into(),
        now: NOW,
        by_system: true,
    }
}

fn profile(t: &mut Txn<'_>, qsys: ObjectId, user: &str, uid: u32, gid: u32, group: &str) {
    let mut p = UsrPrf::new(user, 40);
    p.uid = uid;
    p.gid = gid;
    if !group.is_empty() {
        p.grpprf = group.into();
    }
    sec::profile::objects::create(t, qsys, &p, "QSECOFR", Authority::EXCLUDE, NOW).unwrap();
}

/// A store with `QGPL/QCLSRC(HELLO)`, `QGPL/CUST` (data), `QGPL/SPACE`
/// and the profiles `QSECOFR`, `BOB`, `ANN` (in group `DEV`).
fn store() -> (Store, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, 7));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
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
    let qgpl = ops::create(&mut t, &params(qsys, "QGPL", types::LIB, "QSYS")).unwrap();
    let opts = CreateFileOpts {
        owner: "BOB".into(),
        now: NOW,
        member: MemberOpt::Named(name("HELLO")),
        max_members: 0,
        ..CreateFileOpts::default()
    };
    let src = db::file::create_srcpf(&mut t, qgpl, name("QCLSRC"), 92, &opts).unwrap();
    db::source::srcput(
        &mut t,
        JobHandle(1),
        src,
        MemberSpec::Name(name("HELLO")),
        "PGM\nENDPGM\n",
        Some(name("CLLE")),
        NOW,
    )
    .unwrap();
    let opts = CreateFileOpts {
        member: MemberOpt::None,
        ..opts
    };
    db::file::create_pf_rcdlen(&mut t, qgpl, name("CUST"), 10, &opts).unwrap();
    usrspc::create(
        &mut t,
        &params(qgpl, "SPACE", types::USRSPC, "BOB"),
        16,
        true,
        0xC1,
    )
    .unwrap();
    profile(&mut t, qsys, "QSECOFR", 0, 0, "");
    profile(&mut t, qsys, "DEV", 200, 500, "");
    profile(&mut t, qsys, "BOB", 100, 0, "");
    profile(&mut t, qsys, "ANN", 101, 0, "DEV");
    t.commit().unwrap();
    (store, qsys)
}

fn user(u: &str) -> Principal {
    let mut p = Principal {
        user: u.into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    };
    if u == "QSECOFR" {
        p.spcaut = SpecialAuth::ALLOBJ.0;
    }
    if u == "ANN" {
        p.groups = vec!["DEV".into()];
    }
    p
}

fn view() -> StoreQsys<MemAccess> {
    let (s, qsys) = store();
    let a = MemAccess::new(s, qsys);
    a.set_sysval("QCRTOBJAUD", "*CHANGE");
    StoreQsys::new(a)
}

#[test]
fn listing_and_entries() {
    let v = view();
    let libs = v.libraries().unwrap();
    assert_eq!(libs.len(), 1);
    assert_eq!(libs[0].name, "QGPL");
    assert_eq!(libs[0].objtype, "*LIB");
    assert_eq!(libs[0].owner, "QSYS");
    let qsys: Vec<String> = v
        .objects("QSYS")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(
        !qsys.contains(&"QGPL".to_string()),
        "libraries are not objects of QSYS: {qsys:?}"
    );
    assert!(qsys.contains(&"BOB".to_string()));
    let mut objs = v.objects("QGPL").unwrap();
    objs.sort_by(|a, b| a.name.cmp(&b.name));
    let names: Vec<(String, String, String)> = objs
        .iter()
        .map(|e| (e.name.clone(), e.objtype.clone(), e.attr.clone()))
        .collect();
    assert_eq!(
        names,
        vec![
            (
                "CUST".to_string(),
                "*FILE".to_string(),
                "PF-DTA".to_string()
            ),
            (
                "QCLSRC".to_string(),
                "*FILE".to_string(),
                "PF-SRC".to_string()
            ),
            ("SPACE".to_string(), "*USRSPC".to_string(), String::new()),
        ]
    );
    let space = v.object("QGPL", "SPACE", "*USRSPC").unwrap();
    assert_eq!(space.size, 16);
    assert_eq!(space.owner, "BOB");
    assert_eq!(space.text, "made for the view");
    let m = v.members("QGPL", "QCLSRC").unwrap();
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].name, "HELLO");
    assert_eq!(m[0].attr, "CLLE");
    assert_eq!(m[0].size, 2 * (80 + 2));
    assert_eq!(m[0].ccsid, 37);
    assert_eq!(v.object("QGPL", "NOPE", "*FILE"), Err(ifs::Errno::ENOENT));
    assert_eq!(v.object("QGPL", "CUST", "*PGM"), Err(ifs::Errno::ENOENT));
}

#[test]
fn members_and_spaces_as_streams() {
    let v = view();
    let m = v.read_member("QGPL", "QCLSRC", "HELLO").unwrap();
    assert!(m.source);
    assert_eq!(m.record_len, 80);
    assert_eq!(m.records.len(), 2);
    assert_eq!(ccsid::from_ebcdic(&m.records[1]).trim_end(), "ENDPGM");
    let lines = vec![
        ccsid::to_ebcdic("PGM").unwrap(),
        ccsid::to_ebcdic("DCL &A *CHAR 10").unwrap(),
        ccsid::to_ebcdic("ENDPGM").unwrap(),
    ];
    v.write_member("QGPL", "QCLSRC", "HELLO", lines).unwrap();
    let m = v.read_member("QGPL", "QCLSRC", "HELLO").unwrap();
    assert_eq!(m.records.len(), 3);
    assert_eq!(m.records[1].len(), 80);
    assert_eq!(
        ccsid::from_ebcdic(&m.records[1]).trim_end(),
        "DCL &A *CHAR 10"
    );
    assert_eq!(v.members("QGPL", "QCLSRC").unwrap()[0].size, 3 * 82);
    // A data member: fixed records, written back as given.
    v.create_member("QGPL", "CUST", "CUST", &user("BOB"))
        .unwrap();
    v.write_member("QGPL", "CUST", "CUST", vec![vec![0xC1; 10], vec![0xC2; 10]])
        .unwrap();
    let d = v.read_member("QGPL", "CUST", "CUST").unwrap();
    assert!(!d.source);
    assert_eq!(d.records, vec![vec![0xC1; 10], vec![0xC2; 10]]);
    // A user space grows and shrinks with what is written.
    assert_eq!(
        v.read_stream("QGPL", "SPACE", "*USRSPC").unwrap(),
        vec![0xC1; 16]
    );
    v.write_stream("QGPL", "SPACE", "*USRSPC", &[1, 2, 3])
        .unwrap();
    assert_eq!(
        v.read_stream("QGPL", "SPACE", "*USRSPC").unwrap(),
        vec![1, 2, 3]
    );
    assert_eq!(v.object("QGPL", "SPACE", "*USRSPC").unwrap().size, 3);
    assert_eq!(
        v.read_stream("QGPL", "CUST", "*FILE"),
        Err(ifs::Errno::ENOTSUP)
    );
    // A save file is a binary stream too.
    let savf = store::obj::type_by_name("*SAVF").unwrap().code;
    v.access()
        .with_main(&mut |t, qsys| {
            let qgpl = ops::find_library(t, qsys, name("QGPL")).unwrap().id;
            ops::create(t, &params(qgpl, "BACKUP", savf, "BOB")).unwrap();
            Ok(())
        })
        .unwrap();
    assert_eq!(v.read_stream("QGPL", "BACKUP", "*SAVF").unwrap(), b"");
    let data: Vec<u8> = (0..9000u32).map(|i| (i % 251) as u8).collect();
    v.write_stream("QGPL", "BACKUP", "*SAVF", &data).unwrap();
    assert_eq!(v.read_stream("QGPL", "BACKUP", "*SAVF").unwrap(), data);
    let e = v.object("QGPL", "BACKUP", "*SAVF").unwrap();
    assert_eq!((e.size, e.attr.as_str()), (9000, "SAVF"));
    v.write_stream("QGPL", "BACKUP", "*SAVF", &[7; 3]).unwrap();
    assert_eq!(
        v.read_stream("QGPL", "BACKUP", "*SAVF").unwrap(),
        vec![7; 3]
    );
}

#[test]
fn authority_create_delete() {
    let v = view();
    assert_eq!(
        v.authority("QGPL", "SPACE", "*USRSPC", &user("QSECOFR"))
            .unwrap(),
        Auth(Auth::MASK)
    );
    assert_eq!(
        v.authority("QGPL", "SPACE", "*USRSPC", &user("BOB"))
            .unwrap(),
        Auth::ALL
    );
    // *PUBLIC of a created object is *CHANGE here (QCRTAUT not applied by the test).
    assert_eq!(
        v.authority("QGPL", "SPACE", "*USRSPC", &user("ANN"))
            .unwrap(),
        Auth::CHANGE
    );
    v.create_library("MYLIB", &user("BOB")).unwrap();
    assert_eq!(v.object("QSYS", "MYLIB", "*LIB").unwrap().owner, "BOB");
    assert_eq!(
        v.create_library("MYLIB", &user("BOB")),
        Err(ifs::Errno::EEXIST)
    );
    v.create_member("QGPL", "QCLSRC", "NEW", &user("BOB"))
        .unwrap();
    assert_eq!(v.members("QGPL", "QCLSRC").unwrap().len(), 2);
    assert_eq!(
        v.create_member("QGPL", "QCLSRC", "NEW", &user("BOB")),
        Err(ifs::Errno::EEXIST)
    );
    v.delete(
        &QsysNode::Mbr("QGPL".into(), "QCLSRC".into(), "NEW".into()),
        &user("BOB"),
    )
    .unwrap();
    assert_eq!(v.members("QGPL", "QCLSRC").unwrap().len(), 1);
    assert_eq!(
        v.delete(&QsysNode::Lib("QGPL".into()), &user("QSECOFR")),
        Err(ifs::Errno::ENOTEMPTY)
    );
    v.delete(&QsysNode::Lib("MYLIB".into()), &user("BOB"))
        .unwrap();
    assert_eq!(v.object("QSYS", "MYLIB", "*LIB"), Err(ifs::Errno::ENOENT));
    v.delete(
        &QsysNode::Obj("QGPL".into(), "SPACE".into(), "*USRSPC".into()),
        &user("BOB"),
    )
    .unwrap();
    v.delete(
        &QsysNode::Obj("QGPL".into(), "CUST".into(), "*FILE".into()),
        &user("BOB"),
    )
    .unwrap();
    assert_eq!(v.objects("QGPL").unwrap().len(), 1);
}

#[test]
fn environment() {
    let (s, qsys) = store();
    let a = MemAccess::new(s, qsys);
    a.set_sysval("QCRTOBJAUD", "*ALL");
    let env = StoreEnv::new(&a);
    assert_eq!(env.uid_of("BOB"), Some(100));
    assert_eq!(env.profile_of_uid(101).as_deref(), Some("ANN"));
    assert_eq!(env.gid_of("DEV"), Some(500));
    assert_eq!(env.gid_of("BOB"), None);
    assert_eq!(env.profile_of_gid(500).as_deref(), Some("DEV"));
    assert_eq!(env.profile_of_gid(0), None);
    assert!(env.profile_exists("ANN"));
    assert!(!env.profile_exists("NOBODY"));
    assert!(env.is_group("DEV"));
    assert!(!env.is_group("ANN"));
    assert_eq!(env.crtobjaud(), ifs::meta::ObjAudit::All);
    assert!(env.autl("NOLIST").is_none());
    a.with_main(&mut |txn, qsys| {
        sec::auth::autl::create(txn, qsys, "PAYLIST", "pay", "BOB", Authority::EXCLUDE, NOW)
            .unwrap();
        let id = sec::auth::autl::find(txn, qsys, "PAYLIST").unwrap();
        sec::auth::grant::set_private(txn, qsys, id, "ANN", Some(Auth::USE)).unwrap();
        Ok(())
    })
    .unwrap();
    let l = env.autl("PAYLIST").unwrap();
    assert_eq!(l.owner, "BOB");
    assert_eq!(l.entries, vec![("ANN".to_string(), Auth::USE)]);
}

#[test]
fn behind_the_ifs() {
    let (s, qsys) = store();
    let access = Arc::new(MemAccess::new(s, qsys));
    let ifs = Ifs::new(
        MemBackend::new(),
        Box::new(StoreQsys::new(Arc::clone(&access))),
        Config::default(),
    );
    let env = StoreEnv::new(&*access);
    ifs.ensure_layout(&env).unwrap();
    let bob = user("BOB");
    let cx = Ctx {
        owner: Owner(1),
        principal: &bob,
        env: &env,
        audit: &NoAudit,
        job_ccsid: 37,
    };
    let names: Vec<String> = ifs
        .read_dir(&cx, "/QSYS.LIB/QGPL.LIB")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(names.contains(&"QCLSRC.FILE".to_string()));
    assert!(names.contains(&"SPACE.USRSPC".to_string()));
    let m = "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR";
    let st = ifs.stat(&cx, m).unwrap();
    assert_eq!(st.objtype, "*MBR");
    assert_eq!(st.size, 2 * 82);
    assert_eq!(st.owner, "BOB");
    assert_eq!(st.uid, 100);
    let fd = ifs
        .open(
            &cx,
            m,
            OpenArgs::new(O_RDONLY | O_TEXTDATA | O_CCSID).conv(1208),
        )
        .unwrap();
    let mut buf = [0u8; 256];
    let n = ifs.read(&cx, fd, &mut buf).unwrap();
    assert_eq!(
        &buf[..n],
        format!("{:<80}\n{:<80}\n", "PGM", "ENDPGM").as_bytes()
    );
    ifs.close(&cx, fd).unwrap();
    let fd = ifs
        .open(
            &cx,
            m,
            OpenArgs::new(O_WRONLY | O_TRUNC | O_TEXTDATA | O_CCSID).conv(1208),
        )
        .unwrap();
    ifs.write(&cx, fd, b"PGM\nSNDMSG MSG(HI)\nENDPGM\n")
        .unwrap();
    ifs.close(&cx, fd).unwrap();
    assert_eq!(ifs.stat(&cx, m).unwrap().size, 3 * 82);
    // A binary open reads the records back to back.
    let fd = ifs.open(&cx, m, OpenArgs::new(O_RDONLY)).unwrap();
    let n = ifs.read(&cx, fd, &mut buf).unwrap();
    assert_eq!(n, 240);
    assert_eq!(
        ccsid::from_ebcdic(&buf[80..95]).trim_end(),
        "SNDMSG MSG(HI)"
    );
    ifs.close(&cx, fd).unwrap();
    // A library needs authority to QSYS itself: the security officer.
    let sec = user("QSECOFR");
    let scx = Ctx {
        owner: Owner(2),
        principal: &sec,
        env: &env,
        audit: &NoAudit,
        job_ccsid: 37,
    };
    ifs.mkdir(&scx, "/QSYS.LIB/BOBLIB.LIB", 0o755).unwrap();
    assert_eq!(
        ifs.stat(&cx, "/QSYS.LIB/BOBLIB.LIB").unwrap().objtype,
        "*LIB"
    );
    ifs.rmdir(&scx, "/QSYS.LIB/BOBLIB.LIB").unwrap();
    assert_eq!(
        ifs.stat(&cx, "/QSYS.LIB/BOBLIB.LIB"),
        Err(ifs::Errno::ENOENT)
    );
}
