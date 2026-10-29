// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Formatting whole devices in memory.

use xfsfmt::{Error, Options, SparseDev, Timestamp, format, probe};

const MIB: u64 = 1024 * 1024;
const T: Timestamp = Timestamp {
    secs: 1_792_854_247,
    nsecs: 0,
};

fn opts() -> Options {
    Options::new(uuid::uuid!("51533430-0000-4000-8000-000000000005"), T)
}

fn formatted(bytes: u64, o: &Options) -> SparseDev {
    let mut dev = SparseDev::new(bytes);
    format(&mut dev, bytes, o).unwrap();
    dev
}

fn same(a: &SparseDev, b: &SparseDev) -> bool {
    a.pages().eq(b.pages())
}

#[test]
fn probe_reads_back_the_superblock() {
    let mut dev = formatted(64 * MIB, &opts());
    let info = probe(&mut dev).unwrap().unwrap();
    assert_eq!(info.label, "QS400-IFS");
    assert_eq!(info.uuid, opts().uuid);
    assert_eq!(
        (info.block_size, info.dblocks, info.ag_count),
        (4096, 16384, 4)
    );
    assert_eq!(info.fdblocks, 13796);
    assert!(probe(&mut SparseDev::new(MIB)).unwrap().is_none());
}

#[test]
fn output_is_deterministic() {
    for sector_size in [512, 4096] {
        let o = Options {
            sector_size,
            zeroed: true,
            ..opts()
        };
        assert!(same(&formatted(1024 * MIB, &o), &formatted(1024 * MIB, &o)));
    }
    let a = formatted(64 * MIB, &opts());
    let b = formatted(64 * MIB, &Options::new(uuid::Uuid::from_u128(7), T));
    assert!(!same(&a, &b));
}

#[test]
fn zeroed_skips_only_the_log_clear() {
    let o = opts();
    let z = Options {
        zeroed: true,
        ..opts()
    };
    let (a, b) = (formatted(512 * MIB, &o), formatted(512 * MIB, &z));
    // Same bytes; the uncleared device simply has fewer pages written.
    for (at, page) in b.pages() {
        assert_eq!(a.read_at(at, page.len()), page);
    }
    assert!(a.pages().count() > b.pages().count());
    for (at, page) in a.pages() {
        assert_eq!(b.read_at(at, page.len()), page);
    }
}

#[test]
fn a_reformat_replaces_everything_it_wrote() {
    let mut dev = formatted(64 * MIB, &Options::new(uuid::Uuid::from_u128(1), T));
    format(&mut dev, 64 * MIB, &opts()).unwrap();
    assert!(same(&dev, &formatted(64 * MIB, &opts())));
}

#[test]
fn refusals() {
    let mut dev = SparseDev::new(64 * MIB);
    let long = Options {
        label: "THIRTEEN-CHAR".into(),
        ..opts()
    };
    assert!(matches!(
        format(&mut dev, 64 * MIB, &long),
        Err(Error::Label(13))
    ));
    assert!(matches!(
        format(&mut dev, 32 * MIB, &opts()),
        Err(Error::TooSmall { .. })
    ));
    let odd = Options {
        sector_size: 2048,
        ..opts()
    };
    assert!(matches!(
        format(&mut dev, 64 * MIB, &odd),
        Err(Error::SectorSize(2048))
    ));
}
