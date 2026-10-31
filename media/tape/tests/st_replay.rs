// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `StTransport` and `SgChanger` against the hand-written transcripts in
//! `tests/fixtures/`.

mod common;

use common::replay::Replay;
use tape::st::{SgChanger, StTransport, density_name, parse_element_status};
use tape::volume::{VolumeKind, identify};
use tape::{
    Changer, ChangerError, ElementKind, MAX_BLOCK, Position, ReadOutcome, TapeTransport,
    TransportError,
};

fn fixture(name: &str) -> Replay {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    Replay::parse(&std::fs::read_to_string(path).unwrap())
}

#[test]
fn st_session() {
    let mut t = StTransport::new(fixture("st_session.txt"));
    let st = t.status().unwrap();
    assert!(st.loaded && st.bot && !st.eod && !st.write_protected);
    assert_eq!(st.density, "*ULTRIUM5");
    assert_eq!(st.block_size, 0);
    t.set_block_size(0).unwrap();

    let VolumeKind::Labeled { vol1, .. } = identify(&mut t).unwrap() else {
        panic!("not labeled")
    };
    assert_eq!(vol1.volume_id, "TAPE01");

    let big = vec![0x5a; MAX_BLOCK];
    assert!(!t.write_block(&[0x40; 80]).unwrap().early_warning);
    t.write_eof(1).unwrap();
    assert!(!t.write_block(&big).unwrap().early_warning);
    assert!(t.write_block(&big).unwrap().early_warning);
    assert!(t.write_block(&big).unwrap().early_warning);
    assert!(matches!(
        t.write_block(&big),
        Err(TransportError::EndOfMedium)
    ));

    t.rewind().unwrap();
    let mut buf = vec![0u8; MAX_BLOCK];
    assert_eq!(t.read_block(&mut buf).unwrap(), ReadOutcome::Block(80));
    assert!(buf[..80].iter().all(|&b| b == 0x40));
    assert_eq!(t.read_block(&mut buf).unwrap(), ReadOutcome::TapeMark);
    assert!(matches!(
        t.read_block(&mut buf),
        Err(TransportError::BlockTooLarge { .. })
    ));

    assert!(matches!(t.fsf(5), Err(TransportError::EndOfData)));
    assert_eq!(t.read_block(&mut buf).unwrap(), ReadOutcome::EndOfData);
    assert!(matches!(t.bsf(9), Err(TransportError::BeginningOfTape)));
    assert!(matches!(t.fsr(3), Err(TransportError::TapeMark)));
    assert_eq!(t.logical_block().unwrap(), 3);
    assert!(matches!(t.read_block(&mut buf), Err(TransportError::Io(_))));

    t.set_density("*ULTRIUM5").unwrap();
    assert!(matches!(
        t.set_density("*QIC120"),
        Err(TransportError::Density(_))
    ));
    t.eom().unwrap();
    t.erase().unwrap();

    t.rewind().unwrap();
    assert!(matches!(
        t.write_block(&[0; 80]),
        Err(TransportError::WriteProtected)
    ));
    let st = t.status().unwrap();
    assert!(st.write_protected);
    assert_eq!(st.position, Position { file: 0, block: 0 });

    t.unload().unwrap();
    assert!(!t.status().unwrap().loaded);
    assert!(matches!(t.rewind(), Err(TransportError::NotLoaded)));
    t.into_inner().finish();
}

#[test]
fn sg_session() {
    let mut c = SgChanger::new(fixture("sg_session.txt"));
    let elems = c.read_element_status().unwrap();
    let summary: Vec<_> = elems
        .iter()
        .map(|e| (e.kind, e.address, e.full, e.volume_tag.as_deref(), e.source))
        .collect();
    assert_eq!(
        summary,
        [
            (ElementKind::Transport, 0x0000, false, None, None),
            (
                ElementKind::Drive,
                0x0100,
                true,
                Some("E01002L5"),
                Some(0x0401)
            ),
            (ElementKind::Drive, 0x0101, false, None, None),
            (ElementKind::ImportExport, 0x0200, false, None, None),
            (ElementKind::Storage, 0x0400, true, Some("E01001L5"), None),
            (ElementKind::Storage, 0x0401, false, None, None),
            (ElementKind::Storage, 0x0402, true, Some("E01003L5"), None),
            (ElementKind::Storage, 0x0403, false, None, None),
        ]
    );
    c.move_medium(0x0400, 0x0101).unwrap();
    assert!(matches!(
        c.move_medium(0x0401, 0x0100),
        Err(ChangerError::SourceEmpty(0x0401))
    ));
    assert!(matches!(
        c.move_medium(0x0402, 0x0100),
        Err(ChangerError::DestinationFull(0x0100))
    ));
    assert!(matches!(
        c.move_medium(0x0100, 0x0401),
        Err(ChangerError::Check {
            key: 5,
            asc: 0x53,
            ascq: 2
        })
    ));
    assert!(matches!(
        c.move_medium(0x0403, 0x0200),
        Err(ChangerError::Io(_))
    ));
    c.into_inner().finish();
}

#[test]
fn a_fresh_changer_reads_status_before_moving() {
    // The transport element is at 0x0007 here; MOVE MEDIUM must use it.
    let text = "\
sg_io b8 10 00 00 ff ff 00 01 00 00 00 00 from 65536 -> status=0 resid=65508
data 0007 0001 00 000014
data 01 00 000c 00 00000c
data 0007 00 00 00 00 000000 00 0000
sg_io a5 00 00 07 04 00 01 00 00 00 00 00 none 0 -> status=0
";
    let mut c = SgChanger::new(Replay::parse(text));
    c.move_medium(0x0400, 0x0100).unwrap();
    c.into_inner().finish();
}

#[test]
fn bad_element_status() {
    assert!(parse_element_status(&[0; 4]).is_err());
    // A page that claims more bytes than there are.
    let d = [0, 0, 0, 1, 0, 0, 0, 16, 2, 0x80, 0, 52, 0, 0, 0, 52, 0, 0];
    assert!(parse_element_status(&d).is_err());
    assert_eq!(parse_element_status(&[0; 8]).unwrap(), []);
    assert_eq!(density_name(0x60), "*ULTRIUM9");
    assert_eq!(density_name(0x13), "X'13'");
    assert_eq!(density_name(0), "*DEVTYPE");
}
