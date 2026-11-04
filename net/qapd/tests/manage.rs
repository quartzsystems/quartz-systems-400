// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 7 (draft): the `Manage` service against the test system:
//! lists paged, the commands' authority checks and audit entries, the
//! exit point, power down, and older versions without it.

mod common;

use std::time::Duration;

use qap_proto::ops::connection::HelloReply;
use qap_proto::ops::manage::{
    DiskUnits, EndHow, EndSubsystem, GetSystemValues, HardwareResources, IplSource, IplStatus,
    ListHardwareResources, ListPtfs, ListSubsystems, PowerDown, Protection, PtfStatus, Ptfs, ResourceType,
    SetSystemValue, SubsystemRef, SubsystemStatus, Subsystems, SystemValue, SystemValues, UnitStatus,
};
use qap_proto::types::ObjectName;
use qap_proto::{Kind, Op, Service, Status};
use qapd::manage::{EXIT_FORMAT, EXIT_POINT, ExitRequest};

use common::{Run, cfg, start};

fn signed_on(user: &str, password: &str) -> Run {
    let mut r = start(cfg());
    let h: HelloReply = r.client.hello_range(1, 7).body_as().unwrap();
    assert_eq!(h.version, 7);
    assert!(h.services.contains(&Service::Manage));
    r.client.sign_on(user, password);
    r
}

fn sbs(lib: &str, name: &str) -> ObjectName {
    ObjectName {
        library: lib.into(),
        name: name.into(),
    }
}

fn audit(r: &Run) -> Vec<String> {
    r.c.audit.lock().unwrap().clone()
}

#[test]
fn subsystems_listed_and_paged() {
    let mut r = signed_on("BOB", "SECRET1");
    let all: Subsystems = r
        .client
        .call(
            Op::ListSubsystems,
            &ListSubsystems {
                library: "*ALL".into(),
                start: 0,
                max: 0,
            },
        )
        .body_as()
        .unwrap();
    assert_eq!(all.total, 8);
    let names: Vec<&str> = all.subsystems.iter().map(|s| s.sbsd.name.as_str()).collect();
    assert_eq!(names, ["QBATCH", "QCTL", "QINTER", "QSPL", "QSYSWRK", "QUSRWRK", "HLSBS", "QSERVER"]);
    assert!(all.subsystems[0].monitor.is_some());
    let page: Subsystems = r
        .client
        .call(
            Op::ListSubsystems,
            &ListSubsystems {
                library: "QSYS".into(),
                start: 5,
                max: 2,
            },
        )
        .body_as()
        .unwrap();
    assert_eq!(page.total, 7);
    assert_eq!(page.subsystems.len(), 2);
    assert_eq!(page.subsystems[1].sbsd.name, "QSERVER");
}

#[test]
fn starting_and_ending_need_jobctl() {
    let mut bob = signed_on("BOB", "SECRET1");
    let rep = bob.client.call(Op::StartSubsystem, &SubsystemRef { sbsd: sbs("QSYS", "QSERVER") });
    assert_eq!(rep.status, Status::NotAuthorized);
    assert_eq!(rep.message.unwrap().id, "CPF222E");
    assert!(audit(&bob).contains(&"AF K BOB".to_string()));

    let mut op = common::start_with(cfg(), bob.c.clone());
    op.client.hello_range(7, 7);
    op.client.sign_on("OPER", "OPERPW1");
    let start = |r: &mut Run, lib: &str, name: &str| r.client.call(Op::StartSubsystem, &SubsystemRef { sbsd: sbs(lib, name) });
    assert_eq!(start(&mut op, "*LIBL", "QSERVER").status, Status::Ok);
    let again = start(&mut op, "QSYS", "QSERVER");
    assert_eq!((again.status, again.message.unwrap().id.as_str()), (Status::Error, "CPF1010"));
    let none = start(&mut op, "QGPL", "NOSUCH");
    assert_eq!((none.status, none.message.unwrap().id.as_str()), (Status::NotFound, "CPF9801"));

    let end = |r: &mut Run, sbsd: Option<ObjectName>| {
        r.client.call(
            Op::EndSubsystem,
            &EndSubsystem {
                sbsd,
                how: EndHow::Controlled,
                delay: Some(30),
            },
        )
    };
    assert_eq!(end(&mut op, Some(sbs("QSYS", "QSERVER"))).status, Status::Ok);
    let inactive = end(&mut op, Some(sbs("HLINV", "HLSBS")));
    assert_eq!((inactive.status, inactive.message.unwrap().id.as_str()), (Status::Error, "CPF1054"));
    let ctl = end(&mut op, Some(sbs("QSYS", "QCTL")));
    assert_eq!(ctl.message.unwrap().id, "CPF1034");
    // *ALL: the system is restricted.
    assert_eq!(end(&mut op, None).status, Status::Ok);
    let ipl: IplStatus = op.client.call(Op::IplStatus, &()).body_as().unwrap();
    assert!(ipl.restricted);
    let list: Subsystems = op
        .client
        .call(Op::ListSubsystems, &ListSubsystems { library: "*ALL".into(), start: 0, max: 0 })
        .body_as()
        .unwrap();
    assert!(list.subsystems.iter().all(|s| s.status == SubsystemStatus::Inactive && s.monitor.is_none()));
}

#[test]
fn system_values_read_and_changed() {
    let mut r = signed_on("BOB", "SECRET1");
    let got: SystemValues = r
        .client
        .call(
            Op::GetSystemValues,
            &GetSystemValues {
                names: vec!["qsyslibl".into(), "QMAXSIGN".into()],
            },
        )
        .body_as()
        .unwrap();
    let pairs: Vec<(&str, &str)> = got.values.iter().map(|v| (v.name.as_str(), v.value.as_str())).collect();
    assert_eq!(pairs, [("QSYSLIBL", "QSYS QSYS2 QHLPSYS QUSRSYS"), ("QMAXSIGN", "3")]);
    let all: SystemValues = r.client.call(Op::GetSystemValues, &GetSystemValues::default()).body_as().unwrap();
    assert!(all.values.windows(2).all(|w| w[0].name < w[1].name));
    let missing = r.client.call(
        Op::GetSystemValues,
        &GetSystemValues {
            names: vec!["QNOSUCH".into()],
        },
    );
    assert_eq!((missing.status, missing.message.unwrap().id.as_str()), (Status::NotFound, "CPF1842"));
    let set = |r: &mut Run, name: &str, value: &str| {
        r.client.call(
            Op::SetSystemValue,
            &SetSystemValue {
                name: name.into(),
                value: value.into(),
            },
        )
    };
    // BOB has no *ALLOBJ.
    assert_eq!(set(&mut r, "QDATFMT", "YMD").status, Status::NotAuthorized);

    let mut sec = common::start_with(cfg(), r.c.clone());
    sec.client.hello_range(1, 7);
    sec.client.sign_on("SECOFR", "SECPW01");
    let ok = set(&mut sec, "QMAXSIGN", "5");
    assert_eq!(ok.status, Status::Ok);
    let v: SystemValue = ok.body_as().unwrap();
    assert_eq!(v.value, "5");
    assert!(audit(&sec).contains(&"SV A QMAXSIGN".to_string()));
    let bad = set(&mut sec, "QSECURITY", "20");
    assert_eq!((bad.status, bad.message.unwrap().id.as_str()), (Status::Error, "TCP1A5F"));
    let bad = set(&mut sec, "QMAXSIGN", "many");
    assert_eq!(bad.status, Status::Error);
    // OPER has *JOBCTL only: a security value needs *ALLOBJ and *SECADM.
    let mut op = common::start_with(cfg(), r.c.clone());
    op.client.hello_range(1, 7);
    op.client.sign_on("OPER", "OPERPW1");
    let no = set(&mut op, "QPWDEXPITV", "30");
    assert_eq!((no.status, no.message.unwrap().id.as_str()), (Status::NotAuthorized, "CPF222E"));
}

#[test]
fn ptfs_disks_hardware_and_ipl() {
    let mut r = signed_on("BOB", "SECRET1");
    let p: Ptfs = r
        .client
        .call(
            Op::ListPtfs,
            &ListPtfs {
                product: "*ALL".into(),
                start: 2,
                max: 2,
            },
        )
        .body_as()
        .unwrap();
    assert_eq!(p.total, 6);
    assert_eq!(p.ptfs[0].id, "QS00003");
    assert_eq!(p.ptfs[0].status, PtfStatus::NotApplied);
    assert!(p.ptfs[0].delayed);
    let other: Ptfs = r
        .client
        .call(Op::ListPtfs, &ListPtfs { product: "QS00WDS".into(), start: 0, max: 0 })
        .body_as()
        .unwrap();
    assert_eq!(other.total, 0);

    let d: DiskUnits = r.client.call(Op::ListDiskUnits, &()).body_as().unwrap();
    assert_eq!(d.units.len(), 5);
    assert!(d.units[..4].iter().all(|u| u.protection == Protection::Mirrored && u.asp == 1));
    assert_eq!(d.units[4].status, UnitStatus::NonConfigured);
    assert_eq!(d.asps[0].threshold, 90);

    let cmn: HardwareResources = r
        .client
        .call(
            Op::ListHardwareResources,
            &ListHardwareResources {
                kind: ResourceType::Communication,
            },
        )
        .body_as()
        .unwrap();
    let names: Vec<&str> = cmn.resources.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["CMB01", "LIN01", "CMN01", "CMN02", "CMN03"]);
    assert_eq!(cmn.resources[4].location, "org.qemu.guest_agent.0");
    let all: HardwareResources = r
        .client
        .call(Op::ListHardwareResources, &ListHardwareResources { kind: ResourceType::All })
        .body_as()
        .unwrap();
    assert_eq!(all.resources.len(), 14);
    let crp: HardwareResources = r
        .client
        .call(Op::ListHardwareResources, &ListHardwareResources { kind: ResourceType::Cryptographic })
        .body_as()
        .unwrap();
    assert!(crp.resources.is_empty());

    let ipl: IplStatus = r.client.call(Op::IplStatus, &()).body_as().unwrap();
    assert_eq!((ipl.last_src.as_str(), ipl.ipl_type.as_str()), ("C9002A00", "B"));
    assert!(!ipl.restricted);
}

#[test]
fn the_exit_point_sees_every_request() {
    let mut r = signed_on("BOB", "SECRET1");
    r.c.manage.lock().unwrap().exit_refuses = Some(Box::new(|e: &ExitRequest| e.name == "GetSystemValues"));
    assert_eq!(r.client.call(Op::IplStatus, &()).status, Status::Ok);
    let no = r.client.call(Op::GetSystemValues, &GetSystemValues::default());
    assert_eq!((no.status, no.message.unwrap().id.as_str()), (Status::NotAuthorized, "TCP1A5E"));
    assert!(no.body.is_empty());
    assert!(audit(&r).contains(&"GR F BOB".to_string()));
    let calls = r.c.manage.lock().unwrap().exit_calls.clone();
    assert_eq!(calls.len(), 2);
    // QAPX0100: the user, the server, the format, the operation.
    assert_eq!(ccsid::from_ebcdic(&calls[1][..10]).trim(), "BOB");
    assert_eq!(ccsid::from_ebcdic(&calls[1][20..28]), EXIT_FORMAT);
    assert_eq!(calls[1][28..32], [0, 0, 0, 4]);
    assert_eq!(ccsid::from_ebcdic(&calls[1][52..116]).trim(), "*ALL");
    assert_eq!(EXIT_POINT, "QS400_QAP_MGMT");
    // A refused change changes nothing.
    r.c.manage.lock().unwrap().exit_refuses = Some(Box::new(|e: &ExitRequest| e.target == "QMAXSIGN"));
    let mut sec = common::start_with(cfg(), r.c.clone());
    sec.client.hello_range(1, 7);
    sec.client.sign_on("SECOFR", "SECPW01");
    let no = sec.client.call(Op::SetSystemValue, &SetSystemValue { name: "QMAXSIGN".into(), value: "9".into() });
    assert_eq!(no.status, Status::NotAuthorized);
    let v = sec.c.manage.lock().unwrap().values(&["QMAXSIGN".into()]).unwrap();
    assert_eq!(v[0].value, "3");
}

#[test]
fn power_down_answers_then_the_server_ends() {
    let mut bob = signed_on("BOB", "SECRET1");
    let no = bob.client.call(Op::PowerDown, &PowerDown::default());
    assert_eq!(no.status, Status::NotAuthorized);
    assert!(bob.c.manage.lock().unwrap().powered.is_none());

    let mut op = common::start_with(cfg(), bob.c.clone());
    op.client.hello_range(1, 7);
    op.client.sign_on("OPER", "OPERPW1");
    let req = PowerDown {
        how: EndHow::Controlled,
        delay: Some(600),
        restart: true,
        ipl_source: IplSource::Panel,
    };
    assert_eq!(op.client.call(Op::PowerDown, &req).status, Status::Ok);
    assert_eq!(op.c.manage.lock().unwrap().powered.as_ref(), Some(&req));
    let ev = op.client.try_recv(Duration::from_secs(5)).expect("ServerEnding");
    assert_eq!((ev.kind, ev.status), (Kind::Event, Status::ServerEnding));
    assert!(op.client.closed());
}

#[test]
fn version_6_sees_no_manage() {
    let mut r = start(cfg());
    let h: HelloReply = r.client.hello_range(1, 6).body_as().unwrap();
    assert_eq!(h.version, 6);
    assert!(!h.services.contains(&Service::Manage));
    r.client.sign_on("SECOFR", "SECPW01");
    for op in Op::ALL.into_iter().filter(|o| o.since() == 7) {
        let rep = r.client.call(op, &());
        assert_eq!(rep.status, Status::InvalidRequest, "{op:?}");
        assert!(rep.body.is_empty());
    }
    assert!(r.c.manage.lock().unwrap().exit_calls.is_empty());
}

#[test]
fn not_before_sign_on() {
    let mut r = start(cfg());
    r.client.hello_range(1, 7);
    assert_eq!(r.client.call(Op::IplStatus, &()).status, Status::NotSignedOn);
    // A body that does not decode.
    r.client.sign_on("BOB", "SECRET1");
    assert_eq!(r.client.call(Op::ListPtfs, &()).status, Status::InvalidRequest);
}
