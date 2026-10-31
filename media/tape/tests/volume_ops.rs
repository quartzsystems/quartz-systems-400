// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The label layer over virtual volumes: INZTAP, files at SEQNBR, ENDOPT,
//! CHKTAP, DUPTAP, the CPYTOTAP/CPYFRMTAP data, *NL and multi-volume.

#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::type_complexity
)]

use std::io::Cursor;

use tape::label::{Code, Date, Expiry, FileLabel, LabelSet, RecordFormat};
use tape::volume::{
    Check, ChkOptions, DupOptions, DupSeq, FileReader, FileWriter, InzOptions, LibraryProvider,
    NoMoreVolumes, ReadOptions, ToSeq, VolumeKind, WriteOptions, blocks_to_records, chktap, duptap,
    inztap, labels, records_to_blocks,
};
use tape::{
    Changer, EndOpt, MAX_BLOCK, SeqNbr, TapeError, TapeTransport, VirtualLibrary, VirtualTape,
    VolumeOptions, VolumeProvider, VolumeRequest,
};

type Mem = VirtualTape<Cursor<Vec<u8>>>;
type Lib = VirtualLibrary<Cursor<Vec<u8>>>;

fn today() -> Date {
    Date::from_ymd(2026, 10, 30).unwrap()
}

fn volume(tag: &str, capacity: u64) -> Mem {
    let o = VolumeOptions {
        capacity,
        density: "*VRT256K".into(),
        write_protect: false,
        tag: tag.into(),
    };
    VirtualTape::create(Cursor::new(Vec::new()), &o).unwrap()
}

fn inz(t: &mut impl TapeTransport, vol: &str) {
    inztap(
        t,
        &InzOptions {
            new_vol: Some(vol.into()),
            new_owner: "QS400TEST".into(),
            check: Check::No,
            ..InzOptions::default()
        },
    )
    .unwrap();
}

fn label(id: &str, expires: Expiry) -> FileLabel {
    FileLabel {
        file_id: id.into(),
        created: Some(today()),
        expires,
        record_format: RecordFormat::Undefined,
        block_length: 4096,
        record_length: 4096,
        ..FileLabel::default()
    }
}

fn wopts(seq: SeqNbr, l: FileLabel) -> WriteOptions {
    WriteOptions {
        vols: Vec::new(),
        seq,
        label: l,
        unlabeled: false,
        today: Some(today()),
        allow_active: false,
        end_opt: EndOpt::Rewind,
    }
}

fn data(seed: u8, n: usize, len: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|i| vec![seed.wrapping_add(u8::try_from(i % 256).unwrap()); len])
        .collect()
}

fn write<D: TapeTransport>(
    t: &mut D,
    p: &mut dyn VolumeProvider<D>,
    o: WriteOptions,
    blocks: &[Vec<u8>],
) -> Result<tape::volume::WriteSummary, TapeError> {
    let mut w = FileWriter::open(t, p, o)?;
    for b in blocks {
        w.write_block(b)?;
    }
    w.finish()
}

fn read<D: TapeTransport>(
    t: &mut D,
    p: &mut dyn VolumeProvider<D>,
    o: ReadOptions,
) -> Result<(FileLabel, Vec<Vec<u8>>, Vec<String>), TapeError> {
    let mut r = FileReader::open(t, p, o)?;
    let label = r.label().clone();
    let mut buf = vec![0u8; MAX_BLOCK];
    let mut out = Vec::new();
    while let Some(n) = r.read_block(&mut buf)? {
        out.push(buf[..n].to_vec());
    }
    let vols = r.finish()?;
    Ok((label, out, vols))
}

fn seq(n: u32) -> ReadOptions {
    ReadOptions {
        seq: Some(n),
        ..ReadOptions::default()
    }
}

#[test]
fn inztap_labels_and_checks() {
    let mut t = volume("VOL001", 1 << 22);
    inz(&mut t, "VOL001");
    let l = labels(&mut t, Some("VOL001"), EndOpt::Rewind).unwrap();
    let VolumeKind::Labeled { vol1, code } = &l.kind else {
        panic!("{:?}", l.kind)
    };
    assert_eq!(vol1.owner, "QS400TEST");
    assert_eq!(*code, Code::Ebcdic);
    assert!(l.files.is_empty());

    // Two files, the second protected.
    write(
        &mut t,
        &mut NoMoreVolumes,
        wopts(SeqNbr::End, label("A", Expiry::None)),
        &data(1, 2, 100),
    )
    .unwrap();
    write(
        &mut t,
        &mut NoMoreVolumes,
        wopts(SeqNbr::End, label("B", Expiry::Perm)),
        &data(2, 2, 100),
    )
    .unwrap();

    let again = |check, vol: Option<&str>| InzOptions {
        new_vol: Some("VOL002".into()),
        vol: vol.map(str::to_string),
        check,
        today: Some(today()),
        ..InzOptions::default()
    };
    let e = inztap(&mut t, &again(Check::Yes, None)).unwrap_err();
    assert!(matches!(e, TapeError::ActiveFile { seq: 2, .. }), "{e}");
    assert_eq!(e.message_id(), "CPA4036");
    let e = inztap(&mut t, &again(Check::First, Some("VOLXXX"))).unwrap_err();
    assert!(matches!(e, TapeError::WrongVolume { .. }));
    assert_eq!(e.message_id(), "CPF6720");
    // CHECK(*FIRST) only looks at file 1, which has expired.
    inztap(&mut t, &again(Check::First, Some("VOL001"))).unwrap();
    let l = labels(&mut t, None, EndOpt::Rewind).unwrap();
    assert_eq!(l.kind.id(), "VOL002");
    assert!(l.files.is_empty());

    // NEWVOL(*NONE) leaves it unlabeled; CODE(*ASCII); CLEAR; DENSITY.
    inztap(
        &mut t,
        &InzOptions {
            check: Check::No,
            ..InzOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        labels(&mut t, None, EndOpt::Rewind).unwrap().kind,
        VolumeKind::Unlabeled
    );
    inztap(
        &mut t,
        &InzOptions {
            new_vol: Some("ASC001".into()),
            code: Code::Ascii,
            clear: true,
            density: Some("*VRT64K".into()),
            check: Check::Yes,
            end_opt: EndOpt::Unload,
            ..InzOptions::default()
        },
    )
    .unwrap();
    assert!(!t.status().unwrap().loaded);
    t.load();
    let l = labels(&mut t, Some("ASC001"), EndOpt::Rewind).unwrap();
    assert!(matches!(
        l.kind,
        VolumeKind::Labeled {
            code: Code::Ascii,
            ..
        }
    ));
    assert_eq!(t.status().unwrap().density, "*VRT64K");

    // Bad parameters and write protection.
    let bad = InzOptions {
        new_vol: Some("TOOLONG1".into()),
        ..InzOptions::default()
    };
    assert!(matches!(
        inztap(&mut t, &bad),
        Err(TapeError::Parameter {
            param: "NEWVOL",
            ..
        })
    ));
    t.set_write_protect(true).unwrap();
    let e = inztap(&mut t, &again(Check::No, None)).unwrap_err();
    assert_eq!(e.message_id(), "CPF6768");
}

#[test]
fn files_at_seqnbr_and_active_files() {
    let mut t = volume("VOL001", 1 << 22);
    inz(&mut t, "VOL001");
    let p = &mut NoMoreVolumes;
    for (i, id) in ["ONE", "TWO", "THREE"].iter().enumerate() {
        let s = write(
            &mut t,
            p,
            wopts(SeqNbr::End, label(id, Expiry::None)),
            &data(10 * u8::try_from(i).unwrap(), 3, 500),
        )
        .unwrap();
        assert_eq!(s.file_seq, u32::try_from(i).unwrap() + 1);
        assert_eq!(s.volumes, vec!["VOL001".to_string()]);
    }
    let l = labels(&mut t, None, EndOpt::Rewind).unwrap();
    let ids: Vec<_> = l
        .files
        .iter()
        .map(|f| {
            (
                f.label.file_seq,
                f.label.file_id.clone(),
                f.label.block_count,
                f.trailer,
            )
        })
        .collect();
    assert_eq!(
        ids,
        vec![
            (1, "ONE".into(), 3, LabelSet::EndOfFile),
            (2, "TWO".into(), 3, LabelSet::EndOfFile),
            (3, "THREE".into(), 3, LabelSet::EndOfFile)
        ]
    );
    assert_eq!(l.files[1].label.set_id, "VOL001");
    assert_eq!(l.files[1].label.system_code, "QS/400");

    let (lab, got, _) = read(&mut t, p, seq(2)).unwrap();
    assert_eq!(lab.file_id, "TWO");
    assert_eq!(got, data(10, 3, 500));
    let by_label = ReadOptions {
        file_label: Some("THREE".into()),
        ..ReadOptions::default()
    };
    assert_eq!(read(&mut t, p, by_label).unwrap().1, data(20, 3, 500));

    // Overwriting file 2 loses file 3.
    write(
        &mut t,
        p,
        wopts(SeqNbr::Number(2), label("NEW2", Expiry::Perm)),
        &data(99, 1, 10),
    )
    .unwrap();
    let l = labels(&mut t, None, EndOpt::Rewind).unwrap();
    assert_eq!(l.files.len(), 2);
    assert_eq!(l.files[1].label.file_id, "NEW2");
    assert_eq!(read(&mut t, p, seq(1)).unwrap().1, data(0, 3, 500));
    // A gap is refused; so is overwriting the protected file.
    let e = write(
        &mut t,
        p,
        wopts(SeqNbr::Number(4), label("GAP", Expiry::None)),
        &[],
    )
    .unwrap_err();
    assert!(matches!(e, TapeError::FileNotFound { .. }));
    let e = write(
        &mut t,
        p,
        wopts(SeqNbr::Number(1), label("X", Expiry::None)),
        &[],
    )
    .unwrap_err();
    assert!(matches!(e, TapeError::ActiveFile { seq: 2, .. }), "{e}");
    let mut allowed = wopts(SeqNbr::Number(1), label("X", Expiry::None));
    allowed.allow_active = true;
    write(&mut t, p, allowed, &data(5, 1, 10)).unwrap();
    assert_eq!(labels(&mut t, None, EndOpt::Rewind).unwrap().files.len(), 1);
    // Wrong volume, file not found.
    let mut wrong = wopts(SeqNbr::End, label("Y", Expiry::None));
    wrong.vols = vec!["OTHER".into()];
    assert!(matches!(
        write(&mut t, p, wrong, &[]),
        Err(TapeError::WrongVolume { .. })
    ));
    assert!(matches!(
        read(&mut t, p, seq(7)),
        Err(TapeError::FileNotFound { .. })
    ));
}

#[test]
fn end_options() {
    let mut t = volume("VOL001", 1 << 22);
    inz(&mut t, "VOL001");
    let mut o = wopts(SeqNbr::End, label("A", Expiry::None));
    o.end_opt = EndOpt::Leave;
    write(&mut t, &mut NoMoreVolumes, o.clone(), &data(1, 2, 10)).unwrap();
    let st = t.status().unwrap();
    assert!(!st.bot && st.loaded);
    // *LEAVE stops between the last two tape marks: the next file goes there.
    assert_eq!(st.position.file, 3);
    o.end_opt = EndOpt::Rewind;
    write(&mut t, &mut NoMoreVolumes, o.clone(), &data(1, 2, 10)).unwrap();
    assert!(t.status().unwrap().bot);
    o.end_opt = EndOpt::Unload;
    write(&mut t, &mut NoMoreVolumes, o, &data(1, 2, 10)).unwrap();
    assert!(!t.status().unwrap().loaded);
    t.load();
    let r = ReadOptions {
        seq: Some(3),
        end_opt: EndOpt::Leave,
        ..ReadOptions::default()
    };
    read(&mut t, &mut NoMoreVolumes, r).unwrap();
    assert!(!t.status().unwrap().bot);
    assert_eq!(labels(&mut t, None, EndOpt::Rewind).unwrap().files.len(), 3);
}

#[test]
fn chktap_finds_files() {
    let mut t = volume("VOL001", 1 << 22);
    inz(&mut t, "VOL001");
    for id in ["A", "B"] {
        write(
            &mut t,
            &mut NoMoreVolumes,
            wopts(SeqNbr::End, label(id, Expiry::None)),
            &data(1, 1, 10),
        )
        .unwrap();
    }
    let r = chktap(
        &mut t,
        &ChkOptions {
            vol: Some("VOL001".into()),
            ..ChkOptions::default()
        },
    )
    .unwrap();
    assert!(r.file.is_none());
    let r = chktap(
        &mut t,
        &ChkOptions {
            file_label: Some("B".into()),
            ..ChkOptions::default()
        },
    )
    .unwrap();
    assert_eq!(r.file.unwrap().label.file_seq, 2);
    let o = ChkOptions {
        seq: Some(SeqNbr::Number(1)),
        end_opt: EndOpt::Leave,
        ..ChkOptions::default()
    };
    let r = chktap(&mut t, &o).unwrap();
    assert_eq!(r.file.unwrap().label.file_id, "A");
    assert!(!t.status().unwrap().bot);
    let o = ChkOptions {
        seq: Some(SeqNbr::Number(2)),
        file_label: Some("A".into()),
        ..ChkOptions::default()
    };
    let e = chktap(&mut t, &o).unwrap_err();
    assert_eq!(e.message_id(), "CPF6790");
    let o = ChkOptions {
        seq: Some(SeqNbr::End),
        end_opt: EndOpt::Unload,
        ..ChkOptions::default()
    };
    chktap(&mut t, &o).unwrap();
    assert!(!t.status().unwrap().loaded);
    t.load();
    assert!(
        chktap(
            &mut t,
            &ChkOptions {
                vol: Some("NOPE".into()),
                ..ChkOptions::default()
            }
        )
        .is_err()
    );
}

#[test]
fn duptap_copies_files_and_labels() {
    let mut a = volume("SRC001", 1 << 22);
    let mut b = volume("DST001", 1 << 22);
    inz(&mut a, "SRC001");
    inz(&mut b, "DST001");
    for (i, id) in ["F1", "F2", "F3"].iter().enumerate() {
        let n = u8::try_from(i).unwrap();
        write(
            &mut a,
            &mut NoMoreVolumes,
            wopts(SeqNbr::End, label(id, Expiry::None)),
            &data(n, 4, 300 + usize::from(n)),
        )
        .unwrap();
    }
    let o = DupOptions {
        from_vol: Some("SRC001".into()),
        to_vol: Some("DST001".into()),
        today: Some(today()),
        ..DupOptions::default()
    };
    let out = duptap(&mut a, &mut NoMoreVolumes, &mut b, &mut NoMoreVolumes, &o).unwrap();
    assert_eq!(out.len(), 3);
    let src = labels(&mut a, None, EndOpt::Rewind).unwrap();
    let dst = labels(&mut b, None, EndOpt::Rewind).unwrap();
    assert_eq!(dst.kind.id(), "DST001");
    for (s, d) in src.files.iter().zip(&dst.files) {
        let mut want = s.label.clone();
        want.set_id = "DST001".into();
        assert_eq!(d.label, want);
    }
    for n in 1..=3 {
        assert_eq!(
            read(&mut a, &mut NoMoreVolumes, seq(n)).unwrap().1,
            read(&mut b, &mut NoMoreVolumes, seq(n)).unwrap().1
        );
    }
    // FROMSEQNBR(2 3) TOSEQNBR(*END) appends; TOSEQNBR(2) overwrites.
    let o2 = DupOptions {
        from_seq: DupSeq::Range(2, 3),
        to_seq: ToSeq::End,
        ..o.clone()
    };
    duptap(&mut a, &mut NoMoreVolumes, &mut b, &mut NoMoreVolumes, &o2).unwrap();
    let ids: Vec<_> = labels(&mut b, None, EndOpt::Rewind)
        .unwrap()
        .files
        .iter()
        .map(|f| f.label.file_id.clone())
        .collect();
    assert_eq!(ids, ["F1", "F2", "F3", "F2", "F3"]);
    let o3 = DupOptions {
        from_seq: DupSeq::Range(3, 3),
        to_seq: ToSeq::Number(2),
        ..o.clone()
    };
    duptap(&mut a, &mut NoMoreVolumes, &mut b, &mut NoMoreVolumes, &o3).unwrap();
    let ids: Vec<_> = labels(&mut b, None, EndOpt::Rewind)
        .unwrap()
        .files
        .iter()
        .map(|f| f.label.file_id.clone())
        .collect();
    assert_eq!(ids, ["F1", "F3"]);
    let o4 = DupOptions {
        from_seq: DupSeq::Range(8, 9),
        ..o
    };
    assert!(duptap(&mut a, &mut NoMoreVolumes, &mut b, &mut NoMoreVolumes, &o4).is_err());
}

#[test]
fn records_through_tape_files() {
    let recs: Vec<Vec<u8>> = (0..37u8)
        .map(|i| vec![b'A' + i % 26; usize::from(i % 9) + 1])
        .collect();
    let refs = || recs.iter().map(Vec::as_slice);

    let f = records_to_blocks(refs(), RecordFormat::Fixed, 10, 40, 0x40).unwrap();
    assert_eq!(f.len(), 10);
    assert!(f[..9].iter().all(|b| b.len() == 40));
    assert_eq!(f[9].len(), 10);
    let back = blocks_to_records(f.iter().map(Vec::as_slice), RecordFormat::Fixed, 10).unwrap();
    for (r, b) in recs.iter().zip(&back) {
        assert_eq!(&b[..r.len()], &r[..]);
        assert!(b[r.len()..].iter().all(|&x| x == 0x40));
    }

    let v = records_to_blocks(refs(), RecordFormat::Variable, 14, 64, 0).unwrap();
    assert!(
        v.iter()
            .all(|b| b.len() <= 64 && usize::from(u16::from_be_bytes([b[0], b[1]])) == b.len())
    );
    let back = blocks_to_records(v.iter().map(Vec::as_slice), RecordFormat::Variable, 14).unwrap();
    assert_eq!(back, recs);

    let u = records_to_blocks(refs(), RecordFormat::Undefined, 9, 9, 0).unwrap();
    assert_eq!(
        blocks_to_records(u.iter().map(Vec::as_slice), RecordFormat::Undefined, 9).unwrap(),
        recs
    );

    assert!(records_to_blocks([&[0u8; 11][..]], RecordFormat::Fixed, 10, 40, 0).is_err());
    assert!(records_to_blocks([&[0u8; 11][..]], RecordFormat::Variable, 14, 64, 0).is_err());

    // Through a labeled file: CPYTOTAP then CPYFRMTAP.
    let mut t = volume("VOL001", 1 << 22);
    inz(&mut t, "VOL001");
    let l = FileLabel {
        record_format: RecordFormat::Variable,
        record_length: 14,
        block_length: 64,
        ..label("QCUSTCDT", Expiry::None)
    };
    write(&mut t, &mut NoMoreVolumes, wopts(SeqNbr::End, l), &v).unwrap();
    let (got_label, blocks, _) = read(&mut t, &mut NoMoreVolumes, seq(1)).unwrap();
    assert_eq!(
        (got_label.record_format, got_label.record_length),
        (RecordFormat::Variable, 14)
    );
    let back = blocks_to_records(
        blocks.iter().map(Vec::as_slice),
        got_label.record_format,
        14,
    )
    .unwrap();
    assert_eq!(back, recs);
}

#[test]
fn unlabeled_volumes() {
    let mut t = volume("NL0001", 1 << 22);
    inztap(
        &mut t,
        &InzOptions {
            check: Check::No,
            ..InzOptions::default()
        },
    )
    .unwrap();
    let mut nl = wopts(SeqNbr::End, FileLabel::default());
    nl.unlabeled = true;
    for n in 0..3 {
        let s = write(&mut t, &mut NoMoreVolumes, nl.clone(), &data(n, 2, 64)).unwrap();
        assert_eq!(s.file_seq, u32::from(n) + 1);
    }
    let r = |s| ReadOptions {
        seq: Some(s),
        unlabeled: true,
        ..ReadOptions::default()
    };
    assert_eq!(
        read(&mut t, &mut NoMoreVolumes, r(2)).unwrap().1,
        data(1, 2, 64)
    );
    assert_eq!(
        read(&mut t, &mut NoMoreVolumes, r(3)).unwrap().1,
        data(2, 2, 64)
    );
    // Rewrite file 2: file 3 is gone.
    let mut two = nl.clone();
    two.seq = SeqNbr::Number(2);
    write(&mut t, &mut NoMoreVolumes, two, &data(50, 1, 8)).unwrap();
    assert_eq!(
        read(&mut t, &mut NoMoreVolumes, r(2)).unwrap().1,
        data(50, 1, 8)
    );
    assert!(read(&mut t, &mut NoMoreVolumes, r(3)).unwrap().1.is_empty());
    // Labeled operations refuse it, and *NL writes refuse labeled volumes.
    assert!(matches!(
        read(&mut t, &mut NoMoreVolumes, seq(1)),
        Err(TapeError::NotLabeled)
    ));
    let mut l = volume("LAB001", 1 << 20);
    inz(&mut l, "LAB001");
    assert!(matches!(
        write(&mut l, &mut NoMoreVolumes, nl, &[]),
        Err(TapeError::Labeled)
    ));
    // DUPTAP of an *NL volume.
    let mut c = volume("NL0002", 1 << 22);
    let o = DupOptions::default();
    duptap(&mut t, &mut NoMoreVolumes, &mut c, &mut NoMoreVolumes, &o).unwrap();
    assert_eq!(
        read(&mut c, &mut NoMoreVolumes, r(1)).unwrap().1,
        data(0, 2, 64)
    );
}

fn library(tags: &[&str], capacity: u64) -> Lib {
    let mut lib = VirtualLibrary::new(tags.len() + 2, 1, 1);
    for (i, tag) in tags.iter().enumerate() {
        let mut v = volume(tag, capacity);
        inz(&mut v, tag);
        lib.insert(i, v).unwrap();
    }
    lib
}

fn mount(lib: &mut Lib, tag: &str) {
    let from = lib.find(tag).unwrap();
    let to = lib.drive_address(0);
    lib.move_medium(from, to).unwrap();
}

#[test]
fn multi_volume_write_and_read() {
    let mut lib = library(&["MV0001", "MV0002", "MV0003"], 3 * 64 * 1024);
    mount(&mut lib, "MV0001");
    let blocks = data(7, 40, 4000);
    let mut o = wopts(SeqNbr::End, label("BIGFILE", Expiry::None));
    o.vols = vec!["MV0001".into(), "MV0002".into(), "MV0003".into()];
    let mut p = LibraryProvider::default();
    let s = write(&mut lib, &mut p, o.clone(), &blocks).unwrap();
    assert!(s.volumes.len() >= 2, "{:?}", s.volumes);
    assert_eq!(s.volumes[0], "MV0001");
    assert_eq!(s.blocks, 40);
    let last = s.volumes.last().unwrap().clone();
    assert_eq!(p.mounted, s.volumes[1..]);

    // The volumes' labels: EOV on the first, continued HDR on the next.
    let l2 = labels(&mut lib, Some(&last), EndOpt::Rewind).unwrap();
    assert_eq!(l2.files[0].label.file_id, "BIGFILE");
    assert!(l2.files[0].label.continued);
    assert_eq!(l2.files[0].label.set_id, "MV0001");
    assert_eq!(
        l2.files[0].label.volume_seq,
        u32::try_from(s.volumes.len()).unwrap()
    );
    assert_eq!(l2.files[0].trailer, LabelSet::EndOfFile);
    mount_back(&mut lib, "MV0001");
    let l1 = labels(&mut lib, Some("MV0001"), EndOpt::Rewind).unwrap();
    assert_eq!(l1.files[0].trailer, LabelSet::EndOfVolume);
    let total: u64 = l1.files[0].label.block_count + l2.files[0].label.block_count;
    if s.volumes.len() == 2 {
        assert_eq!(total, 40);
    }

    // Read it back across the volumes, by list.
    let r = ReadOptions {
        vols: o.vols.clone(),
        seq: Some(1),
        ..ReadOptions::default()
    };
    let mut p = LibraryProvider::default();
    let (_, got, vols) = read(&mut lib, &mut p, r).unwrap();
    assert_eq!(vols, s.volumes);
    assert_eq!(got, blocks);

    // VOL(*MOUNTED) with the operator's choice of volumes.
    mount_back(&mut lib, "MV0001");
    let mut p = LibraryProvider {
        spares: s.volumes[1..].to_vec(),
        ..LibraryProvider::default()
    };
    let (_, got, _) = read(&mut lib, &mut p, seq(1)).unwrap();
    assert_eq!(got, blocks);

    // No further volume: the request is cancelled.
    mount_back(&mut lib, "MV0001");
    let mut short = wopts(SeqNbr::Number(1), label("BIGFILE", Expiry::None));
    short.vols = vec!["MV0001".into()];
    let e = write(&mut lib, &mut NoMoreVolumes, short, &blocks).unwrap_err();
    assert!(matches!(e, TapeError::Cancelled), "{e}");

    // A continuation volume that does not continue the file.
    mount_back(&mut lib, "MV0001");
    write(&mut lib, &mut LibraryProvider::default(), o, &blocks).unwrap();
    mount_back(&mut lib, "MV0001");
    let r = ReadOptions {
        vols: vec!["MV0001".into(), "MV0003".into()],
        seq: Some(1),
        ..ReadOptions::default()
    };
    let e = read(&mut lib, &mut LibraryProvider::default(), r).unwrap_err();
    assert!(
        matches!(
            e,
            TapeError::NotContinuation { .. } | TapeError::WrongVolume { .. }
        ),
        "{e}"
    );
}

/// Put `tag` in the drive, returning whatever is there to its slot.
fn mount_back(lib: &mut Lib, tag: &str) {
    let _ = lib.unload();
    let drive = lib.drive_address(0);
    let elems = lib.read_element_status().unwrap();
    let d = elems.iter().find(|e| e.address == drive).unwrap();
    if d.volume_tag.as_deref() == Some(tag) {
        lib.drive(0).unwrap().load();
        return;
    }
    if d.full {
        lib.move_medium(drive, d.source.unwrap()).unwrap();
    }
    mount(lib, tag);
}

/// A provider that records the requests it saw.
struct Recorder(Vec<VolumeRequest>, LibraryProvider);

impl VolumeProvider<Lib> for Recorder {
    fn next_volume(&mut self, lib: &mut Lib, req: &VolumeRequest) -> Result<(), TapeError> {
        self.0.push(req.clone());
        self.1.next_volume(lib, req)
    }
}

#[test]
fn volume_requests_say_what_is_wanted() {
    let mut lib = library(&["RQ0001", "RQ0002"], 2 * 64 * 1024);
    mount(&mut lib, "RQ0001");
    let mut o = wopts(SeqNbr::End, label("REQ", Expiry::None));
    o.vols = vec!["RQ0001".into()];
    let mut rec = Recorder(
        Vec::new(),
        LibraryProvider {
            spares: vec!["RQ0002".into()],
            ..LibraryProvider::default()
        },
    );
    write(&mut lib, &mut rec, o, &data(3, 20, 4000)).unwrap();
    assert_eq!(
        rec.0[0],
        VolumeRequest {
            volume_seq: 2,
            expected: None,
            previous: "RQ0001".into(),
            file_label: "REQ".into(),
            file_seq: 1,
            writing: true,
        }
    );
}
