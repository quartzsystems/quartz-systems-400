// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Delete disk unit data on a disk holding a QS/400 image: no GPT, no
//! partition head and no unit header survive.

use blockdev::{BLOCK_SIZE, BlockDev, BlockError, MemBlockDev};
use diskimg::wipe::{EDGE_BYTES, wipe};
use diskimg::xfsfmt::SparseDev;
use diskimg::{Guids, MIN_DISK_BYTES, Plan, fat, write_image};

const BS: u64 = BLOCK_SIZE as u64;
const MARK: &[u8] = b"QS400 UNIT HEADER";

/// A QS/400 disk in memory, with a unit header stand-in at the head of
/// ASP 1, and its plan and written pages' offsets.
fn qs400_disk() -> (MemBlockDev, Plan, Vec<u64>) {
    let plan = Plan::for_bytes(MIN_DISK_BYTES).unwrap();
    let bytes = plan.total_sectors * 512;
    let mut img = SparseDev::new(bytes);
    let esp = fat::build(&fat::DISK_ESP, &[("EFI/BOOT/BOOTX64.EFI", b"uki")]).unwrap();
    write_image(&mut img, &plan, &Guids::FIXED, &esp).unwrap();
    let dev = MemBlockDev::new(bytes / BS);
    let mut written = Vec::new();
    for (at, page) in img.pages() {
        let page = &page[..usize::try_from((bytes - at).min(page.len() as u64)).unwrap()];
        dev.write(at / BS, page).unwrap();
        written.push(at);
    }
    let mut head = vec![0u8; BLOCK_SIZE as usize];
    head[..MARK.len()].copy_from_slice(MARK);
    dev.write(plan.asp1.offset() / BS, &head).unwrap();
    dev.flush().unwrap();
    (dev, plan, written)
}

fn sector(dev: &dyn BlockDev, lba: u64) -> Vec<u8> {
    let mut b = vec![0u8; BLOCK_SIZE as usize];
    dev.read(lba * 512 / BS, &mut b).unwrap();
    let off = usize::try_from(lba * 512 % BS).unwrap();
    b[off..off + 512].to_vec()
}

fn block_is_zero(dev: &dyn BlockDev, lba: u64) -> bool {
    let mut b = vec![0u8; BLOCK_SIZE as usize];
    dev.read(lba, &mut b).unwrap();
    b.iter().all(|&x| x == 0)
}

/// No GPT header at either end, and the partition heads are zeros.
fn assert_blank_signatures(dev: &dyn BlockDev, plan: &Plan) {
    let last = dev.blocks() * BS / 512 - 1;
    assert_ne!(&sector(dev, 1)[..8], b"EFI PART", "primary GPT");
    assert_ne!(&sector(dev, last)[..8], b"EFI PART", "backup GPT");
    for part in [&plan.esp, &plan.lic, &plan.asp1] {
        assert!(block_is_zero(dev, part.offset() / BS), "head of {part:?}");
    }
}

#[test]
fn a_qs400_disk_comes_out_blank() {
    let (dev, plan, written) = qs400_disk();
    let last = dev.blocks() * BS / 512 - 1;
    assert_eq!(&sector(&dev, 1)[..8], b"EFI PART");
    assert_eq!(&sector(&dev, last)[..8], b"EFI PART");
    let mut seen = Vec::new();
    let w = wipe(&dev, &mut |p| seen.push(p)).unwrap();
    assert_eq!(w.partitions, 3);
    assert_eq!(w.discarded, dev.blocks());
    assert_eq!(seen.first(), Some(&0));
    assert_eq!(seen.last(), Some(&100));
    assert!(seen.windows(2).all(|p| p[0] <= p[1]));
    assert_blank_signatures(&dev, &plan);
    // Discarded blocks read as zeros: nothing the image wrote is left.
    for at in written {
        assert!(block_is_zero(&dev, at / BS), "page at {at}");
    }
}

/// A drive without discard: only the zero writes count.
struct NoDiscard(MemBlockDev);

impl BlockDev for NoDiscard {
    fn block_size(&self) -> u32 {
        self.0.block_size()
    }
    fn blocks(&self) -> u64 {
        self.0.blocks()
    }
    fn read(&self, lba: u64, buf: &mut [u8]) -> blockdev::Result<()> {
        self.0.read(lba, buf)
    }
    fn write(&self, lba: u64, buf: &[u8]) -> blockdev::Result<()> {
        self.0.write(lba, buf)
    }
    fn flush(&self) -> blockdev::Result<()> {
        self.0.flush()
    }
    fn discard(&self, lba: u64, _count: u64) -> blockdev::Result<()> {
        Err(BlockError::Io {
            op: "discard",
            lba,
            source: std::io::Error::other("not supported"),
        })
    }
}

#[test]
fn without_discard_the_tables_and_headers_still_go() {
    let (dev, plan, _) = qs400_disk();
    let dev = NoDiscard(dev);
    let w = wipe(&dev, &mut |_| {}).unwrap();
    assert_eq!(w.discarded, 0);
    assert_eq!(w.partitions, 3);
    // Both edges and three 1 MiB heads (the ESP's lies in the first edge).
    assert_eq!(w.zeroed, 2 * EDGE_BYTES / BS + 2 * (1 << 20) / BS);
    assert_blank_signatures(&dev, &plan);
    let mut b = vec![0u8; BLOCK_SIZE as usize];
    dev.read(plan.asp1.offset() / BS, &mut b).unwrap();
    assert!(!b.starts_with(MARK));
}
