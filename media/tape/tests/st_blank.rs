// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A volume never written, through the `st` driver's answers (`StSim`):
//! its first read is BLANK CHECK, which the driver gives as EIO without
//! noting the end of data. It is a volume with no labels: `*NL`, a save
//! refused with `CPF6772`, `INZTAP CHECK(*YES)` initializes it. An EIO
//! the drive explains otherwise stays `CPF6708`.

use std::io;

use tape::label::{Date, Expiry, FileLabel, RecordFormat};
use tape::st::ioctl::{GMT_BOT, GMT_EOD, MtGet, MtOp, StIoctl};
use tape::st::{SimCartridge, SimRecord, StSim, StTransport};
use tape::volume::{
    Check, FileWriter, InzOptions, VolumeKind, WriteOptions, identify, inztap, labels,
};
use tape::{
    EndOpt, MAX_BLOCK, ReadOutcome, SeqNbr, TapeError, TapeTransport, VolumeProvider, VolumeRequest,
};

const KB: u64 = 1024;
const EIO: i32 = 5;

fn blank() -> StTransport<StSim> {
    StTransport::new(StSim::new(
        vec![SimCartridge::blank("NEW", 900 * KB, 1000 * KB)],
        true,
    ))
}

struct NoOperator;

impl<D: TapeTransport + ?Sized> VolumeProvider<D> for NoOperator {
    fn next_volume(&mut self, _: &mut D, _: &VolumeRequest) -> Result<(), TapeError> {
        Err(TapeError::Cancelled)
    }
}

fn save_options() -> WriteOptions {
    let today = Date::from_ymd(2026, 11, 18).unwrap();
    WriteOptions {
        vols: Vec::new(),
        seq: SeqNbr::End,
        label: FileLabel {
            file_id: "TAPLIB".into(),
            created: Some(today),
            expires: Expiry::None,
            record_format: RecordFormat::Undefined,
            block_length: 8192,
            record_length: 8192,
            ..FileLabel::default()
        },
        unlabeled: false,
        today: Some(today),
        allow_active: false,
        end_opt: EndOpt::Rewind,
    }
}

#[test]
fn the_driver_gives_a_blank_volumes_first_read_as_eio_without_the_end() {
    // st.c's read_tape: BLANK CHECK not after a file mark is EIO, the
    // block number lost, the end of data not noted.
    let mut s = StSim::new(vec![SimCartridge::blank("NEW", 900 * KB, 1000 * KB)], true);
    s.mtop(MtOp::Rew, 1).unwrap();
    let mut buf = vec![0; MAX_BLOCK];
    let e = s.read(&mut buf).unwrap_err();
    assert_eq!(e.raw_os_error(), Some(EIO));
    let g = s.mtget().unwrap();
    assert_eq!(g.gstat & (GMT_EOD | GMT_BOT), 0, "{:#x}", g.gstat);
    assert_eq!(g.blkno, -1);
    // The drive itself says why.
    assert_eq!(s.read_sense().unwrap(), Some((0x08, 0x00, 0x05)));
}

#[test]
fn a_blank_volume_reads_as_the_end_of_data_and_has_no_labels() {
    let mut t = blank();
    t.rewind().unwrap();
    let mut buf = vec![0; MAX_BLOCK];
    assert_eq!(t.read_block(&mut buf).unwrap(), ReadOutcome::EndOfData);
    let kind = identify(&mut t).unwrap();
    assert_eq!(kind, VolumeKind::Blank);
    assert_eq!(kind.id(), "*NL", "DSPTAP shows *NL");
    let l = labels(&mut t, None, EndOpt::Rewind).unwrap();
    assert!(l.files.is_empty());
}

#[test]
fn a_save_to_a_blank_volume_is_refused_with_cpf6772() {
    let mut t = blank();
    let mut op = NoOperator;
    let r = FileWriter::open(&mut t, &mut op, save_options());
    let e = r.err().expect("refused");
    assert!(matches!(e, TapeError::NotLabeled), "{e:?}");
    assert_eq!(e.message_id(), "CPF6772");
}

#[test]
fn inztap_check_yes_initializes_a_blank_volume() {
    let mut t = blank();
    inztap(
        &mut t,
        &InzOptions {
            new_vol: Some("VOL001".into()),
            new_owner: "QS400TEST".into(),
            check: Check::Yes,
            ..InzOptions::default()
        },
    )
    .unwrap();
    match identify(&mut t).unwrap() {
        VolumeKind::Labeled { vol1, .. } => assert_eq!(vol1.volume_id, "VOL001"),
        k => panic!("{k:?}"),
    }
}

/// A drive whose first block cannot be read: EIO, and the drive says
/// MEDIUM ERROR, unrecovered read error (03/11/00).
struct Damaged(StSim);

impl StIoctl for Damaged {
    fn mtop(&mut self, op: MtOp, count: i32) -> io::Result<()> {
        self.0.mtop(op, count)
    }
    fn mtget(&mut self) -> io::Result<MtGet> {
        let mut g = self.0.mtget()?;
        g.gstat &= !GMT_EOD;
        Ok(g)
    }
    fn mtpos(&mut self) -> io::Result<u64> {
        self.0.mtpos()
    }
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::from_raw_os_error(EIO))
    }
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.0.write(data)
    }
    fn read_sense(&mut self) -> io::Result<Option<(u8, u8, u8)>> {
        Ok(Some((0x03, 0x11, 0x00)))
    }
}

#[test]
fn an_eio_the_drive_explains_otherwise_stays_a_device_error() {
    let mut t = StTransport::new(Damaged(StSim::new(
        vec![SimCartridge::blank("BAD", 900 * KB, 1000 * KB)],
        true,
    )));
    let e = identify(&mut t).unwrap_err();
    assert_eq!(e.message_id(), "CPF6708", "{e:?}");
    let e = inztap(
        &mut t,
        &InzOptions {
            new_vol: Some("VOL001".into()),
            check: Check::Yes,
            ..InzOptions::default()
        },
    )
    .unwrap_err();
    assert_eq!(e.message_id(), "CPF6708", "{e:?}");
}

#[test]
fn data_that_ends_without_a_file_mark_ends_at_the_blank_check() {
    // Past the last block, no file mark: EIO from the driver, BLANK
    // CHECK from the drive, the end of data. Not rewound: not the first.
    let mut c = SimCartridge::blank("ONE", 900 * KB, 1000 * KB);
    c.records.push(SimRecord::Block(vec![0x40; 80]));
    let mut t = StTransport::new(StSim::new(vec![c], true));
    t.rewind().unwrap();
    let mut buf = vec![0; MAX_BLOCK];
    assert_eq!(t.read_block(&mut buf).unwrap(), ReadOutcome::Block(80));
    assert_eq!(t.read_block(&mut buf).unwrap(), ReadOutcome::EndOfData);
    let s = t.into_inner();
    assert_eq!(s.calls("read sense"), 1);
    assert_eq!(s.calls("mtop MTREW"), 1, "only the caller's rewind");
}

#[test]
fn a_medium_error_past_the_start_stays_a_device_error() {
    let mut t = StTransport::new(Damaged(StSim::new(
        vec![SimCartridge::blank("BAD", 900 * KB, 1000 * KB)],
        true,
    )));
    let mut buf = vec![0; MAX_BLOCK];
    t.fsr(1).ok();
    let e = TapeError::from(t.read_block(&mut buf).unwrap_err());
    assert_eq!(e.message_id(), "CPF6708", "{e:?}");
}
