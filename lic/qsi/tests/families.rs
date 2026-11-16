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
    assert_eq!(F::VERSION, 1);
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

#[test]
fn the_families_and_their_capabilities() {
    let families = every_family();
    let ids: Vec<u16> = families.iter().map(|f| f.id).collect();
    assert_eq!(ids, FAMILIES);
    for f in &families {
        assert_eq!(caps::of(f.id), f.cap);
        assert_eq!((caps::HOST_V1 | caps::SERVICING) & f.cap, f.cap);
    }
    let all = families.iter().fold(0, |a, f| {
        assert_eq!(a & f.cap, 0, "two families share a bit");
        a | f.cap
    });
    assert_eq!(all & caps::SYSTEM_DUMPS, 0, "the dumps' bit is no family's");
    assert_eq!(all | caps::SYSTEM_DUMPS, caps::HOST_V1 | caps::SERVICING);
    // HOST_V1 is what it was: the new bits are beside it.
    assert_eq!(caps::HOST_V1 & caps::SERVICING, 0);
    assert_eq!(caps::HOST_V1, 0x1_ff03);
}

#[test]
fn what_each_message_needs() {
    for (q, p) in samples::exchanges() {
        let need = caps::of_request(q.family(), q.op());
        assert_eq!(need & caps::of(q.family()), caps::of(q.family()));
        let dump = q.family() == qsi::family::BASE && base::op::is_dump(q.op());
        assert_eq!(need & caps::SYSTEM_DUMPS != 0, dump, "{q:?}");
        assert_eq!(caps::of_request(p.family(), p.op()), need);
    }
    for e in samples::events() {
        let need = caps::of_event(e.family(), e.op());
        let dump = e.family() == qsi::family::BASE && base::event::is_dump(e.op());
        assert_eq!(need & caps::SYSTEM_DUMPS != 0, dump, "{e:?}");
    }
    // Base's version 1 operations need base alone.
    for op in 1..=7 {
        assert_eq!(caps::of_request(qsi::family::BASE, op), caps::BASE);
    }
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
    }
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
    let adapter = put.profile.ethernet[0].clone();
    put.profile.ethernet = vec![adapter; model::MAX_ITEMS + 1];
    assert!(profiles::Request::Put(put).check().is_err());
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
