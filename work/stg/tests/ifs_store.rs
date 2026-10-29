// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS over the link as the storage service runs it, with a real
//! object store behind `/QSYS.LIB` (`qsyslib::StoreQsys`), the store's
//! profiles as the environment, and audit events written to
//! `QSYS/QAUDJRN` after each request as the service's flush writes
//! them.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::CreateFileOpts;
use db::file::MemberOpt;
use ifs::Errno;
use ifs::backend::mem::MemBackend;
use ifs::client::{IfsClient, read_all, write_all};
use ifs::ops::flags::{O_CCSID, O_CREAT, O_RDONLY, O_TEXTDATA, O_TRUNC, O_WRONLY};
use ifs::ops::{Config, Ctx, Ifs, OpenArgs, Owner};
use qsname::{MemberSpec, Name};
use qsyslib::{JournalAudit, MemAccess, StoreAccess, StoreEnv, StoreQsys};
use sec::auth::principal::Principal;
use sec::profile::payload::UsrPrf;
use sec::profile::spcaut::SpecialAuth;
use stg::ifs::serve;
use stg::remote::RemoteIfs;
use stg::{Link, Request};
use store::obj::header::Authority;
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types};
use store::txn::Txn;
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
}

fn profile(t: &mut Txn<'_>, qsys: ObjectId, user: &str, uid: u32) {
    let mut p = UsrPrf::new(user, 40);
    p.uid = uid;
    sec::profile::objects::create(t, qsys, &p, "QSECOFR", Authority::EXCLUDE, NOW).unwrap();
}

/// A store with `QSYS/QAUDJRN`, `QGPL/QCLSRC(HELLO)` owned by BOB with
/// `*PUBLIC *EXCLUDE`, a data file `QGPL/CUST` and three profiles.
fn store() -> Arc<MemAccess> {
    let d = Arc::new(MemBlockDev::with_seed(16384, 9));
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
    let qgpl = ops::create(
        &mut t,
        &CreateParams {
            library: qsys,
            name: name("QGPL"),
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
    let (page, mut h) = ops::read_header(&mut t, src).unwrap();
    h.public_authority = Authority::EXCLUDE;
    ops::write_header(&mut t, page, &h).unwrap();
    let opts = CreateFileOpts {
        member: MemberOpt::Named(name("CUST")),
        ..opts
    };
    db::file::create_pf_rcdlen(&mut t, qgpl, name("CUST"), 4, &opts).unwrap();
    profile(&mut t, qsys, "QSYS", 0);
    profile(&mut t, qsys, "QSECOFR", 1);
    profile(&mut t, qsys, "BOB", 100);
    profile(&mut t, qsys, "ANN", 101);
    let rcv = jrn::rcv::crtjrnrcv(
        &mut t,
        qsys,
        name("AUDRCV0001"),
        &jrn::rcv::CrtJrnRcvOpts {
            now: NOW,
            ..jrn::rcv::CrtJrnRcvOpts::default()
        },
    )
    .unwrap();
    jrn::journal::crtjrn(
        &mut t,
        qsys,
        name("QAUDJRN"),
        rcv,
        &jrn::CrtJrnOpts {
            now: NOW,
            ..jrn::CrtJrnOpts::default()
        },
    )
    .unwrap();
    t.commit().unwrap();
    Arc::new(MemAccess::new(store, qsys))
}

struct Far {
    access: Arc<MemAccess>,
    ifs: Arc<Ifs<MemBackend>>,
}

fn far() -> Far {
    let access = store();
    let ifs = Ifs::new(
        MemBackend::new(),
        Box::new(StoreQsys::new(Arc::clone(&access))),
        Config::default(),
    );
    ifs.ensure_layout(&StoreEnv::new(&*access)).unwrap();
    Far {
        access,
        ifs: Arc::new(ifs),
    }
}

fn principal(user: &str) -> Principal {
    let mut p = Principal {
        user: user.into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    };
    if user == "QSECOFR" {
        p.spcaut = SpecialAuth::ALLOBJ.0;
    }
    p
}

/// A job's connection, served as the service serves it: the request
/// answered, then the audit events it recorded written to `QAUDJRN`.
fn connect(far: &Far, user: &str, owner: u64) -> RemoteIfs {
    let ifs = Arc::clone(&far.ifs);
    let access = Arc::clone(&far.access);
    let p = principal(user);
    let link = Link::answering(move |req| {
        let Request::IfsReq { op } = req else {
            panic!("not an IFS request: {}", req.name());
        };
        let env = StoreEnv::new(&*access);
        let cx = Ctx {
            owner: Owner(owner),
            principal: &p,
            env: &env,
            audit: &JournalAudit,
            job_ccsid: 37,
        };
        let reply = serve(&ifs::client::Direct { ifs: &ifs, cx: &cx }, op);
        let job = jrn::JobInfo {
            name: "QPADEV0001".into(),
            user: p.user.clone(),
            number: u32::try_from(owner).unwrap(),
        };
        let events: Vec<_> = sec::audit::take()
            .into_iter()
            .map(|e| (e, job.clone()))
            .collect();
        access
            .with_main(&mut |t, qsys| {
                sec::audit::write::write(t, qsys, &events).map_err(|_| Errno::EIO)
            })
            .unwrap();
        ipc::encode(&reply)
    });
    RemoteIfs::new(Rc::new(RefCell::new(link)))
}

/// The audit entries in the journal.
fn journal(far: &Far) -> Vec<(jrn::Entry, sec::audit::Event)> {
    far.access
        .with_main(&mut |t, qsys| {
            sec::audit::write::entries(t, qsys, &jrn::Filter::default()).map_err(|_| Errno::EIO)
        })
        .unwrap()
}

fn source(far: &Far) -> Vec<String> {
    far.access
        .with_main(&mut |t, qsys| {
            let qgpl = ops::find_library(t, qsys, name("QGPL")).unwrap().id;
            let l = store::obj::library::Library::load(t, qgpl).unwrap();
            let f = l.lookup(t, types::FILE, name("QCLSRC")).unwrap().unwrap();
            Ok(db::source::records(t, f, MemberSpec::Name(name("HELLO")))
                .unwrap()
                .into_iter()
                .map(|(_, _, d)| ccsid::from_ebcdic(&d).trim_end().to_string())
                .collect())
        })
        .unwrap()
}

#[test]
fn members_through_the_store() {
    let far = far();
    let bob = connect(&far, "BOB", 1);
    let m = "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR";
    let st = bob.stat(m, true).unwrap();
    assert_eq!(
        (st.objtype.as_str(), st.owner.as_str(), st.uid),
        ("*MBR", "BOB", 100)
    );
    assert_eq!(st.ccsid, 37);
    // Text mode: the records with line ends, in the open's CCSID.
    let text = OpenArgs::new(O_RDONLY | O_TEXTDATA | O_CCSID).conv(1208);
    let fd = bob.open(m, text).unwrap();
    let got = String::from_utf8(read_all(&bob, fd).unwrap()).unwrap();
    bob.close(fd).unwrap();
    assert_eq!(got, format!("{:<80}\n{:<80}\n", "PGM", "ENDPGM"));
    // Written as text: lines become records.
    let w = OpenArgs::new(O_WRONLY | O_TRUNC | O_TEXTDATA | O_CCSID).conv(1208);
    let fd = bob.open(m, w).unwrap();
    write_all(&bob, fd, b"PGM\r\nDCL &N *DEC 5\nENDPGM\n").unwrap();
    bob.close(fd).unwrap();
    assert_eq!(source(&far), vec!["PGM", "DCL &N *DEC 5", "ENDPGM"]);
    // Binary: the records back to back, and back again.
    let fd = bob.open(m, OpenArgs::new(O_RDONLY)).unwrap();
    let raw = read_all(&bob, fd).unwrap();
    bob.close(fd).unwrap();
    assert_eq!(raw.len(), 3 * 80);
    let fd = bob.open(m, OpenArgs::new(O_WRONLY | O_TRUNC)).unwrap();
    write_all(&bob, fd, &raw[..160]).unwrap();
    bob.close(fd).unwrap();
    assert_eq!(source(&far), vec!["PGM", "DCL &N *DEC 5"]);
    // A data member: fixed records.
    let d = "/QSYS.LIB/QGPL.LIB/CUST.FILE/CUST.MBR";
    let fd = bob.open(d, OpenArgs::new(O_WRONLY | O_TRUNC)).unwrap();
    write_all(&bob, fd, &[0xC1, 0xC2, 0xC3, 0xC4, 0xF1, 0xF2]).unwrap();
    bob.close(fd).unwrap();
    let fd = bob.open(d, OpenArgs::new(O_RDONLY)).unwrap();
    assert_eq!(
        read_all(&bob, fd).unwrap(),
        vec![0xC1, 0xC2, 0xC3, 0xC4, 0xF1, 0xF2, 0x40, 0x40]
    );
    bob.close(fd).unwrap();
    // A new member, then gone.
    let n = "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/NEW.MBR";
    let fd = bob.open(n, OpenArgs::new(O_WRONLY | O_CREAT)).unwrap();
    bob.close(fd).unwrap();
    assert_eq!(
        bob.read_dir("/QSYS.LIB/QGPL.LIB/QCLSRC.FILE")
            .unwrap()
            .len(),
        2
    );
    bob.unlink(n).unwrap();
    assert_eq!(bob.stat(n, true), Err(Errno::ENOENT));
}

#[test]
fn refusals_reach_the_audit_journal() {
    let far = far();
    let ann = connect(&far, "ANN", 2);
    let m = "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR";
    // QCLSRC is *PUBLIC *EXCLUDE: ANN may not even open it.
    assert_eq!(ann.open(m, OpenArgs::new(O_RDONLY)), Err(Errno::EACCES));
    assert_eq!(ann.mkdir("/home/ann", 0o755), Err(Errno::EACCES));
    let back = journal(&far);
    let af: Vec<&sec::audit::Event> = back
        .iter()
        .filter(|(e, _)| &e.kind == b"AF")
        .map(|(_, ev)| ev)
        .collect();
    assert!(af.len() >= 2, "{back:?}");
    assert!(back.iter().all(|(e, _)| e.job.user == "ANN"));
    let home = af
        .iter()
        .find(|e| e.path() == "/home")
        .expect("an AF for /home");
    assert_eq!(home.get("VIOL"), "A");
    assert_eq!(home.get("OTYP"), "*DIR");
    assert_eq!(home.get("APIN"), "Y");
    assert_eq!(home.get("PNLN"), "5");
    assert_eq!(home.get("OFID").len(), 32);
    assert!(
        af.iter()
            .any(|e| e.path().starts_with("/QSYS.LIB/QGPL.LIB/QCLSRC.FILE"))
    );
    // What BOB does to his own file is not a failure.
    let bob = connect(&far, "BOB", 1);
    let fd = bob.open(m, OpenArgs::new(O_RDONLY)).unwrap();
    bob.close(fd).unwrap();
    assert_eq!(
        journal(&far)
            .iter()
            .filter(|(e, _)| &e.kind == b"AF")
            .count(),
        af.len()
    );
    // A connection's descriptors go with it.
    let fd = bob.open(m, OpenArgs::new(O_RDONLY)).unwrap();
    assert_eq!(far.ifs.open_count(), 1);
    far.ifs.release_owner(Owner(1));
    assert_eq!(far.ifs.open_count(), 0);
    assert_eq!(bob.close(fd), Err(Errno::EBADF));
}
