// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One sample of every request, reply and event of every family, with
//! values a virtual server host would give: for the session's tests
//! (every message of every family exchanged) and as fuzz seeds.

use qsipart::console::{Cell, Frame, Key, Run, Shape, SrcShown, Status as Cursor, Update};

use crate::any::{AnyEvent, AnyReply, AnyRequest};
use crate::base::{self, DumpKind, Keylock, PowerAction, PowerState, Refcode, Replay};
use crate::hello::PeerKind;
use crate::host::model::{
    AutoStart, Boot, Channel, ConsoleMode, Current, Definition, Environment, EthernetAdapter,
    Holder, IplSource, IplType, Lun, MediaDevice, Memory, Note, PartitionState, PhysicalIo,
    ProcessorMode, Processors, Profile, ScsiAdapter, Sharing,
};
use crate::host::{console, dumps, host, panel, partition, partitions, profiles, refcodes, vres};
use crate::message::Empty;
use crate::{pair, service, update};

/// Every request with a reply to it, in family and operation order.
#[must_use]
pub fn exchanges() -> Vec<(AnyRequest, AnyReply)> {
    let mut out = Vec::new();
    out.extend(base_exchanges());
    out.extend(pair_exchanges());
    out.extend(service_exchanges());
    out.extend(service_v2_exchanges());
    out.extend(update_exchanges());
    out.extend(host_exchanges());
    out.extend(partitions_exchanges());
    out.extend(partition_exchanges());
    out.extend(profiles_exchanges());
    out.extend(panel_exchanges());
    out.extend(refcodes_exchanges());
    out.extend(dumps_exchanges());
    out.extend(console_exchanges());
    out.extend(vres_exchanges());
    out
}

/// Every event, in family and event order.
#[must_use]
pub fn events() -> Vec<AnyEvent> {
    let mut out = Vec::new();
    out.extend(base_events());
    out.extend(service_events());
    out.extend(update_events());
    out.extend(host_events());
    out.extend(partitions_events());
    out.extend(partition_events());
    out.extend(profiles_events());
    out.extend(refcodes_events());
    out.extend(dumps_events());
    out.extend(vres_events());
    out
}

/// A time, seconds since the epoch: 2026-11-15.
const T: u64 = 1_794_700_000;

/// A physical tape drive's location code: a SAS drive behind the card
/// in slot 4.
const TAPE_DRIVE: &str = "SPCIE-SLOT-4-T0.0.3.0";

fn s(text: &str) -> String {
    text.to_string()
}

fn refcode(seq: u64, code: &str, text: &str) -> Refcode {
    Refcode {
        seq,
        code: s(code),
        words: if code.starts_with('C') && seq.is_multiple_of(2) {
            Vec::new()
        } else {
            vec![s("00000000"), s("0000A1F0")]
        },
        text: s(text),
        time_ms: T * 1000 + seq,
    }
}

fn definition() -> Definition {
    Definition {
        id: 3,
        name: s("PROD"),
        environment: Environment::Standalone,
        serial: s("K2M4103"),
        keylock: Keylock::Normal,
        default_profile: s("NORMAL"),
        auto_start: AutoStart {
            enabled: true,
            order: 10,
            delay_secs: 30,
            wait: true,
            wait_secs: 600,
        },
        created: T - 86_400 * 30,
    }
}

fn profile() -> Profile {
    Profile {
        name: s("NORMAL"),
        processors: Processors {
            mode: ProcessorMode::Shared,
            minimum: 1,
            desired: 2,
            maximum: 4,
            minimum_units: Some(50),
            desired_units: Some(150),
            maximum_units: Some(400),
            sharing: Some(Sharing::Uncapped),
            weight: Some(128),
            pool: Some(s("DEFAULT")),
        },
        memory: Memory {
            minimum_mib: 2048,
            desired_mib: 8192,
            maximum_mib: 16_384,
        },
        boot: Boot {
            source: IplSource::Recorded,
            ipl_type: IplType::Unattended,
            console: ConsoleMode::Partition,
        },
        scsi: vec![ScsiAdapter {
            slot: 2,
            luns: vec![
                Lun {
                    lun: 0,
                    backing: s("POOL1/PROD_LS"),
                    serial: None,
                },
                Lun {
                    lun: 1,
                    backing: s("POOL1/PROD_D1"),
                    serial: Some(s("PRODD1")),
                },
            ],
        }],
        ethernet: vec![EthernetAdapter {
            slot: 5,
            switch: Some(s("PRODNET")),
            vlan: 10,
            tagged: vec![20],
            mac: Some(s("02:5a:3c:00:03:05")),
            required: true,
        }],
        optical: vec![MediaDevice {
            slot: 3,
            media: Some(s("V1R2M0_BASE")),
        }],
        tape: vec![
            MediaDevice {
                slot: 4,
                media: None,
            },
            MediaDevice::drive(6, TAPE_DRIVE),
        ],
        physical_io: vec![PhysicalIo {
            location: s("SPCIE-SLOT-3"),
            required: false,
        }],
    }
}

fn current() -> Current {
    Current {
        mode: ProcessorMode::Shared,
        cores: Vec::new(),
        pool: s("DEFAULT"),
        units: 150,
        virtuals: 2,
        capped: false,
        weight: 128,
        memory_mib: 8192,
    }
}

fn channel() -> Channel {
    Channel {
        up: true,
        none: false,
        responding: true,
        kept: false,
        system: s("PROD"),
        release: s("V1R2M0"),
        fix_level: s("C2631200"),
        addresses: vec![s("10.0.2.15/24"), s("fd00::15/64")],
        version: 1,
        caps: qsipart::caps::SERVICE_V1 | qsipart::caps::CONSOLE_V1,
    }
}

fn status() -> partitions::Status {
    partitions::Status {
        id: 3,
        name: s("PROD"),
        state: PartitionState::Running,
        code: s("C6004500"),
        text: s("IPL complete"),
        current: Some(current()),
        profile: s("NORMAL"),
        last_end: s("exit 0"),
        message: None,
        ethernet: vec![partitions::Link {
            slot: 5,
            switch: s("PRODNET"),
            mac: s("02:5a:3c:00:03:05"),
            link: true,
        }],
        channel: channel(),
    }
}

fn idle_status() -> partitions::Status {
    partitions::Status {
        id: 4,
        name: s("TEST"),
        state: PartitionState::NotActivated,
        code: s("A2005100"),
        text: s("Partition could not IPL: TEST"),
        current: None,
        profile: s("NORMAL"),
        last_end: s("exit 3"),
        message: Some(Note {
            msgid: s("QVS2108"),
            text: s("Partition TEST did not IPL: A2005050."),
            help: s("The VMM stopped the IPL; the code before says why."),
        }),
        ethernet: Vec::new(),
        channel: Channel::default(),
    }
}

fn posted(seq: u64, code: &str, text: &str, source: refcodes::Source) -> refcodes::Posted {
    refcodes::Posted {
        refcode: refcode(seq, code, text),
        source,
    }
}

fn cells(text: &str, cols: usize) -> Vec<Cell> {
    let mut v: Vec<Cell> = text
        .chars()
        .map(|ch| Cell {
            ch,
            attr: qsipart::console::attr::GREEN,
        })
        .collect();
    v.resize(cols, Cell::default());
    v
}

fn frame() -> Frame {
    let mut all = cells("                              Sign On", 80);
    for _ in 1..24 {
        all.extend(cells("", 80));
    }
    Frame {
        seq: 7,
        rows: 24,
        cols: 80,
        cells: all,
        status: Cursor {
            row: 5,
            col: 52,
            shape: Shape::Underline,
            indicators: 0,
            src: None,
        },
    }
}

fn hold(partition: u32, slot: u32) -> Holder {
    Holder { partition, slot }
}

fn storage_pool() -> vres::StoragePool {
    vres::StoragePool {
        name: s("POOL1"),
        uuid: s("6f0c2a51-9d3e-4b7a-8c21-5e4f7a90b1d2"),
        protection: s("mirrored"),
        state: s("Rebuilding"),
        work: s("rebuilding"),
        work_unit: 2,
        percent: 41,
        bytes: 1_920_383_410_176,
        allocated: 412_316_860_416,
        provisioned: 858_993_459_200,
        repository: true,
        units: vec![
            vres::PoolUnit {
                number: 1,
                disk: s("nvme0n1"),
                serial: s("S5GXNF0R401234"),
                location: s("SM-2-SLOT-1-N1"),
                bytes: 1_920_383_410_176,
                state: s("Active"),
            },
            vres::PoolUnit {
                number: 2,
                disk: s("nvme1n1"),
                serial: s("S5GXNF0R405678"),
                location: s("SM-2-SLOT-2-N1"),
                bytes: 1_920_383_410_176,
                state: s("Rebuilding"),
            },
        ],
        failure: String::new(),
    }
}

fn vdisk() -> vres::Vdisk {
    vres::Vdisk {
        pool: s("POOL1"),
        name: s("PROD_LS"),
        bytes: 107_374_182_400,
        allocated: 21_474_836_480,
        thick: false,
        holder: hold(3, 2),
    }
}

fn image() -> vres::Image {
    vres::Image {
        name: s("V1R2M0_BASE"),
        bytes: 3_221_225_472,
        label: s("QSV1R2M0"),
        holder: hold(3, 3),
    }
}

fn volume() -> vres::Volume {
    vres::Volume {
        name: s("SAVE01"),
        identifier: s("SAV001"),
        capacity_mib: 102_400,
        used: 5_368_709_120,
        protected: false,
        holder: hold(0, 0),
    }
}

fn drive() -> vres::Drive {
    vres::Drive {
        slot: 4,
        kind: vres::DriveKind::Tape,
        name: s("SAVE01"),
        prevented: true,
    }
}

fn switch() -> vres::Switch {
    vres::Switch {
        name: s("PRODNET"),
        id: 1,
        vlans: vec![10, 20],
        mtu: 1500,
        uplink: Some(vres::Uplink {
            interfaces: vec![s("SPCIE-SLOT-2-E0"), s("SPCIE-SLOT-2-E1")],
            bond: Some(vres::Bond::Lacp),
            native_vlan: Some(10),
        }),
        uplink_state: s("Degraded"),
        members: vec![
            vres::Member {
                location: s("SPCIE-SLOT-2-E0"),
                name: s("enp3s0f0"),
                carrier: true,
                speed_mbps: 10_000,
            },
            vres::Member {
                location: s("SPCIE-SLOT-2-E1"),
                name: s("enp3s0f1"),
                carrier: false,
                speed_mbps: 0,
            },
        ],
        ports: 2,
        failure: String::new(),
    }
}

fn host_value() -> host::Host {
    host::Host {
        sockets: 1,
        cores: 16,
        threads_per_core: 2,
        processors: host::Amount {
            installed: 1600,
            configurable: 1500,
            available: 1150,
        },
        memory: host::Amount {
            installed: 131_072,
            configurable: 124_928,
            available: 108_544,
        },
        memory_block_mib: 16,
        reserved_cores: vec![0],
        reserved_memory_mib: 6144,
        pools: vec![
            host::ProcPool {
                name: s("DEFAULT"),
                cores: (1..14).collect(),
                maximum: 1300,
                entitled: 150,
                available: 1150,
                partitions: 1,
            },
            host::ProcPool {
                name: s("BATCH"),
                cores: vec![14, 15],
                maximum: 150,
                entitled: 0,
                available: 150,
                partitions: 0,
            },
        ],
        partitions: 2,
        max_partitions: 64,
    }
}

fn disk_item() -> host::Item {
    host::Item {
        location: s("SM-2-SLOT-1-N1"),
        kind: host::ItemKind::Disk,
        name: s("nvme0n1"),
        description: s("Samsung SSD 980 PRO 2TB"),
        serial: s("S5GXNF0R401234"),
        bytes: 1_920_383_410_176,
        mac: String::new(),
        speed_mbps: 0,
        carrier: false,
        owner: host::Owner::Pool,
        owner_name: s("POOL1"),
        partition: 0,
        what: s("POOL1 unit 1, Active"),
    }
}

fn base_exchanges() -> Vec<(AnyRequest, AnyReply)> {
    use base::{Reply as P, Request as Q};
    let w = |q, p| (AnyRequest::Base(q), AnyReply::Base(p));
    vec![
        w(
            Q::Identify(Empty),
            P::Identify(base::Identity {
                kind: PeerKind::Host,
                name: s("VS01"),
                release: s("V1R0M0"),
                serial: s("7XK2M41"),
                model: s("Quartz Model 9"),
                build: s("cebf3d2"),
            }),
        ),
        w(
            Q::State(Empty),
            P::State(base::State {
                power: PowerState::Running,
                refcode: Some(refcode(412, "C2001090", "Host IPL complete")),
                release: s("V1R0M0"),
                slot: s("A"),
                keylock: Some(Keylock::Normal),
            }),
        ),
        w(
            Q::Power(base::Power {
                action: PowerAction::Restart,
                immediate: false,
                delay: 600,
            }),
            P::Power(Empty),
        ),
        w(
            Q::Refcodes(base::Refcodes {
                before: Some(412),
                limit: 2,
            }),
            P::Refcodes(base::RefcodeList {
                codes: vec![
                    refcode(
                        411,
                        "C2006000",
                        "Partition manager ready: 2 defined, 1 running",
                    ),
                    refcode(410, "A2007050", "Pool POOL1 is 80% allocated"),
                ],
                more: Some(410),
            }),
        ),
        w(
            Q::Subscribe(base::Subscribe { since: Some(9120) }),
            P::Subscribe(base::Subscribed {
                replay: Replay::Resumed,
                replayed: 3,
                next: 9124,
            }),
        ),
        w(Q::Unsubscribe(Empty), P::Unsubscribe(Empty)),
        w(Q::Time(Empty), P::Time(time())),
        w(
            Q::FixLevel(Empty),
            P::FixLevel(base::Level {
                level: s("C6305110"),
                date: T - 86_400 * 4,
            }),
        ),
        w(Q::Resources(Empty), P::Resources(resources())),
        w(Q::Dumps(Empty), P::Dumps(vec![system_dump()])),
        w(
            Q::TakeDump(base::TakeDump {
                kind: DumpKind::ServiceData,
                kind_name: String::new(),
            }),
            P::TakeDump(base::DumpBegun {
                name: s("SVC-1794700000"),
            }),
        ),
        w(
            Q::DeleteDump(base::DeleteDump {
                name: s("SVC-1794600000"),
            }),
            P::DeleteDump(Empty),
        ),
        w(
            Q::DownloadDump(base::DownloadDump {
                name: s("SVC-1794700000"),
            }),
            P::DownloadDump(base::DumpDownloading {
                name: s("SVC-1794700000"),
                bytes: 41_943_040,
            }),
        ),
        w(Q::DumpSpace(Empty), P::DumpSpace(space())),
    ]
}

/// The system's own dumps' space: one dump of three, under a 4 GiB
/// limit.
fn time() -> base::Time {
    base::Time {
        unix_ns: 1_794_700_000_123_456_789,
        offset_s: -18_000,
        zone: s("America/New_York"),
    }
}

fn resources() -> base::Resources {
    base::Resources {
        processors: 4,
        cpu_used: 1250,
        memory_kb: 16 * 1024 * 1024,
        memory_used_kb: 9 * 1024 * 1024,
        asp_mb: 512_000,
        asp_used: 4137,
        temp_mb: 1_840,
        total_jobs: 212,
        active_jobs: 187,
        signed_on: 3,
        batch_running: 11,
        threads: 640,
        up_secs: 86_400 * 3 + 3_600,
    }
}

fn space() -> base::Space {
    base::Space {
        bytes: 41_943_040,
        limit: 4_294_967_296,
        dumps: 1,
        max_dumps: 3,
    }
}

fn system_dump() -> base::SystemDump {
    base::SystemDump {
        name: s("SVC-1794700000"),
        kind: DumpKind::ServiceData,
        kind_name: s("service data"),
        bytes: 41_943_040,
        time: T,
        by: s("QSMC01 OPERATOR"),
    }
}

fn base_events() -> Vec<AnyEvent> {
    use base::Event as E;
    vec![
        E::Refcode(refcode(
            413,
            "A2007020",
            "Pool POOL1 unit 2 failed: SM-2-SLOT-2-N1",
        )),
        E::State(base::State {
            power: PowerState::Ending,
            refcode: Some(refcode(414, "D2001010", "Host powering down")),
            release: s("V1R0M0"),
            slot: s("A"),
            keylock: None,
        }),
        E::Identity(base::Identity {
            kind: PeerKind::Host,
            name: s("VS01"),
            release: s("V1R0M0"),
            serial: s("7XK2M41"),
            model: String::new(),
            build: s("cebf3d2"),
        }),
        E::DumpTaken(base::DumpTaken {
            name: s("SVC-1794700000"),
            dump: Some(system_dump()),
            error: None,
            time: T,
        }),
        E::DumpRemoved(base::DumpRemoved {
            name: s("SVC-1794600000"),
            time: T,
        }),
    ]
    .into_iter()
    .map(AnyEvent::Base)
    .collect()
}

fn source_partition() -> service::Source {
    service::Source {
        kind: service::SourceKind::Partition,
        id: 3,
        name: s("PROD"),
    }
}

fn problem_summary() -> service::Summary {
    service::Summary {
        number: 27,
        status: service::ProblemStatus::Open,
        first_seen: T - 600,
        last_seen: T,
        count: 4,
        refcode: refcode(415, "A2007020", "Pool unit failed: POOL1 unit 2"),
        source: service::Source {
            kind: service::SourceKind::System,
            id: 0,
            name: s("VS01"),
        },
        description: s("A unit of storage pool POOL1 failed"),
        msgid: s("QVS3101"),
        holds: true,
        closed_by: String::new(),
        closed_at: 0,
    }
}

fn service_exchanges() -> Vec<(AnyRequest, AnyReply)> {
    use service::{Reply as P, Request as Q};
    let w = |q, p| (AnyRequest::Service(q), AnyReply::Service(p));
    let closed = closed_summary();
    vec![
        w(
            Q::List(service::List {
                filter: service::Filter {
                    status: Some(service::ProblemStatus::Open),
                    source: None,
                    since: Some(T - 86_400),
                },
                before: None,
                limit: 2,
            }),
            P::List(service::Page {
                problems: vec![problem_summary(), closed.clone()],
                more: Some(26),
            }),
        ),
        w(
            Q::Get(service::Get { number: 27 }),
            P::Get(service::Problem {
                summary: problem_summary(),
                resources: vec![
                    service::Resource {
                        kind: service::ResourceKind::StoragePool,
                        name: s("POOL1"),
                        location: String::new(),
                        serial: String::new(),
                    },
                    service::Resource {
                        kind: service::ResourceKind::DiskUnit,
                        name: s("2"),
                        location: s("SM-2-SLOT-2-N1"),
                        serial: s("S4EVNX0N912345"),
                    },
                ],
                log: vec![
                    service::Line {
                        time: T - 600,
                        text: s("Unit 2 taken out of service: 12 write errors"),
                    },
                    service::Line {
                        time: T,
                        text: s("Seen again at IPL"),
                    },
                ],
            }),
        ),
        w(
            Q::Close(service::Close {
                number: 27,
                anyway: true,
            }),
            P::Close(closed),
        ),
    ]
}

/// A problem closed by a person.
fn closed_summary() -> service::Summary {
    service::Summary {
        status: service::ProblemStatus::Closed,
        holds: false,
        closed_by: s("OPERATOR"),
        closed_at: T + 60,
        source: source_partition(),
        ..problem_summary()
    }
}

/// Version 2: notes and severity.
fn service_v2_exchanges() -> Vec<(AnyRequest, AnyReply)> {
    use service::{Reply as P, Request as Q};
    let w = |q, p| (AnyRequest::Service(q), AnyReply::Service(p));
    vec![
        w(
            Q::Note(service::Note {
                number: 27,
                text: s("Replacement disk ordered"),
            }),
            P::Note(note_line()),
        ),
        w(
            Q::ListRated(service::List {
                filter: service::Filter::default(),
                before: Some(28),
                limit: 2,
            }),
            P::ListRated(service::RatedPage {
                problems: vec![
                    service::Rated {
                        severity: service::Severity::Attention,
                        summary: problem_summary(),
                    },
                    service::Rated {
                        severity: service::Severity::Information,
                        summary: closed_summary(),
                    },
                ],
                more: None,
            }),
        ),
        w(
            Q::GetRated(service::Get { number: 27 }),
            P::GetRated(service::RatedProblem {
                severity: service::Severity::Terminal,
                problem: service::Problem {
                    summary: problem_summary(),
                    resources: Vec::new(),
                    log: vec![note_line()],
                },
            }),
        ),
    ]
}

/// A person's note as the analysis log keeps it.
fn note_line() -> service::Line {
    service::Line {
        time: T + 30,
        text: s("Note by OPERATOR: Replacement disk ordered"),
    }
}

fn service_events() -> Vec<AnyEvent> {
    use service::Event as E;
    let at = |problem| service::Noticed { problem, time: T };
    let cleared = service::Summary {
        holds: false,
        ..problem_summary()
    };
    let closed = service::Summary {
        status: service::ProblemStatus::Closed,
        closed_by: s("OPERATOR"),
        closed_at: T,
        ..cleared.clone()
    };
    vec![
        E::Opened(at(service::Summary {
            count: 1,
            ..problem_summary()
        })),
        E::Seen(at(problem_summary())),
        E::Cleared(at(cleared)),
        E::Closed(at(closed)),
        E::Noted(service::Noted {
            number: 27,
            line: note_line(),
            by: s("OPERATOR"),
            time: T + 30,
        }),
    ]
    .into_iter()
    .map(AnyEvent::Service)
    .collect()
}

fn update_image() -> update::Image {
    update::Image {
        product: s("VIRTUAL SERVER"),
        release: s("V1R1M0"),
        build: s("5d1e0a2"),
        fixes: vec![s("VS00012"), s("VS00013")],
        bytes: 96_468_992,
    }
}

fn update_exchanges() -> Vec<(AnyRequest, AnyReply)> {
    use update::{Reply as P, Request as Q};
    let w = |q, p| (AnyRequest::Update(q), AnyReply::Update(p));
    let slot = |name: &str, running: bool| update::Slot {
        name: s(name),
        release: s("V1R0M0"),
        build: s("19c7be0"),
        fixes: if running {
            vec![s("VS00011")]
        } else {
            Vec::new()
        },
        running,
        default: running,
        last: if running {
            update::Attempt::Confirmed
        } else {
            update::Attempt::RolledBack
        },
        reason: if running {
            String::new()
        } else {
            s("did not confirm within 3 attempts")
        },
        time: T - 86_400,
    };
    vec![
        w(
            Q::Status(Empty),
            P::Status(update::Status {
                slots: vec![slot("A", true), slot("B", false)],
                staged: Some(update::StagedImage {
                    name: s("V1R1M0"),
                    bytes: 96_468_992,
                    verification: update::Verification::Verified,
                    image: Some(update_image()),
                    error: None,
                    time: T,
                }),
                installing: Some(update::Progress {
                    slot: s("B"),
                    percent: 40,
                    time: T,
                }),
            }),
        ),
        w(
            Q::Stage(update::Stage {
                name: s("V1R1M0"),
                bytes: 96_468_992,
                allow_older: false,
            }),
            P::Stage(update::Staging {
                name: s("V1R1M0"),
                bytes: 96_468_992,
            }),
        ),
        w(Q::Discard(Empty), P::Discard(Empty)),
        w(
            Q::Install(Empty),
            P::Install(update::Target { slot: s("B") }),
        ),
        w(
            Q::Activate(update::Activate {
                partitions: update::Partitions::ShutDown,
                timeout: 900,
            }),
            P::Activate(Empty),
        ),
        w(Q::Cancel(Empty), P::Cancel(Empty)),
    ]
}

fn update_events() -> Vec<AnyEvent> {
    use update::Event as E;
    vec![
        E::Staged(update::Staged {
            name: s("V1R1M0"),
            bytes: 96_468_992,
            error: None,
            time: T,
        }),
        E::Verified(update::Verified {
            name: s("V1R1M0"),
            image: None,
            error: Some(Note {
                msgid: s(update::msgid::BAD_SIGNATURE),
                text: s("V1R1M0 is not signed by a key this host trusts."),
                help: String::new(),
            }),
            time: T,
        }),
        E::Installing(update::Progress {
            slot: s("B"),
            percent: 60,
            time: T,
        }),
        E::Installed(update::Installed {
            slot: s("B"),
            image: Some(update_image()),
            error: None,
            time: T,
        }),
        E::Activating(update::Activating {
            slot: s("B"),
            time: T,
        }),
        E::Confirmed(update::Confirmed {
            slot: s("B"),
            release: s("V1R1M0"),
            build: s("5d1e0a2"),
            time: T,
        }),
        E::RolledBack(update::RolledBack {
            slot: s("B"),
            reason: s("the slot did not confirm"),
            code: s("A2001035"),
            time: T,
        }),
    ]
    .into_iter()
    .map(AnyEvent::Update)
    .collect()
}

fn pair_exchanges() -> Vec<(AnyRequest, AnyReply)> {
    use pair::{Reply as P, Request as Q};
    let w = |q, p| (AnyRequest::Pair(q), AnyReply::Pair(p));
    vec![
        w(
            Q::Start(pair::Start {
                spake: vec![0x5a; 33],
                name: s("QSMC01"),
            }),
            P::Answer(pair::Answer {
                spake: vec![0xa5; 33],
                confirm: vec![0x11; 32],
                name: s("VS01"),
            }),
        ),
        w(
            Q::Confirm(pair::Confirm {
                confirm: vec![0x22; 32],
            }),
            P::Done(Empty),
        ),
    ]
}

fn host_exchanges() -> Vec<(AnyRequest, AnyReply)> {
    use host::{Reply as P, Request as Q};
    let w = |q, p| (AnyRequest::Host(q), AnyReply::Host(p));
    vec![
        w(Q::Host(Empty), P::Host(host_value())),
        w(
            Q::Settings(Empty),
            P::Settings(host::Settings {
                max_partitions: 64,
                memory_block_mib: 16,
                reserved_cores: 1,
                reserved_memory_mib: Some(6144),
                mac_prefix: Some(s("02:5a:3c")),
                delayed_grace_secs: 120,
                system_timeout_secs: 300,
                host_timeout_secs: 300,
                channel_wait_secs: 30,
                max_dumps: 3,
                max_dump_mib: 262_144,
            }),
        ),
        w(
            Q::Hardware(Empty),
            P::Hardware(host::Hardware {
                manufacturer: s("Supermicro"),
                product: s("AS-1115S-FWTRT"),
                processor: s("AMD EPYC 8124P 16-Core Processor"),
                virtualization: s("SVM"),
                items: vec![
                    disk_item(),
                    host::Item {
                        location: s("SPCIE-SLOT-2-E0"),
                        kind: host::ItemKind::Interface,
                        name: s("enp3s0f0"),
                        description: s("ixgbe"),
                        serial: String::new(),
                        bytes: 0,
                        mac: s("3c:ec:ef:12:34:56"),
                        speed_mbps: 10_000,
                        carrier: true,
                        owner: host::Owner::Switch,
                        owner_name: s("PRODNET"),
                        partition: 0,
                        what: s("Bond member of PRODNET"),
                    },
                    host::Item {
                        location: s("P0000-B00-D1F-F2"),
                        kind: host::ItemKind::Pci,
                        name: s("0000:00:1f.2"),
                        description: s("8086:2922 010601 ahci"),
                        serial: String::new(),
                        bytes: 0,
                        mac: String::new(),
                        speed_mbps: 0,
                        carrier: false,
                        owner: host::Owner::Host,
                        owner_name: String::new(),
                        partition: 0,
                        what: s("Load source adapter"),
                    },
                ],
            }),
        ),
        w(
            Q::Devices(Empty),
            P::Devices(vec![device(), blocked_device()]),
        ),
        w(Q::PowerOn(Empty), P::PowerOn(power_on())),
        w(
            Q::SetPowerOn(host::PowerOn {
                start_partitions: false,
                power_off: host::PowerOff::Immediate,
                delay: 0,
            }),
            P::SetPowerOn(Empty),
        ),
    ]
}

/// A host that starts its partitions and gives each ten minutes to end.
fn power_on() -> host::PowerOn {
    host::PowerOn {
        start_partitions: true,
        power_off: host::PowerOff::Delayed,
        delay: 600,
    }
}

fn holder() -> host::DeviceHolder {
    host::DeviceHolder {
        partition: 3,
        name: s("PROD"),
        profile: s("NORMAL"),
    }
}

/// A card released for partitions and held by one.
fn device() -> host::Device {
    host::Device {
        location: s("SPCIE-SLOT-3"),
        address: s("0000:41:00.0"),
        class: 0x02_00_00,
        vendor: 0x8086,
        device: 0x1572,
        description: s("Intel Ethernet Controller X710 for 10GbE SFP+"),
        released: true,
        assignable: true,
        blocked: None,
        group: 31,
        with: vec![s("SPCIE-SLOT-3-F1")],
        holder: Some(holder()),
    }
}

/// A function not released: it shares its group with one the host
/// uses.
fn blocked_device() -> host::Device {
    host::Device {
        location: s("P0000-B00-D14-F0"),
        address: s("0000:00:14.0"),
        class: 0x0C_03_30,
        vendor: 0x8086,
        device: 0xA36D,
        description: s("USB 3.1 xHCI Host Controller"),
        released: false,
        assignable: false,
        blocked: Some(host::Blocked {
            reason: host::Blocker::GroupNotReleased,
            note: Note {
                msgid: s("QVS1402"),
                text: s("P0000-B00-D14-F0 shares its group with P0000-B00-D14-F2."),
                help: s("Release every device of the group, or none."),
            },
        }),
        group: 7,
        with: vec![s("P0000-B00-D14-F2")],
        holder: None,
    }
}

fn host_events() -> Vec<AnyEvent> {
    use host::Event as E;
    vec![
        E::Resources(host_value()),
        E::Item(host::ItemChanged {
            location: s("SM-2-SLOT-1-N1"),
            item: Some(disk_item()),
            time: T,
        }),
        E::Released(host::DeviceChanged {
            location: s("SPCIE-SLOT-3"),
            device: Some(host::Device {
                holder: None,
                ..device()
            }),
            time: T,
        }),
        E::TakenBack(host::DeviceChanged {
            location: s("P0000-B00-D14-F0"),
            device: Some(blocked_device()),
            time: T,
        }),
        E::Assigned(host::DeviceHeld {
            location: s("SPCIE-SLOT-3"),
            holder: holder(),
            time: T,
        }),
        E::Returned(host::DeviceHeld {
            location: s("SPCIE-SLOT-3"),
            holder: holder(),
            time: T,
        }),
        E::ResetFailed(host::ResetFailed {
            location: s("SPCIE-SLOT-3"),
            partition: 3,
            note: Note {
                msgid: s("QVS1408"),
                text: s("SPCIE-SLOT-3 did not reset."),
                help: String::new(),
            },
            time: T,
        }),
        E::PowerOn(host::PowerOnChanged {
            power_on: power_on(),
            by: s("OPER1"),
            time: T,
        }),
    ]
    .into_iter()
    .map(AnyEvent::Host)
    .collect()
}

fn partitions_exchanges() -> Vec<(AnyRequest, AnyReply)> {
    use partitions::{Reply as P, Request as Q};
    let w = |q, p| (AnyRequest::Partitions(q), AnyReply::Partitions(p));
    vec![
        w(Q::List(Empty), P::List(vec![status(), idle_status()])),
        w(
            Q::Get(partitions::Get { id: 3 }),
            P::Get(partitions::Partition {
                definition: definition(),
                status: status(),
            }),
        ),
        w(
            Q::Adapters(partitions::Adapters { id: 0 }),
            P::Adapters(vec![partitions::Adapter {
                partition: 3,
                name: s("PROD"),
                profile: s("NORMAL"),
                slot: 5,
                switch: s("PRODNET"),
                vlan: 10,
                tagged: vec![20],
                mac: s("02:5a:3c:00:03:05"),
                required: true,
            }]),
        ),
        w(
            Q::Activations(Empty),
            P::Activations(vec![
                partitions::Activation {
                    id: 3,
                    since: Some(T - 3600),
                    profile: s("NORMAL"),
                },
                partitions::Activation {
                    id: 4,
                    since: None,
                    profile: s("INSTALL"),
                },
            ]),
        ),
    ]
}

fn partitions_events() -> Vec<AnyEvent> {
    use partitions::Event as E;
    vec![
        E::State(partitions::StateChanged {
            id: 3,
            name: s("PROD"),
            state: PartitionState::ShuttingDown,
            time: T,
        }),
        E::Resources(partitions::ResourcesChanged {
            id: 3,
            name: s("PROD"),
            current: Some(current()),
            time: T,
        }),
        E::Defined(partitions::Defined {
            id: 4,
            name: s("TEST"),
            exists: false,
            time: T,
        }),
        E::Channel(partitions::ChannelChanged {
            id: 3,
            name: s("PROD"),
            channel: Channel {
                responding: false,
                ..channel()
            },
            time: T,
        }),
    ]
    .into_iter()
    .map(AnyEvent::Partitions)
    .collect()
}

fn partition_exchanges() -> Vec<(AnyRequest, AnyReply)> {
    use partition::{Reply as P, Request as Q};
    let w = |q, p| (AnyRequest::Partition(q), AnyReply::Partition(p));
    vec![
        w(
            Q::Create(partition::Create { name: s("PROD") }),
            P::Create(Definition {
                default_profile: String::new(),
                auto_start: AutoStart::default(),
                ..definition()
            }),
        ),
        w(
            Q::Change(partition::Change {
                definition: Definition {
                    keylock: Keylock::Manual,
                    ..definition()
                },
            }),
            P::Change(Empty),
        ),
        w(
            Q::Delete(partition::Delete { id: 4, disks: true }),
            P::Delete(Empty),
        ),
        w(
            Q::Activate(partition::Activate {
                id: 3,
                profile: String::new(),
                source: Some(IplSource::B),
                keylock: Some(Keylock::Manual),
            }),
            P::Activate(current()),
        ),
        w(
            Q::ShutDown(partition::ShutDown {
                id: 3,
                kind: partition::ShutdownKind::Delayed,
                delay: 600,
                restart: true,
            }),
            P::ShutDown(Empty),
        ),
        w(Q::Restart(partition::Restart { id: 3 }), P::Restart(Empty)),
        w(
            Q::Entitlement(partition::Entitlement {
                id: 3,
                units: Some(250),
                sharing: Some(Sharing::Capped),
                weight: None,
            }),
            P::Entitlement(entitled()),
        ),
        w(
            Q::CreateWith(partition::CreateWith {
                id: Some(3),
                name: s("PROD"),
                settings: settings(),
            }),
            P::CreateWith(Definition {
                default_profile: String::new(),
                auto_start: AutoStart::default(),
                ..definition()
            }),
        ),
        w(
            Q::GetSettings(partition::GetSettings { id: 3 }),
            P::GetSettings(settings()),
        ),
        w(
            Q::SetSettings(partition::SetSettings {
                id: 3,
                settings: partition::Settings {
                    panel: false,
                    with_host: partition::WithHost::Immediate,
                    delay: 0,
                    ..settings()
                },
            }),
            P::SetSettings(Empty),
        ),
    ]
}

/// A partition's settings: its panel open to consoles, ended with the
/// host after a controlled power down of up to ten minutes.
fn settings() -> partition::Settings {
    partition::Settings {
        description: s("Production: order entry and warehouse"),
        panel: true,
        with_host: partition::WithHost::Delayed,
        delay: 600,
    }
}

/// [`current`] after its entitlement was changed to 2.50 units, capped.
fn entitled() -> Current {
    Current {
        units: 250,
        capped: true,
        weight: 0,
        ..current()
    }
}

fn partition_events() -> Vec<AnyEvent> {
    vec![AnyEvent::Partition(partition::Event::Entitlement(
        partition::EntitlementChanged {
            id: 3,
            name: s("PROD"),
            before: current(),
            current: entitled(),
            by: s("OPER1"),
            time: T,
        },
    ))]
}

fn profiles_exchanges() -> Vec<(AnyRequest, AnyReply)> {
    use profiles::{Reply as P, Request as Q};
    let w = |q, p| (AnyRequest::Profiles(q), AnyReply::Profiles(p));
    vec![
        w(
            Q::List(profiles::List { id: 3 }),
            P::List(vec![
                profiles::Entry {
                    name: s("NORMAL"),
                    revision: 7,
                    default: true,
                },
                profiles::Entry {
                    name: s("INSTALL"),
                    revision: 2,
                    default: false,
                },
            ]),
        ),
        w(
            Q::Get(profiles::Get {
                id: 3,
                name: s("NORMAL"),
            }),
            P::Get(profiles::Revised {
                profile: profile(),
                revision: 7,
            }),
        ),
        w(
            Q::Put(profiles::Put {
                id: 3,
                profile: profile(),
                revision: Some(7),
            }),
            P::Put(profiles::Saved { revision: 8 }),
        ),
        w(
            Q::Delete(profiles::Delete {
                id: 3,
                name: s("INSTALL"),
            }),
            P::Delete(Empty),
        ),
        w(
            Q::SetDefault(profiles::SetDefault {
                id: 3,
                name: s("NORMAL"),
            }),
            P::SetDefault(Empty),
        ),
        w(
            Q::SaveCurrent(profiles::SaveCurrent {
                id: 3,
                name: s("RUNNING"),
                revision: None,
            }),
            P::SaveCurrent(profiles::Saved { revision: 1 }),
        ),
        w(
            Q::Activated(profiles::Activated { id: 3 }),
            P::Activated(vec![
                profiles::ProfileUse {
                    name: s("INSTALL"),
                    last: None,
                },
                profiles::ProfileUse {
                    name: s("NORMAL"),
                    last: Some(T - 3600),
                },
            ]),
        ),
    ]
}

fn profiles_events() -> Vec<AnyEvent> {
    vec![AnyEvent::Profiles(profiles::Event::Changed(
        profiles::Changed {
            id: 3,
            name: s("NORMAL"),
            revision: Some(8),
            default: true,
            time: T,
        },
    ))]
}

fn panel_exchanges() -> Vec<(AnyRequest, AnyReply)> {
    use panel::{Reply as P, Request as Q};
    let w = |q, p| (AnyRequest::Panel(q), AnyReply::Panel(p));
    let info = |function, text: &str, manual, confirm| panel::FunctionInfo {
        function,
        text: s(text),
        manual,
        confirm,
    };
    vec![
        w(
            Q::Function(panel::Function {
                id: 3,
                function: panel::function::SELECT_IPL,
                args: panel::PanelArgs {
                    source: Some(IplSource::D),
                    ipl_type: Some(IplType::Attended),
                    keylock: None,
                },
            }),
            P::Function(panel::Shown {
                text: s("02  IPL source D, IPL type attended, keylock manual"),
            }),
        ),
        w(
            Q::Functions(panel::Functions { id: 0 }),
            P::Functions(vec![
                info(1, "Display the IPL source, type and keylock", false, false),
                info(3, "Restart the IPL", true, true),
                info(21, "Dedicated service tools", true, false),
            ]),
        ),
    ]
}

fn refcodes_exchanges() -> Vec<(AnyRequest, AnyReply)> {
    use refcodes::{Reply as P, Request as Q, Source};
    let w = |q, p| (AnyRequest::Refcodes(q), AnyReply::Refcodes(p));
    vec![w(
        Q::History(refcodes::History {
            id: 3,
            before: None,
            limit: 3,
        }),
        P::History(refcodes::Page {
            codes: vec![
                posted(41, "C6004500", "IPL complete", Source::System),
                posted(
                    40,
                    "C6001090",
                    "Storage management recovery",
                    Source::System,
                ),
                posted(
                    39,
                    "C2005090",
                    "Partition started: PROD, side A",
                    Source::Host,
                ),
            ],
            more: Some(39),
        }),
    )]
}

fn refcodes_events() -> Vec<AnyEvent> {
    vec![AnyEvent::Refcodes(refcodes::Event::Code(refcodes::Code {
        id: 3,
        name: s("PROD"),
        posted: posted(
            42,
            "A2009020",
            "Partition not responding: PROD",
            refcodes::Source::Host,
        ),
        current: true,
    }))]
}

fn dumps_exchanges() -> Vec<(AnyRequest, AnyReply)> {
    use dumps::{Reply as P, Request as Q};
    let w = |q, p| (AnyRequest::Dumps(q), AnyReply::Dumps(p));
    let name = format!("PROD-{T}.core");
    vec![
        w(
            Q::List(dumps::List { id: 3 }),
            P::List(vec![dumps::DumpInfo {
                name: name.clone(),
                time: T,
                bytes: 8_590_000_128,
                allocated: 568_000_000,
                code: s("B1001010"),
                reason: s("function34"),
            }]),
        ),
        w(
            Q::Take(dumps::Take { id: 3 }),
            P::Take(dumps::Begun { name: name.clone() }),
        ),
        w(
            Q::Delete(dumps::Delete {
                id: 3,
                name: name.clone(),
            }),
            P::Delete(Empty),
        ),
        w(
            Q::Download(dumps::Download {
                id: 3,
                name: name.clone(),
            }),
            P::Download(dumps::Downloading {
                name,
                bytes: 8_590_000_128,
            }),
        ),
        w(
            Q::Space(dumps::GetSpace { id: 0 }),
            P::Space(vec![
                dumps::PartitionSpace {
                    id: 3,
                    space: base::Space {
                        bytes: 568_000_000,
                        limit: 274_877_906_944,
                        dumps: 1,
                        max_dumps: 3,
                    },
                },
                dumps::PartitionSpace {
                    id: 4,
                    space: base::Space {
                        limit: 274_877_906_944,
                        max_dumps: 3,
                        ..base::Space::default()
                    },
                },
            ]),
        ),
    ]
}

fn dumps_events() -> Vec<AnyEvent> {
    use dumps::Event as E;
    vec![
        E::Taken(dumps::Taken {
            id: 3,
            name: s("PROD"),
            dump: format!("PROD-{T}.core"),
            error: Some(Note {
                msgid: s("QVS2311"),
                text: s("The dump of PROD could not be written: the pool is full."),
                help: String::new(),
            }),
            time: T,
        }),
        E::Removed(dumps::Removed {
            id: 3,
            dump: s("PROD-1794000000.core"),
            time: T,
        }),
    ]
    .into_iter()
    .map(AnyEvent::Dumps)
    .collect()
}

fn console_exchanges() -> Vec<(AnyRequest, AnyReply)> {
    use console::{Reply as P, Request as Q};
    let w = |q, p| (AnyRequest::Console(q), AnyReply::Console(p));
    vec![
        w(
            Q::Attach(console::Attach {
                id: 3,
                take_over: true,
            }),
            P::Attach(console::Attached { serial: false }),
        ),
        w(
            Q::Screen(console::Screen { id: 3 }),
            P::Screen(console::Capture {
                serial: false,
                frame: Some(frame()),
                text: s("                              Sign On"),
            }),
        ),
    ]
}

/// One of every kind a console's stream carries from the host.
#[must_use]
pub fn console_out() -> Vec<console::Out> {
    vec![
        console::Out::Frame(frame()),
        console::Out::Update(Update {
            seq: 8,
            runs: vec![Run {
                row: 5,
                col: 52,
                cells: cells("QSECOFR", 7),
            }],
            status: Cursor {
                row: 5,
                col: 59,
                shape: Shape::Underline,
                indicators: qsipart::console::indicator::INSERT,
                src: Some(SrcShown {
                    code: s("C6004500"),
                    text: s("IPL complete"),
                }),
            },
        }),
        console::Out::Serial(b"Quartz IPL\r\n".to_vec()),
        console::Out::Ended(s("taken over")),
    ]
}

/// One of every kind a console's stream carries to the host.
#[must_use]
pub fn console_in() -> Vec<console::In> {
    vec![
        console::In::Key(Key::Char('Q')),
        console::In::Key(Key::Function(3)),
        console::In::Refresh,
        console::In::Bytes(b"\r".to_vec()),
    ]
}

fn vres_exchanges() -> Vec<(AnyRequest, AnyReply)> {
    let mut out = vres_storage();
    out.extend(vres_pools());
    out.extend(vres_media());
    out.extend(vres_network());
    out.extend(vres_switches());
    out
}

/// Managing storage pools.
fn vres_pools() -> Vec<(AnyRequest, AnyReply)> {
    use vres::{Reply as P, Request as Q};
    let w = |q, p| (AnyRequest::Vres(q), AnyReply::Vres(p));
    let pool2 = || vres::StoragePool {
        name: s("POOL2"),
        protection: s("raid5"),
        state: s("Active"),
        work: String::new(),
        work_unit: 0,
        percent: 0,
        allocated: 0,
        provisioned: 0,
        repository: false,
        ..storage_pool()
    };
    let disks = || vec![s("nvme2n1"), s("nvme3n1"), s("nvme4n1")];
    vec![
        w(Q::Disks(Empty), P::Disks(pool_disks())),
        w(
            Q::PoolCreate(vres::PoolCreate {
                name: s("POOL2"),
                protection: vres::Protection::Raid5,
                disks: disks(),
                spares: vec![s("nvme5n1")],
                confirm: vec![s("nvme4n1")],
            }),
            P::PoolCreate(pool2()),
        ),
        w(
            Q::PoolDelete(vres::PoolDelete { name: s("OLDPOOL") }),
            P::PoolDelete(Empty),
        ),
        w(
            Q::PoolRename(vres::PoolRename {
                name: s("POOL2"),
                to: s("DATA"),
            }),
            P::PoolRename(Empty),
        ),
        w(
            Q::UnitReplace(vres::UnitReplace {
                pool: s("POOL1"),
                unit: 2,
                disk: s("nvme6n1"),
                confirm: false,
            }),
            P::UnitReplace(Empty),
        ),
        w(
            Q::SpareAdd(vres::SpareAdd {
                pool: s("POOL1"),
                disk: s("nvme7n1"),
                confirm: true,
            }),
            P::SpareAdd(Empty),
        ),
        w(
            Q::SpareRemove(vres::SpareRemove {
                pool: s("POOL1"),
                disk: s("nvme7n1"),
            }),
            P::SpareRemove(Empty),
        ),
        w(
            Q::Designate(vres::Designate { pool: s("POOL2") }),
            P::Designate(Empty),
        ),
        w(
            Q::PoolExtend(vres::PoolExtend {
                pool: s("POOL2"),
                disks: vec![s("nvme8n1")],
                confirm: Vec::new(),
            }),
            P::PoolExtend(pool2()),
        ),
    ]
}

/// A disk of a pool and a foreign one.
fn pool_disks() -> Vec<vres::PoolDisk> {
    vec![
        vres::PoolDisk {
            name: s("nvme0n1"),
            serial: s("S5GXNF0R401234"),
            location: s("SM-2-SLOT-1-N1"),
            bytes: 1_920_383_410_176,
            block_size: 512,
            what: s("POOL1 unit 1, Active"),
            pool: s("POOL1"),
            free: false,
            foreign: false,
        },
        vres::PoolDisk {
            name: s("nvme4n1"),
            serial: s("S5GXNF0R409999"),
            location: s("SM-2-SLOT-5-N1"),
            bytes: 1_920_383_410_176,
            block_size: 512,
            what: s("Foreign: an xfs file system"),
            pool: String::new(),
            free: false,
            foreign: true,
        },
    ]
}

/// Managing switches.
fn vres_switches() -> Vec<(AnyRequest, AnyReply)> {
    use vres::{Reply as P, Request as Q};
    let w = |q, p| (AnyRequest::Vres(q), AnyReply::Vres(p));
    vec![
        w(
            Q::SwitchCreate(vres::SwitchSpec {
                name: s("TESTNET"),
                vlans: vec![30],
                mtu: 0,
            }),
            P::SwitchCreate(vres::Switch {
                name: s("TESTNET"),
                id: 2,
                vlans: vec![30],
                mtu: 1500,
                uplink: None,
                uplink_state: s("None"),
                members: Vec::new(),
                ports: 0,
                failure: String::new(),
            }),
        ),
        w(
            Q::SwitchChange(vres::SwitchSpec {
                name: s("TESTNET"),
                vlans: vec![30, 31],
                mtu: 9000,
            }),
            P::SwitchChange(Empty),
        ),
        w(
            Q::SwitchRename(vres::SwitchRename {
                name: s("TESTNET"),
                to: s("DEVNET"),
            }),
            P::SwitchRename(Empty),
        ),
        w(
            Q::SwitchDelete(vres::SwitchDelete {
                name: s("OLDNET"),
                force: true,
            }),
            P::SwitchDelete(Empty),
        ),
        w(
            Q::UplinkSet(vres::UplinkSet {
                switch: s("DEVNET"),
                uplink: vres::Uplink {
                    interfaces: vec![s("SPCIE-SLOT-6-E0")],
                    bond: None,
                    native_vlan: Some(30),
                },
            }),
            P::UplinkSet(Empty),
        ),
        w(
            Q::UplinkClear(vres::UplinkClear {
                switch: s("DEVNET"),
            }),
            P::UplinkClear(Empty),
        ),
    ]
}

fn vres_storage() -> Vec<(AnyRequest, AnyReply)> {
    use vres::{Reply as P, Request as Q};
    let w = |q, p| (AnyRequest::Vres(q), AnyReply::Vres(p));
    vec![
        w(Q::Pools(Empty), P::Pools(vec![storage_pool()])),
        w(
            Q::Vdisks(vres::Vdisks { pool: s("POOL1") }),
            P::Vdisks(vec![vdisk()]),
        ),
        w(
            Q::VdiskCreate(vres::VdiskCreate {
                pool: s("POOL1"),
                name: s("PROD_LS"),
                mib: 102_400,
                thick: false,
            }),
            P::VdiskCreate(vres::Vdisk {
                allocated: 0,
                holder: Holder::default(),
                ..vdisk()
            }),
        ),
        w(
            Q::VdiskResize(vres::VdiskResize {
                pool: s("POOL1"),
                name: s("PROD_LS"),
                mib: 204_800,
            }),
            P::VdiskResize(vres::Vdisk {
                bytes: 214_748_364_800,
                holder: Holder::default(),
                ..vdisk()
            }),
        ),
        w(
            Q::VdiskDelete(vres::VdiskDelete {
                pool: s("POOL1"),
                name: s("OLD_D1"),
                force: true,
            }),
            P::VdiskDelete(Empty),
        ),
        w(
            Q::VdiskNamed(vres::VdiskNamed {
                pool: s("POOL1"),
                name: s("PROD_LS"),
            }),
            P::VdiskNamed(vec![vres::Naming {
                partition: 3,
                name: s("PROD"),
                profile: s("NORMAL"),
                slot: 2,
                lun: 0,
            }]),
        ),
    ]
}

fn vres_media() -> Vec<(AnyRequest, AnyReply)> {
    use vres::{Reply as P, Request as Q};
    let w = |q, p| (AnyRequest::Vres(q), AnyReply::Vres(p));
    vec![
        w(Q::Images(Empty), P::Images(vec![image()])),
        w(
            Q::ImageRename(vres::ImageRename {
                name: s("V1R2M0_BASE"),
                to: s("V1R2M0_SS1"),
            }),
            P::ImageRename(Empty),
        ),
        w(
            Q::ImageDelete(vres::ImageDelete {
                name: s("V1R1M0_BASE"),
            }),
            P::ImageDelete(Empty),
        ),
        w(
            Q::ImageUpload(vres::ImageUpload {
                name: s("V1R2M0_PTF"),
                bytes: 734_003_200,
            }),
            P::ImageUpload(vres::Uploading {
                name: s("V1R2M0_PTF"),
                bytes: 734_003_200,
            }),
        ),
        w(
            Q::Drives(vres::Drives { id: 3 }),
            P::Drives(vec![
                vres::Drive {
                    slot: 3,
                    kind: vres::DriveKind::Optical,
                    name: s("V1R2M0_BASE"),
                    prevented: false,
                },
                drive(),
                vres::Drive {
                    slot: 6,
                    kind: vres::DriveKind::PhysicalTape,
                    name: s(TAPE_DRIVE),
                    prevented: false,
                },
            ]),
        ),
        w(
            Q::Load(vres::Load {
                id: 3,
                slot: 4,
                name: s("SAVE01"),
            }),
            P::Load(Empty),
        ),
        w(
            Q::Unload(vres::Unload {
                id: 3,
                slot: 4,
                forced: false,
            }),
            P::Unload(vres::Unloaded { name: s("SAVE01") }),
        ),
        w(Q::Volumes(Empty), P::Volumes(vec![volume()])),
        w(
            Q::VolumeCreate(vres::VolumeCreate {
                name: s("SAVE01"),
                identifier: s("SAV001"),
                mib: 102_400,
            }),
            P::VolumeCreate(vres::Volume {
                used: 0,
                ..volume()
            }),
        ),
        w(
            Q::VolumeDelete(vres::VolumeDelete { name: s("SAVE00") }),
            P::VolumeDelete(Empty),
        ),
        w(
            Q::VolumeProtect(vres::VolumeProtect {
                name: s("SAVE01"),
                on: true,
            }),
            P::VolumeProtect(Empty),
        ),
        w(Q::TapeDrives(Empty), P::TapeDrives(vec![tape_drive()])),
    ]
}

fn tape_drive() -> vres::TapeDrive {
    vres::TapeDrive {
        location: s(TAPE_DRIVE),
        vendor: s("IBM"),
        product: s("ULT3580-HH9"),
        serial: s("10WT123456"),
        released: true,
        loaded: true,
        holder: hold(3, 6),
    }
}

fn vres_network() -> Vec<(AnyRequest, AnyReply)> {
    use vres::{Reply as P, Request as Q};
    let w = |q, p| (AnyRequest::Vres(q), AnyReply::Vres(p));
    vec![
        w(Q::Switches(Empty), P::Switches(vec![switch()])),
        w(
            Q::Ports(vres::Ports {
                switch: s("PRODNET"),
            }),
            P::Ports(vec![vres::Port {
                partition: 3,
                slot: 5,
                switch: s("PRODNET"),
                vlan: 10,
                tagged: vec![20],
                mac: s("02:5a:3c:00:03:05"),
                link: true,
                counters: vres::Counters {
                    sent_bytes: 91_234_567,
                    sent_packets: 120_455,
                    sent_dropped: 0,
                    received_bytes: 1_203_998_112,
                    received_packets: 880_120,
                    received_dropped: 12,
                },
            }]),
        ),
        w(
            Q::Fdb(vres::Fdb {
                switch: s("PRODNET"),
            }),
            P::Fdb(vec![
                vres::FdbRow {
                    mac: s("02:5a:3c:00:03:05"),
                    vlan: 10,
                    port: s("p3s5"),
                    partition: 3,
                    slot: 5,
                    origin: s("static"),
                },
                vres::FdbRow {
                    mac: s("00:1b:21:aa:bb:cc"),
                    vlan: 10,
                    port: s("uplink"),
                    partition: 0,
                    slot: 0,
                    origin: s("learned"),
                },
            ]),
        ),
        w(
            Q::Vlans(Empty),
            P::Vlans(vec![vres::VlanUse {
                switch: s("PRODNET"),
                vlan: 20,
                native: false,
                ports: vec![vres::VlanPort {
                    partition: 3,
                    slot: 5,
                    tagged: true,
                }],
            }]),
        ),
        w(
            Q::Limits(Empty),
            P::Limits(vres::Limits {
                switches: 16,
                vlans_per_switch: 256,
                adapters_per_partition: 8,
                ports_per_switch: 256,
            }),
        ),
        w(
            Q::ProcPoolCreate(vres::ProcPoolSpec {
                name: s("BATCH"),
                cores: vec![14, 15],
                maximum_units: Some(150),
            }),
            P::ProcPoolCreate(Empty),
        ),
        w(
            Q::ProcPoolChange(vres::ProcPoolSpec {
                name: s("BATCH"),
                cores: vec![13, 14, 15],
                maximum_units: None,
            }),
            P::ProcPoolChange(Empty),
        ),
        w(
            Q::ProcPoolDelete(vres::ProcPoolDelete { name: s("BATCH") }),
            P::ProcPoolDelete(Empty),
        ),
    ]
}

fn vres_events() -> Vec<AnyEvent> {
    use vres::Event as E;
    vec![
        E::Pool(vres::PoolChanged {
            name: s("POOL1"),
            pool: Some(storage_pool()),
            time: T,
        }),
        E::Vdisk(vres::VdiskChanged {
            pool: s("POOL1"),
            name: s("OLD_D1"),
            vdisk: None,
            time: T,
        }),
        E::Image(vres::ImageChanged {
            name: s("V1R2M0_BASE"),
            image: Some(image()),
            time: T,
        }),
        E::Upload(vres::UploadEnded {
            name: s("V1R2M0_PTF"),
            image: None,
            error: Some(Note {
                msgid: s("QVS3206"),
                text: s("V1R2M0_PTF is not an ISO 9660 image."),
                help: String::new(),
            }),
            time: T,
        }),
        E::Volume(vres::VolumeChanged {
            name: s("SAVE01"),
            volume: Some(volume()),
            time: T,
        }),
        E::Drive(vres::DriveChanged {
            id: 3,
            drive: drive(),
            time: T,
        }),
        E::Switch(vres::SwitchChanged {
            name: s("PRODNET"),
            switch: Some(switch()),
            time: T,
        }),
        E::Uplink(vres::UplinkChanged {
            switch: s("PRODNET"),
            location: s("SPCIE-SLOT-2-E1"),
            carrier: false,
            state: s("Degraded"),
            time: T,
        }),
        E::Port(vres::PortChanged {
            partition: 3,
            slot: 5,
            switch: s("PRODNET"),
            open: true,
            time: T,
        }),
        E::TapeDrive(vres::TapeDriveChanged {
            location: s(TAPE_DRIVE),
            drive: Some(tape_drive()),
            time: T,
        }),
    ]
    .into_iter()
    .map(AnyEvent::Vres)
    .collect()
}
