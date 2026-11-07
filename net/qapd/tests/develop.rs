// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 8 (draft): the `Develop` service against the test system,
//! one test per operation, the lanes, `WriteMember`'s pieces and dates,
//! the member locks, and older versions without it.

mod common;

use qap_proto::ops::command::{CommandReply, CommandText};
use qap_proto::ops::connection::HelloReply;
use qap_proto::ops::develop::{
    ChangeMember, CopyMember, CreateMember, DescribeObject, Libraries, LibraryKind, LibraryList,
    ListLibraries, ListMembers, ListObjects, MemberData, MemberRef, Members, ObjectDescription,
    Objects, ReadMember, RenameMember, SetLibraryList, WriteMember, Written,
};
use qap_proto::types::{MemberName, ObjectName, SourceRecord};
use qap_proto::{Envelope, Kind, Op, Service, Status};
use qapd::develop::yymmdd;
use qapd::testing::develop::{BIG_RECORDS, OTHER_JOB};

use common::{Run, cfg, start, start_with};

fn signed_on(user: &str, password: &str) -> Run {
    let mut r = start(cfg());
    let h: HelloReply = r.client.hello_range(1, 8).body_as().unwrap();
    assert_eq!(h.version, 8);
    assert!(h.services.contains(&Service::Develop) && h.services.contains(&Service::Jobs));
    r.client.sign_on(user, password);
    r
}

fn bob() -> Run {
    signed_on("BOB", "SECRET1")
}

fn obj(lib: &str, name: &str) -> ObjectName {
    ObjectName {
        library: lib.into(),
        name: name.into(),
    }
}

fn mbr(lib: &str, file: &str, member: &str) -> MemberName {
    MemberName {
        file: obj(lib, file),
        member: member.into(),
    }
}

fn assets() -> MemberName {
    mbr("HLINV", "QRPGLESRC", "ASSETS")
}

fn id(e: &Envelope) -> String {
    e.message.as_ref().map(|m| m.id.clone()).unwrap_or_default()
}

fn read(r: &mut Run, m: &MemberName, lock: bool) -> Envelope {
    r.client.call(
        Op::ReadMember,
        &ReadMember {
            member: m.clone(),
            lock,
        },
    )
}

fn write(r: &mut Run, m: &MemberName, records: Vec<SourceRecord>, renumber: bool) -> Envelope {
    r.client.call(
        Op::WriteMember,
        &WriteMember {
            member: m.clone(),
            records,
            renumber,
            srctype: None,
            text: None,
            first: true,
            last: true,
        },
    )
}

#[test]
fn version_7_peers_never_see_develop() {
    let mut r = start(cfg());
    let h: HelloReply = r.client.hello_range(1, 7).body_as().unwrap();
    assert_eq!(h.version, 7);
    assert!(!h.services.contains(&Service::Develop) && !h.services.contains(&Service::Jobs));
    r.client.sign_on("BOB", "SECRET1");
    for op in [
        Op::GetLibraryList,
        Op::ListLibraries,
        Op::ReadMember,
        Op::Compile,
        Op::ListJobs,
        Op::ListRoutines,
        Op::ListIndexes,
    ] {
        let rep = r.client.call(op, &());
        assert_eq!(rep.status, Status::InvalidRequest, "{op:?}");
    }
    // Version 8 itself: not before sign-on.
    let mut r = start(cfg());
    r.client.hello_range(8, 8);
    let rep = r.client.call(Op::ListLibraries, &ListLibraries::default());
    assert_eq!(rep.status, Status::NotSignedOn);
}

#[test]
fn get_library_list() {
    let mut r = bob();
    let l: LibraryList = r.client.call(Op::GetLibraryList, &()).body_as().unwrap();
    assert_eq!(l.current, None);
    assert_eq!(l.system, ["QSYS", "QSYS2", "QHLPSYS", "QUSRSYS"]);
    assert_eq!(l.user, ["QGPL", "QTEMP"]);
}

#[test]
fn set_library_list() {
    let mut r = bob();
    let set = |r: &mut Run, current: Option<&str>, user: &[&str]| {
        r.client.call(
            Op::SetLibraryList,
            &SetLibraryList {
                current: current.map(String::from),
                user: user.iter().map(|s| (*s).to_string()).collect(),
            },
        )
    };
    let rep = set(&mut r, Some("hlinv"), &["HLTEST", "QGPL"]);
    assert_eq!(rep.status, Status::Ok);
    let l: LibraryList = rep.body_as().unwrap();
    assert_eq!(
        (l.current.as_deref(), l.user.as_slice()),
        (Some("HLINV"), &["HLTEST".to_string(), "QGPL".into()][..])
    );
    // The same list comes back, and a set of what Get answers is a no-op.
    let again: LibraryList = r.client.call(Op::GetLibraryList, &()).body_as().unwrap();
    assert_eq!(again, l);
    let rep = set(&mut r, l.current.as_deref(), &["HLTEST", "QGPL"]);
    assert_eq!(rep.body_as::<LibraryList>().unwrap(), l);
    // `*LIBL` searches of the other operations see it.
    let objs: Objects = r
        .client
        .call(
            Op::ListObjects,
            &ListObjects {
                library: "*CURLIB".into(),
                pattern: "ASSETS".into(),
                types: vec!["*PGM".into()],
                ..ListObjects::default()
            },
        )
        .body_as()
        .unwrap();
    assert_eq!(objs.objects[0].library, "HLINV");
    let e = set(&mut r, None, &["NOSUCH"]);
    assert_eq!((e.status, id(&e).as_str()), (Status::NotFound, "CPF2110"));
    let e = set(&mut r, None, &["HLPAY"]);
    assert_eq!(
        (e.status, id(&e).as_str()),
        (Status::NotAuthorized, "CPF2182")
    );
    let e = set(&mut r, None, &["QGPL", "QGPL"]);
    assert_eq!((e.status, id(&e).as_str()), (Status::Error, "CPF2103"));
    // Another connection has its own list.
    let mut other = start_with(cfg(), r.c.clone());
    other.client.hello_range(8, 8);
    other.client.sign_on("BOB", "SECRET1");
    let l: LibraryList = other
        .client
        .call(Op::GetLibraryList, &())
        .body_as()
        .unwrap();
    assert_eq!(l.current, None);
}

#[test]
fn list_libraries() {
    let mut r = bob();
    let all: Libraries = r
        .client
        .call(Op::ListLibraries, &ListLibraries::default())
        .body_as()
        .unwrap();
    let names: Vec<&str> = all.libraries.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "HLINV", "HLTEST", "QGPL", "QHLPSYS", "QSYS", "QSYS2", "QUSRSYS"
        ]
    );
    assert_eq!(all.libraries[0].kind, LibraryKind::Test);
    let page: Libraries = r
        .client
        .call(
            Op::ListLibraries,
            &ListLibraries {
                pattern: "Q*".into(),
                start: 1,
                max: 2,
            },
        )
        .body_as()
        .unwrap();
    assert_eq!(page.total, 5);
    assert_eq!(page.libraries[0].name, "QHLPSYS");
    // ANN sees her own library.
    let mut ann = signed_on("ANN", "ANNPW01");
    let l: Libraries = ann
        .client
        .call(
            Op::ListLibraries,
            &ListLibraries {
                pattern: "HL*".into(),
                ..ListLibraries::default()
            },
        )
        .body_as()
        .unwrap();
    assert_eq!(l.total, 3);
}

#[test]
fn list_objects() {
    let mut r = bob();
    let list = |r: &mut Run, library: &str, types: &[&str], attr: Option<&str>| {
        r.client.call(
            Op::ListObjects,
            &ListObjects {
                library: library.into(),
                pattern: String::new(),
                types: types.iter().map(|s| (*s).to_string()).collect(),
                attribute: attr.map(String::from),
                start: 0,
                max: 0,
            },
        )
    };
    let o: Objects = list(&mut r, "HLINV", &[], None).body_as().unwrap();
    let names: Vec<(&str, &str)> = o
        .objects
        .iter()
        .map(|o| (o.name.as_str(), o.r#type.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("ASSETS", "*FILE"),
            ("ASSETS", "*PGM"),
            ("ASSETSL1", "*FILE"),
            ("HLSBS", "*SBSD"),
            ("QDDSSRC", "*FILE"),
            ("QRPGLESRC", "*FILE"),
            ("STKRPT", "*PGM")
        ]
    );
    assert!(o.objects.iter().all(|o| o.size > 0 && o.changed > 0));
    let pgms: Objects = list(&mut r, "HLINV", &["*PGM"], Some("RPGLE"))
        .body_as()
        .unwrap();
    assert_eq!(pgms.total, 2);
    let libl: Objects = list(&mut r, "*LIBL", &["*PGM"], None).body_as().unwrap();
    assert_eq!(libl.objects[0].name, "QCMD");
    let all: Objects = list(&mut r, "*ALL", &["*FILE"], Some("PF"))
        .body_as()
        .unwrap();
    assert!(all.objects.iter().all(|o| o.library != "HLPAY"));
    let e = list(&mut r, "NOSUCH", &[], None);
    assert_eq!((e.status, id(&e).as_str()), (Status::NotFound, "CPF2110"));
    let e = list(&mut r, "HLPAY", &[], None);
    assert_eq!(
        (e.status, id(&e).as_str()),
        (Status::NotAuthorized, "CPF2182")
    );
}

#[test]
fn describe_object() {
    let mut r = bob();
    let d: ObjectDescription = r
        .client
        .call(
            Op::DescribeObject,
            &DescribeObject {
                object: obj("HLINV", "ASSETS"),
                r#type: "*PGM".into(),
            },
        )
        .body_as()
        .unwrap();
    let get = |n: &str| {
        d.attributes
            .iter()
            .find(|a| a.name == n)
            .map(|a| a.value.clone())
    };
    assert_eq!(get("Attribute").as_deref(), Some("RPGLE"));
    assert_eq!(get("Owner").as_deref(), Some("QPGMR"));
    assert_eq!(d.attributes[0].name, "Object");
    let e = r.client.call(
        Op::DescribeObject,
        &DescribeObject {
            object: obj("*LIBL", "NOSUCH"),
            r#type: "*PGM".into(),
        },
    );
    assert_eq!((e.status, id(&e).as_str()), (Status::NotFound, "CPF9801"));
}

#[test]
fn list_members() {
    let mut r = bob();
    let m: Members = r
        .client
        .call(
            Op::ListMembers,
            &ListMembers {
                file: obj("HLINV", "QRPGLESRC"),
                pattern: String::new(),
                start: 0,
                max: 0,
            },
        )
        .body_as()
        .unwrap();
    let names: Vec<&str> = m.members.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["ASSETS", "BIG", "LOCKED", "STKRPT"]);
    assert_eq!(
        (m.members[0].srctype.as_str(), m.members[0].records),
        ("RPGLE", 40)
    );
    let page: Members = r
        .client
        .call(
            Op::ListMembers,
            &ListMembers {
                file: obj("HLINV", "QRPGLESRC"),
                pattern: "S*".into(),
                start: 0,
                max: 1,
            },
        )
        .body_as()
        .unwrap();
    assert_eq!((page.total, page.members[0].name.as_str()), (1, "STKRPT"));
    let e = r.client.call(
        Op::ListMembers,
        &ListMembers {
            file: obj("HLINV", "NOSUCH"),
            ..ListMembers::default()
        },
    );
    assert_eq!((e.status, id(&e).as_str()), (Status::NotFound, "CPF9812"));
}

#[test]
fn read_member() {
    let mut r = bob();
    let d: MemberData = read(&mut r, &assets(), false).body_as().unwrap();
    assert_eq!((d.width, d.ccsid, d.records.len()), (100, 37, 40));
    assert_eq!(
        d.records[0],
        SourceRecord {
            seq: 100,
            date: 260_105,
            data: "**FREE".into()
        }
    );
    // Over 1 MiB: split with More, joined by the client.
    let id1 = r.client.request(
        Op::ReadMember,
        &ReadMember {
            member: mbr("HLINV", "QRPGLESRC", "BIG"),
            lock: false,
        },
    );
    let mut frames = 0;
    loop {
        let f = r.client.recv_frame().unwrap();
        assert_eq!((f.kind, f.request_id), (Kind::Reply, id1));
        frames += 1;
        if !f.more() {
            break;
        }
    }
    assert!(frames > 1);
    let big: MemberData = read(&mut r, &mbr("HLINV", "QRPGLESRC", "BIG"), false)
        .body_as()
        .unwrap();
    assert_eq!(big.records.len(), BIG_RECORDS);
    let e = read(&mut r, &mbr("HLINV", "QRPGLESRC", "NOSUCH"), false);
    assert_eq!((e.status, id(&e).as_str()), (Status::NotFound, "CPF9815"));
    let e = read(&mut r, &mbr("HLINV", "ASSETS", "ASSETS"), false);
    assert_eq!(e.status, Status::Error);
    let e = read(&mut r, &mbr("HLTEST", "QBINSRC", "RAW"), false);
    assert_eq!((e.status, id(&e).as_str()), (Status::Error, "TCP1A5A"));
    let e = read(&mut r, &mbr("HLPAY", "QRPGLESRC", "PAYROLL"), false);
    assert_eq!(e.status, Status::NotAuthorized);
}

#[test]
fn locks_hold_a_member_for_the_connection() {
    let mut r = bob();
    // Another job's lock.
    let e = read(&mut r, &mbr("HLINV", "QRPGLESRC", "LOCKED"), true);
    assert_eq!((e.status, id(&e).as_str()), (Status::InUse, "CPF1002"));
    assert_eq!(
        r.c.develop.lock().unwrap().locks.values().next(),
        Some(&OTHER_JOB)
    );
    // Read without a lock is fine.
    assert_eq!(
        read(&mut r, &mbr("HLINV", "QRPGLESRC", "LOCKED"), false).status,
        Status::Ok
    );
    // This connection's lock keeps the other connection out, until unlocked.
    assert_eq!(read(&mut r, &assets(), true).status, Status::Ok);
    let mut other = start_with(cfg(), r.c.clone());
    other.client.hello_range(8, 8);
    other.client.sign_on("BOB", "SECRET1");
    assert_eq!(read(&mut other, &assets(), true).status, Status::InUse);
    let d: MemberData = read(&mut r, &assets(), false).body_as().unwrap();
    let e = write(&mut other, &assets(), d.records.clone(), false);
    assert_eq!((e.status, id(&e).as_str()), (Status::InUse, "CPF1002"));
    let unlock = |r: &mut Run| {
        r.client
            .call(Op::UnlockMember, &MemberRef { member: assets() })
    };
    assert_eq!(unlock(&mut r).status, Status::Ok);
    let e = unlock(&mut r);
    assert_eq!((e.status, id(&e).as_str()), (Status::NotFound, "TCP1A61"));
    assert_eq!(read(&mut other, &assets(), true).status, Status::Ok);
    // A write by the holder ends its lock; the connection's end does too.
    assert_eq!(
        write(&mut other, &assets(), d.records, false).status,
        Status::Ok
    );
    assert_eq!(read(&mut r, &assets(), true).status, Status::Ok);
    drop(r.client);
    let _ = r.server.join();
    assert_eq!(read(&mut other, &assets(), true).status, Status::Ok);
}

#[test]
fn write_member_keeps_dates_of_records_unchanged() {
    let mut r = bob();
    let today = yymmdd(r.c.now());
    let d: MemberData = read(&mut r, &assets(), false).body_as().unwrap();
    // The same records back: nothing changes.
    let w: Written = write(&mut r, &assets(), d.records.clone(), false)
        .body_as()
        .unwrap();
    assert_eq!(w.records, 40);
    let same: MemberData = read(&mut r, &assets(), false).body_as().unwrap();
    assert_eq!(same.records, d.records);
    // One changed, one added: those two carry today's date.
    let mut recs = d.records.clone();
    recs[3].data = "dcl-s count int(20) inz(0);".into();
    recs.insert(
        4,
        SourceRecord {
            seq: 450,
            date: 0,
            data: "// new".into(),
        },
    );
    write(&mut r, &assets(), recs, false);
    let now: MemberData = read(&mut r, &assets(), false).body_as().unwrap();
    assert_eq!(now.records.len(), 41);
    assert_eq!((now.records[3].date, now.records[4].date), (today, today));
    assert_eq!(now.records[4].seq, 450);
    assert_eq!(now.records[2], d.records[2]);
    // Renumbered: 1.00 steps, dates kept by the old sequence numbers.
    write(&mut r, &assets(), now.records.clone(), true);
    let renum: MemberData = read(&mut r, &assets(), false).body_as().unwrap();
    assert!(
        renum
            .records
            .iter()
            .zip(1u32..)
            .all(|(x, n)| x.seq == n * 100)
    );
    assert_eq!(renum.records[5].date, now.records[5].date);
    // Sequence numbers out of order, a record too long, a character CCSID
    // 37 cannot hold.
    let mut bad = d.records.clone();
    bad.swap(0, 1);
    assert_eq!(
        write(&mut r, &assets(), bad, false).status,
        Status::InvalidRequest
    );
    let long = vec![SourceRecord {
        seq: 100,
        date: 0,
        data: "x".repeat(101),
    }];
    let e = write(&mut r, &assets(), long, false);
    assert_eq!(
        (e.status, id(&e).as_str()),
        (Status::InvalidRequest, "TCP1A62")
    );
    let snow = vec![SourceRecord {
        seq: 100,
        date: 0,
        data: "\u{2603}".into(),
    }];
    let e = write(&mut r, &assets(), snow, false);
    assert_eq!((e.status, id(&e).as_str()), (Status::Error, "TCP1A5A"));
    let after: MemberData = read(&mut r, &assets(), false).body_as().unwrap();
    assert_eq!(after.records, renum.records, "nothing written");
    let e = write(
        &mut r,
        &mbr("HLINV", "QRPGLESRC", "NOSUCH"),
        Vec::new(),
        false,
    );
    assert_eq!(e.status, Status::NotFound);
}

#[test]
fn write_member_in_pieces() {
    let mut r = bob();
    let piece = |r: &mut Run, recs: &[SourceRecord], first, last| {
        r.client.call(
            Op::WriteMember,
            &WriteMember {
                member: mbr("HLINV", "QRPGLESRC", "STKRPT"),
                records: recs.to_vec(),
                renumber: false,
                srctype: Some("SQLRPGLE".into()),
                text: Some("in pieces".into()),
                first,
                last,
            },
        )
    };
    let recs: Vec<SourceRecord> = (1..=30)
        .map(|n| SourceRecord {
            seq: n * 100,
            date: 260_101,
            data: format!("// {n}"),
        })
        .collect();
    // A piece without a first is refused.
    assert_eq!(
        piece(&mut r, &recs[..10], false, false).status,
        Status::InvalidRequest
    );
    let w: Written = piece(&mut r, &recs[..10], true, false).body_as().unwrap();
    assert_eq!(w.records, 10);
    // Nothing written until the last.
    let mid: MemberData = read(&mut r, &mbr("HLINV", "QRPGLESRC", "STKRPT"), false)
        .body_as()
        .unwrap();
    assert_eq!(mid.records.len(), 2);
    let w: Written = piece(&mut r, &recs[10..20], false, false)
        .body_as()
        .unwrap();
    assert_eq!(w.records, 20);
    let w: Written = piece(&mut r, &recs[20..], false, true).body_as().unwrap();
    assert_eq!(w.records, 30);
    let d: MemberData = read(&mut r, &mbr("HLINV", "QRPGLESRC", "STKRPT"), false)
        .body_as()
        .unwrap();
    assert_eq!(d.records.len(), 30);
    assert_eq!(
        (d.srctype.as_str(), d.text.as_str()),
        ("SQLRPGLE", "in pieces")
    );
    // A first again starts over.
    piece(&mut r, &recs[..5], true, false);
    let w: Written = piece(&mut r, &recs[..3], true, true).body_as().unwrap();
    assert_eq!(w.records, 3);
}

#[test]
fn create_copy_rename_change_and_delete_members() {
    let mut r = bob();
    let create = |r: &mut Run, name: &str| {
        r.client.call(
            Op::CreateMember,
            &CreateMember {
                member: mbr("HLINV", "QRPGLESRC", name),
                srctype: "rpgle".into(),
                text: "new one".into(),
            },
        )
    };
    assert_eq!(create(&mut r, "NEWPGM").status, Status::Ok);
    let e = create(&mut r, "NEWPGM");
    assert_eq!((e.status, id(&e).as_str()), (Status::Exists, "CPF5812"));
    let copy = |r: &mut Run, to: &str, replace| {
        r.client.call(
            Op::CopyMember,
            &CopyMember {
                from: assets(),
                to: mbr("HLINV", "QRPGLESRC", to),
                replace,
            },
        )
    };
    assert_eq!(copy(&mut r, "COPY1", false).status, Status::Ok);
    let orig: MemberData = read(&mut r, &assets(), false).body_as().unwrap();
    let c: MemberData = read(&mut r, &mbr("HLINV", "QRPGLESRC", "COPY1"), false)
        .body_as()
        .unwrap();
    assert_eq!(c.records, orig.records, "sequence numbers and dates kept");
    assert_eq!(copy(&mut r, "NEWPGM", false).status, Status::Exists);
    assert_eq!(copy(&mut r, "NEWPGM", true).status, Status::Ok);
    let e = copy(&mut r, "LOCKED", true);
    assert_eq!(e.status, Status::InUse);
    let rename = |r: &mut Run, from: &str, to: &str| {
        r.client.call(
            Op::RenameMember,
            &RenameMember {
                member: mbr("HLINV", "QRPGLESRC", from),
                new_name: to.into(),
            },
        )
    };
    assert_eq!(rename(&mut r, "COPY1", "COPY2").status, Status::Ok);
    assert_eq!(rename(&mut r, "COPY2", "ASSETS").status, Status::Exists);
    assert_eq!(rename(&mut r, "NOSUCH", "X").status, Status::NotFound);
    let change = r.client.call(
        Op::ChangeMember,
        &ChangeMember {
            member: mbr("HLINV", "QRPGLESRC", "COPY2"),
            srctype: Some("sqlrpgle".into()),
            text: None,
        },
    );
    assert_eq!(change.status, Status::Ok);
    let c: MemberData = read(&mut r, &mbr("HLINV", "QRPGLESRC", "COPY2"), false)
        .body_as()
        .unwrap();
    assert_eq!(
        (c.srctype.as_str(), c.text.as_str()),
        ("SQLRPGLE", "Asset list")
    );
    let delete = |r: &mut Run, name: &str| {
        r.client.call(
            Op::DeleteMember,
            &MemberRef {
                member: mbr("HLINV", "QRPGLESRC", name),
            },
        )
    };
    assert_eq!(delete(&mut r, "COPY2").status, Status::Ok);
    assert_eq!(delete(&mut r, "COPY2").status, Status::NotFound);
    assert_eq!(delete(&mut r, "LOCKED").status, Status::InUse);
}

#[test]
fn compile_runs_in_the_lane() {
    // The test system has no compiler: Compile answers Error with
    // TCP1A60, in the lane, after the command before it.
    let mut r = bob();
    let first = r.client.request(
        Op::RunCommand,
        &CommandText {
            command: "CHGCURLIB CURLIB(HLINV)".into(),
        },
    );
    let libl = r.client.request(Op::GetLibraryList, &());
    let comp = r.client.request(
        Op::Compile,
        &CommandText {
            command: "CRTBNDRPG PGM(HLINV/ASSETS)".into(),
        },
    );
    let ra = r.client.reply(first);
    assert_eq!(ra.status, Status::Ok);
    let _: CommandReply = ra.body_as().unwrap();
    let rb = r.client.reply(libl);
    let l: LibraryList = rb.body_as().unwrap();
    assert_eq!(l.current.as_deref(), Some("HLINV"));
    let rc = r.client.reply(comp);
    assert_eq!((rc.status, id(&rc).as_str()), (Status::Error, "TCP1A60"));
}
