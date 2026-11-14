// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The path past the early warning through the `st` driver's answers
//! (`StSim`): the driver's own return codes, `StTransport` over them in
//! variable, asynchronous and fixed block modes, and labeled files
//! across two and three cartridges with the EOV labels between.

#![allow(clippy::cast_possible_truncation, clippy::many_single_char_names)]

use tape::label::{Code, Date, Expiry, FileLabel, LabelSet, RecordFormat};
use tape::st::ioctl::{GMT_EOD, GMT_EOT, MtOp, StIoctl};
use tape::st::{SimCartridge, SimRecord, StSim, StTransport};
use tape::volume::{
    Check, FileReader, FileWriter, InzOptions, ReadOptions, WriteOptions, WriteSummary, inztap,
};
use tape::{
    EndOpt, MAX_BLOCK, ReadOutcome, SeqNbr, TapeError, TapeTransport, TransportError,
    VolumeProvider, VolumeRequest,
};

const KB: u64 = 1024;
const ENOSPC: i32 = 28;

type Drive = StTransport<StSim>;

fn enospc(r: &std::io::Result<usize>) -> bool {
    matches!(r, Err(e) if e.raw_os_error() == Some(ENOSPC))
}

/// Block `i`: its number in the first bytes, so a lost or doubled block
/// shows.
fn block(i: usize, len: usize) -> Vec<u8> {
    let mut b = vec![u8::try_from(i % 251).unwrap(); len];
    b[..8].copy_from_slice(&(i as u64).to_be_bytes());
    b
}

#[test]
fn sim_answers_as_st_does_past_the_early_warning() {
    // st.rst: the write that reaches the early warning returns its
    // count; then ENOSPC and a count alternate up to the physical end.
    let mut s = StSim::new(vec![SimCartridge::blank("A", 10 * KB, 29 * KB - 1)], true);
    let b = vec![0x11; 4096];
    assert_eq!(s.write(&b).unwrap(), 4096);
    assert_eq!(s.write(&b).unwrap(), 4096);
    assert_eq!(s.write(&b).unwrap(), 4096, "reaches the early warning");
    assert_ne!(s.mtget().unwrap().gstat & GMT_EOT, 0);
    assert!(enospc(&s.write(&b)), "nothing written");
    let g = s.mtget().unwrap().gstat;
    assert_eq!(g & GMT_EOT, 0, "ST_EOD_1 shows as GMT_EOD");
    assert_ne!(g & GMT_EOD, 0);
    assert_eq!(s.write(&b).unwrap(), 4096);
    assert!(enospc(&s.write(&b)));
    // A file mark past the early warning is written and ends the
    // alternation; the next write is let through.
    s.mtop(MtOp::Weof, 1).unwrap();
    assert_eq!(s.mtget().unwrap().gstat & GMT_EOT, 0);
    assert_eq!(s.write(&b).unwrap(), 4096);
    assert!(enospc(&s.write(&b)));
    assert_eq!(s.write(&b).unwrap(), 4096);
    assert!(enospc(&s.write(&b)));
    assert_eq!(s.write(&b).unwrap(), 4096);
    // 7 blocks and a mark: 28.5 KiB of 29. The next does not fit.
    assert!(enospc(&s.write(&b)));
    assert!(enospc(&s.write(&b)), "VOLUME OVERFLOW");
    assert!(enospc(&s.write(&b)));
    assert!(enospc(&s.write(&b)), "VOLUME OVERFLOW again");
    // The file mark MTREW writes after writes does not fit either.
    assert!(s.mtop(MtOp::Rew, 1).is_err());
    assert!(s.mtop(MtOp::Rew, 1).is_err(), "still writing");
    let data = s.cartridges[0]
        .records
        .iter()
        .filter(|r| matches!(r, SimRecord::Block(_)))
        .count();
    assert_eq!(data, 7);

    // Asynchronous writes: the early warning shows on the next call,
    // here MTIOCGET, which then did nothing.
    let mut s = StSim::new(vec![SimCartridge::blank("A", 10 * KB, 30 * KB)], true);
    s.async_writes = true;
    for _ in 0..3 {
        assert_eq!(s.write(&b).unwrap(), 4096);
    }
    assert!(matches!(s.mtget(), Err(e) if e.raw_os_error() == Some(ENOSPC)));
    assert!(s.mtget().is_ok());
}

/// Write blocks until the end of the medium: the early-warning flags of
/// each block written, and the drive.
fn fill(t: &mut Drive, len: usize) -> Vec<bool> {
    let mut flags = Vec::new();
    loop {
        match t.write_block(&block(flags.len(), len)) {
            Ok(w) => flags.push(w.early_warning),
            Err(TransportError::EndOfMedium) => return flags,
            Err(e) => panic!("{e}"),
        }
    }
}

fn read_all(t: &mut Drive) -> Vec<Vec<u8>> {
    t.rewind().unwrap();
    let mut buf = vec![0u8; MAX_BLOCK];
    let mut out = Vec::new();
    loop {
        match t.read_block(&mut buf).unwrap() {
            ReadOutcome::Block(n) => out.push(buf[..n].to_vec()),
            ReadOutcome::TapeMark => {}
            ReadOutcome::EndOfData => return out,
        }
    }
}

fn each_block_once(asynchronous: bool, fixed: u32) {
    let mut s = StSim::new(vec![SimCartridge::blank("A", 40 * KB, 80 * KB)], true);
    s.async_writes = asynchronous;
    let mut t = StTransport::new(s);
    t.set_block_size(fixed).unwrap();
    let flags = fill(&mut t, 4096);
    // Ten blocks to the early warning, the eleventh reaches it (the
    // driver says nothing yet), the twelfth is refused once and written
    // past it; then up to the physical end.
    assert_eq!(flags.len(), 20, "{flags:?}");
    assert!(flags[..11].iter().all(|f| !f), "{flags:?}");
    assert!(flags[11..].iter().all(|f| *f), "{flags:?}");
    let got = read_all(&mut t);
    let want: Vec<_> = (0..20).map(|i| block(i, 4096)).collect();
    assert_eq!(got, want, "every block written once, in order");
    let s = t.into_inner();
    assert!(s.calls("write 4096 -> ENOSPC") >= 2, "{:?}", s.log);
}

#[test]
fn transport_writes_each_block_once_past_the_early_warning() {
    each_block_once(false, 0);
    each_block_once(true, 0);
    each_block_once(false, 4096);
}

#[test]
fn file_marks_past_the_early_warning_and_at_the_end() {
    let mut t = StTransport::new(StSim::new(
        vec![SimCartridge::blank("A", 8 * KB, 24 * KB)],
        true,
    ));
    for i in 0..3 {
        assert!(!t.write_block(&block(i, 4096)).unwrap().early_warning);
    }
    assert!(t.write_block(&block(3, 4096)).unwrap().early_warning);
    // A file mark past the early warning; the write after it is let
    // through at once, still past the early warning.
    t.write_eof(1).unwrap();
    assert!(t.status().unwrap().eot);
    assert!(t.write_block(&block(4, 4096)).unwrap().early_warning);
    // 20.5 KiB used: room for seven marks, not ten.
    assert!(matches!(t.write_eof(10), Err(TransportError::EndOfMedium)));

    // Asynchronous writes: MTWEOF right after the write that reached the
    // early warning is refused once with ENOSPC, then done.
    let mut s = StSim::new(vec![SimCartridge::blank("A", 8 * KB, 64 * KB)], true);
    s.async_writes = true;
    let mut t = StTransport::new(s);
    for i in 0..3 {
        assert!(!t.write_block(&block(i, 4096)).unwrap().early_warning);
    }
    t.write_eof(1).unwrap();
    let s = t.into_inner();
    assert_eq!(s.calls("mtop MTWEOF 1 -> ENOSPC"), 1, "{:?}", s.log);
    assert_eq!(s.calls("mtop MTWEOF 1 -> ()"), 1, "{:?}", s.log);
}

#[test]
fn fixed_blocks_and_write_protection() {
    let mut t = StTransport::new(StSim::new(
        vec![SimCartridge::blank("A", 8 * KB, 16 * KB)],
        true,
    ));
    t.set_block_size(512).unwrap();
    assert!(matches!(
        t.write_block(&[0; 1024]),
        Err(TransportError::BlockSize {
            len: 1024,
            size: 512
        })
    ));
    t.write_block(&[0; 512]).unwrap();

    // A protected cartridge: open fell back to read-only.
    let mut c = SimCartridge::blank("P", 8 * KB, 16 * KB);
    c.write_protected = true;
    let mut t = StTransport::new(StSim::new(vec![c.clone()], false));
    assert!(t.status().unwrap().write_protected);
    assert!(matches!(
        t.write_block(&[0; 80]),
        Err(TransportError::WriteProtected)
    ));
    assert!(matches!(
        t.write_eof(1),
        Err(TransportError::WriteProtected)
    ));

    // On a read-write node MTLOAD of a protected cartridge is EROFS: it
    // is loaded, and writes are refused.
    let a = SimCartridge::blank("A", 8 * KB, 16 * KB);
    let mut t = StTransport::new(StSim::new(vec![a, c], true));
    t.unload().unwrap();
    let st = t.status().unwrap();
    assert!(st.loaded && st.write_protected);
    assert!(matches!(
        t.write_block(&[0; 80]),
        Err(TransportError::WriteProtected)
    ));
    // Nothing queued: after the unload the drive stays empty.
    t.unload().unwrap();
    assert!(!t.status().unwrap().loaded);
    assert!(matches!(t.rewind(), Err(TransportError::NotLoaded)));
}

/// The real drive's next volume as the storage service gets it: the
/// full one unloaded, the next loaded (here from the queue).
#[derive(Default)]
struct Operator {
    asked: Vec<VolumeRequest>,
}

impl<D: TapeTransport + ?Sized> VolumeProvider<D> for Operator {
    fn next_volume(&mut self, d: &mut D, req: &VolumeRequest) -> Result<(), TapeError> {
        self.asked.push(req.clone());
        match d.unload() {
            Ok(()) | Err(TransportError::NotLoaded) => {}
            Err(e) => return Err(e.into()),
        }
        if d.status()?.loaded {
            Ok(())
        } else {
            Err(TapeError::Cancelled)
        }
    }
}

fn labeled(name: &str, ew: u64, cap: u64) -> SimCartridge {
    let mut t = StTransport::new(StSim::new(vec![SimCartridge::blank(name, ew, cap)], true));
    inztap(
        &mut t,
        &InzOptions {
            new_vol: Some(name.into()),
            new_owner: "QS400TEST".into(),
            check: Check::No,
            ..InzOptions::default()
        },
    )
    .unwrap();
    t.into_inner().cartridges.remove(0)
}

fn today() -> Date {
    Date::from_ymd(2026, 11, 14).unwrap()
}

fn wopts(vols: &[&str]) -> WriteOptions {
    WriteOptions {
        vols: vols.iter().map(|v| (*v).to_string()).collect(),
        seq: SeqNbr::End,
        label: FileLabel {
            file_id: "QSYS".into(),
            created: Some(today()),
            expires: Expiry::None,
            record_format: RecordFormat::Undefined,
            block_length: 8192,
            record_length: 8192,
            ..FileLabel::default()
        },
        unlabeled: false,
        today: Some(today()),
        allow_active: false,
        end_opt: EndOpt::Rewind,
    }
}

fn write_file(
    carts: Vec<SimCartridge>,
    asynchronous: bool,
    vols: &[&str],
    blocks: &[Vec<u8>],
) -> (Result<WriteSummary, TapeError>, StSim, Vec<VolumeRequest>) {
    let mut s = StSim::new(carts, true);
    s.async_writes = asynchronous;
    let mut t = StTransport::new(s);
    let mut p = Operator::default();
    let r = (|| {
        let mut w = FileWriter::open(&mut t, &mut p, wopts(vols))?;
        for b in blocks {
            w.write_block(b)?;
        }
        w.finish()
    })();
    (r, t.into_inner(), p.asked)
}

fn label(rec: &SimRecord) -> &[u8] {
    match rec {
        SimRecord::Block(b) if b.len() == 80 => b,
        r => panic!("not a label: {r:?}"),
    }
}

/// One cartridge's file section: its header and trailer labels, and its
/// data blocks, checking the layout
/// `VOL1 HDR1 HDR2 TM data TM xxx1 xxx2 TM TM`.
fn section(c: &SimCartridge) -> (FileLabel, (LabelSet, FileLabel), Vec<Vec<u8>>) {
    let r = &c.records;
    let n = r.len();
    assert!(n >= 10, "{}: {} records", c.name, n);
    assert_eq!(r[3], SimRecord::Mark, "{}", c.name);
    assert_eq!(r[n - 5], SimRecord::Mark, "{}", c.name);
    assert_eq!(
        &r[n - 2..],
        [SimRecord::Mark, SimRecord::Mark],
        "{}",
        c.name
    );
    let (set, head) = FileLabel::decode(label(&r[1]), label(&r[2]), Code::Ebcdic).unwrap();
    assert_eq!(set, LabelSet::Header);
    let trailer = FileLabel::decode(label(&r[n - 4]), label(&r[n - 3]), Code::Ebcdic).unwrap();
    let data = r[4..n - 5]
        .iter()
        .map(|x| match x {
            SimRecord::Block(b) => b.clone(),
            SimRecord::Mark => panic!("{}: a mark in the data", c.name),
        })
        .collect();
    (head, trailer, data)
}

fn across(names: &[&str], asynchronous: bool, nblocks: usize) {
    let carts: Vec<_> = names
        .iter()
        .map(|n| labeled(n, 64 * KB, 160 * KB))
        .collect();
    let blocks: Vec<_> = (0..nblocks).map(|i| block(i, 8192)).collect();
    let (r, sim, asked) = write_file(carts, asynchronous, names, &blocks);
    let s = r.unwrap();
    assert_eq!(s.volumes, names);
    assert_eq!(s.blocks, nblocks as u64);
    assert_eq!(asked.len(), names.len() - 1);
    for (i, q) in asked.iter().enumerate() {
        assert!(q.writing);
        assert_eq!(q.volume_seq, u32::try_from(i).unwrap() + 2);
        assert_eq!(q.expected.as_deref(), Some(names[i + 1]));
    }
    assert!(sim.calls("write 8192 -> ENOSPC") >= names.len() - 1);
    assert_eq!(sim.calls("write 80 -> EIO"), 0);

    // Each cartridge: its section, numbered, continued, counted.
    let mut joined = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let c = sim.cartridges.iter().find(|c| c.name == *name).unwrap();
        let (head, (set, tail), data) = section(c);
        let last = i + 1 == names.len();
        assert_eq!(head.volume_seq, u32::try_from(i).unwrap() + 1, "{name}");
        assert_eq!(head.continued, i > 0, "{name}");
        assert_eq!(head.set_id, names[0]);
        assert_eq!(head.file_seq, 1);
        let want = if last {
            LabelSet::EndOfFile
        } else {
            LabelSet::EndOfVolume
        };
        assert_eq!(set, want, "{name}");
        assert_eq!(tail.block_count, data.len() as u64, "{name}");
        assert_eq!(tail.volume_seq, head.volume_seq);
        if !last {
            // Past the early warning: the volume was full.
            assert!(data.len() >= 8, "{name}: {} blocks", data.len());
        }
        joined.extend(data);
    }
    assert_eq!(joined, blocks, "the blocks join exactly once");

    // Read back across the cartridges, by the VOL list and *MOUNTED.
    for vols in [names.iter().map(|v| (*v).to_string()).collect(), Vec::new()] {
        let mut ordered: Vec<SimCartridge> = names
            .iter()
            .map(|n| {
                sim.cartridges
                    .iter()
                    .find(|c| c.name == *n)
                    .unwrap()
                    .clone()
            })
            .collect();
        for c in &mut ordered {
            c.write_protected = true;
        }
        let mut t = StTransport::new(StSim::new(ordered, false));
        let mut p = Operator::default();
        let o = ReadOptions {
            vols,
            seq: Some(1),
            file_label: Some("QSYS".into()),
            ..ReadOptions::default()
        };
        let mut r = FileReader::open(&mut t, &mut p, o).unwrap();
        let mut buf = vec![0u8; MAX_BLOCK];
        let mut got = Vec::new();
        while let Some(n) = r.read_block(&mut buf).unwrap() {
            got.push(buf[..n].to_vec());
        }
        assert_eq!(r.finish().unwrap(), names);
        assert_eq!(got, blocks);
        assert!(p.asked.iter().all(|q| !q.writing));
    }
}

#[test]
fn a_file_across_two_and_three_cartridges() {
    across(&["VOL001", "VOL002"], false, 14);
    across(&["VOL001", "VOL002", "VOL003"], false, 26);
    across(&["VOL001", "VOL002", "VOL003"], true, 26);
}

#[test]
fn next_cartridges_that_cannot_continue() {
    let blocks: Vec<_> = (0..14).map(|i| block(i, 8192)).collect();
    // No next cartridge: the operator cancels.
    let one = vec![labeled("VOL001", 64 * KB, 160 * KB)];
    let (r, _, asked) = write_file(one, false, &[], &blocks);
    assert!(matches!(r, Err(TapeError::Cancelled)), "{r:?}");
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].expected, None);

    // A protected next cartridge.
    let mut p = labeled("VOL002", 64 * KB, 160 * KB);
    p.write_protected = true;
    let carts = vec![labeled("VOL001", 64 * KB, 160 * KB), p];
    let (r, _, _) = write_file(carts, false, &[], &blocks);
    assert!(
        matches!(r, Err(TapeError::Transport(TransportError::WriteProtected))),
        "{r:?}"
    );

    // The wrong cartridge.
    let carts = vec![
        labeled("VOL001", 64 * KB, 160 * KB),
        labeled("VOL009", 64 * KB, 160 * KB),
    ];
    let (r, _, _) = write_file(carts, false, &["VOL001", "VOL002"], &blocks);
    assert!(matches!(r, Err(TapeError::WrongVolume { .. })), "{r:?}");
}
