// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Round trips, and what a reader makes of torn, truncated and damaged
//! streams.

use std::collections::BTreeMap;
use std::io::Cursor;

use savfmt::object::{NO_OWNER, flags};
use savfmt::{
    AuthorityEntry, Compression, Error, MediaHeader, ObjectDescriptor, Part, Reader, SaveKind,
    Scan, SavedObject, Writer, digest, name10, read_index,
};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn header(c: Compression) -> MediaHeader {
    MediaHeader {
        release: (1, 0, 0),
        target: (1, 0, 0),
        compression: c,
        kind: SaveKind::Library,
        created: 1_793_000_000,
        stream_id: *b"0123456789abcdef",
        system: "QS400".into(),
        serial: "10A1B2C".into(),
        command: "SAVLIB".into(),
        user: "QSECOFR".into(),
    }
}

/// One object to save: descriptor, tail, authorities, parts.
struct Obj {
    desc: ObjectDescriptor,
    tail: Vec<u8>,
    auths: Vec<AuthorityEntry>,
    parts: BTreeMap<Part, Vec<u8>>,
    sign: bool,
}

fn data(rng: &mut Rng, len: usize) -> Vec<u8> {
    // Half compressible, half noise.
    let mut v = Vec::with_capacity(len);
    while v.len() < len {
        if rng.below(2) == 0 {
            let b = u8::try_from(rng.below(256)).unwrap();
            let run = usize::try_from(rng.below(300)).unwrap() + 1;
            v.extend(std::iter::repeat_n(b, run));
        } else {
            v.extend((0..64).map(|_| u8::try_from(rng.below(256)).unwrap()));
        }
    }
    v.truncate(len);
    v
}

fn corpus(rng: &mut Rng, n: usize) -> Vec<Obj> {
    let mut out: Vec<Obj> = Vec::new();
    for i in 0..n {
        let mut parts = BTreeMap::new();
        parts.insert(Part::ATTR, data(rng, 1024));
        let size = match rng.below(4) {
            0 => 0,
            1 => usize::try_from(rng.below(5000)).unwrap(),
            2 => usize::try_from(rng.below(200_000)).unwrap(),
            _ => (1 << 20) + usize::try_from(rng.below(300_000)).unwrap(),
        };
        if size > 0 {
            parts.insert(Part::SPACE, data(rng, size));
        }
        if rng.below(3) == 0 {
            parts.insert(Part::tree(0), data(rng, 777));
        }
        let ifs = rng.below(5) == 0;
        let owned = !ifs && i > 0 && rng.below(4) == 0;
        let auths: Vec<AuthorityEntry> = (0..rng.below(3))
            .map(|k| AuthorityEntry {
                profile: name10(&format!("USER{k}")),
                bits: 0x21,
            })
            .collect();
        let mut desc = ObjectDescriptor {
            type_code: if ifs { 0 } else { 0x13 },
            owner_index: if owned {
                u32::try_from(i - 1).unwrap()
            } else {
                NO_OWNER
            },
            library: name10("QSTEST"),
            name: name10(&format!("OBJ{i}")),
            attribute: name10("PF"),
            original_id: [u8::try_from(i % 256).unwrap(); 16],
            size_bytes: parts.values().map(|p| p.len() as u64).sum(),
            part_count: u16::try_from(parts.len()).unwrap(),
            ..ObjectDescriptor::default()
        };
        if !auths.is_empty() {
            desc.flags |= flags::PRIVATE_AUTH;
        }
        let sign = rng.below(3) == 0;
        if sign {
            desc.flags |= flags::SIGNED;
        }
        let tail = if ifs {
            desc.flags |= flags::IFS;
            format!("/home/QSTEST/dir{i}/file.txt").into_bytes()
        } else {
            Vec::new()
        };
        out.push(Obj {
            desc,
            tail,
            auths,
            parts,
            sign,
        });
    }
    out
}

fn write(objs: &[Obj], c: Compression) -> Vec<u8> {
    let mut w = Writer::new(Vec::new(), &header(c)).unwrap();
    w.parameters("SAVLIB LIB(QSTEST) DEV(*SAVF) SAVF(QSTEST/S1)").unwrap();
    for o in objs {
        w.begin_object(o.desc.clone(), &o.tail).unwrap();
        if !o.auths.is_empty() {
            w.authorities(&o.auths).unwrap();
        }
        for (p, b) in &o.parts {
            // Feed in uneven pieces to exercise chunking.
            w.begin_part(*p, b.len() as u64).unwrap();
            for piece in b.chunks(70_001) {
                w.write_part(piece).unwrap();
            }
        }
        let d = w.end_object().unwrap();
        assert_eq!(d, digest(&o.desc, &o.parts));
        if o.sign {
            w.signature([7; 8], [9; 64]).unwrap();
        }
    }
    let (summary, bytes) = w.finish(2).unwrap();
    assert_eq!(summary.bytes, bytes.len() as u64);
    bytes
}

fn read_all(bytes: &[u8]) -> (Vec<Scan>, Option<Error>) {
    let mut out = Vec::new();
    let mut r = match Reader::new(Cursor::new(bytes)) {
        Ok(r) => r,
        Err(e) => return (out, Some(e)),
    };
    loop {
        match r.next_object() {
            Ok(Some(s)) => out.push(s),
            Ok(None) => return (out, None),
            Err(e) => return (out, Some(e)),
        }
    }
}

fn same(o: &Obj, s: &SavedObject, index: usize) {
    assert_eq!(s.descriptor.index as usize, index);
    assert_eq!(s.descriptor.name, o.desc.name);
    assert_eq!(s.tail, o.tail);
    assert_eq!(s.authorities, o.auths);
    assert_eq!(s.parts, o.parts);
    assert_eq!(s.signature.is_some(), o.sign);
}

#[test]
fn round_trip_every_compression() {
    for c in [
        Compression::None,
        Compression::Low,
        Compression::Medium,
        Compression::High,
    ] {
        let mut rng = Rng(0x5eed_0001);
        let objs = corpus(&mut rng, 24);
        let bytes = write(&objs, c);
        let (scans, err) = read_all(&bytes);
        assert!(err.is_none(), "{c:?}: {err:?}");
        assert_eq!(scans.len(), objs.len());
        for (i, (o, s)) in objs.iter().zip(&scans).enumerate() {
            let Scan::Object(s) = s else {
                panic!("{c:?}: object {i} damaged")
            };
            same(o, s, i);
        }
    }
}

#[test]
fn compression_shrinks_compressible_data() {
    let mut rng = Rng(3);
    let objs = corpus(&mut rng, 12);
    let none = write(&objs, Compression::None).len();
    let low = write(&objs, Compression::Low).len();
    let high = write(&objs, Compression::High).len();
    assert!(low < none && high < none, "{none} {low} {high}");
}

#[test]
fn index_finds_every_object_without_a_scan() {
    let mut rng = Rng(42);
    let objs = corpus(&mut rng, 30);
    let bytes = write(&objs, Compression::Low);
    let mut cur = Cursor::new(bytes.clone());
    let (head, trailer, index) = read_index(&mut cur).unwrap();
    assert_eq!(head.command, "SAVLIB");
    assert_eq!(trailer.objects, 30);
    assert_eq!(trailer.not_saved, 2);
    assert_eq!(index.len(), 30);
    // Read object 17 alone from its offset.
    let e = &index[17];
    assert_eq!(e.name, objs[17].desc.name);
    let mut c = Cursor::new(bytes);
    c.set_position(e.offset);
    let mut r = Reader::positioned(c, head, e.offset);
    let Some(Scan::Object(s)) = r.next_object().unwrap() else {
        panic!("object 17")
    };
    same(&objs[17], &s, 17);
}

#[test]
fn torn_stream_keeps_every_whole_object() {
    let mut rng = Rng(7);
    let objs = corpus(&mut rng, 16);
    let bytes = write(&objs, Compression::Medium);
    for _ in 0..200 {
        let cut = usize::try_from(rng.below(bytes.len() as u64)).unwrap();
        let (scans, err) = read_all(&bytes[..cut]);
        assert!(err.is_some(), "a cut at {cut} must not read as complete");
        for (i, s) in scans.iter().enumerate() {
            if let Scan::Object(s) = s {
                same(&objs[i], s, i);
            }
        }
        // Only the last object may be incomplete.
        let damaged = scans.iter().filter(|s| matches!(s, Scan::Damaged(_))).count();
        assert!(damaged <= 1, "cut at {cut}: {damaged} damaged");
    }
}

#[test]
fn truncated_volume_has_no_index() {
    let mut rng = Rng(9);
    let objs = corpus(&mut rng, 8);
    let bytes = write(&objs, Compression::None);
    let half = bytes.len() / 2;
    let mut cur = Cursor::new(bytes[..half].to_vec());
    assert!(matches!(read_index(&mut cur), Err(Error::Truncated)));
    let (_, err) = read_all(&bytes[..half]);
    assert!(matches!(err, Some(Error::Truncated)));
}

#[test]
fn bad_crc_damages_one_object_only() {
    let mut rng = Rng(11);
    let objs = corpus(&mut rng, 10);
    let bytes = write(&objs, Compression::Low);
    let (_, _, index) = read_index(&mut Cursor::new(bytes.clone())).unwrap();
    // Flip a byte in the last data record of an object with a space.
    let k = (0..9).find(|&i| index[i].size_bytes > 4096).expect("a big object");
    let end = usize::try_from(index[k + 1].offset).unwrap();
    let mut bad = bytes.clone();
    bad[end - 48 - 24 - 10] ^= 0x5a;
    let (scans, err) = read_all(&bad);
    assert!(err.is_none(), "{err:?}");
    assert_eq!(scans.len(), 10);
    for (i, s) in scans.iter().enumerate() {
        match (i, s) {
            (i, Scan::Damaged(Some(d))) if i == k => assert_eq!(d.name, objs[k].desc.name),
            (i, other) if i == k => panic!("object {k}: {other:?}"),
            (_, Scan::Object(s)) => same(&objs[i], s, i),
            (_, Scan::Damaged(_)) => panic!("object {i} damaged"),
        }
    }
}

#[test]
fn random_damage_never_yields_a_wrong_object() {
    let mut rng = Rng(13);
    let objs = corpus(&mut rng, 10);
    let bytes = write(&objs, Compression::High);
    for _ in 0..300 {
        let mut bad = bytes.clone();
        for _ in 0..=rng.below(4) {
            let at = usize::try_from(rng.below(bad.len() as u64)).unwrap();
            bad[at] ^= u8::try_from(1 + rng.below(255)).unwrap();
        }
        let (scans, _) = read_all(&bad);
        for s in &scans {
            if let Scan::Object(s) = s {
                let i = s.descriptor.index as usize;
                same(&objs[i], s, i);
            }
        }
    }
}

#[test]
fn header_damage_is_reported() {
    let mut rng = Rng(15);
    let objs = corpus(&mut rng, 2);
    let mut bytes = write(&objs, Compression::None);
    bytes[40] ^= 1;
    assert!(matches!(
        Reader::new(Cursor::new(&bytes)),
        Err(Error::HeaderDamaged)
    ));
    assert!(matches!(
        Reader::new(Cursor::new(b"not a save".as_slice())),
        Err(Error::NotSave)
    ));
}

#[test]
fn writer_refuses_misuse() {
    let mut w = Writer::new(Vec::new(), &header(Compression::None)).unwrap();
    let d = ObjectDescriptor {
        size_bytes: 4,
        part_count: 1,
        ..ObjectDescriptor::default()
    };
    w.begin_object(d.clone(), &[]).unwrap();
    assert!(w.begin_object(d.clone(), &[]).is_err());
    w.part(Part::SPACE, b"abcd").unwrap();
    assert!(w.part(Part::ATTR, b"x").is_err(), "parts in order");
    w.end_object().unwrap();
    let owned = ObjectDescriptor {
        owner_index: 5,
        ..ObjectDescriptor::default()
    };
    assert!(w.begin_object(owned, &[]).is_err(), "owner first");
    let short = ObjectDescriptor {
        size_bytes: 10,
        part_count: 1,
        ..ObjectDescriptor::default()
    };
    w.begin_object(short, &[]).unwrap();
    w.part(Part::SPACE, b"abc").unwrap();
    assert!(w.end_object().is_err(), "size differs");
}
