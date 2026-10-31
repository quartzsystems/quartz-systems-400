// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Tape files: the object's payload, the `*CALC` lengths, and records
//! to tape and back in each record format, labeled and unlabeled.

use std::io::Cursor;

use medio::tapf::{CopyIn, CopyOut, TapeFileDef, copy_from_tape, copy_to_tape, lengths, record_format};
use tape::label::{Date, Expiry, FileLabel, RecordFormat};
use tape::volume::{Check, InzOptions, NoMoreVolumes, inztap, labels};
use tape::{EndOpt, SeqNbr, VirtualTape, VolumeOptions};

fn tape(labeled: bool) -> VirtualTape<Cursor<Vec<u8>>> {
    let mut t = VirtualTape::create(Cursor::new(Vec::new()), &VolumeOptions::imgsiz(2, "TAPF01")).unwrap();
    inztap(
        &mut t,
        &InzOptions {
            new_vol: labeled.then(|| "TAPF01".to_string()),
            check: Check::No,
            ..InzOptions::default()
        },
    )
    .unwrap();
    t
}

fn records() -> Vec<Vec<u8>> {
    vec![b"FIRST RECORD".to_vec(), b"2".to_vec(), b"THE THIRD ONE IS LONGEST".to_vec()]
}

fn out(rcdblk: &str, label: &str, unlabeled: bool) -> CopyOut {
    let (format, blocked) = record_format(rcdblk);
    let longest = u32::try_from(records().iter().map(Vec::len).max().unwrap()).unwrap();
    let (rcdlen, blklen) = lengths(format, blocked, 0, 0, longest);
    CopyOut {
        vols: Vec::new(),
        seq: SeqNbr::End,
        label: FileLabel {
            file_id: label.into(),
            created: Some(Date { year: 2026, day: 304 }),
            expires: Expiry::None,
            record_format: format,
            block_length: blklen,
            record_length: rcdlen,
            ..FileLabel::default()
        },
        unlabeled,
        today: Some(Date { year: 2026, day: 304 }),
        allow_active: false,
        end_opt: EndOpt::Rewind,
        records: records(),
        pad: b' ',
    }
}

#[test]
fn the_payload_round_trips() {
    let d = TapeFileDef {
        devs: vec!["TAPVRT01".into()],
        vols: vec!["VOL001".into(), "VOL002".into()],
        seq: None,
        label: "ORDERS".into(),
        rcdlen: 80,
        rcdblk: "*VB".into(),
        ascii: true,
        expires: Expiry::Perm,
        end_opt: EndOpt::Unload,
        text: "Orders out".into(),
        ..TapeFileDef::default()
    };
    assert_eq!(TapeFileDef::decode(&d.encode()).unwrap(), d);
    assert!(TapeFileDef::decode(b"nonsense").is_err());
    assert!(d.describe().contains("*VB"));
    assert_eq!(lengths(RecordFormat::Fixed, true, 80, 0, 0), (80, 32_720));
    assert_eq!(lengths(RecordFormat::Fixed, false, 0, 0, 24), (24, 24));
    assert_eq!(lengths(RecordFormat::Variable, false, 0, 0, 24), (28, 32));
}

#[test]
fn records_go_to_tape_and_back() {
    for (rcdblk, label) in [("*FB", "FIXEDB"), ("*F", "FIXED"), ("*VB", "VARB"), ("*V", "VAR"), ("*U", "UNDEF")] {
        let mut t = tape(true);
        let s = copy_to_tape(&mut t, &mut NoMoreVolumes, &out(rcdblk, label, false)).unwrap();
        assert_eq!(s.file_seq, 1, "{rcdblk}");
        let (back, l) = copy_from_tape(
            &mut t,
            &mut NoMoreVolumes,
            &CopyIn {
                label: Some(label.into()),
                ..CopyIn::default()
            },
        )
        .unwrap();
        assert_eq!(l.file_id, label);
        let want: Vec<Vec<u8>> = match l.record_format {
            RecordFormat::Fixed => records()
                .into_iter()
                .map(|mut r| {
                    r.resize(l.record_length as usize, b' ');
                    r
                })
                .collect(),
            _ => records(),
        };
        assert_eq!(back, want, "{rcdblk}");
    }
}

#[test]
fn a_second_file_and_an_unlabeled_volume() {
    let mut t = tape(true);
    copy_to_tape(&mut t, &mut NoMoreVolumes, &out("*FB", "ONE", false)).unwrap();
    let s = copy_to_tape(&mut t, &mut NoMoreVolumes, &out("*U", "TWO", false)).unwrap();
    assert_eq!(s.file_seq, 2);
    let l = labels(&mut t, None, EndOpt::Rewind).unwrap();
    assert_eq!(l.files.len(), 2);
    assert_eq!(l.files[1].label.record_format, RecordFormat::Undefined);
    let (back, _) = copy_from_tape(
        &mut t,
        &mut NoMoreVolumes,
        &CopyIn {
            seq: Some(2),
            ..CopyIn::default()
        },
    )
    .unwrap();
    assert_eq!(back, records());

    let mut nl = tape(false);
    copy_to_tape(&mut nl, &mut NoMoreVolumes, &out("*U", "", true)).unwrap();
    let (back, _) = copy_from_tape(
        &mut nl,
        &mut NoMoreVolumes,
        &CopyIn {
            seq: Some(1),
            unlabeled: true,
            format: Some(RecordFormat::Undefined),
            ..CopyIn::default()
        },
    )
    .unwrap();
    assert_eq!(back, records());
}
