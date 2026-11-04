// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 7 (draft): the `Manage` service's bodies and values, and
//! that versions 1 to 6 are what they were.

use std::fmt::Debug;

use ipc::{Wire, decode, encode};
use qap_proto::ops::connection::choose_version;
use qap_proto::ops::manage::{
    Asp, DiskUnit, DiskUnits, EndHow, EndSubsystem, GetSystemValues, HardwareResource, HardwareResources, IplAction,
    IplMode, IplSource, IplStatus, ListHardwareResources, ListPtfs, ListSubsystems, PowerDown, PreviousEnd, Protection,
    Ptf, PtfStatus, Ptfs, ResourceStatus, ResourceType, SetSystemValue, Subsystem, SubsystemRef, SubsystemStatus,
    Subsystems, SystemValue, SystemValues, UnitStatus, ValueKind,
};
use qap_proto::types::{JobId, ObjectName};
use qap_proto::{Envelope, Op, Service, Status, VERSION};

fn same<T: Wire + PartialEq + Debug>(v: &T) {
    assert_eq!(&decode::<T>(&encode(v)).unwrap(), v);
}

fn qbatch() -> ObjectName {
    ObjectName {
        library: "QSYS".into(),
        name: "QBATCH".into(),
    }
}

#[test]
fn version_7_values() {
    assert_eq!(VERSION, 7);
    assert_eq!(encode(&Service::Manage), [8]);
    assert_eq!(Service::Manage.since(), 7);
    let ops = [
        Op::ListSubsystems,
        Op::StartSubsystem,
        Op::EndSubsystem,
        Op::GetSystemValues,
        Op::SetSystemValue,
        Op::ListPtfs,
        Op::ListDiskUnits,
        Op::ListHardwareResources,
        Op::IplStatus,
        Op::PowerDown,
    ];
    for (code, op) in (1..=10).zip(ops) {
        assert_eq!(Op::of(Service::Manage, code), Some(op));
        assert_eq!(op.since(), 7, "{op:?}");
        assert!(!op.before_signon() && !op.in_lane(), "{op:?}");
    }
    assert_eq!(Op::of(Service::Manage, 11), None);
    for v in 1..=6 {
        assert!(!Service::of_version(v).contains(&Service::Manage));
        assert!(Op::ALL.iter().filter(|o| o.since() <= v).all(|o| o.code().0 != Service::Manage));
    }
    assert_eq!(Service::of_version(7), Service::ALL.to_vec());
    // Version 7 adds no status and no event.
    assert_eq!(Status::ALL.len(), 16);
    assert!(Status::ALL.iter().all(|s| s.since() <= 5));
    assert_eq!(qap_proto::ops::event::since(Service::Manage, 100), None);
    let req = Envelope::request(3, Op::IplStatus, &());
    let reply = Envelope::reply(&req, Status::Ok, None, Vec::new());
    assert!(!reply.defined_in(6) && reply.defined_in(7));
}

#[test]
fn versions_1_to_6_unchanged() {
    // Every operation of 1 to 6 keeps its service, code and version.
    let old: Vec<(Op, (Service, u16), u16)> =
        Op::ALL.iter().filter(|o| o.since() <= 6).map(|o| (*o, o.code(), o.since())).collect();
    assert_eq!(old.len(), 49);
    for (op, code, since) in old {
        assert_eq!(Op::of(code.0, code.1), Some(op));
        assert!(since <= 6);
    }
    for max in 1..=7 {
        assert_eq!(choose_version(1, max, VERSION), Some(max));
    }
    assert_eq!(choose_version(1, 9, VERSION), Some(7));
    assert_eq!(choose_version(8, 9, VERSION), None);
}

#[test]
fn enums_by_place() {
    for (i, s) in SubsystemStatus::ALL.iter().enumerate() {
        assert_eq!(encode(s), [u8::try_from(i).unwrap()]);
    }
    assert!(decode::<SubsystemStatus>(&[4]).is_err());
    for (i, k) in ValueKind::ALL.iter().enumerate() {
        assert_eq!(encode(k), [u8::try_from(i).unwrap()]);
    }
    for (i, s) in PtfStatus::ALL.iter().enumerate() {
        assert_eq!(encode(s), [u8::try_from(i).unwrap()]);
    }
    assert!(decode::<PtfStatus>(&[8]).is_err());
    for (i, t) in ResourceType::ALL.iter().enumerate() {
        assert_eq!(encode(t), [u8::try_from(i).unwrap()]);
    }
    assert_eq!(ResourceType::Communication.special(), "*CMN");
    assert_eq!(encode(&EndHow::Immediate), [1]);
    assert_eq!(encode(&IplAction::Remove), [2]);
    assert_eq!(encode(&UnitStatus::NonConfigured), [4]);
    assert_eq!(encode(&Protection::Parity), [2]);
    assert_eq!(encode(&IplSource::B), [2]);
}

#[test]
fn bodies_round_trip() {
    let sbs = Subsystem {
        sbsd: qbatch(),
        status: SubsystemStatus::Active,
        jobs: 3,
        monitor: Some(JobId {
            name: "QBATCH".into(),
            user: "QSYS".into(),
            number: "000101".into(),
        }),
        text: "Batch".into(),
    };
    same(&ListSubsystems {
        library: "*ALL".into(),
        start: 2,
        max: 5,
    });
    same(&Subsystems {
        total: 9,
        subsystems: vec![sbs.clone(), Subsystem::default()],
    });
    same(&SubsystemRef { sbsd: qbatch() });
    same(&EndSubsystem {
        sbsd: None,
        how: EndHow::Immediate,
        delay: None,
    });
    same(&EndSubsystem {
        sbsd: Some(qbatch()),
        how: EndHow::Controlled,
        delay: Some(600),
    });
    let v = SystemValue {
        name: "QSYSLIBL".into(),
        kind: ValueKind::List,
        value: "QSYS QSYS2".into(),
        text: "System part of the library list".into(),
        category: "LIBL".into(),
    };
    same(&GetSystemValues {
        names: vec!["QMAXSIGN".into(), "QSYSLIBL".into()],
    });
    same(&SystemValues { values: vec![v.clone()] });
    same(&SetSystemValue {
        name: "QMAXSIGN".into(),
        value: "5".into(),
    });
}

#[test]
fn more_bodies_round_trip() {
    same(&ListPtfs {
        product: "*ALL".into(),
        start: 0,
        max: 0,
    });
    same(&Ptfs {
        total: 1,
        ptfs: vec![Ptf {
            product: "QS00SS1".into(),
            id: "QS00003".into(),
            release: "V1R1M0".into(),
            status: PtfStatus::NotApplied,
            ipl_action: IplAction::Apply,
            delayed: true,
            text: "Fix".into(),
        }],
    });
    same(&DiskUnits {
        units: vec![DiskUnit {
            unit: 1,
            asp: 1,
            resource: "DD001".into(),
            type_model: "SSD".into(),
            serial: "S1".into(),
            capacity: 1 << 40,
            used: 1 << 38,
            status: UnitStatus::Resuming,
            protection: Protection::Mirrored,
        }],
        asps: vec![Asp {
            asp: 1,
            capacity: 1 << 41,
            used: 1 << 39,
            threshold: 90,
        }],
    });
    same(&ListHardwareResources {
        kind: ResourceType::Storage,
    });
    same(&HardwareResources {
        resources: vec![HardwareResource {
            name: "CMN03".into(),
            depth: 1,
            type_model: "QV01-001".into(),
            status: ResourceStatus::NotDetected,
            text: "Virtual Channel".into(),
            location: "org.qemu.guest_agent.0".into(),
            serial: String::new(),
            parent: "CMB01".into(),
        }],
    });
    same(&IplStatus {
        ipl_time: 1_792_000_000_000,
        ipl_type: "B".into(),
        mode: IplMode::Manual,
        slot: "A".into(),
        next_slot: Some("B".into()),
        last_src: "C9002A00".into(),
        src_text: "IPL complete".into(),
        previous_end: PreviousEnd::Abnormal,
        restricted: true,
    });
    same(&PowerDown {
        how: EndHow::Controlled,
        delay: Some(60),
        restart: true,
        ipl_source: IplSource::A,
    });
    // A flag is one byte, 0 or 1.
    let mut b = encode(&PowerDown::default());
    assert_eq!(b, [0, 0, 0, 0]);
    b[2] = 2;
    assert!(decode::<PowerDown>(&b).is_err());
}
