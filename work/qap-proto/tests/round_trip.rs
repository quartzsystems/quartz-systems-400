// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every type encodes and decodes to itself; envelopes over 1 MiB are
//! refused; split replies join again.

use std::fmt::Debug;

use ipc::{Wire, decode, encode};
use qap_proto::envelope::{MORE, Reassembly};
use qap_proto::ops::command::{
    CheckReply, CommandDefinition, CommandReply, CommandText, DescribeCommand, ParmKind, ParmNode,
    Range, Role, Show,
};
use qap_proto::ops::connection::{Hello, HelloReply, PingReply, SignOnMethod};
use qap_proto::ops::signon::{
    ChangePassword, PasswordStatus, Resume, SignOn, SignOnReply, TelnetTicketReply,
};
use qap_proto::ops::spool::{
    DeviceType, HoldSpooledFile, ListOutputQueues, ListSpooledFiles, MoveSpooledFile, OutputQueue,
    OutputQueues, ReadFormat, ReadSpooledFile, SpoolStatus, SpooledData, SpooledFile,
    SpooledFileId, SpooledFileRef, SpooledFiles,
};
use qap_proto::ops::system::{Product, SystemInfo};
use qap_proto::types::{JobId, ObjectName};
use qap_proto::{
    Envelope, Kind, MAX_FRAME, Op, ProtoError, Secret, ServerMessage, Service, Status, Token,
};

fn same<T: Wire + PartialEq + Debug>(v: &T) {
    assert_eq!(&decode::<T>(&encode(v)).unwrap(), v);
}

fn msg() -> ServerMessage {
    ServerMessage {
        id: "CPF1107".into(),
        severity: 40,
        data: vec![0xC1, 0, 0xFF],
        text: "The password is not correct for the user profile.".into(),
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
    for (i, k) in [Kind::Request, Kind::Reply, Kind::Event, Kind::Cancel]
        .iter()
        .enumerate()
    {
        assert_eq!(encode(k), [u8::try_from(i).unwrap()]);
    }
    for (i, s) in Service::ALL.iter().enumerate() {
        same(s);
        assert_eq!(encode(s), [u8::try_from(i).unwrap()]);
    }
    // 0 to 15: Exists 13 came with version 5.
    for (s, n) in Status::ALL.iter().zip(0u16..) {
        same(s);
        assert_eq!(encode(s), n.to_le_bytes());
    }
    assert!(decode::<Status>(&16u16.to_le_bytes()).is_err());
    assert!(decode::<Kind>(&[4]).is_err());
    assert!(decode::<Service>(&[9]).is_err());
    for (i, s) in SpoolStatus::ALL.iter().enumerate() {
        same(s);
        assert_eq!(encode(s), [u8::try_from(i).unwrap()]);
    }
    for (i, d) in [
        DeviceType::Scs,
        DeviceType::Afp,
        DeviceType::Ascii,
        DeviceType::Line,
    ]
    .iter()
    .enumerate()
    {
        same(d);
        assert_eq!(encode(d), [u8::try_from(i).unwrap()]);
    }
    for (i, f) in [ReadFormat::Scs, ReadFormat::Text, ReadFormat::Pdf]
        .iter()
        .enumerate()
    {
        same(f);
        assert_eq!(encode(f), [u8::try_from(i).unwrap()]);
    }
    assert!(decode::<SpoolStatus>(&[9]).is_err());
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
    same(&PingReply {
        server_time: 1_793_000_000_000,
    });
    same(&SignOn {
        user: "QSECOFR".into(),
        password: Secret::from("é pass"),
    });
    same(&ChangePassword {
        user: "U".into(),
        old_password: Secret::from("old"),
        new_password: Secret::from("new"),
    });
    same(&Resume {
        token: Token([9; 32]),
    });
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
    assert_eq!(
        encode(&Resume {
            token: Token([1; 32])
        })
        .len(),
        32
    );
}

#[test]
fn spool_bodies() {
    let id = SpooledFileId {
        job: JobId {
            name: "QPADEV0001".into(),
            user: "BOB".into(),
            number: "123456".into(),
        },
        file: "QSYSPRT".into(),
        number: 3,
    };
    let q = ObjectName {
        library: "QGPL".into(),
        name: "QPRINT".into(),
    };
    same(&ListSpooledFiles {
        user: "*ALL".into(),
        outq: Some(q.clone()),
        status: Some(SpoolStatus::Held),
        form_type: Some("*STD".into()),
        user_data: None,
        start: 10,
        max: 20,
    });
    same(&SpooledFiles {
        total: 61,
        files: vec![SpooledFile {
            id: id.clone(),
            status: SpoolStatus::Printing,
            outq: q.clone(),
            user: "BOB".into(),
            user_data: "PAYROLL".into(),
            form_type: "*STD".into(),
            device_type: DeviceType::Afp,
            pages: 30,
            copies: 2,
            size: 123_456,
            created: 1_793_000_000_000,
        }],
    });
    same(&ReadSpooledFile {
        id: id.clone(),
        format: ReadFormat::Pdf,
        first_page: 2,
        last_page: 0,
    });
    same(&SpooledData {
        format: ReadFormat::Text,
        pages: 30,
        data: b"line\x0c".to_vec(),
    });
    same(&HoldSpooledFile {
        id: id.clone(),
        immediately: true,
    });
    same(&SpooledFileRef { id: id.clone() });
    same(&MoveSpooledFile {
        id,
        outq: q.clone(),
        first: true,
    });
    same(&ListOutputQueues {
        library: "*LIBL".into(),
        start: 0,
        max: 0,
    });
    same(&OutputQueues {
        total: 1,
        queues: vec![OutputQueue {
            outq: q,
            text: "Default printer queue".into(),
            files: 60,
            writer: "PRT01".into(),
            held: false,
        }],
    });
    // A flag is 0 or 1; any other value does not decode.
    let mut b = encode(&HoldSpooledFile::default());
    *b.last_mut().unwrap() = 2;
    assert!(decode::<HoldSpooledFile>(&b).is_err());
}

#[test]
fn envelopes() {
    let req = Envelope::request(7, Op::SignOn, &SignOn::default());
    assert_eq!(
        (req.kind, req.service, req.op),
        (Kind::Request, Service::SignOn, 1)
    );
    same(&req);
    let rep = Envelope::reply(&req, Status::SignOnFailed, Some(msg()), Vec::new());
    same(&rep);
    same(&Envelope::event(
        Service::Connection,
        100,
        Status::ServerEnding,
        Some(msg()),
    ));
    same(&Envelope::cancel(7));
    for op in Op::ALL {
        let (s, c) = op.code();
        assert_eq!(Op::of(s, c), Some(op));
    }
    assert_eq!(Op::of(Service::System, 9), None);
    assert!(
        !Op::SignOff.before_signon()
            && !Op::SystemInfo.before_signon()
            && Op::Resume.before_signon()
    );
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
    assert!(matches!(
        Envelope::from_frame(&vec![0; MAX_FRAME + 1]),
        Err(ProtoError::TooLong { .. })
    ));
}

#[test]
fn split_replies_join() {
    let req = Envelope::request(5, Op::SystemInfo, &());
    let body: Vec<u8> = (0..3 * MAX_FRAME + 17)
        .map(|i| u8::try_from(i % 251).unwrap())
        .collect();
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
    assert_eq!(
        Envelope::replies(&req, Status::Ok, None, vec![1, 2])
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn version_2_bodies() {
    same(&CommandText {
        command: "DSPLIB QGPL".into(),
    });
    same(&CommandReply {
        job: JobId {
            name: "QAPCNN".into(),
            user: "QUSER".into(),
            number: "000123".into(),
        },
        messages: vec![msg(), msg()],
    });
    same(&CheckReply {
        messages: vec![msg()],
    });
    same(&DescribeCommand {
        command: ObjectName {
            library: "*LIBL".into(),
            name: "CRTLIB".into(),
        },
    });
    let node = ParmNode {
        index: 1,
        parent: Some(0),
        role: Role::Qualifier,
        keyword: String::new(),
        prompt: "Library".into(),
        help: String::new(),
        kind: ParmKind::Name,
        length: 10,
        decimals: 0,
        required: true,
        min_values: 1,
        max_values: 1,
        default: Some("*LIBL".into()),
        special_values: vec!["*LIBL".into(), "*CURLIB".into()],
        values: Vec::new(),
        range: Some(Range {
            low: "1".into(),
            high: "9".into(),
        }),
        show: Show::Additional,
    };
    same(&node);
    same(&CommandDefinition {
        name: ObjectName {
            library: "QSYS".into(),
            name: "CRTLIB".into(),
        },
        text: "Create Library".into(),
        help: String::new(),
        nodes: vec![ParmNode::default(), node],
    });
    for r in [Role::Parameter, Role::Element, Role::Qualifier] {
        same(&r);
    }
    for (i, k) in [
        ParmKind::Name,
        ParmKind::Character,
        ParmKind::Decimal,
        ParmKind::Integer,
        ParmKind::Logical,
        ParmKind::Date,
        ParmKind::Time,
        ParmKind::Generic,
        ParmKind::Path,
        ParmKind::Command,
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(encode(k), [u8::try_from(i).unwrap()]);
    }
    for s in [Show::Always, Show::Additional, Show::Never] {
        same(&s);
    }
    same(&TelnetTicketReply {
        ticket: Token([3; 32]),
        expires_secs: 60,
    });
    assert_eq!(encode(&TelnetTicketReply::default()).len(), 36);
    same(&PasswordStatus {
        expires: Some(1_793_000_000_000),
        warning_days: 7,
    });
    same(&PasswordStatus::default());
}

#[test]
fn values_by_version() {
    assert_eq!(
        Service::of_version(1),
        [Service::Connection, Service::SignOn, Service::System]
    );
    assert_eq!(Service::of_version(2), Service::ALL[..4].to_vec());
    assert_eq!(Service::of_version(3), Service::ALL[..5].to_vec());
    assert_eq!(Service::of_version(4), Service::ALL[..6].to_vec());
    assert_eq!(Service::of_version(5), Service::ALL[..7].to_vec());
    assert_eq!(Service::of_version(6), Service::ALL[..8].to_vec());
    assert_eq!(Service::of_version(7), Service::ALL.to_vec());
    assert_eq!(encode(&Service::Command), [3]);
    assert_eq!(encode(&Service::Spool), [4]);
    for code in 1..=7 {
        let op = Op::of(Service::Spool, code).unwrap();
        assert_eq!(op.since(), 3, "{op:?}");
        assert!(!op.before_signon());
    }
    // 8 and 9, the watches, are version 6's.
    assert_eq!(Op::of(Service::Spool, 8).map(Op::since), Some(6));
    assert_eq!(Op::of(Service::Spool, 10), None);
    // InUse is version 3's: older peers get Error.
    assert_eq!(Status::InUse.for_version(1), Status::Error);
    assert_eq!(Status::InUse.for_version(2), Status::Error);
    assert_eq!(Status::InUse.for_version(3), Status::InUse);
    for op in [
        Op::TelnetTicket,
        Op::PasswordStatus,
        Op::RunCommand,
        Op::CheckCommand,
        Op::DescribeCommand,
    ] {
        assert_eq!(op.since(), 2, "{op:?}");
        assert!(!op.before_signon());
    }
    assert_eq!(Op::of(Service::SignOn, 5), Some(Op::TelnetTicket));
    assert_eq!(Op::of(Service::Command, 3), Some(Op::DescribeCommand));
    // Version 2 adds no status: each of 1 and 2 is sent to a version-1
    // peer as is.
    for s in Status::ALL.into_iter().filter(|s| s.since() <= 2) {
        assert_eq!(s.for_version(1), s);
    }
}
