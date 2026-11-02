// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every type encodes and decodes to itself; envelopes over 1 MiB are
//! refused; split replies join again.

use std::fmt::Debug;

use ipc::{Wire, decode, encode};
use qap_proto::envelope::{MORE, Reassembly};
use qap_proto::ops::connection::{Hello, HelloReply, PingReply, SignOnMethod};
use qap_proto::ops::signon::{ChangePassword, Resume, SignOn, SignOnReply};
use qap_proto::ops::system::{Product, SystemInfo};
use qap_proto::{Envelope, Kind, MAX_FRAME, Op, ProtoError, Secret, ServerMessage, Service, Status, Token};

fn same<T: Wire + PartialEq + Debug>(v: &T) {
    assert_eq!(&decode::<T>(&encode(v)).unwrap(), v);
}

fn msg() -> ServerMessage {
    ServerMessage {
        id: "CPF1107".into(),
        severity: 40,
        data: vec![0xC1, 0, 0xFF],
        text: "Password not correct for user profile.".into(),
        help: "Cause . . . : ...".into(),
    }
}

fn signon_reply() -> SignOnReply {
    SignOnReply {
        token: Token([0xA5; 32]),
        idle_timeout_secs: 3600,
        profile: "QSECOFR".into(),
        text: "Security officer".into(),
        ccsid: 37,
        date_format: "*MDY".into(),
        date_separator: "/".into(),
        time_separator: ":".into(),
        home_directory: "/home/QSECOFR".into(),
        special_authorities: vec!["*ALLOBJ".into(), "*SECADM".into()],
    }
}

#[test]
fn enums() {
    for k in [Kind::Request, Kind::Reply, Kind::Event, Kind::Cancel] {
        same(&k);
    }
    for (i, k) in [Kind::Request, Kind::Reply, Kind::Event, Kind::Cancel].iter().enumerate() {
        assert_eq!(encode(k), [u8::try_from(i).unwrap()]);
    }
    for (i, s) in Service::ALL.iter().enumerate() {
        same(s);
        assert_eq!(encode(s), [u8::try_from(i).unwrap()]);
    }
    for (i, s) in Status::ALL.iter().enumerate() {
        same(s);
        assert_eq!(encode(s), u16::try_from(i).unwrap().to_le_bytes());
    }
    assert!(decode::<Status>(&13u16.to_le_bytes()).is_err());
    assert!(decode::<Kind>(&[4]).is_err());
    assert!(decode::<Service>(&[3]).is_err());
    same(&SignOnMethod::Password);
    assert_eq!(encode(&SignOnMethod::Password), [0]);
}

#[test]
fn bodies() {
    same(&msg());
    same(&Hello {
        min_version: 1,
        max_version: 3,
        client_name: "QS/400 Access".into(),
        client_version: "1.0.0".into(),
        capabilities: vec!["x".into()],
    });
    same(&HelloReply {
        version: 1,
        system_name: "QSHQ400".into(),
        release: "V1R0M0".into(),
        services: Service::ALL.to_vec(),
        capabilities: Vec::new(),
        signon_methods: vec![SignOnMethod::Password],
        server_time: -1,
        time_zone: "QN0500EST".into(),
        system_ccsid: 37,
    });
    same(&PingReply { server_time: 1_793_000_000_000 });
    same(&SignOn {
        user: "QSECOFR".into(),
        password: Secret::from("é pass"),
    });
    same(&ChangePassword {
        user: "U".into(),
        old_password: Secret::from("old"),
        new_password: Secret::from("new"),
    });
    same(&Resume { token: Token([9; 32]) });
    same(&signon_reply());
    same(&Product {
        id: "QS00SS1".into(),
        option: "0000".into(),
        release: "V1R0M0".into(),
    });
    same(&SystemInfo {
        system_name: "QSHQ400".into(),
        release: "V1R0M0".into(),
        serial: "10ABCDE".into(),
        model: "Q01".into(),
        ipl_time: 1_793_000_000_000,
        products: vec![Product::default(), Product::default()],
    });
    // The token is exactly 32 bytes on the wire.
    assert_eq!(encode(&Resume { token: Token([1; 32]) }).len(), 32);
}

#[test]
fn envelopes() {
    let req = Envelope::request(7, Op::SignOn, &SignOn::default());
    assert_eq!((req.kind, req.service, req.op), (Kind::Request, Service::SignOn, 1));
    same(&req);
    let rep = Envelope::reply(&req, Status::SignOnFailed, Some(msg()), Vec::new());
    same(&rep);
    same(&Envelope::event(Service::Connection, 100, Status::ServerEnding, Some(msg())));
    same(&Envelope::cancel(7));
    for op in Op::ALL {
        let (s, c) = op.code();
        assert_eq!(Op::of(s, c), Some(op));
    }
    assert_eq!(Op::of(Service::System, 9), None);
    assert!(!Op::SignOff.before_signon() && !Op::SystemInfo.before_signon() && Op::Resume.before_signon());
}

#[test]
fn frames_over_one_mib_are_refused() {
    let req = Envelope::request(2, Op::Ping, &());
    let mut big = Envelope::reply(&req, Status::Ok, None, vec![0; MAX_FRAME]);
    assert!(matches!(big.to_frame(), Err(ProtoError::TooLong { .. })));
    let mut out = Vec::new();
    assert!(big.write_to(&mut out).is_err());
    assert!(out.is_empty());
    // A frame that just fits goes through.
    let over = encode(&big).len() - MAX_FRAME;
    big.body.truncate(big.body.len() - over);
    let mut out = Vec::new();
    big.write_to(&mut out).unwrap();
    assert_eq!(Envelope::read_from(&mut out.as_slice()).unwrap(), big);
    // A length prefix over the limit is refused before the bytes come.
    let mut hostile = u32::try_from(MAX_FRAME + 1).unwrap().to_le_bytes().to_vec();
    hostile.extend_from_slice(&[0; 16]);
    assert!(matches!(
        Envelope::read_from(&mut hostile.as_slice()),
        Err(ProtoError::TooLong { len }) if len == MAX_FRAME + 1
    ));
    assert!(matches!(Envelope::from_frame(&vec![0; MAX_FRAME + 1]), Err(ProtoError::TooLong { .. })));
}

#[test]
fn split_replies_join() {
    let req = Envelope::request(5, Op::SystemInfo, &());
    let body: Vec<u8> = (0..3 * MAX_FRAME + 17).map(|i| u8::try_from(i % 251).unwrap()).collect();
    let frames = Envelope::replies(&req, Status::Ok, Some(msg()), body.clone()).unwrap();
    assert_eq!(frames.len(), 4);
    let mut r = Reassembly::new();
    let mut whole = None;
    for (i, f) in frames.iter().enumerate() {
        assert!(f.to_frame().is_ok(), "frame {i} too long");
        assert_eq!(f.flags & MORE != 0, i + 1 < frames.len());
        assert_eq!(f.message.is_some(), i + 1 == frames.len());
        assert_eq!(f.status, Status::Ok);
        let mut wire = Vec::new();
        f.write_to(&mut wire).unwrap();
        whole = r.push(Envelope::read_from(&mut wire.as_slice()).unwrap());
        assert_eq!(whole.is_some(), i + 1 == frames.len());
    }
    let whole = whole.unwrap();
    assert_eq!(whole.body, body);
    assert_eq!(whole.message, Some(msg()));
    assert_eq!(r.pending(), 0);
    // Small replies stay one frame.
    assert_eq!(Envelope::replies(&req, Status::Ok, None, vec![1, 2]).unwrap().len(), 1);
}
