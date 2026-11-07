// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 5: the `Ifs` bodies and values, `Exists`, and what each
//! version defines.

use std::fmt::Debug;

use ipc::{Wire, decode, encode};
use qap_proto::ops::connection::choose_version;
use qap_proto::ops::ifs::{
    CreateDirectory, Entries, Entry, EntryKind, FileData, FromTo, ListDirectory, PathRef, ReadFile,
    WriteFile, WriteMode, Written,
};
use qap_proto::{Envelope, Op, Service, Status, VERSION};

fn same<T: Wire + PartialEq + Debug>(v: &T) {
    assert_eq!(&decode::<T>(&encode(v)).unwrap(), v);
}

fn entry(kind: EntryKind) -> Entry {
    Entry {
        name: "naïve.txt".into(),
        kind,
        size: 1 << 33,
        modified: 1_793_404_800_123,
        owner: "BOB".into(),
        ccsid: 1208,
        target: (kind == EntryKind::Symlink).then(|| "/home/BOB/a.txt".into()),
    }
}

#[test]
fn ifs_bodies() {
    for (i, k) in EntryKind::ALL.iter().enumerate() {
        same(k);
        assert_eq!(encode(k), [u8::try_from(i).unwrap()]);
    }
    assert!(decode::<EntryKind>(&[4]).is_err());
    for (i, m) in WriteMode::ALL.iter().enumerate() {
        same(m);
        assert_eq!(encode(m), [u8::try_from(i).unwrap()]);
    }
    assert!(decode::<WriteMode>(&[4]).is_err());
    same(&ListDirectory {
        path: "/home/BOB".into(),
        pattern: "*.t?t".into(),
        start: 100,
        max: 50,
    });
    same(&Entries {
        total: 1200,
        entries: EntryKind::ALL.iter().map(|k| entry(*k)).collect(),
    });
    same(&entry(EntryKind::Symlink));
    same(&PathRef {
        path: "/QSYS.LIB/QGPL.LIB".into(),
    });
    same(&ReadFile {
        path: "/home/BOB/a.txt".into(),
        offset: 7,
        length: 0,
        as_ccsid: Some(1208),
    });
    same(&ReadFile::default());
    same(&FileData {
        size: 12,
        next_offset: 11,
        data: vec![0x0E, 0x45, 0x41, 0x0F],
    });
    same(&WriteFile {
        path: "/home/BOB/b.txt".into(),
        mode: WriteMode::Append,
        offset: 0,
        ccsid: 0,
        data: b"text".to_vec(),
        data_ccsid: Some(1208),
    });
    same(&Written { size: 99 });
    same(&CreateDirectory {
        path: "/home/BOB/x/y".into(),
        parents: true,
    });
    same(&FromTo {
        from: "/a".into(),
        to: "/b".into(),
        replace: true,
    });
}

#[test]
fn layouts() {
    // offset and length are u64, the option a tag byte then the u16.
    let b = encode(&ReadFile {
        path: "/a".into(),
        offset: 1,
        length: 2,
        as_ccsid: Some(37),
    });
    assert_eq!(b[b.len() - 3..], [1, 37, 0]);
    assert_eq!(b.len(), 4 + 2 + 8 + 8 + 3);
    let b = encode(&FileData {
        size: 3,
        next_offset: 2,
        data: vec![9],
    });
    assert_eq!(
        b,
        [
            3, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 9
        ]
    );
}

#[test]
fn flags_are_0_or_1() {
    let mut b = encode(&CreateDirectory {
        path: String::new(),
        parents: true,
    });
    *b.last_mut().unwrap() = 2;
    assert!(decode::<CreateDirectory>(&b).is_err());
    let mut b = encode(&FromTo::default());
    *b.last_mut().unwrap() = 7;
    assert!(decode::<FromTo>(&b).is_err());
}

#[test]
fn version_5_values() {
    assert_eq!(encode(&Service::Ifs), [6]);
    assert_eq!(Service::Ifs.since(), 5);
    let names = [
        Op::ListDirectory,
        Op::GetInfo,
        Op::ReadFile,
        Op::WriteFile,
        Op::CreateDirectory,
        Op::Delete,
        Op::Rename,
        Op::Copy,
    ];
    for (code, want) in (1..=8).zip(names) {
        let op = Op::of(Service::Ifs, code).unwrap();
        assert_eq!(op, want);
        assert_eq!(op.since(), 5, "{op:?}");
        assert!(!op.before_signon());
        // Every Ifs operation runs concurrently, never in the lane.
        assert!(!op.in_lane(), "{op:?}");
    }
    // 9, Stat, is version 6's.
    assert_eq!(Op::of(Service::Ifs, 9).map(Op::since), Some(6));
    assert_eq!(Op::of(Service::Ifs, 10), None);
    assert_eq!(Op::of(Service::Ifs, 0), None);
    // Exists is 13 and version 5's: older peers get Error.
    assert_eq!(encode(&Status::Exists), [13, 0]);
    assert_eq!(decode::<Status>(&[13, 0]).unwrap(), Status::Exists);
    for v in 1..=4 {
        assert_eq!(Status::Exists.for_version(v), Status::Error);
        assert!(!Service::of_version(v).contains(&Service::Ifs));
    }
    assert_eq!(Status::Exists.for_version(5), Status::Exists);
    assert_eq!(Status::InUse.for_version(5), Status::InUse);
    assert_eq!(Status::SqlError.for_version(5), Status::SqlError);
    assert_eq!(Service::of_version(5), Service::ALL[..7].to_vec());
    // Every status's value is its place.
    for (i, s) in Status::ALL.iter().enumerate() {
        assert_eq!(encode(s), u16::try_from(i).unwrap().to_le_bytes());
    }
}

#[test]
fn negotiation_of_1_to_5() {
    for max in 1..=5 {
        assert_eq!(choose_version(1, max, VERSION), Some(max));
        assert_eq!(choose_version(max, max, VERSION), Some(max));
    }
    // A version-5 server picks 5 for any range reaching it.
    assert_eq!(choose_version(1, 9, 5), Some(5));
    assert_eq!(choose_version(5, 5, 5), Some(5));
    assert_eq!(choose_version(6, 9, 5), None);
}

#[test]
fn large_reads_split_and_join() {
    let req = Envelope::request(
        9,
        Op::ReadFile,
        &ReadFile {
            path: "/big".into(),
            ..ReadFile::default()
        },
    );
    let data: Vec<u8> = (0..3 * qap_proto::MAX_FRAME)
        .map(|i| u8::try_from(i % 251).unwrap())
        .collect();
    let body = encode(&FileData {
        size: u64::try_from(data.len()).unwrap(),
        next_offset: u64::try_from(data.len()).unwrap(),
        data: data.clone(),
    });
    let frames = Envelope::replies(&req, Status::Ok, None, body).unwrap();
    assert_eq!(frames.len(), 4);
    let mut r = qap_proto::Reassembly::new();
    let mut whole = None;
    for f in frames {
        assert!(f.to_frame().is_ok());
        whole = r.push(f);
    }
    let got: FileData = whole.unwrap().body_as().unwrap();
    assert_eq!(got.data, data);
}
