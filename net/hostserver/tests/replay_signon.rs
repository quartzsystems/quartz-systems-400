// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The port mapper, sign-on and central servers against their traces.
//! The primary oracle's sign-on and central servers close every
//! connection without a reply, so those traces only show where QS/400
//! answers and the oracle did not; the sign-on flows themselves are
//! tested at every password level.

mod common;

use common::mem::Host;
use common::{Masks, TraceAuth, accept, replay, trace};
use hostserver::signon::Signon;
use hostserver::{Handler, NoExits, Outcome};
use hostsvr::common::{auth, rc};
use hostsvr::crypt::{Level, Seeds, change_password, substitute};
use hostserver::central::Central;
use hostsvr::frame::{Frame, Param};
use hostsvr::trace::Dir;

#[test]
fn svrmap() {
    let t = trace("svrmap", "svrmap");
    let mut n = 0;
    for c in &t.connections {
        let ch: Vec<_> = c.chunks().collect();
        let (req, rep) = (&ch[0], &ch[1]);
        assert_eq!(req.dir, Dir::Request);
        assert_eq!(hostserver::svrmap::answer(&req.bytes), rep.bytes, "connection {}", c.id);
        n += 1;
    }
    assert_eq!(n, 7);
}

#[test]
fn oracle_closed_these() {
    for (server, scenarios) in [("signon", &["signon", "pwdlevel", "svrmap"][..]), ("central", &["central", "pwdlevel", "svrmap", "dtaq", "ifs"][..])] {
        for s in scenarios {
            let t = trace(server, s);
            let auth = TraceAuth::of(&t);
            let diffs = replay(&t, &Masks::default(), &mut || {
                if server == "signon" {
                    Box::new(Signon::new(Host::new(0, "QSECOFR", "SECRET"), NoExits)) as Box<dyn Handler>
                } else {
                    Box::new(Central::new(auth.clone(), NoExits)) as Box<dyn Handler>
                }
            });
            accept(&format!("{server}-{s}"), &diffs);
        }
    }
}

fn user_param(u: &str) -> Param {
    Param::new(0x1104, hostserver::text::ebcdic_fixed(u, 10))
}

fn request(id: u16, template: Vec<u8>, params: Vec<Param>) -> Vec<u8> {
    Frame::new(0xE009, id, template, params).encode()
}

fn rc_of(o: &Outcome) -> u32 {
    let f = Frame::decode(&o.replies[0]).unwrap();
    u32::from_be_bytes(f.template[..4].try_into().unwrap())
}

fn exchange(s: &mut Signon<Host, NoExits>, client: [u8; 8]) -> (Seeds, u8) {
    let o = s.handle(&request(
        0x7003,
        Vec::new(),
        vec![Param::u32(0x1101, 1), Param::u16(0x1102, 10), Param::new(0x1103, client)],
    ));
    let f = Frame::decode(&o.replies[0]).unwrap();
    let a = hostsvr::signon::Attributes::from_params(&f.params).unwrap();
    (
        Seeds {
            client,
            server: a.seed.unwrap(),
        },
        a.password_level.unwrap(),
    )
}

/// Sign-on information, tokens and change password at `QPWDLVL` 0 to 4.
#[test]
fn every_password_level() {
    for level in 0..=4u8 {
        let mut s = Signon::new(Host::new(level, "QSECOFR", "Secret12"), NoExits);
        let (seeds, l) = exchange(&mut s, [1, 1, 2, 3, 5, 8, 13, 21]);
        assert_eq!(l, level);
        let lv = Level::from_qpwdlvl(level);
        let password = if lv == Level::Des { "SECRET12" } else { "Secret12" };
        let sub = substitute(lv, "QSECOFR", password, &seeds).unwrap();
        assert_eq!(sub.len(), lv.substitute_len());
        let o = s.handle(&request(0x7004, vec![lv.auth_scheme()], vec![Param::u32(0x1113, 13488), Param::new(0x1105, sub.clone()), user_param("QSECOFR"), Param::byte(0x1128, 1)]));
        assert_eq!(rc_of(&o), rc::OK, "level {level}");
        let info = hostsvr::signon::SignonInfo::from_params(&Frame::decode(&o.replies[0]).unwrap().params).unwrap();
        assert_eq!(info.current.unwrap().year, 2026);
        assert_eq!(info.server_ccsid, Some(37));
        // A wrong password.
        let bad = substitute(lv, "QSECOFR", "WRONG", &seeds).unwrap();
        let o = s.handle(&request(0x7004, vec![lv.auth_scheme()], vec![Param::new(0x1105, bad), user_param("QSECOFR")]));
        assert_eq!(rc_of(&o), rc::PASSWORD_INCORRECT);
        // A profile token, then a start with it.
        let o = s.handle(&request(0x7007, vec![lv.auth_scheme(), 1], vec![Param::new(0x1105, sub.clone()), user_param("QSECOFR"), Param::byte(0x1116, 0xF1), Param::u32(0x1117, 3600)]));
        assert_eq!(rc_of(&o), rc::OK);
        let tok = Frame::decode(&o.replies[0]).unwrap();
        assert_eq!(&o.replies[0][30..62], &[0x5A; 32]);
        assert_eq!(tok.param(0x1115).unwrap().data.len(), 32);
        // Change the password, then sign on with the new one.
        let c = change_password(lv, "QSECOFR", password, "Newpass34", &seeds).unwrap();
        let mut params = vec![user_param("QSECOFR"), Param::new(0x1105, c.substitute), Param::new(0x110C, c.old_protected), Param::new(0x110D, c.new_protected)];
        if lv != Level::Des {
            params.extend([Param::u32(0x111C, c.old_len), Param::u32(0x111D, c.new_len), Param::u32(0x111E, 13488)]);
        }
        let o = s.handle(&request(0x7005, vec![lv.auth_scheme()], params));
        assert_eq!(rc_of(&o), rc::OK, "change at level {level}");
        let newpw = if lv == Level::Des { "NEWPASS34" } else { "Newpass34" };
        let sub = substitute(lv, "QSECOFR", newpw, &seeds).unwrap();
        let o = s.handle(&request(0x7004, vec![lv.auth_scheme()], vec![Param::new(0x1105, sub), user_param("QSECOFR")]));
        assert_eq!(rc_of(&o), rc::OK, "new password at level {level}");
    }
    let _ = auth::DES;
}

/// Start server with each form of proof.
#[test]
fn start_server_proves_the_user() {
    use hostserver::session::Session;
    for level in [0u8, 2] {
        let mut h = Host::new(level, "BOB", "PW1");
        let mut s = Session::default();
        let client = [3u8; 8];
        let seed = Frame::new(0xE008, 0x7001, client.to_vec(), Vec::new()).encode();
        let o = s.common(&Frame::decode(&seed).unwrap(), &mut h).unwrap();
        let server: [u8; 8] = Frame::decode(&o.replies[0]).unwrap().template[4..12].try_into().unwrap();
        let lv = Level::from_qpwdlvl(level);
        let sub = substitute(lv, "BOB", "PW1", &Seeds { client, server }).unwrap();
        let start = Frame::new(0xE008, 0x7002, vec![lv.auth_scheme(), 1], vec![Param::new(0x1105, sub), user_param("BOB")]).encode();
        let o = s.common(&Frame::decode(&start).unwrap(), &mut h).unwrap();
        let f = Frame::decode(&o.replies[0]).unwrap();
        assert_eq!(&f.template, &[0, 0, 0, 0]);
        assert!(f.param(0x111F).is_some());
        assert_eq!(s.user.as_deref(), Some("BOB"));
    }
}
