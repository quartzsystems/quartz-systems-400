// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The authority matrix through the operations: search on the path,
//! create and unlink in a directory, open modes, owner-only changes,
//! `CHGAUT` limits, authorization lists, adoption, restricted rename,
//! inheritance, and the audit events.

mod common;

use common::{sys, user};
use ifs::Errno;
use ifs::auth::AutlInfo;
use ifs::auth::data::{DataAuth, R, RWX, RX, W, WX, X};
use ifs::ops::audit::AuditKind;
use ifs::ops::flags::{O_RDONLY, O_RDWR, O_WRONLY};
use ifs::ops::{ChgAut, OpenArgs, RenameMode, Who};
use sec::auth::bits::Auth;
use sec::auth::principal::Adopted;

/// `/home/bob` owned by BOB with `*PUBLIC` `pub_dir`, holding `f`
/// with `*PUBLIC` `pub_file`.
fn setup(pub_dir: Auth, pub_file: Auth) -> common::Sys {
    let s = sys();
    let sec = user("QSECOFR");
    let cx = s.cx(&sec, 1);
    s.ifs.mkdir(&cx, "/home/bob", 0o700).unwrap();
    s.ifs.chgown(&cx, "/home/bob", "BOB", true).unwrap();
    let bob = user("BOB");
    let bx = s.cx(&bob, 2);
    s.put(&bx, "/home/bob/f", b"secret");
    for (p, a) in [("/home/bob", pub_dir), ("/home/bob/f", pub_file)] {
        s.ifs
            .chgaut(
                &bx,
                p,
                &ChgAut {
                    who: vec![Who::Public],
                    data: Some(DataAuth::of(a)),
                    ..ChgAut::default()
                },
            )
            .unwrap();
    }
    s
}

#[test]
fn search_needs_x_on_every_directory() {
    let s = setup(Auth::EXCLUDE, RWX);
    let zed = user("ZED");
    let zx = s.cx(&zed, 3);
    assert_eq!(s.ifs.stat(&zx, "/home/bob/f"), Err(Errno::EACCES));
    let s = setup(X, RWX);
    let zx = s.cx(&zed, 3);
    assert!(s.ifs.stat(&zx, "/home/bob/f").is_ok());
    // *X lets it pass but not list.
    assert_eq!(s.ifs.opendir(&zx, "/home/bob"), Err(Errno::EACCES));
    assert_eq!(Errno::EACCES.command_message(), "CPFA09C");
}

#[test]
fn open_modes_need_r_and_w() {
    let zed = user("ZED");
    for (file, mode, ok) in [
        (R, O_RDONLY, true),
        (R, O_WRONLY, false),
        (R, O_RDWR, false),
        (W, O_WRONLY, true),
        (W, O_RDONLY, false),
        (DataAuth(6).auth(), O_RDWR, true),
        (X, O_RDONLY, false),
        (Auth::EXCLUDE, O_RDONLY, false),
    ] {
        let s = setup(RX, file);
        let zx = s.cx(&zed, 3);
        let r = s.ifs.open(&zx, "/home/bob/f", OpenArgs::new(mode));
        assert_eq!(r.is_ok(), ok, "{file:?} {mode}");
        if !ok {
            assert_eq!(r, Err(Errno::EACCES));
        }
    }
}

#[test]
fn create_and_remove_need_wx_and_objexist() {
    let zed = user("ZED");
    // *RX on the directory: no create.
    let s = setup(RX, RWX);
    let zx = s.cx(&zed, 3);
    assert_eq!(s.ifs.creat(&zx, "/home/bob/z", 0o644), Err(Errno::EACCES));
    assert_eq!(s.ifs.mkdir(&zx, "/home/bob/zd", 0o755), Err(Errno::EACCES));
    // *WX: create yes; unlinking BOB's file needs *OBJEXIST on it too.
    let s = setup(WX, RWX);
    let zx = s.cx(&zed, 3);
    let fd = s.ifs.creat(&zx, "/home/bob/z", 0o644).unwrap();
    s.ifs.close(&zx, fd).unwrap();
    assert_eq!(s.ifs.stat(&zx, "/home/bob/z").unwrap().owner, "ZED");
    assert_eq!(s.ifs.unlink(&zx, "/home/bob/f"), Err(Errno::EACCES));
    s.ifs.unlink(&zx, "/home/bob/z").unwrap();
    // Give ZED *OBJEXIST on f.
    let bob = user("BOB");
    let bx = s.cx(&bob, 2);
    s.ifs
        .chgaut(
            &bx,
            "/home/bob/f",
            &ChgAut {
                who: vec![Who::User("ZED".into())],
                data: Some(DataAuth(0)),
                objects: Some(ifs::auth::need::OBJEXIST),
                ..ChgAut::default()
            },
        )
        .unwrap();
    s.ifs.unlink(&zx, "/home/bob/f").unwrap();
    let ev = s.audit.take();
    assert!(
        ev.iter()
            .any(|e| e.kind == AuditKind::AuthFail && e.path == "/home/bob/f" && e.user == "ZED")
    );
    assert!(
        ev.iter()
            .any(|e| e.kind == AuditKind::Delete && e.path == "/home/bob/f")
    );
}

#[test]
fn owner_only_changes() {
    let s = setup(RX, RWX);
    let zed = user("ZED");
    let zx = s.cx(&zed, 3);
    let bob = user("BOB");
    let bx = s.cx(&bob, 2);
    assert_eq!(s.ifs.chmod(&zx, "/home/bob/f", 0o777), Err(Errno::EPERM));
    s.ifs.chmod(&bx, "/home/bob/f", 0o640).unwrap();
    let st = s.ifs.stat(&bx, "/home/bob/f").unwrap();
    assert_eq!(
        st.mode & 0o777,
        0o600,
        "no primary group: group bits ignored"
    );
    assert_eq!(st.uid, 100);
    assert_eq!(
        s.ifs.chgown(&zx, "/home/bob/f", "ZED", true),
        Err(Errno::EPERM)
    );
    // BOB gives it away; with RVKOLDAUT(*NO) BOB keeps a private authority.
    s.ifs.chgown(&bx, "/home/bob/f", "ANN", false).unwrap();
    let list = s.ifs.dspaut(&bx, "/home/bob/f").unwrap();
    assert_eq!(list.owner, "ANN");
    assert!(
        list.entries
            .iter()
            .any(|e| e.who == "BOB" && e.data == "*RW")
    );
    assert_eq!(
        s.ifs.chown(&bx, "/home/bob/f", 102, u32::MAX),
        Err(Errno::EPERM)
    );
    let ann = user("ANN");
    let ax = s.cx(&ann, 4);
    // ANN (in DEV) makes DEV the primary group; ZED is not in DEV.
    s.ifs.chgpgp(&ax, "/home/bob/f", Some("DEV"), true).unwrap();
    assert_eq!(s.ifs.stat(&ax, "/home/bob/f").unwrap().gid, 500);
    assert_eq!(
        s.ifs.chgpgp(&ax, "/home/bob/f", Some("BOB"), true),
        Err(Errno::EINVAL)
    );
    let ev = s.audit.take();
    assert!(ev.iter().any(|e| e.kind == AuditKind::Owner));
    assert!(ev.iter().any(|e| e.kind == AuditKind::Group));
    assert!(ev.iter().any(|e| e.kind == AuditKind::AuthChange));
}

#[test]
fn chgaut_limits_for_objmgt_users() {
    let s = setup(RX, R);
    let bob = user("BOB");
    let bx = s.cx(&bob, 2);
    // ZED gets *R plus *OBJMGT privately.
    s.ifs
        .chgaut(
            &bx,
            "/home/bob/f",
            &ChgAut {
                who: vec![Who::User("ZED".into())],
                data: Some(DataAuth::of(R)),
                objects: Some(ifs::auth::need::OBJMGT),
                ..ChgAut::default()
            },
        )
        .unwrap();
    let zed = user("ZED");
    let zx = s.cx(&zed, 3);
    // It may pass on what it has...
    s.ifs
        .chgaut(
            &zx,
            "/home/bob/f",
            &ChgAut {
                who: vec![Who::User("ANN".into())],
                data: Some(DataAuth::of(R)),
                ..ChgAut::default()
            },
        )
        .unwrap();
    // ...but not *W, and not the list.
    let more = ChgAut {
        who: vec![Who::User("ANN".into())],
        data: Some(DataAuth::of(RWX)),
        ..ChgAut::default()
    };
    assert_eq!(s.ifs.chgaut(&zx, "/home/bob/f", &more), Err(Errno::EPERM));
    // ZED sees the whole list (it has *OBJMGT); ANN only its row and *PUBLIC.
    assert_eq!(s.ifs.dspaut(&zx, "/home/bob/f").unwrap().entries.len(), 4);
    let ann = user("ANN");
    let ax = s.cx(&ann, 4);
    let rows = s.ifs.dspaut(&ax, "/home/bob/f").unwrap().entries;
    assert_eq!(
        rows.iter().map(|e| e.who.as_str()).collect::<Vec<_>>(),
        vec!["*PUBLIC", "ANN"]
    );
    // A private *EXCLUDE decides even though *PUBLIC has *R.
    s.ifs
        .chgaut(
            &bx,
            "/home/bob/f",
            &ChgAut {
                who: vec![Who::User("ANN".into())],
                data: Some(DataAuth(0)),
                ..ChgAut::default()
            },
        )
        .unwrap();
    assert_eq!(
        s.ifs.open(&ax, "/home/bob/f", OpenArgs::new(O_RDONLY)),
        Err(Errno::EACCES)
    );
    s.ifs.rvkaut(&bx, "/home/bob/f", "ANN").unwrap();
    assert!(
        s.ifs
            .open(&ax, "/home/bob/f", OpenArgs::new(O_RDONLY))
            .is_ok()
    );
}

#[test]
fn authorization_lists_and_adoption() {
    let mut s = setup(RX, Auth::EXCLUDE);
    s.env = s.env.clone().autl(
        "PAYROLL",
        AutlInfo {
            owner: "QSECOFR".into(),
            owner_auth: Auth::ALL,
            public: Auth::EXCLUDE,
            entries: vec![("ZED".into(), R)],
        },
    );
    let bob = user("BOB");
    let bx = s.cx(&bob, 2);
    s.ifs
        .chgaut(
            &bx,
            "/home/bob/f",
            &ChgAut {
                autl: Some(Some("PAYROLL".into())),
                ..ChgAut::default()
            },
        )
        .unwrap();
    let zed = user("ZED");
    let zx = s.cx(&zed, 3);
    assert!(
        s.ifs
            .open(&zx, "/home/bob/f", OpenArgs::new(O_RDONLY))
            .is_ok()
    );
    assert_eq!(
        s.ifs.open(&zx, "/home/bob/f", OpenArgs::new(O_RDWR)),
        Err(Errno::EACCES)
    );
    assert_eq!(
        s.ifs.dspaut(&bx, "/home/bob/f").unwrap().autl.as_deref(),
        Some("PAYROLL")
    );
    // Adopting BOB's authority lets ANN write.
    let mut ann = user("ANN");
    let ax = s.cx(&ann, 4);
    assert_eq!(
        s.ifs.open(&ax, "/home/bob/f", OpenArgs::new(O_RDWR)),
        Err(Errno::EACCES)
    );
    ann.adopted.push(Adopted {
        program: "APP/PGM".into(),
        owner: "BOB".into(),
        groups: Vec::new(),
        spcaut: 0,
    });
    let ax = s.cx(&ann, 4);
    assert!(
        s.ifs
            .open(&ax, "/home/bob/f", OpenArgs::new(O_RDWR))
            .is_ok()
    );
    // Only the owner or *ALLOBJ changes the list.
    let zx = s.cx(&zed, 3);
    assert_eq!(
        s.ifs.chgaut(
            &zx,
            "/home/bob/f",
            &ChgAut {
                autl: Some(None),
                ..ChgAut::default()
            }
        ),
        Err(Errno::EACCES)
    );
}

#[test]
fn restricted_rename_in_tmp() {
    let s = sys();
    let bob = user("BOB");
    let zed = user("ZED");
    let bx = s.cx(&bob, 2);
    let zx = s.cx(&zed, 3);
    s.put(&bx, "/tmp/bobs", b"x");
    s.ifs.chmod(&bx, "/tmp/bobs", 0o777).unwrap();
    // ZED has *WX on /tmp and *OBJEXIST? No: but even with it, /tmp
    // restricts rename and unlink to the owner.
    assert_eq!(s.ifs.unlink(&zx, "/tmp/bobs"), Err(Errno::EACCES));
    assert_eq!(
        s.ifs
            .rename(&zx, "/tmp/bobs", "/tmp/zeds", RenameMode::Keep),
        Err(Errno::EACCES)
    );
    s.ifs
        .rename(&bx, "/tmp/bobs", "/tmp/b2", RenameMode::Keep)
        .unwrap();
    let sec = user("QSECOFR");
    let cx = s.cx(&sec, 1);
    s.ifs
        .chgaut(
            &cx,
            "/tmp/b2",
            &ChgAut {
                who: vec![Who::User("ZED".into())],
                data: Some(DataAuth::of(RWX)),
                objects: Some(Auth::ALL),
                ..ChgAut::default()
            },
        )
        .unwrap();
    assert_eq!(s.ifs.unlink(&zx, "/tmp/b2"), Err(Errno::EPERM));
    s.ifs.unlink(&bx, "/tmp/b2").unwrap();
}

#[test]
fn new_objects_inherit() {
    let s = sys();
    let sec = user("QSECOFR");
    let cx = s.cx(&sec, 1);
    s.ifs.mkdir(&cx, "/home/proj", 0o770).unwrap();
    s.ifs.chgpgp(&cx, "/home/proj", Some("DEV"), true).unwrap();
    s.ifs.chmod(&cx, "/home/proj", 0o770).unwrap();
    s.ifs
        .chgaut(
            &cx,
            "/home/proj",
            &ChgAut {
                who: vec![Who::User("ZED".into())],
                data: Some(DataAuth::of(RX)),
                ..ChgAut::default()
            },
        )
        .unwrap();
    s.ifs
        .set_attr(
            &cx,
            "/home/proj",
            &ifs::ops::Attr::Crtobjaud(ifs::meta::CrtObjAud::Value(ifs::meta::ObjAudit::All)),
            true,
        )
        .unwrap();
    let ann = user("ANN");
    let ax = s.cx(&ann, 4);
    let fd = s.ifs.creat(&ax, "/home/proj/new", 0o666).unwrap();
    s.ifs.close(&ax, fd).unwrap();
    let st = s.ifs.stat(&ax, "/home/proj/new").unwrap();
    assert_eq!(st.owner, "ANN");
    assert_eq!(st.group.as_deref(), Some("DEV"));
    assert_eq!(st.mode & 0o777, 0o644, "umask 022");
    let list = s.ifs.dspaut(&ax, "/home/proj/new").unwrap();
    assert!(
        list.entries
            .iter()
            .any(|e| e.who == "ZED" && e.data == "*RX"),
        "{list:?}"
    );
    let attrs = s
        .ifs
        .get_attrs(&ax, "/home/proj/new", &[ifs::ops::AttrId::Audit], true)
        .unwrap();
    assert_eq!(attrs, vec![ifs::ops::Attr::Audit(ifs::meta::ObjAudit::All)]);
    // Object auditing *ALL: reads and changes are reported.
    s.audit.take();
    s.get(&ax, "/home/proj/new");
    s.put(&ax, "/home/proj/new", b"x");
    let kinds: Vec<AuditKind> = s.audit.take().into_iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&AuditKind::Read));
    assert!(kinds.contains(&AuditKind::Change));
    // O_INHERITMODE: data authorities from the directory.
    let fd = s
        .ifs
        .open(
            &ax,
            "/home/proj/inh",
            OpenArgs::new(O_WRONLY | ifs::ops::flags::O_CREAT | ifs::ops::flags::O_INHERITMODE)
                .mode(0o600),
        )
        .unwrap();
    s.ifs.close(&ax, fd).unwrap();
    assert_eq!(
        s.ifs.stat(&ax, "/home/proj/inh").unwrap().mode & 0o777,
        0o770
    );
    // Changing auditing needs *AUDIT.
    assert_eq!(
        s.ifs.set_attr(
            &ax,
            "/home/proj/new",
            &ifs::ops::Attr::Audit(ifs::meta::ObjAudit::None),
            true
        ),
        Err(Errno::EPERM)
    );
}

#[test]
fn access_reports_the_callers_authority() {
    use ifs::ops::flags::{R_OK, W_OK, X_OK};
    let s = setup(RX, R);
    let zed = user("ZED");
    let zx = s.cx(&zed, 3);
    s.ifs.access(&zx, "/home/bob/f", R_OK).unwrap();
    assert_eq!(s.ifs.access(&zx, "/home/bob/f", W_OK), Err(Errno::EACCES));
    assert_eq!(
        s.ifs.access(&zx, "/home/bob/f", R_OK | X_OK),
        Err(Errno::EACCES)
    );
    let bob = user("BOB");
    let bx = s.cx(&bob, 2);
    s.ifs.access(&bx, "/home/bob/f", R_OK | W_OK).unwrap();
    assert_eq!(s.ifs.access(&bx, "/home/bob/f", X_OK), Err(Errno::EACCES));
}
