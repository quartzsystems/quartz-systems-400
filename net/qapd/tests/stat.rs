// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 6's `Stat` against the test system's file system: what
//! `GetInfo` answers and its statuses, the change counter rising on
//! changes made through QAP and on the system, two changes within one
//! millisecond, and version 5 refused.

#![allow(clippy::many_single_char_names)]

mod common;

use std::sync::Arc;

use ifs::ops::Attr;
use qap_proto::ops::connection::HelloReply;
use qap_proto::ops::ifs::{Entry, EntryKind, FromTo, PathRef, Stat, WriteFile, WriteMode};
use qap_proto::{Envelope, Op, Status};
use qapd::testing::files::{BROKEN, BROKEN_TARGET, LINK, PRIVATE_FILE, TEXT_DIR, text_file};

use common::{Controls, Run, cfg, start_with};

/// Version 6, signed on as `BOB`.
fn bob() -> (Arc<Controls>, Run) {
    let c = Controls::new();
    let mut r = start_with(cfg(), Arc::clone(&c));
    let h: HelloReply = r.client.hello_range(1, 6).body_as().unwrap();
    assert_eq!(h.version, 6);
    r.client.sign_on("BOB", "SECRET1");
    (c, r)
}

fn stat_of(r: &mut Run, path: &str) -> Envelope {
    r.client.call(Op::Stat, &PathRef { path: path.into() })
}

fn stat(r: &mut Run, path: &str) -> Stat {
    let e = stat_of(r, path);
    assert_eq!(e.status, Status::Ok, "{path}: {e:?}");
    e.body_as().unwrap()
}

fn info(r: &mut Run, path: &str) -> Envelope {
    r.client.call(Op::GetInfo, &PathRef { path: path.into() })
}

/// The status and message (identifier and data) of a reply.
fn said(e: &Envelope) -> (Status, Option<(String, Vec<u8>)>) {
    (
        e.status,
        e.message.as_ref().map(|m| (m.id.clone(), m.data.clone())),
    )
}

#[test]
fn stat_answers_what_get_info_answers() {
    let (_c, mut r) = bob();
    for p in [
        text_file(37),
        TEXT_DIR.to_string(),
        LINK.to_string(),
        BROKEN.to_string(),
    ] {
        let s = stat(&mut r, &p);
        let e: Entry = info(&mut r, &p).body_as().unwrap();
        assert_eq!(s.entry, e, "{p}");
        assert!(s.change > 0, "{p}");
    }
    assert_eq!(stat(&mut r, &text_file(37)).entry.kind, EntryKind::File);
    assert_eq!(stat(&mut r, TEXT_DIR).entry.kind, EntryKind::Directory);
    // A link is not followed: it has a counter of its own.
    let l = stat(&mut r, LINK);
    assert_eq!(
        (l.entry.kind, l.entry.target.as_deref()),
        (EntryKind::Symlink, Some(text_file(1208).as_str()))
    );
    assert_ne!(l.change, stat(&mut r, &text_file(1208)).change);
    // Statuses and messages as GetInfo's.
    for p in [BROKEN_TARGET, PRIVATE_FILE] {
        assert_eq!(said(&stat_of(&mut r, p)), said(&info(&mut r, p)), "{p}");
    }
    assert_eq!(stat_of(&mut r, BROKEN_TARGET).status, Status::NotFound);
    assert_eq!(stat_of(&mut r, PRIVATE_FILE).status, Status::NotAuthorized);
}

#[test]
fn changes_through_qap_raise_it() {
    let (_c, mut r) = bob();
    let a = "/home/BOB/a.txt";
    let w = |r: &mut Run, mode, data: &[u8]| {
        let e = r.client.call(
            Op::WriteFile,
            &WriteFile {
                path: a.into(),
                mode,
                offset: 0,
                ccsid: 0,
                data: data.to_vec(),
                data_ccsid: None,
            },
        );
        assert_eq!(e.status, Status::Ok, "{e:?}");
    };
    w(&mut r, WriteMode::Create, b"one");
    let first = stat(&mut r, a).change;
    w(&mut r, WriteMode::Append, b"two");
    let second = stat(&mut r, a).change;
    assert!(second > first);
    // Its directory changed when it was made.
    let home = stat(&mut r, "/home/BOB").change;
    let b = "/home/BOB/b.txt";
    let e = r.client.call(
        Op::Rename,
        &FromTo {
            from: a.into(),
            to: b.into(),
            replace: false,
        },
    );
    assert_eq!(e.status, Status::Ok, "{e:?}");
    assert!(stat(&mut r, b).change > second);
    assert!(stat(&mut r, "/home/BOB").change > home);
}

#[test]
fn changes_on_the_system_raise_it() {
    let (c, mut r) = bob();
    let p = text_file(819);
    let before = stat(&mut r, &p);
    c.files
        .as_user("QSECOFR", 9, |cl| {
            cl.set_attr(&p, &Attr::Ccsid(1252), false)
        })
        .unwrap();
    let ccsid = stat(&mut r, &p);
    assert_eq!(ccsid.entry.ccsid, 1252);
    assert!(ccsid.change > before.change);
    c.files
        .as_user("QSECOFR", 9, |cl| cl.chgown(&p, "ANN", false))
        .unwrap();
    let owner = stat(&mut r, &p);
    assert_eq!(owner.entry.owner, "ANN");
    assert!(owner.change > ccsid.change);
}

#[test]
fn two_changes_in_one_millisecond() {
    let (c, mut r) = bob();
    let p = text_file(37);
    let before = stat(&mut r, &p);
    c.files.change_twice(&p);
    let after = stat(&mut r, &p);
    // Same size, same time to the millisecond: only the counter tells.
    assert_eq!(after.entry.modified, before.entry.modified);
    assert_eq!(after.entry, before.entry);
    assert!(
        after.change >= before.change + 2,
        "{} then {}",
        before.change,
        after.change
    );
}

#[test]
fn version_5_has_no_stat() {
    let c = Controls::new();
    let mut r = start_with(cfg(), Arc::clone(&c));
    let h: HelloReply = r.client.hello_range(1, 5).body_as().unwrap();
    assert_eq!(h.version, 5);
    r.client.sign_on("BOB", "SECRET1");
    assert_eq!(
        stat_of(&mut r, &text_file(37)).status,
        Status::InvalidRequest
    );
    assert_eq!(info(&mut r, &text_file(37)).status, Status::Ok);
}
