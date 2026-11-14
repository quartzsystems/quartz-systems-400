// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Whole images pinned by a CRC-64 of every written page: the output
//! without reflink must stay what it was before reflink was added.

use xfsfmt::{Options, SparseDev, Timestamp, format};

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
const TIB: u64 = 1024 * GIB;
/// Fixed AG sizes: 64 MiB and 6.25 GiB.
const AG64M: Option<u32> = Some(16_384);
const AG6G: Option<u32> = Some(1_638_400);

fn image_crc(bytes: u64, o: &Options) -> u64 {
    let mut dev = SparseDev::new(bytes);
    format(&mut dev, bytes, o).unwrap();
    let crc = crc::Crc::<u64>::new(&crc::CRC_64_ECMA_182);
    let mut d = crc.digest();
    for (at, page) in dev.pages() {
        d.update(&at.to_be_bytes());
        d.update(page);
    }
    d.finalize()
}

fn opts(sector_size: u32, zeroed: bool, ag_blocks: Option<u32>) -> Options {
    let mut o = Options::new(
        uuid::uuid!("51533430-0000-4000-8000-000000000005"),
        Timestamp {
            secs: 1_792_854_247,
            nsecs: 13_483_000,
        },
    );
    o.sector_size = sector_size;
    o.zeroed = zeroed;
    o.ag_blocks = ag_blocks;
    o
}

/// `(bytes, sector, zeroed, ag_blocks, crc)`, taken from the formatter
/// before reflink existed (commit 297771eec).
const PINNED: &[(u64, u32, bool, Option<u32>, u64)] = &[
    (64 * MIB, 512, false, None, 0x2222_82af_aa15_d0cf),
    (64 * MIB, 4096, false, None, 0xd7b1_40a9_a441_d502),
    (100 * MIB + 12345, 512, true, None, 0x1810_d1df_a651_03d1),
    (200 * MIB, 4096, true, None, 0x3ea8_c8fa_9533_e955),
    (512 * MIB, 512, false, None, 0x44b4_dbce_bbb0_f13c),
    (777 * MIB + 12345, 4096, true, None, 0x19b0_15fd_9472_b563),
    (GIB + 5 * MIB, 4096, true, AG64M, 0x5358_8bc5_5094_d0d9),
    (20 * GIB, 512, true, None, 0x538c_bf45_75b6_8d31),
    (300 * GIB, 4096, true, AG6G, 0x723f_00b5_2c86_f8a8),
    (5 * TIB, 4096, true, None, 0x2043_8401_7198_6cd6),
    (64 * TIB, 4096, true, None, 0xcbf2_b299_5c4e_64d9),
];

#[test]
fn without_reflink_the_output_is_unchanged() {
    let mut wrong = Vec::new();
    for &(bytes, sect, zeroed, ag, want) in PINNED {
        let got = image_crc(bytes, &opts(sect, zeroed, ag));
        if got != want {
            wrong.push(format!("({bytes}, {sect}, {zeroed}, {ag:?}) = {got:#018x}"));
        }
    }
    assert!(wrong.is_empty(), "changed images:\n{}", wrong.join("\n"));
}
