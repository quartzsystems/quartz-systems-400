// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Save streams through the `st` driver's answers (`StSim`) across two
//! and three cartridges, as SAVLIB and RSTLIB on a real drive write and
//! read them: the early warning, the EOV labels, the next cartridge
//! loaded after the full one is unloaded.

#![allow(clippy::many_single_char_names)]

use medio::tapefile::{RestoreFile, SaveFile, TapeReader, TapeWriter};
use medio::{MediaReader, MediaWriter, SAVE_BLOCK};
use tape::label::{Code, Date, Expiry, FileLabel, LabelSet};
use tape::st::{SimCartridge, SimRecord, StSim, StTransport};
use tape::volume::{Check, InzOptions, inztap};
use tape::{
    EndOpt, SeqNbr, TapeError, TapeTransport, TransportError, VolumeProvider, VolumeRequest,
};

/// The full cartridge unloaded, the next one loaded by the operator.
#[derive(Default)]
struct Operator(Vec<VolumeRequest>);

impl<D: TapeTransport + ?Sized> VolumeProvider<D> for Operator {
    fn next_volume(&mut self, d: &mut D, req: &VolumeRequest) -> Result<(), TapeError> {
        self.0.push(req.clone());
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

fn labeled(name: &str) -> SimCartridge {
    let blank = SimCartridge::blank(name, 4 * SAVE_BLOCK as u64, 8 * SAVE_BLOCK as u64);
    let mut t = StTransport::new(StSim::new(vec![blank], true));
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

fn stream(n: usize) -> Vec<u8> {
    (0..n).map(|i| u8::try_from(i * 7 % 251).unwrap()).collect()
}

fn save_file(vols: &[&str]) -> SaveFile {
    SaveFile {
        vols: vols.iter().map(|v| (*v).to_string()).collect(),
        seq: SeqNbr::End,
        label: "LIBA".into(),
        expires: Expiry::Perm,
        end_opt: EndOpt::Unload,
        today: Date::from_ymd(2026, 11, 14).unwrap(),
        allow_active: false,
        job: "QPADEV0001".into(),
    }
}

/// The trailer label set and the data bytes of a cartridge's section.
fn section(c: &SimCartridge) -> (LabelSet, u64, usize) {
    let r = &c.records;
    let n = r.len();
    let lab = |x: &SimRecord| match x {
        SimRecord::Block(b) => b.clone(),
        SimRecord::Mark => panic!("{}: a mark for a label", c.name),
    };
    let (set, tail) = FileLabel::decode(&lab(&r[n - 4]), &lab(&r[n - 3]), Code::Ebcdic).unwrap();
    let bytes = r[4..n - 5]
        .iter()
        .map(|x| match x {
            SimRecord::Block(b) => b.len(),
            SimRecord::Mark => panic!("{}: a mark in the data", c.name),
        })
        .sum();
    (set, tail.block_count, bytes)
}

fn save_and_restore(names: &[&str], len: usize, asynchronous: bool) {
    let data = stream(len);
    let mut s = StSim::new(names.iter().map(|n| labeled(n)).collect(), true);
    s.async_writes = asynchronous;
    let mut t = StTransport::new(s);
    let mut p = Operator::default();
    let w = TapeWriter::new(&mut t, &mut p, save_file(names))
        .write_stream(&data)
        .unwrap();
    assert_eq!(w.volumes, names);
    assert_eq!(p.0.len(), names.len() - 1);
    let sim = t.into_inner();
    let mut total = 0;
    for (i, name) in names.iter().enumerate() {
        let c = sim.cartridges.iter().find(|c| c.name == *name).unwrap();
        let (set, blocks, bytes) = section(c);
        let last = i + 1 == names.len();
        assert_eq!(
            set,
            if last {
                LabelSet::EndOfFile
            } else {
                LabelSet::EndOfVolume
            },
            "{name}"
        );
        assert_eq!(blocks, bytes.div_ceil(SAVE_BLOCK) as u64, "{name}");
        total += bytes;
    }
    assert_eq!(total, data.len());

    // RSTLIB: the cartridges in order, the first in the drive.
    let carts = names
        .iter()
        .map(|n| {
            sim.cartridges
                .iter()
                .find(|c| c.name == *n)
                .unwrap()
                .clone()
        })
        .collect();
    let mut t = StTransport::new(StSim::new(carts, true));
    let mut p = Operator::default();
    let (back, vols) = TapeReader::new(
        &mut t,
        &mut p,
        RestoreFile {
            label: Some("LIBA".into()),
            end_opt: EndOpt::Rewind,
            ..RestoreFile::default()
        },
    )
    .read_stream()
    .unwrap();
    assert_eq!(vols, names);
    assert!(back == data, "the stream comes back whole");
}

#[test]
fn a_save_across_two_and_three_cartridges() {
    save_and_restore(&["TAP001", "TAP002"], 7 * SAVE_BLOCK + 1234, false);
    save_and_restore(&["TAP001", "TAP002", "TAP003"], 12 * SAVE_BLOCK + 99, false);
    save_and_restore(&["TAP001", "TAP002", "TAP003"], 12 * SAVE_BLOCK + 99, true);
}

#[test]
fn a_save_that_needs_one_more_cartridge() {
    let data = stream(12 * SAVE_BLOCK);
    let mut t = StTransport::new(StSim::new(vec![labeled("TAP001"), labeled("TAP002")], true));
    let mut p = Operator::default();
    let e = TapeWriter::new(&mut t, &mut p, save_file(&[]))
        .write_stream(&data)
        .unwrap_err();
    assert!(
        matches!(e, medio::MediaError::Tape(TapeError::Cancelled)),
        "{e}"
    );
    assert_eq!(p.0.len(), 2);
    assert_eq!(p.0[1].volume_seq, 3);
}
