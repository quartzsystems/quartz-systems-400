// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The virtual volume format: frames, tape marks, positioning, capacity,
//! early warning, write protection and torn writes.

use std::io::Cursor;

use tape::virt::format::{FRAME_LEN, HEADER_LEN};
use tape::{
    MAX_BLOCK, Position, ReadOutcome, TapeTransport, TransportError, VirtualTape, VolumeOptions,
};

type Mem = VirtualTape<Cursor<Vec<u8>>>;

fn opts(capacity: u64) -> VolumeOptions {
    VolumeOptions {
        capacity,
        density: "*VRT256K".into(),
        write_protect: false,
        tag: "VOL001".into(),
    }
}

fn new(capacity: u64) -> Mem {
    VirtualTape::create(Cursor::new(Vec::new()), &opts(capacity)).unwrap()
}

fn reopen(t: Mem) -> Mem {
    VirtualTape::open(Cursor::new(t.into_inner().into_inner())).unwrap()
}

fn block(seed: u32, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| u8::try_from(u32::try_from(i).unwrap().wrapping_mul(31).wrapping_add(seed) % 251).unwrap())
        .collect()
}

/// Read everything from the position: blocks as lengths, marks as `None`.
fn drain(t: &mut Mem) -> Vec<Option<Vec<u8>>> {
    let mut buf = vec![0u8; MAX_BLOCK];
    let mut out = Vec::new();
    loop {
        match t.read_block(&mut buf).unwrap() {
            ReadOutcome::Block(n) => out.push(Some(buf[..n].to_vec())),
            ReadOutcome::TapeMark => out.push(None),
            ReadOutcome::EndOfData => return out,
        }
    }
}

#[test]
fn blocks_and_marks_round_trip_through_reopen() {
    let mut t = new(1 << 22);
    let a = block(1, 100);
    let b = block(2, MAX_BLOCK);
    t.write_block(&a).unwrap();
    t.write_eof(1).unwrap();
    t.write_block(&b).unwrap();
    t.write_eof(2).unwrap();
    let mut t = reopen(t);
    let want = vec![Some(a), None, Some(b), None, None];
    assert_eq!(drain(&mut t), want);
    t.rewind().unwrap();
    assert_eq!(drain(&mut t), want);
    assert_eq!(t.header().tag, "VOL001");
}

#[test]
fn spacing_follows_st() {
    let mut t = new(1 << 20);
    // file 0: two blocks; file 1: one block; file 2: empty.
    for b in [1, 2] {
        t.write_block(&block(b, 10)).unwrap();
    }
    t.write_eof(1).unwrap();
    t.write_block(&block(3, 10)).unwrap();
    t.write_eof(2).unwrap();
    t.rewind().unwrap();
    let st = t.status().unwrap();
    assert!(st.bot && st.loaded && !st.eod);
    t.fsf(1).unwrap();
    assert_eq!(t.status().unwrap().position, Position { file: 1, block: 0 });
    t.fsr(1).unwrap();
    assert_eq!(t.status().unwrap().position, Position { file: 1, block: 1 });
    // Over the mark: stops past it and says so.
    assert!(matches!(t.fsr(1), Err(TransportError::TapeMark)));
    assert_eq!(t.status().unwrap().position, Position { file: 2, block: 0 });
    // bsf leaves the drive before the mark.
    t.bsf(1).unwrap();
    assert_eq!(t.status().unwrap().position, Position { file: 1, block: 1 });
    t.bsf(1).unwrap();
    assert_eq!(t.status().unwrap().position, Position { file: 0, block: 2 });
    assert!(matches!(t.bsf(1), Err(TransportError::BeginningOfTape)));
    assert!(t.status().unwrap().bot);
    t.fsf(3).unwrap();
    assert!(t.status().unwrap().eod);
    assert!(matches!(t.fsf(1), Err(TransportError::EndOfData)));
    t.bsr(0).unwrap();
    t.rewind().unwrap();
    t.fsr(2).unwrap();
    assert!(matches!(t.bsr(3), Ok(()) | Err(TransportError::BeginningOfTape)));
    t.eom().unwrap();
    assert!(t.status().unwrap().eod);
}

#[test]
fn a_write_truncates_what_follows() {
    let mut t = new(1 << 20);
    for b in 0..5 {
        t.write_block(&block(b, 50)).unwrap();
    }
    t.rewind().unwrap();
    t.fsr(2).unwrap();
    t.write_block(&block(9, 20)).unwrap();
    let mut t = reopen(t);
    let got = drain(&mut t);
    assert_eq!(
        got,
        vec![Some(block(0, 50)), Some(block(1, 50)), Some(block(9, 20))]
    );
}

#[test]
fn stale_frames_never_rejoin_after_an_overwrite() {
    // A long chain, then a shorter overwrite from the start, reopened
    // several times: the old tail must stay gone.
    let mut t = new(1 << 20);
    for b in 0..20 {
        t.write_block(&block(b, 64)).unwrap();
    }
    t.rewind().unwrap();
    t.write_block(&block(100, 64)).unwrap();
    let mut t = reopen(t);
    assert_eq!(drain(&mut t), vec![Some(block(100, 64))]);
    // Append in a new session: blocks after it are still the new ones.
    t.write_block(&block(101, 64)).unwrap();
    let mut t = reopen(t);
    assert_eq!(drain(&mut t), vec![Some(block(100, 64)), Some(block(101, 64))]);
}

#[test]
fn a_torn_last_block_is_not_there() {
    for cut in [1, FRAME_LEN - 1, FRAME_LEN, FRAME_LEN + 1, FRAME_LEN + 499] {
        let mut t = new(1 << 20);
        t.write_block(&block(1, 300)).unwrap();
        t.write_eof(1).unwrap();
        let before = t.used();
        t.write_block(&block(2, 500)).unwrap();
        let full = t.into_inner().into_inner();
        let keep = HEADER_LEN + usize::try_from(before).unwrap() + cut;
        let mut bytes = full.clone();
        bytes.truncate(keep);
        let mut t = VirtualTape::open(Cursor::new(bytes)).unwrap();
        assert_eq!(t.used(), before, "cut {cut}");
        assert_eq!(drain(&mut t), vec![Some(block(1, 300)), None], "cut {cut}");
        // Writing again at the end of data replaces the torn bytes.
        t.write_block(&block(3, 40)).unwrap();
        let mut t = reopen(t);
        assert_eq!(drain(&mut t), vec![Some(block(1, 300)), None, Some(block(3, 40))]);
    }
}

#[test]
fn a_corrupt_block_ends_the_data() {
    let mut t = new(1 << 20);
    for b in 0..3 {
        t.write_block(&block(b, 100)).unwrap();
    }
    let mut bytes = t.into_inner().into_inner();
    // Flip a payload byte of the second frame.
    bytes[HEADER_LEN + (FRAME_LEN + 100) + FRAME_LEN + 7] ^= 0xff;
    let mut t = VirtualTape::open(Cursor::new(bytes)).unwrap();
    assert_eq!(drain(&mut t), vec![Some(block(0, 100))]);
}

#[test]
fn capacity_early_warning_and_end_of_medium() {
    let cap = 64 * 1024;
    let mut t = new(cap);
    let ew = t.header().early_warning;
    assert!(ew < cap && ew >= cap / 2);
    let mut warned = false;
    let mut written = 0u64;
    loop {
        match t.write_block(&block(0, 1000)) {
            Ok(w) => {
                written += (FRAME_LEN + 1000) as u64;
                assert_eq!(w.early_warning, written > ew);
                warned |= w.early_warning;
            }
            Err(TransportError::EndOfMedium) => break,
            Err(e) => panic!("{e}"),
        }
    }
    assert!(warned);
    assert!(t.used() <= cap);
    assert!(t.used() + (FRAME_LEN + 1000) as u64 > cap);
    assert!(t.status().unwrap().eot);
    // A mark still fits when a block does not, until it too does not.
    let mut marks = 0;
    while t.write_eof(1).is_ok() {
        marks += 1;
    }
    assert!(marks <= 1000 / FRAME_LEN + 1);
    assert!(t.used() <= cap);
}

#[test]
fn write_protect_refuses_writes_and_survives_reopen() {
    let mut t = new(1 << 20);
    t.write_block(&block(0, 10)).unwrap();
    t.set_write_protect(true).unwrap();
    let mut t = reopen(t);
    assert!(t.status().unwrap().write_protected);
    t.rewind().unwrap();
    assert!(matches!(t.write_block(&[1]), Err(TransportError::WriteProtected)));
    assert!(matches!(t.write_eof(1), Err(TransportError::WriteProtected)));
    assert!(matches!(t.erase(), Err(TransportError::WriteProtected)));
    assert_eq!(drain(&mut t), vec![Some(block(0, 10))]);
    let mut wp = VolumeOptions { write_protect: true, ..opts(1 << 20) };
    wp.tag = "WP0001".into();
    let mut t = VirtualTape::create(Cursor::new(Vec::new()), &wp).unwrap();
    assert!(matches!(t.write_block(&[1]), Err(TransportError::WriteProtected)));
}

#[test]
fn unload_block_size_density_and_erase() {
    let mut t = new(1 << 20);
    t.write_block(&block(0, 10)).unwrap();
    t.unload().unwrap();
    assert!(!t.status().unwrap().loaded);
    assert!(matches!(t.rewind(), Err(TransportError::NotLoaded)));
    t.load();
    t.set_block_size(512).unwrap();
    assert!(matches!(t.write_block(&[0; 100]), Err(TransportError::BlockSize { .. })));
    t.write_block(&[0; 512]).unwrap();
    t.set_block_size(0).unwrap();
    assert!(t.set_block_size(u32::try_from(MAX_BLOCK).unwrap() + 1).is_err());
    t.rewind().unwrap();
    t.set_density("*VRT64K").unwrap();
    assert!(t.set_density("TOO LONG DENSITY NAME").is_err());
    t.erase().unwrap();
    let mut t = reopen(t);
    assert_eq!(t.status().unwrap().density, "*VRT64K");
    assert_eq!(drain(&mut t), vec![]);
    let mut small = vec![0u8; 4];
    t.write_block(&[7; 8]).unwrap();
    t.rewind().unwrap();
    assert!(matches!(t.read_block(&mut small), Err(TransportError::BlockTooLarge { len: 8, max: 4 })));
    assert!(matches!(t.write_block(&[]), Err(TransportError::BlockTooLarge { .. })));
}

#[test]
fn headers_are_checked() {
    let t = new(1 << 20);
    let mut bytes = t.into_inner().into_inner();
    bytes[20] ^= 1;
    assert!(matches!(
        VirtualTape::open(Cursor::new(bytes)),
        Err(TransportError::Format(_))
    ));
    assert!(VirtualTape::open(Cursor::new(vec![0u8; 100])).is_err());
    assert_eq!(VolumeOptions::imgsiz(1000, "X").capacity, 1000 * 1024 * 1024);
}

/// A table of random-ish sessions: whatever is written and then cut
/// anywhere reopens to a prefix of the frames written.
#[test]
fn any_cut_reopens_to_a_prefix() {
    let mut seed = 0x2545_f491u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    for _ in 0..40 {
        let mut t = new(1 << 20);
        let mut frames = Vec::new();
        for _ in 0..=(next() % 12) {
            if next() % 4 == 0 {
                t.write_eof(1).unwrap();
                frames.push(None);
            } else {
                let b = block(next(), (next() % 2000 + 1) as usize);
                t.write_block(&b).unwrap();
                frames.push(Some(b));
            }
        }
        let bytes = t.into_inner().into_inner();
        let cut = HEADER_LEN + (next() as usize % (bytes.len() - HEADER_LEN + 1));
        let mut t = VirtualTape::open(Cursor::new(bytes[..cut].to_vec())).unwrap();
        let got = drain(&mut t);
        assert!(got.len() <= frames.len());
        assert_eq!(got[..], frames[..got.len()]);
    }
}
