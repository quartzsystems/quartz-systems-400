// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every family's messages: the samples round-trip, read past a later
//! version's fields, keep unknown enumeration values, refuse truncated
//! bodies without a panic, and cover every operation and event.

use std::collections::BTreeSet;

use qsi::any::{AnyEvent, AnyReply, AnyRequest, FAMILIES};
use qsi::host::{
    console, dumps, host, model, panel, partition, partitions, profiles, refcodes, vres,
};
use qsi::{Family, Message, base, caps, pair, samples, service, update};

/// A family's number, capability and operations.
struct Ops {
    id: u16,
    cap: u64,
    requests: &'static [u16],
    replies: &'static [u16],
    events: &'static [u16],
}

fn ops<F: Family>() -> Ops {
    assert_eq!(<F::Request as Message>::FAMILY, F::ID);
    assert_eq!(<F::Reply as Message>::FAMILY, F::ID);
    assert_eq!(<F::Event as Message>::FAMILY, F::ID);
    // Partition is at version 3 (the entitlement, then its settings),
    // panel and service at 2 (the functions; notes and severity).
    let version = match F::ID {
        qsi::family::PARTITION => 3,
        qsi::family::PANEL | qsi::family::SERVICE => 2,
        _ => 1,
    };
    assert_eq!(F::VERSION, version);
    Ops {
        id: F::ID,
        cap: F::CAP,
        requests: <F::Request as Message>::OPS,
        replies: <F::Reply as Message>::OPS,
        events: <F::Event as Message>::OPS,
    }
}

fn every_family() -> Vec<Ops> {
    vec![
        ops::<base::Base>(),
        ops::<pair::Pair>(),
        ops::<service::Service>(),
        ops::<update::Update>(),
        ops::<host::HostFamily>(),
        ops::<partitions::Partitions>(),
        ops::<partition::Partition>(),
        ops::<profiles::Profiles>(),
        ops::<panel::Panel>(),
        ops::<refcodes::Refcodes>(),
        ops::<dumps::Dumps>(),
        ops::<console::Console>(),
        ops::<vres::Vres>(),
    ]
}

/// Every sample as (kind, family, op, body).
fn bodies() -> Vec<(&'static str, u16, u16, Vec<u8>)> {
    let mut out = Vec::new();
    for (q, p) in samples::exchanges() {
        out.push(("request", q.family(), q.op(), q.encode()));
        out.push(("reply", p.family(), p.op(), p.encode()));
    }
    for e in samples::events() {
        out.push(("event", e.family(), e.op(), e.encode()));
    }
    out
}

fn decode(kind: &str, family: u16, op: u16, body: &[u8]) -> Result<bool, qsi::Error> {
    Ok(match kind {
        "request" => AnyRequest::decode(family, op, body)?.is_some(),
        "reply" => AnyReply::decode(family, op, body)?.is_some(),
        _ => AnyEvent::decode(family, op, body)?.is_some(),
    })
}

#[test]
fn every_sample_round_trips_and_reads_past_a_later_version() {
    for (q, p) in samples::exchanges() {
        assert_eq!(q.family(), p.family(), "{q:?}");
        assert_eq!(q.op(), p.op(), "a reply has its request's operation: {q:?}");
        for later in [&b""[..], b"\x01later fields"] {
            let mut body = q.encode();
            body.extend_from_slice(later);
            let back = AnyRequest::decode(q.family(), q.op(), &body).unwrap();
            assert_eq!(back.as_ref(), Some(&q));
            let mut body = p.encode();
            body.extend_from_slice(later);
            let back = AnyReply::decode(p.family(), p.op(), &body).unwrap();
            assert_eq!(back.as_ref(), Some(&p));
        }
    }
    for e in samples::events() {
        let mut body = e.encode();
        body.extend_from_slice(b"\x02\x00later");
        let back = AnyEvent::decode(e.family(), e.op(), &body).unwrap();
        assert_eq!(back.as_ref(), Some(&e));
        assert_eq!(
            AnyEvent::decode(e.family(), e.op(), &e.encode()).unwrap(),
            Some(e)
        );
    }
}

#[test]
fn every_operation_and_event_has_a_sample() {
    let families = every_family();
    let mut requests = BTreeSet::new();
    let mut replies = BTreeSet::new();
    let mut events = BTreeSet::new();
    for (q, p) in samples::exchanges() {
        requests.insert((q.family(), q.op()));
        replies.insert((p.family(), p.op()));
    }
    for e in samples::events() {
        events.insert((e.family(), e.op()));
    }
    let mut count = 0;
    for f in &families {
        assert_eq!(f.requests, f.replies, "family {:#06x}", f.id);
        for op in f.requests {
            assert!(
                requests.contains(&(f.id, *op)),
                "request {:#06x}/{op:#06x}",
                f.id
            );
            assert!(
                replies.contains(&(f.id, *op)),
                "reply {:#06x}/{op:#06x}",
                f.id
            );
            count += 1;
        }
        for op in f.events {
            assert!(
                events.contains(&(f.id, *op)),
                "event {:#06x}/{op:#06x}",
                f.id
            );
        }
    }
    assert_eq!(
        requests.len(),
        count,
        "a sample of an operation no family lists"
    );
    assert_eq!(
        events.len(),
        families.iter().map(|f| f.events.len()).sum::<usize>()
    );
}

/// Every bit of operations (not a family's version 1), with which of a
/// family's requests and events need it.
#[allow(clippy::type_complexity)]
fn bits() -> Vec<(u64, fn(u16, u16) -> bool, fn(u16, u16) -> bool)> {
    use qsi::family::{BASE, DUMPS, HOST, PANEL, PARTITION, PARTITIONS, PROFILES, SERVICE, VRES};
    vec![
        (
            caps::SYSTEM_DUMPS,
            |f, op| f == BASE && (base::op::is_dump(op) || base::op::is_dump_space(op)),
            |f, op| f == BASE && base::event::is_dump(op),
        ),
        (
            caps::PHYSICAL_IO,
            |f, op| {
                (f == HOST && host::op::is_physical_io(op))
                    || (f == VRES && vres::op::is_physical_io(op))
            },
            |f, op| {
                (f == HOST && host::event::is_physical_io(op))
                    || (f == VRES && vres::event::is_physical_io(op))
            },
        ),
        (
            caps::PARTITION_V2,
            |f, op| f == PARTITION && partition::op::is_v2(op),
            |f, op| f == PARTITION && partition::event::is_v2(op),
        ),
        (
            caps::SWITCHES,
            |f, op| f == VRES && vres::op::is_switches(op),
            |_, _| false,
        ),
        (
            caps::POOLS,
            |f, op| f == VRES && vres::op::is_pools(op),
            |_, _| false,
        ),
        (
            caps::PARTITION_V3,
            |f, op| f == PARTITION && partition::op::is_v3(op),
            |_, _| false,
        ),
        (
            caps::ACTIVATIONS,
            |f, op| {
                (f == PARTITIONS && partitions::op::is_activations(op))
                    || (f == PROFILES && profiles::op::is_activations(op))
            },
            |_, _| false,
        ),
        (
            caps::POWER_ON,
            |f, op| f == HOST && host::op::is_power_on(op),
            |f, op| f == HOST && host::event::is_power_on(op),
        ),
        (
            caps::PANEL_FUNCTIONS,
            |f, op| f == PANEL && panel::op::is_v2(op),
            |_, _| false,
        ),
        (
            caps::PROBLEM_NOTES,
            |f, op| f == SERVICE && service::op::is_v2(op),
            |f, op| f == SERVICE && service::event::is_v2(op),
        ),
        (
            caps::DUMP_SPACE,
            |f, op| {
                (f == BASE && base::op::is_dump_space(op))
                    || (f == DUMPS && dumps::op::is_space(op))
            },
            |_, _| false,
        ),
        (
            caps::FIX_LEVEL,
            |f, op| f == BASE && base::op::is_fix_level(op),
            |_, _| false,
        ),
    ]
}

#[test]
fn the_families_and_their_capabilities() {
    let families = every_family();
    let ids: Vec<u16> = families.iter().map(|f| f.id).collect();
    assert_eq!(ids, FAMILIES);
    for f in &families {
        assert_eq!(caps::of(f.id), f.cap);
        assert_eq!((caps::HOST_V1 | caps::SERVICING) & f.cap, f.cap);
    }
    let extra = bits().iter().fold(0, |a, (b, _, _)| {
        assert_eq!(a & b, 0, "two groups share a bit");
        assert_eq!(b.count_ones(), 1);
        a | b
    });
    let all = families.iter().fold(0, |a, f| {
        assert_eq!(a & f.cap, 0, "two families share a bit");
        a | f.cap
    });
    assert_eq!(all & extra, 0, "a bit of operations is no family's");
    assert_eq!(all | caps::SYSTEM_DUMPS, caps::HOST_V1 | caps::SERVICING);
    // HOST_V1, SERVICING and MANAGED are what they were: the new bits
    // are beside them.
    assert_eq!(caps::HOST_V1 & caps::SERVICING, 0);
    assert_eq!(caps::HOST_V1 & extra, 0);
    assert_eq!(caps::HOST_V1, 0x1_ff03);
    assert_eq!(caps::SERVICING, 0x1c);
    assert_eq!(caps::MANAGED, 0x3);
    assert_eq!(caps::PHYSICAL_IO | caps::PARTITION_V2, 0x6_0000);
    // Bits 19 to 27 (handoff 24).
    assert_eq!(
        extra & !(caps::SYSTEM_DUMPS | caps::PHYSICAL_IO | caps::PARTITION_V2),
        0x0ff8_0000
    );
    // What a standalone system will say: no code update, no host family
    // but its panel and console.
    let standalone = caps::MANAGED
        | caps::SERVICE
        | caps::SYSTEM_DUMPS
        | caps::PANEL
        | caps::CONSOLE
        | caps::PROBLEM_NOTES
        | caps::DUMP_SPACE
        | caps::PANEL_FUNCTIONS
        | caps::FIX_LEVEL;
    assert_eq!(caps::STANDALONE, standalone);
    assert_eq!(caps::STANDALONE & caps::UPDATE, 0);
    assert_eq!(
        caps::STANDALONE & caps::HOST_V1 & !(caps::MANAGED | caps::PANEL | caps::CONSOLE),
        0
    );
}

#[test]
fn what_each_message_needs() {
    use qsi::family::{BASE, HOST, PARTITION, VRES};
    let bits = bits();
    let mut counts = vec![(0usize, 0usize); bits.len()];
    for (q, p) in samples::exchanges() {
        let (f, op) = (q.family(), q.op());
        let need = caps::of_request(f, op);
        let mut want = caps::of(f);
        for (n, (bit, req, _)) in bits.iter().enumerate() {
            if req(f, op) {
                want |= bit;
                counts[n].0 += 1;
            }
        }
        assert_eq!(need, want, "{q:?}");
        assert_eq!(caps::of_request(p.family(), p.op()), need);
    }
    for e in samples::events() {
        let (f, op) = (e.family(), e.op());
        let mut want = caps::of(f);
        for (n, (bit, _, ev)) in bits.iter().enumerate() {
            if ev(f, op) {
                want |= bit;
                counts[n].1 += 1;
            }
        }
        assert_eq!(caps::of_event(f, op), want, "{e:?}");
    }
    let counted: Vec<(u64, (usize, usize))> = bits.iter().map(|(b, _, _)| *b).zip(counts).collect();
    assert_eq!(
        counted,
        vec![
            // Base's four dumps and the dump space; their two events.
            (caps::SYSTEM_DUMPS, (5, 2)),
            // Devices and tape drives; the host's five events and the
            // drive's.
            (caps::PHYSICAL_IO, (2, 6)),
            // The entitlement and its event.
            (caps::PARTITION_V2, (1, 1)),
            (caps::SWITCHES, (6, 0)),
            (caps::POOLS, (9, 0)),
            (caps::PARTITION_V3, (3, 0)),
            // The partitions' and the profiles'.
            (caps::ACTIVATIONS, (2, 0)),
            (caps::POWER_ON, (2, 1)),
            (caps::PANEL_FUNCTIONS, (1, 0)),
            (caps::PROBLEM_NOTES, (3, 1)),
            // Base's and the dumps family's.
            (caps::DUMP_SPACE, (2, 0)),
            (caps::FIX_LEVEL, (1, 0)),
        ]
    );
    // Base's dump space needs both its bits.
    assert_eq!(
        caps::of_request(BASE, base::op::DUMP_SPACE),
        caps::BASE | caps::SYSTEM_DUMPS | caps::DUMP_SPACE
    );
    // Version 1's operations need their family alone.
    for op in 1..=3 {
        assert_eq!(caps::of_request(HOST, op), caps::HOST);
    }
    for op in 1..=6 {
        assert_eq!(caps::of_request(PARTITION, op), caps::PARTITION);
    }
    for op in [0x0001, 0x0024, 0x0025, 0x0033, 0x0040, 0x0044] {
        assert_eq!(caps::of_request(VRES, op), caps::VRES);
    }
    // Base's version 1 operations need base alone.
    for op in 1..=7 {
        assert_eq!(caps::of_request(BASE, op), caps::BASE);
    }
    // A later version's bit alone, not the one before it.
    assert_eq!(
        caps::of_request(PARTITION, partition::op::SET_SETTINGS),
        caps::PARTITION | caps::PARTITION_V3
    );
    assert_eq!(caps::of_request(0x7777, 1), 0);
}

#[test]
fn an_unknown_operation_or_family_decodes_to_none() {
    for f in every_family() {
        for op in [0, 0x7fff, 0xffff] {
            assert!(AnyRequest::decode(f.id, op, b"").unwrap().is_none());
            assert!(AnyReply::decode(f.id, op, b"xyz").unwrap().is_none());
            assert!(AnyEvent::decode(f.id, op, b"").unwrap().is_none());
        }
    }
    assert!(AnyRequest::decode(0x0fff, 1, b"").unwrap().is_none());
    assert!(AnyEvent::decode(0x0fff, 1, b"").unwrap().is_none());
}

#[test]
fn a_truncated_body_is_an_error() {
    for (kind, family, op, body) in bodies() {
        for n in 0..body.len() {
            assert!(
                decode(kind, family, op, &body[..n]).is_err(),
                "{kind} {family:#06x}/{op:#06x} read from {n} of {} bytes",
                body.len()
            );
        }
    }
}

#[test]
fn damaged_bodies_never_panic() {
    let mut seed: u64 = 0x5153_4930_3031;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for (kind, family, op, body) in bodies() {
        for _ in 0..64 {
            let mut b = body.clone();
            if b.is_empty() {
                b.push(0);
            }
            for _ in 0..=next() % 4 {
                let at = usize::try_from(next() % b.len() as u64).unwrap();
                b[at] = u8::try_from(next() & 0xff).unwrap();
            }
            let _ = decode(kind, family, op, &b);
        }
        // Lengths that claim more than there is.
        let _ = decode(kind, family, op, &[0xff; 16]);
    }
}

#[test]
fn an_unknown_enumeration_value_is_kept() {
    let e = partitions::StateChanged {
        id: 3,
        name: "PROD".into(),
        state: model::PartitionState::Running,
        time: 1,
    };
    let mut body = ipc::encode(&e);
    // The ID, the name's length and bytes, then the state.
    body[4 + 4 + 4] = 200;
    let back = partitions::Event::decode(partitions::event::STATE, &body)
        .unwrap()
        .unwrap();
    let partitions::Event::State(s) = &back else {
        panic!("{back:?}")
    };
    assert_eq!(s.state, model::PartitionState::Other(200));
    assert_eq!(back.encode(), body, "kept to be passed on");

    let q = partition::Request::ShutDown(partition::ShutDown {
        id: 3,
        kind: partition::ShutdownKind::Other(9),
        delay: 0,
        restart: false,
    });
    let back = partition::Request::decode(partition::op::SHUT_DOWN, &q.encode()).unwrap();
    assert_eq!(back, Some(q));
    for b in 0..=u8::MAX {
        assert_eq!(vres::DriveKind::from_u8(b).as_u8(), b);
        assert_eq!(qsi::base::Keylock::from_u8(b).as_u8(), b);
    }
    assert_eq!(qsi::base::Keylock::from_u8(4), qsi::base::Keylock::Secure);
    for b in 0..=u8::MAX {
        assert_eq!(service::ResourceKind::from_u8(b).as_u8(), b);
        assert_eq!(update::Attempt::from_u8(b).as_u8(), b);
        assert_eq!(base::DumpKind::from_u8(b).as_u8(), b);
        assert_eq!(host::Blocker::from_u8(b).as_u8(), b);
        assert_eq!(vres::Protection::from_u8(b).as_u8(), b);
        assert_eq!(service::Severity::from_u8(b).as_u8(), b);
        assert_eq!(partition::WithHost::from_u8(b).as_u8(), b);
        assert_eq!(host::PowerOff::from_u8(b).as_u8(), b);
    }
    assert_eq!(vres::Protection::from_u8(4), vres::Protection::Raid6);
    assert_eq!(service::Severity::from_u8(9), service::Severity::Other(9));
    assert_eq!(vres::DriveKind::from_u8(3), vres::DriveKind::PhysicalTape);
    // A resource of a kind a later version adds is kept.
    let r = service::Resource {
        kind: service::ResourceKind::Other(40),
        name: "X".into(),
        location: String::new(),
        serial: String::new(),
    };
    let back: service::Resource = ipc::decode(&ipc::encode(&r)).unwrap();
    assert_eq!(back, r);
}

#[test]
fn the_samples_are_in_bounds_and_out_of_bounds_is_refused() {
    for (q, _) in samples::exchanges() {
        assert_eq!(q.check(), Ok(()), "{q:?}");
    }
    let long = "N".repeat(model::MAX_NAME + 1);
    let bad = [
        AnyRequest::Partitions(partitions::Request::Get(partitions::Get { id: 0 })),
        AnyRequest::Partitions(partitions::Request::Get(partitions::Get { id: 1000 })),
        AnyRequest::Partition(partition::Request::Create(partition::Create {
            name: long.clone(),
        })),
        AnyRequest::Partition(partition::Request::ShutDown(partition::ShutDown {
            id: 3,
            kind: partition::ShutdownKind::Delayed,
            delay: partition::MAX_DELAY + 1,
            restart: false,
        })),
        AnyRequest::Panel(panel::Request::Function(panel::Function {
            id: 3,
            function: 0,
            args: panel::PanelArgs::default(),
        })),
        AnyRequest::Vres(vres::Request::ProcPoolCreate(vres::ProcPoolSpec {
            name: "BIG".into(),
            cores: vec![1; model::MAX_ITEMS + 1],
            maximum_units: None,
        })),
        AnyRequest::Vres(vres::Request::ImageRename(vres::ImageRename {
            name: "A".into(),
            to: long.clone(),
        })),
        AnyRequest::Base(base::Request::Power(base::Power {
            action: base::PowerAction::Off,
            immediate: false,
            delay: 100_000,
        })),
        AnyRequest::Base(base::Request::DeleteDump(base::DeleteDump { name: long })),
        AnyRequest::Service(service::Request::Get(service::Get { number: 0 })),
        AnyRequest::Service(service::Request::List(service::List {
            filter: service::Filter {
                source: Some(service::Source {
                    kind: service::SourceKind::Partition,
                    id: 1000,
                    name: String::new(),
                }),
                ..service::Filter::default()
            },
            before: None,
            limit: 10,
        })),
        AnyRequest::Update(update::Request::Stage(update::Stage {
            name: "EMPTY".into(),
            bytes: 0,
            allow_older: false,
        })),
        AnyRequest::Update(update::Request::Activate(update::Activate {
            partitions: update::Partitions::ShutDown,
            timeout: 100_000,
        })),
        AnyRequest::Partition(partition::Request::Entitlement(partition::Entitlement {
            id: 3,
            units: None,
            sharing: None,
            weight: None,
        })),
        AnyRequest::Partition(partition::Request::Entitlement(partition::Entitlement {
            id: 3,
            units: None,
            sharing: None,
            weight: Some(partition::MAX_WEIGHT + 1),
        })),
        AnyRequest::Partition(partition::Request::Entitlement(partition::Entitlement {
            id: 0,
            units: Some(100),
            sharing: None,
            weight: None,
        })),
    ];
    for q in bad {
        assert!(q.check().is_err(), "{q:?}");
    }
    // A profile with too many adapters.
    let Some(AnyRequest::Profiles(profiles::Request::Put(mut put))) = samples::exchanges()
        .into_iter()
        .map(|(q, _)| q)
        .find(|q| matches!(q, AnyRequest::Profiles(profiles::Request::Put(_))))
    else {
        panic!("no put sample")
    };
    let mut drive = put.clone();
    let adapter = put.profile.ethernet[0].clone();
    put.profile.ethernet = vec![adapter; model::MAX_ITEMS + 1];
    assert!(profiles::Request::Put(put).check().is_err());
    // A physical drive's location is a text, not a name; too long is
    // refused.
    let location = "L".repeat(model::MAX_NAME + 1);
    drive.profile.tape = vec![model::MediaDevice::drive(6, &location)];
    assert_eq!(profiles::Request::Put(drive.clone()).check(), Ok(()));
    let location = "L".repeat(model::MAX_TEXT + 1);
    drive.profile.tape = vec![model::MediaDevice::drive(6, &location)];
    assert!(profiles::Request::Put(drive).check().is_err());
}

#[test]
#[allow(clippy::too_many_lines)] // one list of bounds, read straight down
fn the_new_requests_are_bounded() {
    let settings = partition::Settings {
        description: String::new(),
        panel: true,
        with_host: partition::WithHost::Delayed,
        delay: partition::MAX_DELAY,
    };
    let names = |n: usize, len: usize| vec!["D".repeat(len); n];
    let bad = [
        // Partition version 3.
        AnyRequest::Partition(partition::Request::CreateWith(partition::CreateWith {
            id: Some(0),
            name: "P".into(),
            settings: settings.clone(),
        })),
        AnyRequest::Partition(partition::Request::CreateWith(partition::CreateWith {
            id: Some(1000),
            name: "P".into(),
            settings: settings.clone(),
        })),
        AnyRequest::Partition(partition::Request::CreateWith(partition::CreateWith {
            id: None,
            name: "P".into(),
            settings: partition::Settings {
                delay: partition::MAX_DELAY + 1,
                ..settings.clone()
            },
        })),
        AnyRequest::Partition(partition::Request::SetSettings(partition::SetSettings {
            id: 3,
            settings: partition::Settings {
                description: "X".repeat(model::MAX_TEXT + 1),
                ..settings.clone()
            },
        })),
        AnyRequest::Partition(partition::Request::SetSettings(partition::SetSettings {
            id: 0,
            settings: settings.clone(),
        })),
        AnyRequest::Partition(partition::Request::GetSettings(partition::GetSettings {
            id: 0,
        })),
        // Activations, power on, panel, dumps.
        AnyRequest::Profiles(profiles::Request::Activated(profiles::Activated { id: 0 })),
        AnyRequest::Host(host::Request::SetPowerOn(host::PowerOn {
            start_partitions: true,
            power_off: host::PowerOff::Delayed,
            delay: host::MAX_DELAY + 1,
        })),
        AnyRequest::Panel(panel::Request::Functions(panel::Functions { id: 1000 })),
        AnyRequest::Panel(panel::Request::Function(panel::Function {
            id: 1000,
            function: 1,
            args: panel::PanelArgs::default(),
        })),
        AnyRequest::Console(console::Request::Attach(console::Attach {
            id: 1000,
            take_over: false,
        })),
        AnyRequest::Dumps(dumps::Request::Space(dumps::GetSpace { id: 1000 })),
        // Notes.
        AnyRequest::Service(service::Request::Note(service::Note {
            number: 27,
            text: String::new(),
        })),
        AnyRequest::Service(service::Request::Note(service::Note {
            number: 27,
            text: "N".repeat(service::MAX_NOTE + 1),
        })),
        AnyRequest::Service(service::Request::Note(service::Note {
            number: 0,
            text: "N".into(),
        })),
        AnyRequest::Service(service::Request::GetRated(service::Get { number: 0 })),
        // Pools and switches.
        AnyRequest::Vres(vres::Request::PoolCreate(vres::PoolCreate {
            name: "P".into(),
            protection: vres::Protection::Mirrored,
            disks: names(2, model::MAX_NAME + 1),
            spares: Vec::new(),
            confirm: Vec::new(),
        })),
        AnyRequest::Vres(vres::Request::PoolCreate(vres::PoolCreate {
            name: "P".into(),
            protection: vres::Protection::Mirrored,
            disks: names(2, 4),
            spares: names(model::MAX_ITEMS + 1, 4),
            confirm: Vec::new(),
        })),
        AnyRequest::Vres(vres::Request::PoolExtend(vres::PoolExtend {
            pool: "P".into(),
            disks: names(1, 4),
            confirm: names(1, model::MAX_NAME + 1),
        })),
        AnyRequest::Vres(vres::Request::UnitReplace(vres::UnitReplace {
            pool: "P".into(),
            unit: 2,
            disk: "D".repeat(model::MAX_NAME + 1),
            confirm: false,
        })),
        AnyRequest::Vres(vres::Request::Designate(vres::Designate {
            pool: "P".repeat(model::MAX_NAME + 1),
        })),
        AnyRequest::Vres(vres::Request::SwitchCreate(vres::SwitchSpec {
            name: "S".into(),
            vlans: vec![1; model::MAX_ITEMS + 1],
            mtu: 0,
        })),
        AnyRequest::Vres(vres::Request::SwitchRename(vres::SwitchRename {
            name: "S".into(),
            to: "S".repeat(model::MAX_NAME + 1),
        })),
        AnyRequest::Vres(vres::Request::UplinkSet(vres::UplinkSet {
            switch: "S".into(),
            uplink: vres::Uplink {
                interfaces: vec!["L".repeat(model::MAX_TEXT + 1)],
                bond: None,
                native_vlan: None,
            },
        })),
        AnyRequest::Vres(vres::Request::UplinkSet(vres::UplinkSet {
            switch: "S".into(),
            uplink: vres::Uplink {
                interfaces: names(model::MAX_ITEMS + 1, 4),
                bond: None,
                native_vlan: None,
            },
        })),
    ];
    for q in bad {
        assert!(q.check().is_err(), "{q:?}");
    }
    // ID 0 is the managed system itself to the panel and console
    // families, and every partition to a dump space; a create may leave
    // its ID to the host; an interface's location is a text.
    let good = [
        AnyRequest::Panel(panel::Request::Function(panel::Function {
            id: model::SYSTEM_ID,
            function: 21,
            args: panel::PanelArgs::default(),
        })),
        AnyRequest::Console(console::Request::Attach(console::Attach {
            id: model::SYSTEM_ID,
            take_over: true,
        })),
        AnyRequest::Console(console::Request::Screen(console::Screen {
            id: model::SYSTEM_ID,
        })),
        AnyRequest::Dumps(dumps::Request::Space(dumps::GetSpace { id: 999 })),
        AnyRequest::Partition(partition::Request::CreateWith(partition::CreateWith {
            id: None,
            name: "P".into(),
            settings,
        })),
        AnyRequest::Service(service::Request::Note(service::Note {
            number: 1,
            text: "N".repeat(service::MAX_NOTE),
        })),
        AnyRequest::Vres(vres::Request::UplinkSet(vres::UplinkSet {
            switch: "S".into(),
            uplink: vres::Uplink {
                interfaces: vec!["L".repeat(model::MAX_NAME + 1)],
                bond: None,
                native_vlan: None,
            },
        })),
        AnyRequest::Vres(vres::Request::Designate(vres::Designate {
            pool: String::new(),
        })),
    ];
    for q in good {
        assert_eq!(q.check(), Ok(()), "{q:?}");
    }
}

#[test]
fn a_tape_device_is_a_volume_or_a_physical_drive() {
    use model::{MediaDevice, TapeSource};
    let d = MediaDevice::drive(6, "SPCIE-SLOT-4-T0.0.3.0");
    assert_eq!(d.media.as_deref(), Some("@SPCIE-SLOT-4-T0.0.3.0"));
    assert_eq!(d.tape_source(), TapeSource::Drive("SPCIE-SLOT-4-T0.0.3.0"));
    let v = MediaDevice {
        slot: 4,
        media: Some("SAVE01".into()),
    };
    assert_eq!(v.tape_source(), TapeSource::Volume("SAVE01"));
    let e = MediaDevice {
        slot: 4,
        media: None,
    };
    assert_eq!(e.tape_source(), TapeSource::Empty);
    // The sample profile has one of each.
    let Some(AnyRequest::Profiles(profiles::Request::Put(put))) = samples::exchanges()
        .into_iter()
        .map(|(q, _)| q)
        .find(|q| matches!(q, AnyRequest::Profiles(profiles::Request::Put(_))))
    else {
        panic!("no put sample")
    };
    let sources: Vec<_> = put
        .profile
        .tape
        .iter()
        .map(MediaDevice::tape_source)
        .collect();
    assert!(sources.contains(&TapeSource::Empty));
    assert!(sources.iter().any(|s| matches!(s, TapeSource::Drive(_))));
}

#[test]
fn the_console_streams_round_trip() {
    for out in samples::console_out() {
        assert_eq!(out.check(), Ok(()));
        let mut b = ipc::encode(&out);
        b.extend_from_slice(b"later");
        assert_eq!(console::Out::decode(&b).unwrap(), out);
    }
    for inp in samples::console_in() {
        assert_eq!(inp.check(), Ok(()));
        let mut b = ipc::encode(&inp);
        b.push(7);
        assert_eq!(console::In::decode(&b).unwrap(), inp);
    }
    // A kind a later version has is kept by its tag.
    assert_eq!(
        console::Out::decode(&[0x40, 1, 2, 3]).unwrap(),
        console::Out::Other(0x40)
    );
    assert_eq!(
        console::In::decode(&[0x41]).unwrap(),
        console::In::Other(0x41)
    );
    assert!(console::Out::decode(&[1, 0]).is_err());
    assert!(console::In::decode(&[]).is_err());
    assert!(
        console::In::Bytes(vec![0; console::MAX_SERIAL + 1])
            .check()
            .is_err()
    );
}

#[test]
fn console_pieces_survive_any_split() {
    let outs = samples::console_out();
    let mut stream = Vec::new();
    for o in &outs {
        stream.extend(o.piece().unwrap());
    }
    // The largest serial piece and the widest frame fit one frame each.
    assert!(
        console::Out::Serial(vec![0; console::MAX_SERIAL])
            .piece()
            .is_ok()
    );
    for size in [1, 2, 7, 1000, stream.len()] {
        let mut p = console::Pieces::default();
        let mut got = Vec::new();
        for chunk in stream.chunks(size) {
            p.push(chunk);
            while let Some(body) = p.next_piece() {
                got.push(console::Out::decode(&body).unwrap());
            }
        }
        assert_eq!(got, outs);
        assert_eq!(p.skipped(), 0);
    }
    let key = console::In::Refresh.piece().unwrap();
    let mut p = console::Pieces::default();
    p.push(&key);
    assert_eq!(
        console::In::decode(&p.next_piece().unwrap()).unwrap(),
        console::In::Refresh
    );
}
