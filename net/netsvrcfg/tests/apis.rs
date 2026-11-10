// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `QZLS*` APIs against a host in memory: shares, attributes, the
//! guest, enabling users, start and end, sessions and their lists.

#![allow(clippy::many_single_char_names, clippy::type_complexity)]

use netsvrcfg::api::{self, ApiError, MemHost, ServerControl};
use netsvrcfg::fmt::text::{Rec, bin4, bin8, fixed, text_at};
use netsvrcfg::{Kind, Permission, Session, SpoolType};

fn b4(v: i32) -> Vec<u8> {
    v.to_be_bytes().to_vec()
}

fn adfs(name: &str, path: &str, perm: i32, max: i32) -> Vec<Vec<u8>> {
    let p = fixed(path, path.len());
    vec![
        fixed(name, 12),
        p.clone(),
        b4(i32::try_from(p.len()).expect("short")),
        b4(0),
        fixed("Shared", 50),
        b4(perm),
        b4(max),
    ]
}

fn reason(e: &ApiError, off: usize) -> i32 {
    bin4(&e.data, off)
}

#[test]
fn file_shares_are_added_changed_and_removed() {
    let mut h = MemHost::new("QS400");
    h.paths.push(("/home/docs".into(), "BOB".into()));
    api::call(&mut h, "QZLSADFS", &adfs("docs", "/home/docs/", 1, 10)).expect("adds");
    let c = h.config.clone().expect("saved");
    let s = c.share("DOCS").expect("there");
    let f = s.as_file().expect("file");
    assert_eq!(
        (f.path.as_str(), f.permission, f.max_users),
        ("/home/docs", Permission::ReadOnly, 10)
    );

    let e = api::call(&mut h, "QZLSADFS", &adfs("DOCS", "/x", 1, 1)).expect_err("duplicate");
    assert_eq!((e.id, reason(&e, 22)), ("CPFB68A", 1));
    assert_eq!(text_at(&e.data, 10, 12), "DOCS");

    let mut ch = adfs("DOCS", "/home/docs", 2, -1);
    ch.extend([
        b4(819),
        vec![0xF1],
        {
            let mut t = Rec::zeros(0);
            t.bin4(0, 3);
            t.char(4, 46, "txt");
            t.bin4(50, 3);
            t.char(54, 46, "MBR");
            t.buf
        },
        b4(2),
        vec![0xF1],
    ]);
    api::call(&mut h, "QZLSCHFS", &ch).expect("changes");
    let c = h.config.clone().expect("saved");
    let s = c.share("DOCS").expect("there");
    let f = s.as_file().expect("file");
    assert_eq!(f.extensions, vec!["TXT".to_string(), "MBR".to_string()]);
    assert!(f.text_conversion && s.encryption_required);
    assert_eq!(f.text_ccsid, 819);

    for (parms, r) in [
        (adfs("NEW", "relative", 1, 1), 7),
        (adfs("NEW", "/a", 3, 1), 3),
        (adfs("NEW", "/a", 1, 0), 4),
    ] {
        let e = api::call(&mut h, "QZLSADFS", &parms).expect_err("refused");
        assert_eq!((e.id, reason(&e, 22)), ("CPFB68A", r));
    }
    let e = api::call(&mut h, "QZLSADFS", &adfs("A/B", "/a", 1, 1)).expect_err("bad name");
    assert_eq!(e.id, "CPFB68B");

    api::call(&mut h, "QZLSRMS", &[fixed("docs", 12)]).expect("removes");
    let e = api::call(&mut h, "QZLSRMS", &[fixed("DOCS", 12)]).expect_err("gone");
    assert_eq!((e.id, reason(&e, 22)), ("CPFB68A", 2));
}

#[test]
fn without_iosyscfg_only_owners_share() {
    let mut h = MemHost::new("QS400");
    h.caller.iosyscfg = false;
    h.caller.user = "BOB".into();
    h.paths.push(("/home/bob".into(), "BOB".into()));
    h.paths.push(("/home/ann".into(), "ANN".into()));
    api::call(&mut h, "QZLSADFS", &adfs("BOB", "/home/bob", 2, -1)).expect("owner shares");
    let e = api::call(&mut h, "QZLSADFS", &adfs("ANN", "/home/ann", 2, -1)).expect_err("not owner");
    assert_eq!((e.id, reason(&e, 10)), ("CPFB684", 4));
    let e = api::call(&mut h, "QZLSADFS", &adfs("NONE", "/nowhere", 2, -1)).expect_err("missing");
    assert_eq!((e.id, reason(&e, 10)), ("CPFB684", 3));
    let e = api::call(
        &mut h,
        "QZLSCHSN",
        &[fixed("X", 15), fixed("Y", 15), fixed("", 50)],
    )
    .expect_err("no authority");
    assert_eq!((e.id, reason(&e, 10)), ("CPFB684", 1));
    let e = api::call(&mut h, "QZLSCHSG", &[fixed("", 10)]).expect_err("no authority");
    assert_eq!((e.id, reason(&e, 10)), ("CPFB684", 2));
}

#[test]
fn print_shares_and_publishing() {
    let mut h = MemHost::new("QS400");
    let mut q = fixed("PRT01", 10);
    q.extend(fixed("QUSRSYS", 10));
    let mut p = vec![
        fixed("LASER", 12),
        q,
        fixed("Laser", 50),
        b4(1),
        fixed("HP LaserJet 4", 50),
    ];
    api::call(&mut h, "QZLSADPS", &p).expect("adds");
    let c = h.config.clone().expect("saved");
    let ps = c
        .share("LASER")
        .and_then(|s| s.as_print())
        .expect("print")
        .clone();
    assert_eq!(
        (ps.outq.name.as_str(), ps.outq.library.as_str()),
        ("PRT01", "QUSRSYS")
    );
    assert_eq!(ps.spool_type, SpoolType::UserAscii);
    p[0] = fixed("LASER2", 12);
    p.extend([fixed("", 20), vec![0xF1]]);
    let e = api::call(&mut h, "QZLSADPS", &p).expect_err("cannot publish");
    assert_eq!((e.id, reason(&e, 10)), ("CPFB682", 6));
    let c = h.config.clone().expect("saved");
    assert_eq!(
        c.share("LASER2")
            .and_then(|s| s.as_print())
            .map(|p| p.publish),
        Some(false)
    );
    p[3] = b4(9);
    p[0] = fixed("LASER3", 12);
    let e = api::call(&mut h, "QZLSADPS", &p).expect_err("bad type");
    assert_eq!((e.id, reason(&e, 22)), ("CPFB68A", 5));
    let e = api::call(&mut h, "QZLSCHFS", &adfs("LASER", "/x", 1, 1)).expect_err("print share");
    assert_eq!((e.id, reason(&e, 22)), ("CPFB68A", 8));
    let ch = vec![
        fixed("QIBM", 12),
        fixed("X         QGPL", 20),
        fixed("", 50),
        b4(4),
        fixed("", 50),
    ];
    let e = api::call(&mut h, "QZLSCHPS", &ch).expect_err("file share");
    assert_eq!((e.id, reason(&e, 22)), ("CPFB68A", 8));
}

#[test]
fn server_name_and_guest_are_pending() {
    let mut h = MemHost::new("QS400");
    api::call(
        &mut h,
        "QZLSCHSN",
        &[
            fixed("newname", 15),
            fixed("WORKGROUP", 15),
            fixed("Files", 50),
            vec![0xF1],
        ],
    )
    .expect("changes");
    let c = h.config.clone().expect("saved");
    assert_eq!(c.attrs.pending.server_name, "NEWNAME");
    assert!(c.attrs.pending.system_name_access);
    assert_eq!(c.attrs.current.server_name, "QS400");
    let e = api::call(
        &mut h,
        "QZLSCHSN",
        &[fixed("BAD-NAME", 15), fixed("W", 15), fixed("", 50)],
    )
    .expect_err("bad");
    assert_eq!((e.id, reason(&e, 10)), ("CPFB68B", 1));
    assert_eq!(text_at(&e.data, 14, 15), "BAD-NAME");

    let e = api::call(&mut h, "QZLSCHSG", &[fixed("NOBODY", 10)]).expect_err("missing");
    assert_eq!(
        (e.id, text_at(&e.data, 10, 10), text_at(&e.data, 20, 10)),
        ("CPFB687", "*USE".into(), "NOBODY".into())
    );
    h.profiles.push(("POWER".into(), vec!["*ALLOBJ".into()]));
    h.profiles.push(("QGUEST".into(), Vec::new()));
    let e = api::call(&mut h, "QZLSCHSG", &[fixed("POWER", 10)]).expect_err("special");
    assert_eq!(
        (e.id, text_at(&e.data, 20, 10)),
        ("CPFB68C", "*ALLOBJ".into())
    );
    api::call(&mut h, "QZLSCHSG", &[fixed("qguest", 10)]).expect("guest");
    assert_eq!(
        h.config.as_ref().expect("saved").attrs.pending.guest,
        "QGUEST"
    );
}

fn zlss0100() -> Rec {
    let mut r = Rec::zeros(310);
    r.bin4(0, 0);
    r.bin4(4, 1200);
    r.bin4(8, 60);
    r.bin4(12, 720_000);
    r.blanks(16, 254);
    r.char(270, 1, "0");
    r.char(280, 1, "0");
    r.bin4(284, 2);
    r.bin4(288, -1);
    r.blanks(300, 10);
    r.bin4(296, 1);
    r
}

fn chsi(h: &mut MemHost<impl ServerControl>, r: &Rec, fmt: &str) -> Result<Vec<u8>, ApiError> {
    api::call(
        h,
        "QZLSCHSI",
        &[
            r.buf.clone(),
            b4(i32::try_from(r.buf.len()).expect("short")),
            fixed(fmt, 8),
        ],
    )
}

#[test]
fn zlss0100_sets_pending_and_refuses_what_qs400_lacks() {
    let mut h = MemHost::new("QS400");
    chsi(&mut h, &zlss0100(), "ZLSS0100").expect("changes");
    let c = h.config.clone().expect("saved");
    assert_eq!(c.attrs.pending.idle_timeout, 1200);
    assert!(c.attrs.pending.signing_required() && c.attrs.pending.encryption_required());
    assert_eq!(c.attrs.current.idle_timeout, 6000);
    let cases: Vec<(Box<dyn Fn(&mut Rec)>, &str, i32)> = vec![
        (Box::new(|r| r.char(16, 15, "10.1.1.1")), "CPFB686", 6),
        (Box::new(|r| r.char(31, 15, "10.1.1.2")), "CPFB686", 7),
        (Box::new(|r| r.bin4(12, 720_001)), "CPFB686", 8),
        (Box::new(|r| r.bin4(8, 0)), "CPFB686", 9),
        (Box::new(|r| r.bin4(4, 0)), "CPFB686", 10),
        (Box::new(|r| r.char(270, 1, "1")), "CPFB686", 6),
        (Box::new(|r| r.char(270, 1, "X")), "CPFB686", 12),
        (Box::new(|r| r.bin4(276, 1)), "CPFB686", 13),
        (Box::new(|r| r.char(280, 1, "2")), "CPFB686", 15),
        (Box::new(|r| r.char(280, 1, "7")), "CPFB686", 14),
        (Box::new(|r| r.bin4(284, 3)), "CPFB686", 16),
        (Box::new(|r| r.bin4(288, 50)), "CPFB686", 17),
        (Box::new(|r| r.bin4(292, 1)), "CPFB686", 18),
    ];
    for (i, (f, id, rc)) in cases.iter().enumerate() {
        let mut r = zlss0100();
        f(&mut r);
        let e = chsi(&mut h, &r, "ZLSS0100").expect_err("refused");
        assert_eq!((e.id, reason(&e, 10)), (*id, *rc), "case {i}");
    }
    let mut r = zlss0100();
    r.char(300, 10, "NOAUTL");
    assert_eq!(
        chsi(&mut h, &r, "ZLSS0100").expect_err("no list").id,
        "CPF2283"
    );
    let short = Rec {
        buf: zlss0100().buf[..200].to_vec(),
    };
    let e = chsi(&mut h, &short, "ZLSS0100").expect_err("short");
    assert_eq!((e.id, reason(&e, 10), reason(&e, 14)), ("CPFB68D", 2, 200));
    let e = chsi(&mut h, &zlss0100(), "ZLSS9999").expect_err("format");
    assert_eq!((e.id, reason(&e, 10)), ("CPFB690", 1));
}

#[test]
fn zlss0200_enables_users() {
    let mut h = MemHost::new("QS400");
    h.profiles.push(("BOB".into(), Vec::new()));
    h.profiles.push(("ANN".into(), Vec::new()));
    h.disabled = vec!["BOB".into(), "ANN".into()];
    h.not_manageable.push("ANN".into());
    let mut r = Rec::zeros(0);
    r.bin4(0, 3);
    r.char(4, 10, "BOB");
    r.char(14, 10, "ANN");
    r.char(24, 10, "NOBODY");
    let e = chsi(&mut h, &r, "ZLSS0200").expect_err("two failed");
    assert_eq!((e.id, reason(&e, 0)), ("CPFB694", 2));
    assert_eq!(h.disabled, vec!["ANN".to_string()]);
    let parms = vec![
        65535i32.to_be_bytes().to_vec(),
        fixed("ZLSL0900", 8),
        fixed("", 15),
    ];
    let got = api::call(&mut h, "QZLSOLST", &parms).expect("lists");
    assert_eq!((bin4(&got, 0), text_at(&got, 64, 10)), (1, "ANN".into()));
}

/// A server that runs, for start, end and sessions.
#[derive(Debug, Default)]
struct Running {
    up: bool,
    ended: Vec<(String, u64)>,
}

impl ServerControl for Running {
    fn active(&mut self) -> bool {
        self.up
    }
    fn start(&mut self, _reset: bool) -> Result<(), ApiError> {
        self.up = true;
        Ok(())
    }
    fn end(&mut self) -> Result<(), ApiError> {
        self.up = false;
        Ok(())
    }
    fn end_session(&mut self, job: &str, id: u64) -> Result<(), ApiError> {
        self.ended.push((job.into(), id));
        Ok(())
    }
}

#[test]
fn start_needs_a_server_and_reset_applies_pending() {
    let mut h = MemHost::new("QS400");
    api::call(
        &mut h,
        "QZLSCHSN",
        &[fixed("NEW", 15), fixed("WG", 15), fixed("", 50)],
    )
    .expect("pending");
    let e = api::call(&mut h, "QZLSSTRS", &[vec![0xF1]]).expect_err("no server job");
    assert_eq!(e.id, "CPFB680");
    assert_eq!(
        h.config.as_ref().expect("saved").attrs.current.server_name,
        "QS400"
    );
    let e = api::call(&mut h, "QZLSENDS", &[]).expect_err("not active");
    assert_eq!(e.id, "CPFB680");
    let e = api::call(&mut h, "QZLSSTRS", &[vec![0xF5]]).expect_err("value");
    assert_eq!((e.id, reason(&e, 10)), ("CPFB685", 5));

    let mut h = MemHost::with_server("QS400", Running::default());
    api::call(
        &mut h,
        "QZLSCHSN",
        &[fixed("NEW", 15), fixed("WG", 15), fixed("", 50)],
    )
    .expect("pending");
    api::call(&mut h, "QZLSSTRS", &[vec![0xF1]]).expect("starts");
    assert_eq!(
        h.config.as_ref().expect("saved").attrs.current.server_name,
        "NEW"
    );
    let e = api::call(&mut h, "QZLSSTRS", &[vec![0xF0]]).expect_err("active");
    assert_eq!((e.id, reason(&e, 10)), ("CPFB685", 2));
    api::call(&mut h, "QZLSENDS", &[]).expect("ends");
}

fn session(ws: &str, user: &str) -> Session {
    Session {
        id: 0,
        workstation: ws.into(),
        expanded: format!("{ws}.example.com"),
        user: user.into(),
        job: format!("000001/QUSER/QZLSFILE {ws}"),
        guest: false,
        since: 1_762_675_000,
        last_active: 1_762_675_100,
        connections: Vec::new(),
        ending: false,
    }
}

#[test]
fn sessions_list_and_end() {
    let mut h = MemHost::with_server(
        "QS400",
        Running {
            up: true,
            ..Running::default()
        },
    );
    let a = h.table.register(session("PC1", "BOB"));
    let b = h.table.register(session("A-VERY-LONG-PC-NAME", "ANN"));
    let c = h
        .table
        .connect(a, "QIBM", 0, 1_762_675_100)
        .expect("connects");
    h.table.set_opens(a, c, 2, 1_762_675_150).expect("opens");
    h.table
        .connect(b, "QIBM", 0, 1_762_675_100)
        .expect("connects");
    let ls = |h: &mut MemHost<Running>, fmt: &str, q: &str| {
        api::call(
            h,
            "QZLSOLST",
            &[65535i32.to_be_bytes().to_vec(), fixed(fmt, 8), fixed(q, 15)],
        )
        .expect("lists")
    };
    let l = ls(&mut h, "ZLSL0300", "*ALL");
    assert_eq!(bin4(&l, 0), 2);
    let r = &l[64..];
    assert_eq!(text_at(r, 0, 15), "PC1");
    assert_eq!(
        (bin4(r, 28), bin4(r, 32), bin4(r, 40), bin4(r, 44)),
        (1, 2, 200, 50)
    );
    assert_eq!(bin8(r, 56), i64::try_from(a).expect("small"));
    assert_eq!(text_at(&r[192..], 0, 15), "*EXPANDED");
    assert_eq!(bin4(&r[192..], 64), 1);
    let l = ls(&mut h, "ZLSL0300", "PC1");
    assert_eq!(bin4(&l, 0), 1);
    let l = ls(&mut h, "ZLSL0600", "PC1");
    assert_eq!(
        (bin4(&l, 0), bin4(&l, 64 + 8), text_at(&l, 64 + 30, 12)),
        (1, 2, "QIBM".into())
    );
    let l = ls(&mut h, "ZLSL0700", "QIBM");
    assert_eq!(bin4(&l, 0), 2);
    let l = ls(&mut h, "ZLSL0100", "QIBM");
    assert_eq!(bin4(&l, 64 + 28), 2);
    let l = ls(&mut h, "ZLSL0400", "*ALL");
    assert_eq!(bin4(&l, 64 + 8), 2);

    let e = api::call(&mut h, "QZLSENSS", &[fixed("NOPC", 15)]).expect_err("none");
    assert_eq!((e.id, reason(&e, 25)), ("CPFB68E", 1));
    let e = api::call(
        &mut h,
        "QZLSENSS",
        &[fixed("PC1", 15), 7i64.to_be_bytes().to_vec()],
    )
    .expect_err("id");
    assert_eq!((e.id, reason(&e, 25)), ("CPFB68E", 4));
    let e = api::call(
        &mut h,
        "QZLSENSS",
        &[fixed("*SESSID", 15), 99i64.to_be_bytes().to_vec()],
    )
    .expect_err("no id");
    assert_eq!((e.id, reason(&e, 25)), ("CPFB68E", 5));
    api::call(&mut h, "QZLSENSS", &[fixed("pc1", 15)]).expect("ends");
    let id = i64::try_from(b).expect("small").to_be_bytes().to_vec();
    api::call(&mut h, "QZLSENSS", &[fixed("*SESSID", 15), id]).expect("ends");
    assert_eq!(h.server.ended.len(), 2);
    assert!(h.table.is_ending(a) && h.table.is_ending(b));

    h.server.up = false;
    let e = api::call(&mut h, "QZLSENSS", &[fixed("PC1", 15)]).expect_err("not active");
    assert_eq!((e.id, reason(&e, 25)), ("CPFB68E", 2));
    let e = api::call(
        &mut h,
        "QZLSOLST",
        &[
            65535i32.to_be_bytes().to_vec(),
            fixed("ZLSL0300", 8),
            fixed("*SESSID", 15),
        ],
    )
    .expect_err("needs an id");
    assert_eq!((e.id, reason(&e, 10)), ("CPFB688", 2));
}

#[test]
fn the_files_carry_what_the_apis_set() {
    let mut h = MemHost::new("QS400");
    h.paths.push(("/home".into(), "QSYS".into()));
    api::call(&mut h, "QZLSADFS", &adfs("HOME", "/home", 2, 5)).expect("adds");
    let c = h.config.clone().expect("saved");
    let (a, s) = netsvrcfg::file::write(&c).expect("writes");
    let back = netsvrcfg::file::read(&a, &s, "OTHER").expect("reads");
    assert_eq!(back, c);
    assert!(matches!(
        back.share("HOME").map(|s| &s.kind),
        Some(Kind::File(_))
    ));
}
