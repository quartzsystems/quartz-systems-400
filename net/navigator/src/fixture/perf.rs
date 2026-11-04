// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sample system's activity over a ten-second interval.

use crate::env::perf::{DiskActivity, JobActivity, Perf, PoolActivity, SysActivity, UnitIo};

/// The sample system's performance figures.
#[derive(Default)]
pub struct FixturePerf {}

/// Number, user, name, type, CPU tenths, I/O, transactions.
const JOBS: &[(&str, &str, &str, &str, u32, u64, u64)] = &[
    ("123501", "QPGMR", "NIGHTLY", "BCH", 148, 812, 0),
    ("123502", "QPGMR", "RGZPFM01", "BCH", 61, 1_420, 0),
    ("123411", "QUSER", "QZDASOINIT", "PJ", 22, 88, 14),
    ("123488", "TESTUSER", "QPADEV0003", "INT", 12, 31, 3),
    ("123303", "QSYS", "QDBSRV03", "SYS", 6, 212, 0),
    ("123456", "QTMHHTTP", "ADMIN", "BCH", 3, 17, 0),
    ("123462", "QTMHHTTP", "NAVIGATOR", "BCH", 3, 9, 0),
    ("123381", "QTCP", "QTVTELNET", "BCH", 1, 2, 0),
    ("123105", "QSYS", "QSPLMAINT", "SYS", 1, 40, 0),
    ("123200", "QSYS", "QINTER", "SBS", 0, 0, 0),
];

/// Number, name, size MB, reserved MB, activity level, DB and other
/// faults (tenths), the three transition counts.
type Pool = (u32, &'static str, u64, u64, Option<u32>, u32, u32, u64);

const POOLS: &[Pool] = &[
    (1, "*MACHINE", 8_192, 1_204, None, 0, 12, 0),
    (2, "*BASE", 32_768, 412, Some(100), 31, 87, 1_204),
    (3, "*INTERACT", 8_192, 0, Some(40), 2, 9, 88),
    (4, "*SPOOL", 1_024, 0, Some(5), 0, 1, 2),
    (5, "QHTTPSVR", 8_192, 0, Some(60), 0, 4, 31),
    (6, "HLINV", 7_168, 0, Some(20), 18, 6, 9),
];

/// Unit, ASP, busy, reads, writes (tenths a second), KB read and
/// written a second, service and wait (tenths of ms), queue.
type Unit = (u32, u32, u32, u32, u32, u64, u64, u32, u32, u32);

const UNITS: &[Unit] = &[
    (1, 1, 182, 410, 885, 1_312, 2_832, 14, 1, 0),
    (2, 1, 179, 402, 885, 1_286, 2_832, 14, 1, 0),
    (3, 1, 224, 601, 900, 1_923, 2_880, 15, 2, 1),
    (4, 1, 220, 598, 900, 1_914, 2_880, 15, 2, 1),
    (5, 1, 91, 203, 301, 650, 963, 12, 0, 0),
    (6, 1, 90, 201, 301, 643, 963, 12, 0, 0),
    (7, 1, 713, 4_025, 1_202, 12_880, 3_846, 39, 21, 4),
    (8, 1, 708, 4_019, 1_202, 12_861, 3_846, 39, 21, 4),
    (9, 2, 42, 110, 61, 352, 195, 10, 0, 0),
    (10, 33, 21, 52, 30, 166, 96, 9, 0, 0),
    (11, 33, 20, 51, 30, 163, 96, 9, 0, 0),
];

impl Perf for FixturePerf {
    fn system(&self, _user: &str) -> SysActivity {
        SysActivity {
            elapsed: 10,
            cpu: 234,
            cpu_db: 91,
            interactive: 14,
            overhead: 20,
            jobs: JOBS
                .iter()
                .map(|(n, u, j, k, cpu, io, tx)| JobActivity {
                    number: (*n).into(),
                    user: (*u).into(),
                    name: (*j).into(),
                    kind: (*k).into(),
                    cpu: *cpu,
                    io: *io,
                    transactions: *tx,
                })
                .collect(),
            pools: POOLS
                .iter()
                .map(|(n, name, size, res, act, db, other, aw)| PoolActivity {
                    number: *n,
                    name: (*name).into(),
                    size_mb: *size,
                    reserved_mb: *res,
                    activity: *act,
                    db_faults: *db,
                    other_faults: *other,
                    act_wait: *aw,
                    wait_inel: 0,
                    act_inel: 0,
                })
                .collect(),
        }
    }

    fn disks(&self, _user: &str) -> DiskActivity {
        DiskActivity {
            elapsed: 10,
            units: UNITS
                .iter()
                .map(|(u, asp, busy, r, w, rk, wk, svc, wait, q)| UnitIo {
                    unit: *u,
                    resource: format!("DD{u:03}"),
                    asp: *asp,
                    busy: *busy,
                    reads: *r,
                    writes: *w,
                    read_kb: *rk,
                    write_kb: *wk,
                    service: *svc,
                    wait: *wait,
                    queue: *q,
                    top_job: if *busy >= 700 {
                        "123502/QPGMR/RGZPFM01".into()
                    } else {
                        String::new()
                    },
                })
                .collect(),
        }
    }
}
