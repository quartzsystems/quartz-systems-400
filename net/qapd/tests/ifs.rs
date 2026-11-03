// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 5's `Ifs` service against the test system's file system:
//! every operation and status, chunked converted reads that join into
//! the whole file's conversion (UTF-8 and mixed double-byte, chunk
//! boundaries inside characters), converted writes in pieces that make
//! the same file as one write, `Copy` keeping the CCSID, the authority
//! refusals the file-system commands give, and versions 1 to 5.

#![allow(clippy::many_single_char_names)]

mod common;

use std::sync::Arc;

use ifs::ops::audit::AuditKind;
use ifscmd::Msg;
use ifscmd::cmds::dirs::{Md, md};
use ifscmd::cmds::links::del;
use qap_proto::ops::connection::HelloReply;
use qap_proto::ops::ifs::{
    CreateDirectory, Entries, Entry, EntryKind, FileData, FromTo, ListDirectory, PathRef, ReadFile, WriteFile,
    WriteMode, Written,
};
use qap_proto::{Envelope, Op, Service, Status};
use qapd::testing::files::{
    BIG_SIZE, BINARY_FILE, BROKEN, BROKEN_TARGET, JAPANESE, LINK, LOCKED, MANY, MANY_COUNT, MIXED, PRIVATE,
    PRIVATE_FILE, TEXT, TEXT_CCSIDS, TEXT_DIR, big_byte, text_file,
};

use common::{Controls, Run, cfg, start_with};

/// Version 5, signed on as `BOB`.
fn bob_on(c: &Arc<Controls>) -> Run {
    let mut r = start_with(cfg(), Arc::clone(c));
    let h: HelloReply = r.client.hello_range(1, 5).body_as().unwrap();
    assert_eq!(h.version, 5);
    assert!(h.services.contains(&Service::Ifs));
    r.client.sign_on("BOB", "SECRET1");
    r
}

fn bob() -> (Arc<Controls>, Run) {
    let c = Controls::new();
    let r = bob_on(&c);
    (c, r)
}

/// The status and message ID of a reply.
fn said(e: &Envelope) -> (Status, String) {
    (e.status, e.message.as_ref().map(|m| m.id.clone()).unwrap_or_default())
}

/// The reply's message is the command's `m`: identifier and data.
fn same_message(e: &Envelope, m: &Msg) {
    let got = e.message.as_ref().expect("a message");
    assert_eq!((got.id.as_str(), got.data.as_slice()), (m.id.as_str(), m.data.as_slice()), "{e:?}");
}

fn list(r: &mut Run, path: &str, pattern: &str, start: u32, max: u32) -> Envelope {
    r.client.call(
        Op::ListDirectory,
        &ListDirectory {
            path: path.into(),
            pattern: pattern.into(),
            start,
            max,
        },
    )
}

fn info(r: &mut Run, path: &str) -> Envelope {
    r.client.call(Op::GetInfo, &PathRef { path: path.into() })
}

fn read(r: &mut Run, path: &str, offset: u64, length: u64, as_ccsid: Option<u16>) -> Envelope {
    r.client.call(
        Op::ReadFile,
        &ReadFile {
            path: path.into(),
            offset,
            length,
            as_ccsid,
        },
    )
}

fn data(e: &Envelope) -> FileData {
    assert_eq!(e.status, Status::Ok, "{e:?}");
    e.body_as().unwrap()
}

fn write(r: &mut Run, path: &str, mode: WriteMode, offset: u64, data: &[u8], data_ccsid: Option<u16>) -> Envelope {
    r.client.call(
        Op::WriteFile,
        &WriteFile {
            path: path.into(),
            mode,
            offset,
            ccsid: 0,
            data: data.to_vec(),
            data_ccsid,
        },
    )
}

fn written(e: &Envelope) -> u64 {
    assert_eq!(e.status, Status::Ok, "{e:?}");
    e.body_as::<Written>().unwrap().size
}

fn from_to(r: &mut Run, op: Op, from: &str, to: &str, replace: bool) -> Envelope {
    r.client.call(
        op,
        &FromTo {
            from: from.into(),
            to: to.into(),
            replace,
        },
    )
}

/// A file's bytes as stored, read by the test system as `user`.
fn stored(c: &Controls, path: &str) -> Vec<u8> {
    c.files.as_user("QSECOFR", 9, |cl| {
        let fd = cl.open(path, ifs::ops::OpenArgs::new(ifs::ops::flags::O_RDONLY)).unwrap();
        let b = ifs::client::read_all(cl, fd).unwrap();
        cl.close(fd).unwrap();
        b
    })
}

/// Read `path` from 0 to its end in reads of `step` file bytes, each
/// from the last `next_offset`: the replies' data.
fn read_on(r: &mut Run, path: &str, step: u64, as_ccsid: Option<u16>) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut at = 0;
    loop {
        let d = data(&read(r, path, at, step, as_ccsid));
        assert!(d.next_offset > at || d.next_offset >= d.size, "no progress at {at}");
        assert!(d.next_offset <= at + step);
        out.push(d.data);
        at = d.next_offset;
        if at >= d.size {
            return out;
        }
    }
}

#[test]
fn list_directory_pages_and_patterns() {
    let (sys, mut r) = bob();
    let e: Entries = list(&mut r, MANY, "*", 0, 0).body_as().unwrap();
    assert_eq!(e.total, u32::try_from(MANY_COUNT).unwrap());
    assert_eq!(e.entries.len(), MANY_COUNT);
    assert_eq!(e.entries[0].name, "f0000.txt");
    assert_eq!(e.entries[1199].name, "f1199.txt");
    // Pages join into the whole list.
    let mut names = Vec::new();
    let mut start = 0;
    loop {
        let p: Entries = list(&mut r, MANY, "", start, 500).body_as().unwrap();
        assert_eq!(p.total, 1200);
        if p.entries.is_empty() {
            break;
        }
        start += u32::try_from(p.entries.len()).unwrap();
        names.extend(p.entries.into_iter().map(|e| e.name));
    }
    assert_eq!(names, e.entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>());
    let p: Entries = list(&mut r, MANY, "f11??.TXT", 90, 0).body_as().unwrap();
    assert_eq!((p.total, p.entries.len()), (100, 10));
    assert_eq!(p.entries[0].name, "f1190.txt");
    let f = &p.entries[0];
    assert_eq!((f.kind, f.size, f.owner.as_str(), f.ccsid), (EntryKind::File, 10, "BOB", 1208));
    assert!(f.modified > 1_700_000_000_000);
    // Kinds, links and their targets.
    let home: Entries = list(&mut r, "/home/BOB", "*", 0, 0).body_as().unwrap();
    let by = |n: &str| home.entries.iter().find(|e| e.name == n).unwrap().clone();
    assert_eq!(by("many").kind, EntryKind::Directory);
    assert_eq!(by("link1208").kind, EntryKind::Symlink);
    assert_eq!(by("link1208").target.as_deref(), Some(text_file(1208).as_str()));
    assert_eq!(by("broken").target.as_deref(), Some(BROKEN_TARGET));
    assert_eq!(by("locked.txt").kind, EntryKind::File);
    // /QSYS.LIB lists as the commands list it.
    let q: Entries = list(&mut r, "/QSYS.LIB", "*.LIB", 0, 0).body_as().unwrap();
    assert!(q.entries.iter().any(|e| e.name == "QGPL.LIB" && e.kind == EntryKind::Directory), "{q:?}");
    let q: Entries = list(&mut r, "/QSYS.LIB/QGPL.LIB", "*", 0, 0).body_as().unwrap();
    assert!(q.entries.iter().any(|e| e.name == "QCLSRC.FILE"), "{q:?}");
    // Statuses.
    assert_eq!(said(&list(&mut r, "/home/BOB/nope", "*", 0, 0)), (Status::NotFound, "CPFA0A9".into()));
    let e = list(&mut r, PRIVATE, "*", 0, 0);
    assert_eq!(said(&e), (Status::NotAuthorized, "CPFA09C".into()));
    // As WRKLNK answers BOB.
    let cmd = sys.files.as_user("BOB", 50, |cl| ifscmd::display::links(cl, &format!("{PRIVATE}/*"), "*ALL"));
    same_message(&e, &Msg::errno(cmd.unwrap_err(), PRIVATE));
}

#[test]
fn get_info() {
    let (_sys, mut r) = bob();
    let e: Entry = info(&mut r, &text_file(37)).body_as().unwrap();
    assert_eq!((e.name.as_str(), e.kind, e.ccsid, e.owner.as_str()), ("text37.txt", EntryKind::File, 37, "BOB"));
    assert_eq!(e.size, ccsid::conv::from_str(37, TEXT).unwrap().len() as u64);
    assert_eq!(e.target, None);
    let l: Entry = info(&mut r, LINK).body_as().unwrap();
    assert_eq!((l.kind, l.target.as_deref()), (EntryKind::Symlink, Some(text_file(1208).as_str())));
    // A broken link is still a link.
    let b: Entry = info(&mut r, BROKEN).body_as().unwrap();
    assert_eq!(b.kind, EntryKind::Symlink);
    let d: Entry = info(&mut r, TEXT_DIR).body_as().unwrap();
    assert_eq!((d.name.as_str(), d.kind), ("text", EntryKind::Directory));
    let q: Entry = info(&mut r, "/QSYS.LIB/QGPL.LIB").body_as().unwrap();
    assert_eq!(q.kind, EntryKind::Directory);
    let bin: Entry = info(&mut r, BINARY_FILE).body_as().unwrap();
    assert_eq!(bin.ccsid, 65535);
    assert_eq!(said(&info(&mut r, BROKEN_TARGET)), (Status::NotFound, "CPFA0A9".into()));
    assert_eq!(said(&info(&mut r, PRIVATE_FILE)).0, Status::NotAuthorized);
    let m: Entry = info(&mut r, &text_file(MIXED)).body_as().unwrap();
    assert_eq!(m.ccsid, MIXED);
}

#[test]
fn read_as_stored() {
    let (c, mut r) = bob();
    let whole = stored(&c, &text_file(37));
    let d = data(&read(&mut r, &text_file(37), 0, 0, None));
    assert_eq!((d.data.as_slice(), d.size, d.next_offset), (whole.as_slice(), whole.len() as u64, whole.len() as u64));
    let d = data(&read(&mut r, &text_file(37), 5, 7, None));
    assert_eq!((d.data.as_slice(), d.next_offset), (&whole[5..12], 12));
    // The same CCSID is as stored.
    let d = data(&read(&mut r, &text_file(37), 0, 0, Some(37)));
    assert_eq!(d.data, whole);
    // Past the end: nothing, next_offset the offset.
    let d = data(&read(&mut r, &text_file(37), 1000, 5, None));
    assert_eq!((d.data.len(), d.next_offset), (0, 1000));
    // A link is followed; a broken one is not found.
    let d = data(&read(&mut r, LINK, 0, 0, None));
    assert_eq!(d.data, TEXT.as_bytes());
    assert_eq!(said(&read(&mut r, BROKEN, 0, 0, None)).0, Status::NotFound);
    assert_eq!(said(&read(&mut r, PRIVATE_FILE, 0, 0, None)), (Status::NotAuthorized, "CPFA09C".into()));
    assert_eq!(said(&read(&mut r, TEXT_DIR, 0, 0, None)).0, Status::Error);
    assert_eq!(read(&mut r, &text_file(37), 0, 0, Some(0)).status, Status::InvalidRequest);
}

#[test]
fn reads_convert_whole_characters() {
    let (c, mut r) = bob();
    for from in TEXT_CCSIDS {
        let path = text_file(from);
        let d = data(&read(&mut r, &path, 0, 0, Some(1208)));
        assert_eq!(String::from_utf8(d.data).unwrap(), TEXT, "{from}");
        for to in TEXT_CCSIDS {
            let whole = ccsid::conv::convert(from, to, &stored(&c, &path)).unwrap();
            // A step shorter than a character could not move on.
            let steps: &[u64] = if from == 1208 { &[4, 5, 7, 64] } else { &[1, 2, 3, 5, 7, 64] };
            for &step in steps {
                let parts = read_on(&mut r, &path, step, Some(to));
                assert_eq!(parts.concat(), whole, "{from} to {to} by {step}");
            }
        }
    }
    // UTF-8 split inside a character: the reply stops before it.
    let path = text_file(1208);
    let at = TEXT.find('ü').unwrap() as u64;
    let d = data(&read(&mut r, &path, 0, at + 1, Some(37)));
    assert_eq!(d.next_offset, at);
    assert_eq!(ccsid::conv::to_string(37, &d.data).unwrap(), &TEXT[..usize::try_from(at).unwrap()]);
    // Without conversion, next_offset is offset plus the bytes.
    let d = data(&read(&mut r, &path, 0, at + 1, None));
    assert_eq!(d.next_offset, at + 1);
}

#[test]
fn mixed_reads_join_into_the_whole_conversion() {
    let (c, mut r) = bob();
    let path = text_file(MIXED);
    let file = stored(&c, &path);
    assert_eq!(file, ccsid::conv::from_str(MIXED, JAPANESE).unwrap());
    let whole = ccsid::conv::convert(MIXED, 1208, &file).unwrap();
    assert_eq!(String::from_utf8(whole.clone()).unwrap(), JAPANESE);
    // Every step size puts boundaries inside pairs and at shifts.
    for step in [2, 3, 4, 5, 6, 7, 11, 64] {
        let parts = read_on(&mut r, &path, step, Some(1208));
        assert_eq!(parts.concat(), whole, "by {step}");
    }
    // A fresh connection reads from the middle without the memo: the
    // state is found by scanning from the start.
    let first = data(&read(&mut r, &path, 0, 9, Some(1208)));
    let mut r2 = bob_on(&c);
    let rest = data(&read(&mut r2, &path, first.next_offset, 0, Some(1208)));
    assert_eq!([first.data, rest.data].concat(), whole);
    // As stored, the file's own CCSID.
    assert_eq!(data(&read(&mut r, &path, 0, 0, Some(MIXED))).data, file);
    // To a mixed CCSID: each reply starts and ends in single-byte state.
    let utf8 = text_file(1208);
    let jp = c.files.as_user("BOB", 60, |cl| {
        let p = "/home/BOB/jp1208.txt";
        let fd = cl
            .open(
                p,
                ifs::ops::OpenArgs::new(ifs::ops::flags::O_WRONLY | ifs::ops::flags::O_CREAT | ifs::ops::flags::O_CCSID)
                    .mode(0o644)
                    .conv(1208),
            )
            .unwrap();
        ifs::client::write_all(cl, fd, JAPANESE.as_bytes()).unwrap();
        cl.close(fd).unwrap();
        p
    });
    let _ = utf8;
    for step in [4, 5, 9, 64] {
        let parts = read_on(&mut r, jp, step, Some(MIXED));
        for p in &parts {
            let last = p.iter().rev().find(|b| **b == 0x0E || **b == 0x0F);
            assert_ne!(last, Some(&0x0E), "a reply ends in double-byte state: {p:02X?}");
        }
        let text: String = parts.iter().map(|p| ccsid::conv::to_string(MIXED, p).unwrap()).collect();
        assert_eq!(text, JAPANESE, "by {step}");
    }
}

#[test]
fn binary_does_not_convert() {
    let (_c, mut r) = bob();
    let e = read(&mut r, BINARY_FILE, 0, 0, Some(1208));
    assert_eq!(said(&e), (Status::Error, "TCP1A5A".into()));
    assert!(e.body.is_empty());
    let d = data(&read(&mut r, BINARY_FILE, 0, 0, None));
    assert_eq!(d.data, qapd::testing::files::BINARY);
    let d = data(&read(&mut r, BINARY_FILE, 0, 0, Some(65535)));
    assert_eq!(d.data, qapd::testing::files::BINARY);
    assert_eq!(said(&read(&mut r, &text_file(37), 0, 0, Some(65535))).0, Status::Error);
}

#[test]
fn a_file_another_job_holds_is_in_use() {
    let (c, mut r) = bob();
    assert_eq!(said(&read(&mut r, LOCKED, 0, 0, None)), (Status::InUse, "CPFA09E".into()));
    assert_eq!(said(&write(&mut r, LOCKED, WriteMode::Append, 0, b"x", None)).0, Status::InUse);
    assert_eq!(said(&from_to(&mut r, Op::Copy, LOCKED, "/home/BOB/copy.txt", false)).0, Status::InUse);
    c.release_lock();
    assert_eq!(data(&read(&mut r, LOCKED, 0, 0, None)).data, b"held by another job\n");
    c.files.hold_lock();
    assert_eq!(read(&mut r, LOCKED, 0, 0, None).status, Status::InUse);
}

#[test]
fn a_byte_range_lock_of_another_job_is_in_use() {
    use ifs::lock::range::RangeKind;
    use ifs::ops::flags::{F_SETLK, O_RDWR, SEEK_SET};
    let (c, mut r) = bob();
    let path = text_file(1208);
    let fd = c.files.as_user("QSECOFR", 70, |cl| {
        let fd = cl.open(&path, ifs::ops::OpenArgs::new(O_RDWR)).unwrap();
        cl.lock_range(fd, F_SETLK, RangeKind::Write, SEEK_SET, 10, 5).unwrap();
        fd
    });
    assert_eq!(read(&mut r, &path, 0, 0, None).status, Status::InUse);
    assert_eq!(read(&mut r, &path, 0, 10, None).status, Status::Ok);
    assert_eq!(write(&mut r, &path, WriteMode::Update, 12, b"x", None).status, Status::InUse);
    c.files.as_user("QSECOFR", 70, |cl| cl.close(fd).unwrap());
    assert_eq!(read(&mut r, &path, 0, 0, None).status, Status::Ok);
}

#[test]
fn the_big_file_reads_in_frames() {
    let (c, mut r) = bob();
    let path = c.big_file();
    let d = data(&read(&mut r, path, 0, 0, None));
    assert_eq!((d.size, d.next_offset, d.data.len() as u64), (BIG_SIZE, BIG_SIZE, BIG_SIZE));
    for i in (0..BIG_SIZE).step_by(65_521) {
        assert_eq!(d.data[usize::try_from(i).unwrap()], big_byte(i), "{i}");
    }
    let d = data(&read(&mut r, path, BIG_SIZE - 3, 10, None));
    assert_eq!(d.data, [BIG_SIZE - 3, BIG_SIZE - 2, BIG_SIZE - 1].map(big_byte));
}

#[test]
fn write_modes() {
    let (c, mut r) = bob();
    let p = "/home/BOB/new.txt";
    assert_eq!(written(&write(&mut r, p, WriteMode::Create, 0, b"hello", None)), 5);
    let e: Entry = info(&mut r, p).body_as().unwrap();
    assert_eq!((e.ccsid, e.owner.as_str()), (1208, "BOB"));
    assert_eq!(said(&write(&mut r, p, WriteMode::Create, 0, b"x", None)), (Status::Exists, "CPFA0A0".into()));
    assert_eq!(written(&write(&mut r, p, WriteMode::Update, 1, b"EL", None)), 5);
    assert_eq!(written(&write(&mut r, p, WriteMode::Append, 0, b" world", None)), 11);
    assert_eq!(stored(&c, p), b"hELlo world");
    assert_eq!(written(&write(&mut r, p, WriteMode::Replace, 0, b"new", None)), 3);
    assert_eq!(stored(&c, p), b"new");
    // Update needs the file; Append and Replace make it.
    assert_eq!(said(&write(&mut r, "/home/BOB/none.txt", WriteMode::Update, 0, b"x", None)).0, Status::NotFound);
    assert_eq!(written(&write(&mut r, "/home/BOB/app.txt", WriteMode::Append, 0, b"ab", None)), 2);
    assert_eq!(said(&write(&mut r, "/home/BOB/nodir/x.txt", WriteMode::Create, 0, b"x", None)).0, Status::NotFound);
    assert_eq!(said(&write(&mut r, "/home/ANN/private/x.txt", WriteMode::Create, 0, b"x", None)).0, Status::NotAuthorized);
    // A new file takes the CCSID asked for.
    let e = r.client.call(
        Op::WriteFile,
        &WriteFile {
            path: "/home/BOB/e.txt".into(),
            mode: WriteMode::Create,
            ccsid: 37,
            data: b"AB".to_vec(),
            data_ccsid: Some(1208),
            ..WriteFile::default()
        },
    );
    assert_eq!(written(&e), 2);
    assert_eq!(stored(&c, "/home/BOB/e.txt"), [0xC1, 0xC2]);
    // The size is the file's, in its own CCSID: ü is two bytes of UTF-8,
    // one of 37.
    assert_eq!(written(&write(&mut r, "/home/BOB/e.txt", WriteMode::Append, 0, "ü".as_bytes(), Some(1208))), 3);
    assert_eq!(said(&write(&mut r, "/home/BOB/e.txt", WriteMode::Append, 0, b"x", Some(65535))), (Status::Error, "TCP1A5A".into()));
    assert_eq!(write(&mut r, "/home/BOB/e.txt", WriteMode::Append, 0, b"x", Some(0)).status, Status::InvalidRequest);
}

/// `text` in pieces of `n` characters.
fn pieces(text: &str, n: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    chars.chunks(n).map(|c| c.iter().collect()).collect()
}

#[test]
fn converted_writes_in_pieces_make_one_file() {
    let (c, mut r) = bob();
    for (ccsid, text) in [(37u16, TEXT), (273, TEXT), (MIXED, JAPANESE)] {
        let one = format!("/home/BOB/one{ccsid}.txt");
        let new = |path: &str| WriteFile {
            path: path.into(),
            mode: WriteMode::Create,
            ccsid,
            data: Vec::new(),
            data_ccsid: Some(1208),
            ..WriteFile::default()
        };
        let e = r.client.call(
            Op::WriteFile,
            &WriteFile {
                data: text.as_bytes().to_vec(),
                ..new(&one)
            },
        );
        let size = written(&e);
        let whole = stored(&c, &one);
        assert_eq!(whole, ccsid::conv::from_str(ccsid, text).unwrap());
        for n in [1, 2, 3, 5, 8] {
            let path = format!("/home/BOB/p{ccsid}-{n}.txt");
            written(&r.client.call(Op::WriteFile, &new(&path)));
            let mut last = 0;
            for piece in pieces(text, n) {
                last = written(&write(&mut r, &path, WriteMode::Append, 0, piece.as_bytes(), Some(1208)));
            }
            assert_eq!(stored(&c, &path), whole, "{ccsid} in pieces of {n}");
            assert_eq!(last, size);
        }
        // Update at the end joins too.
        let path = format!("/home/BOB/u{ccsid}.txt");
        written(&r.client.call(Op::WriteFile, &new(&path)));
        let mut at = 0;
        for piece in pieces(text, 4) {
            at = written(&write(&mut r, &path, WriteMode::Update, at, piece.as_bytes(), Some(1208)));
        }
        assert_eq!(stored(&c, &path), whole, "{ccsid} by Update");
    }
}

#[test]
fn create_directory() {
    let (c, mut r) = bob();
    let mk = |r: &mut Run, path: &str, parents: bool| {
        r.client.call(
            Op::CreateDirectory,
            &CreateDirectory {
                path: path.into(),
                parents,
            },
        )
    };
    assert_eq!(mk(&mut r, "/home/BOB/d", false).status, Status::Ok);
    assert_eq!(info(&mut r, "/home/BOB/d").body_as::<Entry>().unwrap().kind, EntryKind::Directory);
    assert_eq!(said(&mk(&mut r, "/home/BOB/d", false)), (Status::Exists, "CPFA0A0".into()));
    assert_eq!(said(&mk(&mut r, "/home/BOB/x/y/z", false)).0, Status::NotFound);
    assert_eq!(mk(&mut r, "/home/BOB/x/y/z", true).status, Status::Ok);
    assert_eq!(mk(&mut r, "/home/BOB/x/y/z", true).status, Status::Ok);
    assert_eq!(said(&mk(&mut r, "/home/BOB/many/f0001.txt/q", true)).0, Status::Error);
    // As MD answers BOB.
    let e = mk(&mut r, "/home/ANN/mine", false);
    assert_eq!(e.status, Status::NotAuthorized);
    let cmd = c.files.as_user("BOB", 50, |cl| md(cl, &Md::new("/home/ANN/mine")));
    same_message(&e, &cmd.escape.unwrap());
}

#[test]
fn delete() {
    let (c, mut r) = bob();
    let del_ = |r: &mut Run, path: &str| r.client.call(Op::Delete, &PathRef { path: path.into() });
    assert_eq!(del_(&mut r, &text_file(37)).status, Status::Ok);
    assert_eq!(said(&info(&mut r, &text_file(37))).0, Status::NotFound);
    let audit = c.files.audit.take();
    assert!(
        audit.iter().any(|a| a.kind == AuditKind::Delete && a.user == "BOB" && a.path == text_file(37)),
        "{audit:?}"
    );
    // A link goes, not what it names.
    assert_eq!(del_(&mut r, LINK).status, Status::Ok);
    assert_eq!(info(&mut r, &text_file(1208)).status, Status::Ok);
    assert_eq!(del_(&mut r, BROKEN).status, Status::Ok);
    // A directory only when empty.
    assert_eq!(said(&del_(&mut r, MANY)), (Status::Error, "CPFA0AC".into()));
    r.client.call(
        Op::CreateDirectory,
        &CreateDirectory {
            path: "/home/BOB/empty".into(),
            parents: false,
        },
    );
    assert_eq!(del_(&mut r, "/home/BOB/empty").status, Status::Ok);
    assert_eq!(said(&del_(&mut r, "/home/BOB/empty")), (Status::NotFound, "CPFA0A9".into()));
    // As RMVLNK answers BOB for a file of ANN's.
    c.files.as_user("ANN", 51, |cl| {
        let fd = cl
            .open(
                "/home/ANN/a.txt",
                ifs::ops::OpenArgs::new(ifs::ops::flags::O_WRONLY | ifs::ops::flags::O_CREAT).mode(0o644),
            )
            .unwrap();
        cl.close(fd).unwrap();
    });
    let e = del_(&mut r, "/home/ANN/a.txt");
    assert_eq!(e.status, Status::NotAuthorized);
    let cmd = c.files.as_user("BOB", 50, |cl| del(cl, "/home/ANN/a.txt"));
    same_message(&e, &cmd.escape.unwrap());
    assert_eq!(said(&del_(&mut r, PRIVATE_FILE)).0, Status::NotAuthorized);
}

#[test]
fn rename() {
    let (c, mut r) = bob();
    let a = "/home/BOB/a.txt";
    let b = "/home/BOB/many/b.txt";
    written(&write(&mut r, a, WriteMode::Create, 0, b"A", None));
    assert_eq!(from_to(&mut r, Op::Rename, a, b, false).status, Status::Ok);
    assert_eq!(said(&info(&mut r, a)).0, Status::NotFound);
    assert_eq!(stored(&c, b), b"A");
    written(&write(&mut r, a, WriteMode::Create, 0, b"A2", None));
    assert_eq!(said(&from_to(&mut r, Op::Rename, a, b, false)), (Status::Exists, "CPFA0A0".into()));
    assert_eq!(from_to(&mut r, Op::Rename, a, b, true).status, Status::Ok);
    assert_eq!(stored(&c, b), b"A2");
    assert_eq!(said(&from_to(&mut r, Op::Rename, a, b, true)).0, Status::NotFound);
    assert_eq!(said(&from_to(&mut r, Op::Rename, b, "/home/ANN/private/b.txt", false)).0, Status::NotAuthorized);
    // Directories move too.
    assert_eq!(from_to(&mut r, Op::Rename, TEXT_DIR, "/home/BOB/texts", false).status, Status::Ok);
    assert_eq!(info(&mut r, "/home/BOB/texts/text37.txt").status, Status::Ok);
}

#[test]
fn copy_keeps_the_ccsid() {
    let (c, mut r) = bob();
    let to = "/home/BOB/copy939.txt";
    assert_eq!(from_to(&mut r, Op::Copy, &text_file(MIXED), to, false).status, Status::Ok);
    let e: Entry = info(&mut r, to).body_as().unwrap();
    assert_eq!(e.ccsid, MIXED);
    assert_eq!(stored(&c, to), stored(&c, &text_file(MIXED)));
    assert_eq!(said(&from_to(&mut r, Op::Copy, &text_file(37), to, false)), (Status::Exists, "CPFA0A0".into()));
    assert_eq!(from_to(&mut r, Op::Copy, &text_file(37), to, true).status, Status::Ok);
    assert_eq!(info(&mut r, to).body_as::<Entry>().unwrap().ccsid, 37);
    assert_eq!(said(&from_to(&mut r, Op::Copy, TEXT_DIR, "/home/BOB/t2", false)), (Status::Error, "CPFA0DA".into()));
    assert_eq!(said(&from_to(&mut r, Op::Copy, PRIVATE_FILE, "/home/BOB/s.txt", false)).0, Status::NotAuthorized);
    assert_eq!(said(&from_to(&mut r, Op::Copy, BROKEN, "/home/BOB/s.txt", false)).0, Status::NotFound);
    let e = from_to(&mut r, Op::Copy, &text_file(37), "/home/ANN/private/x", false);
    assert_eq!(said(&e).0, Status::NotAuthorized);
}

#[test]
fn ifs_runs_beside_the_lane() {
    let (c, mut r) = bob();
    // A command holds the lane; Ifs operations still answer.
    let cmd = r.client.request(
        Op::RunCommand,
        &qap_proto::ops::command::CommandText {
            command: "DLYJOB DLY(2)".into(),
        },
    );
    assert!(c.wait_running(r.job, std::time::Duration::from_secs(5)).is_some());
    let id = r.client.request(Op::GetInfo, &PathRef { path: LINK.into() });
    assert_eq!(r.client.reply(id).status, Status::Ok);
    assert_eq!(r.client.reply(cmd).kind, qap_proto::Kind::Reply);
}

#[test]
fn not_before_sign_on() {
    let c = Controls::new();
    let mut r = start_with(cfg(), Arc::clone(&c));
    r.client.hello_range(5, 5);
    for op in Op::ALL.into_iter().filter(|o| o.code().0 == Service::Ifs) {
        assert_eq!(r.client.call(op, &PathRef::default()).status, Status::NotSignedOn, "{op:?}");
    }
    r.client.sign_on("BOB", "SECRET1");
    // A body that does not decode.
    let e = Envelope {
        body: vec![1, 2, 3],
        ..Envelope::request(90, Op::ReadFile, &())
    };
    r.client.send(&e);
    assert_eq!(r.client.reply(90).status, Status::InvalidRequest);
}
