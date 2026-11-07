// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 3's `Spool` service against the test system's printer
//! output: every status of every operation, page ranges that join into
//! the whole file and read as its pages do, the authority refusals,
//! reads beside a running command, and versions 1 to 3 negotiated.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use qap_proto::ops::command::CommandText;
use qap_proto::ops::connection::HelloReply;
use qap_proto::ops::spool::{
    HoldSpooledFile, ListOutputQueues, ListSpooledFiles, MoveSpooledFile, OutputQueues, ReadFormat,
    ReadSpooledFile, SpoolStatus, SpooledData, SpooledFile, SpooledFileId, SpooledFileRef,
    SpooledFiles,
};
use qap_proto::types::ObjectName;
use qap_proto::{Envelope, Op, Service, Status};

use common::{Controls, Run, cfg, start_with};

/// Version 3, signed on as `user`.
fn signed_on(c: &Arc<Controls>, user: &str, password: &str) -> Run {
    let mut r = start_with(cfg(), Arc::clone(c));
    let h: HelloReply = r.client.hello_range(1, 3).body_as().unwrap();
    assert_eq!(h.version, 3);
    r.client.sign_on(user, password);
    r
}

fn bob() -> (Arc<Controls>, Run) {
    let c = Controls::new();
    let r = signed_on(&c, "BOB", "SECRET1");
    (c, r)
}

fn q(library: &str, name: &str) -> ObjectName {
    ObjectName {
        library: library.into(),
        name: name.into(),
    }
}

fn list(r: &mut Run, req: &ListSpooledFiles) -> Envelope {
    r.client.call(Op::ListSpooledFiles, req)
}

fn all() -> ListSpooledFiles {
    ListSpooledFiles {
        user: "*ALL".into(),
        ..ListSpooledFiles::default()
    }
}

/// The file named `name`, from every file.
fn file(r: &mut Run, name: &str) -> SpooledFile {
    let l: SpooledFiles = list(r, &all()).body_as().unwrap();
    l.files.into_iter().find(|f| f.id.file == name).expect(name)
}

fn read(r: &mut Run, id: &SpooledFileId, format: ReadFormat, first: u32, last: u32) -> Envelope {
    r.client.call(
        Op::ReadSpooledFile,
        &ReadSpooledFile {
            id: id.clone(),
            format,
            first_page: first,
            last_page: last,
        },
    )
}

fn data(e: &Envelope) -> SpooledData {
    assert_eq!(e.status, Status::Ok, "{:?}", e.message);
    e.body_as().unwrap()
}

fn status_msg(e: &Envelope) -> (Status, String) {
    (
        e.status,
        e.message.as_ref().map(|m| m.id.clone()).unwrap_or_default(),
    )
}

#[test]
fn list_spooled_files_statuses() {
    let (_, mut r) = bob();
    // Ok: BOB's own by default, everyone's with *ALL, paged.
    let mine: SpooledFiles = list(&mut r, &ListSpooledFiles::default())
        .body_as()
        .unwrap();
    assert!(mine.files.iter().all(|f| f.user == "BOB"));
    let everyone: SpooledFiles = list(&mut r, &all()).body_as().unwrap();
    assert!(everyone.files.iter().any(|f| f.user == "ANN"));
    assert_eq!(everyone.total as usize, everyone.files.len());
    let on_qprint = ListSpooledFiles {
        outq: Some(q("QGPL", "QPRINT")),
        max: 25,
        ..all()
    };
    let p1: SpooledFiles = list(&mut r, &on_qprint).body_as().unwrap();
    assert_eq!((p1.total, p1.files.len()), (60, 25));
    let p3: SpooledFiles = list(
        &mut r,
        &ListSpooledFiles {
            start: 50,
            ..on_qprint.clone()
        },
    )
    .body_as()
    .unwrap();
    assert_eq!(p3.files.len(), 10);
    // The filters.
    let held: SpooledFiles = list(
        &mut r,
        &ListSpooledFiles {
            status: Some(SpoolStatus::Held),
            ..all()
        },
    )
    .body_as()
    .unwrap();
    assert!(!held.files.is_empty() && held.files.iter().all(|f| f.status == SpoolStatus::Held));
    let ud: SpooledFiles = list(
        &mut r,
        &ListSpooledFiles {
            user_data: Some("QSYSPRT".into()),
            ..all()
        },
    )
    .body_as()
    .unwrap();
    assert_eq!(ud.files.len(), 1);
    let ft: SpooledFiles = list(
        &mut r,
        &ListSpooledFiles {
            form_type: Some("*NONE".into()),
            ..all()
        },
    )
    .body_as()
    .unwrap();
    assert_eq!(ft.total, 0);
    let ann: SpooledFiles = list(
        &mut r,
        &ListSpooledFiles {
            user: "ANN".into(),
            ..all()
        },
    )
    .body_as()
    .unwrap();
    assert!(ann.total > 0 && ann.files.iter().all(|f| f.user == "ANN"));
    // NotFound: the queue, the profile. NotAuthorized: a queue BOB may
    // not use.
    let nq = list(
        &mut r,
        &ListSpooledFiles {
            outq: Some(q("QGPL", "NOSUCH")),
            ..all()
        },
    );
    assert_eq!(status_msg(&nq), (Status::NotFound, "CPF3357".into()));
    let np = list(
        &mut r,
        &ListSpooledFiles {
            user: "NOBODY".into(),
            ..all()
        },
    );
    assert_eq!(status_msg(&np), (Status::NotFound, "CPF2204".into()));
    let na = list(
        &mut r,
        &ListSpooledFiles {
            outq: Some(q("QUSRSYS", "PRIVATE")),
            ..all()
        },
    );
    assert_eq!(status_msg(&na), (Status::NotAuthorized, "CPF3303".into()));
}

#[test]
fn read_spooled_file_statuses() {
    let (_, mut r) = bob();
    let listing = file(&mut r, "QSYSPRT");
    assert_eq!(listing.pages, 30);
    // Ok in each format.
    let scs = data(&read(&mut r, &listing.id, ReadFormat::Scs, 1, 0));
    assert_eq!((scs.format, scs.pages), (ReadFormat::Scs, 30));
    let text = data(&read(&mut r, &listing.id, ReadFormat::Text, 1, 0));
    let text = String::from_utf8(text.data).unwrap();
    assert_eq!(text.split('\x0c').count(), 30);
    assert!(text.lines().any(|l| l.len() == 132), "132 columns");
    let pdf = data(&read(&mut r, &listing.id, ReadFormat::Pdf, 2, 3));
    assert_eq!(spool::pdf::check(&pdf.data).unwrap(), 2);
    // NotFound.
    let mut gone = listing.id.clone();
    gone.number = 999;
    assert_eq!(
        status_msg(&read(&mut r, &gone, ReadFormat::Scs, 1, 0)),
        (Status::NotFound, "CPF3309".into())
    );
    // NotAuthorized: ANN's file on a queue that shows data to no one else.
    let ann = file(&mut r, "ANNRPT01");
    assert_eq!(
        status_msg(&read(&mut r, &ann.id, ReadFormat::Text, 1, 0)),
        (Status::NotAuthorized, "CPF3303".into())
    );
    // InUse: open, and being written.
    let open = file(&mut r, "OPENFILE");
    assert_eq!(
        status_msg(&read(&mut r, &open.id, ReadFormat::Scs, 1, 0)),
        (Status::InUse, "TCP1A57".into())
    );
    let writing = file(
        &mut r,
        &format!(
            "STS{}",
            SpoolStatus::ALL
                .iter()
                .position(|s| *s == SpoolStatus::Writing)
                .unwrap()
        ),
    );
    assert_eq!(
        read(&mut r, &writing.id, ReadFormat::Scs, 1, 0).status,
        Status::InUse
    );
    // Error: no format of an AFP file, the message says so.
    let afp = file(&mut r, "AFPINV");
    for f in [ReadFormat::Scs, ReadFormat::Text, ReadFormat::Pdf] {
        let e = read(&mut r, &afp.id, f, 1, 0);
        assert_eq!(status_msg(&e), (Status::Error, "TCP1A56".into()), "{f:?}");
        assert!(e.message.unwrap().text.contains("AFPINV"));
    }
    // InvalidRequest: pages that are not the file's, a bad body.
    for (first, last) in [(0, 0), (31, 0), (5, 4)] {
        assert_eq!(
            read(&mut r, &listing.id, ReadFormat::Scs, first, last).status,
            Status::InvalidRequest
        );
    }
    // Past the end reads to the end.
    assert_eq!(
        data(&read(&mut r, &listing.id, ReadFormat::Scs, 29, 99)).data,
        data(&read(&mut r, &listing.id, ReadFormat::Scs, 29, 0)).data
    );
    let bad = Envelope::request(900, Op::ReadSpooledFile, &[7u8; 3]);
    r.client.send(&bad);
    assert_eq!(r.client.reply(900).status, Status::InvalidRequest);
}

#[test]
fn page_ranges_join_into_the_whole_file() {
    let (_, mut r) = bob();
    // In page batches, as a client reads a large file; OVERFLOW's second
    // batch of 25 starts with its overflow page.
    for (name, batch) in [
        ("QSYSPRT", 7),
        ("MIXED", 7),
        ("QPJOBLOG", 7),
        ("LANDSCAPE", 7),
        ("OVERFLOW", 25),
    ] {
        let f = file(&mut r, name);
        let whole = data(&read(&mut r, &f.id, ReadFormat::Scs, 1, 0));
        let idx = spool::scs::index(&whole.data);
        assert_eq!(idx.len(), whole.pages as usize, "{name}");
        let (pages, _) = spool::scs::decode(&whole.data);
        let mut joined = Vec::new();
        let mut first = 1;
        while first <= whole.pages {
            let last = (first + batch - 1).min(whole.pages);
            let part = data(&read(&mut r, &f.id, ReadFormat::Scs, first, last));
            let (got, _) = spool::scs::decode(&part.data);
            assert_eq!(
                got,
                pages[first as usize - 1..last as usize],
                "{name} {first}: the pages"
            );
            assert_eq!(part.pages, whole.pages);
            let state = idx[first as usize - 1].prefix();
            assert!(
                part.data.starts_with(&state),
                "{name} {first}: the state first"
            );
            joined.extend_from_slice(&part.data[state.len()..]);
            first = last + 1;
        }
        assert_eq!(joined, whole.data, "{name}");
    }
}

#[test]
fn a_range_from_mid_file_renders_as_in_the_whole() {
    let (_, mut r) = bob();
    let f = file(&mut r, "MIXED");
    let whole = data(&read(&mut r, &f.id, ReadFormat::Scs, 1, 0));
    let (pages, _) = spool::scs::decode(&whole.data);
    assert!(pages.len() >= 4);
    // Pages 3 and 4 are in 15 pitch and 8 lines per inch: the range
    // starts in them, and its pages are the whole's.
    let part = data(&read(&mut r, &f.id, ReadFormat::Scs, 3, 4));
    let (got, fmt) = spool::scs::decode(&part.data);
    assert_eq!(got, pages[2..4]);
    assert_eq!((fmt.cpi, fmt.lpi), (15, 8));
    // A page that starts with a line past the page length.
    let one = data(&read(&mut r, &f.id, ReadFormat::Scs, 4, 4));
    assert_eq!(spool::scs::decode(&one.data).0, pages[3..4]);
    // Text and PDF of a range are the same pages of the whole.
    let text = String::from_utf8(data(&read(&mut r, &f.id, ReadFormat::Text, 1, 0)).data).unwrap();
    let all: Vec<&str> = text.split('\x0c').collect();
    let mid = String::from_utf8(data(&read(&mut r, &f.id, ReadFormat::Text, 3, 4)).data).unwrap();
    assert_eq!(mid, all[2..4].join("\x0c"));
    let pdf = data(&read(&mut r, &f.id, ReadFormat::Pdf, 3, 4));
    assert_eq!(
        pdf.data,
        spool::pdf::render(&pages[2..4], spool::scs::decode(&whole.data).1)
    );
}

fn hold(r: &mut Run, id: &SpooledFileId) -> Envelope {
    r.client.call(
        Op::HoldSpooledFile,
        &HoldSpooledFile {
            id: id.clone(),
            immediately: false,
        },
    )
}

fn by_ref(r: &mut Run, op: Op, id: &SpooledFileId) -> Envelope {
    r.client.call(op, &SpooledFileRef { id: id.clone() })
}

fn mv(r: &mut Run, id: &SpooledFileId, outq: ObjectName, first: bool) -> Envelope {
    r.client.call(
        Op::MoveSpooledFile,
        &MoveSpooledFile {
            id: id.clone(),
            outq,
            first,
        },
    )
}

#[test]
fn joblog_page_2_alone_starts_where_it_does_in_the_whole() {
    let (_, mut r) = bob();
    let f = file(&mut r, "QPJOBLOG");
    let whole = data(&read(&mut r, &f.id, ReadFormat::Scs, 1, 0));
    let (pages, _) = spool::scs::decode(&whole.data);
    let idx = spool::scs::index(&whole.data);
    let part = data(&read(&mut r, &f.id, ReadFormat::Scs, 2, 2));
    // The state moves to the line absolutely: X'34C4', line.
    let state = idx[1].prefix();
    let line = u8::try_from(idx[1].line).unwrap();
    assert!(
        state.windows(3).any(|w| w == [0x34, 0xC4, line]),
        "{state:02X?}"
    );
    assert!(part.data.starts_with(&state));
    let (got, _) = spool::scs::decode(&part.data);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].lines.keys().next(), pages[1].lines.keys().next());
    assert_eq!(got[0], pages[1]);
}

#[test]
fn hold_release_delete_move_statuses() {
    let (_, mut r) = bob();
    let f = file(&mut r, "RPT001");
    let mut gone = f.id.clone();
    gone.file = "NOSUCH".into();
    let ann = file(&mut r, "ANNRPT02").id;
    // Hold: Ok (at the end of the page), NotFound, NotAuthorized.
    assert_eq!(hold(&mut r, &f.id).status, Status::Ok);
    assert_eq!(file(&mut r, "RPT001").status, SpoolStatus::Held);
    assert_eq!(hold(&mut r, &gone).status, Status::NotFound);
    assert_eq!(
        status_msg(&hold(&mut r, &ann)),
        (Status::NotAuthorized, "CPF3303".into())
    );
    // Release: Ok, then Error (neither held nor saved), NotFound,
    // NotAuthorized.
    assert_eq!(
        by_ref(&mut r, Op::ReleaseSpooledFile, &f.id).status,
        Status::Ok
    );
    assert_eq!(file(&mut r, "RPT001").status, SpoolStatus::Ready);
    assert_eq!(
        status_msg(&by_ref(&mut r, Op::ReleaseSpooledFile, &f.id)),
        (Status::Error, "CPF3337".into())
    );
    assert_eq!(
        by_ref(&mut r, Op::ReleaseSpooledFile, &gone).status,
        Status::NotFound
    );
    assert_eq!(
        by_ref(&mut r, Op::ReleaseSpooledFile, &ann).status,
        Status::NotAuthorized
    );
    // Move: Ok to the top of another queue, NotFound (the queue, the
    // file), NotAuthorized, InUse while printing.
    assert_eq!(
        mv(&mut r, &f.id, q("QGPL", "PRT01"), true).status,
        Status::Ok
    );
    let on: SpooledFiles = list(
        &mut r,
        &ListSpooledFiles {
            outq: Some(q("QGPL", "PRT01")),
            ..all()
        },
    )
    .body_as()
    .unwrap();
    assert_eq!(on.files[0].id, f.id, "at the top");
    assert_eq!(
        mv(&mut r, &f.id, q("QGPL", "QPRINT"), false).status,
        Status::Ok
    );
    let back: SpooledFiles = list(
        &mut r,
        &ListSpooledFiles {
            outq: Some(q("QGPL", "QPRINT")),
            ..all()
        },
    )
    .body_as()
    .unwrap();
    assert_eq!(back.files.last().unwrap().id, f.id, "at the end");
    assert_eq!(
        status_msg(&mv(&mut r, &f.id, q("QGPL", "NOSUCH"), false)),
        (Status::NotFound, "CPF3357".into())
    );
    assert_eq!(
        mv(&mut r, &gone, q("QGPL", "PRT01"), false).status,
        Status::NotFound
    );
    assert_eq!(
        mv(&mut r, &ann, q("QGPL", "PRT01"), false).status,
        Status::NotAuthorized
    );
    let printing = file(
        &mut r,
        &format!(
            "STS{}",
            SpoolStatus::ALL
                .iter()
                .position(|s| *s == SpoolStatus::Printing)
                .unwrap()
        ),
    );
    assert_eq!(
        status_msg(&mv(&mut r, &printing.id, q("QGPL", "QPRINT"), false)),
        (Status::InUse, "CPF3330".into())
    );
    // Delete: InUse while printing, NotAuthorized, Ok, then NotFound.
    assert_eq!(
        by_ref(&mut r, Op::DeleteSpooledFile, &printing.id).status,
        Status::InUse
    );
    assert_eq!(
        by_ref(&mut r, Op::DeleteSpooledFile, &ann).status,
        Status::NotAuthorized
    );
    assert_eq!(
        by_ref(&mut r, Op::DeleteSpooledFile, &f.id).status,
        Status::Ok
    );
    assert_eq!(
        by_ref(&mut r, Op::DeleteSpooledFile, &f.id).status,
        Status::NotFound
    );
    // A held open file goes back to open when released.
    let open = file(&mut r, "OPENFILE");
    assert_eq!(hold(&mut r, &open.id).status, Status::Ok);
    assert_eq!(
        by_ref(&mut r, Op::ReleaseSpooledFile, &open.id).status,
        Status::Ok
    );
    assert_eq!(file(&mut r, "OPENFILE").status, SpoolStatus::Open);
}

#[test]
fn list_output_queues_statuses() {
    let (_, mut r) = bob();
    let lq = |r: &mut Run, library: &str, start: u32, max: u32| {
        r.client.call(
            Op::ListOutputQueues,
            &ListOutputQueues {
                library: library.into(),
                start,
                max,
            },
        )
    };
    let every: OutputQueues = lq(&mut r, "*ALL", 0, 0).body_as().unwrap();
    assert_eq!(every.total, 3);
    let qprint = every
        .queues
        .iter()
        .find(|x| x.outq.name == "QPRINT")
        .unwrap();
    assert_eq!(
        (qprint.files, qprint.writer.as_str(), qprint.held),
        (60, "", false)
    );
    let prt01 = every
        .queues
        .iter()
        .find(|x| x.outq.name == "PRT01")
        .unwrap();
    assert_eq!(prt01.writer, "PRT01");
    let qgpl: OutputQueues = lq(&mut r, "QGPL", 1, 1).body_as().unwrap();
    assert_eq!((qgpl.total, qgpl.queues.len()), (2, 1));
    let libl: OutputQueues = lq(&mut r, "*LIBL", 0, 0).body_as().unwrap();
    assert_eq!(libl.total, 3);
    assert_eq!(
        status_msg(&lq(&mut r, "NOLIB", 0, 0)),
        (Status::NotFound, "CPF2110".into())
    );
    assert_eq!(
        status_msg(&lq(&mut r, "SECLIB", 0, 0)),
        (Status::NotAuthorized, "CPF9802".into())
    );
}

#[test]
fn spool_reads_beside_a_running_command() {
    let (_, mut r) = bob();
    let listing = file(&mut r, "QSYSPRT");
    let at = Instant::now();
    let cmd = r.client.request(
        Op::RunCommand,
        &CommandText {
            command: "DLYJOB DLY(3)".into(),
        },
    );
    std::thread::sleep(Duration::from_millis(50));
    let reads: Vec<u32> = (0..4)
        .map(|i| {
            r.client.request(
                Op::ReadSpooledFile,
                &ReadSpooledFile {
                    id: listing.id.clone(),
                    format: ReadFormat::Scs,
                    first_page: 1 + i * 7,
                    last_page: 7 + i * 7,
                },
            )
        })
        .collect();
    let listed = r.client.request(Op::ListSpooledFiles, &all());
    let mut ids = reads.clone();
    ids.push(listed);
    let got = r.client.replies(&ids);
    assert!(got.values().all(|e| e.status == Status::Ok));
    assert!(
        at.elapsed() < Duration::from_secs(2),
        "answered while the command ran"
    );
    assert_eq!(r.client.reply(cmd).status, Status::Ok);
}

#[test]
fn versions_1_2_and_3() {
    let c = Controls::new();
    let spool_ops = [
        Op::ListSpooledFiles,
        Op::ReadSpooledFile,
        Op::HoldSpooledFile,
        Op::ReleaseSpooledFile,
        Op::DeleteSpooledFile,
        Op::MoveSpooledFile,
        Op::ListOutputQueues,
    ];
    for v in 1..=3u16 {
        let mut r = start_with(cfg(), Arc::clone(&c));
        let h: HelloReply = r.client.hello_range(1, v).body_as().unwrap();
        assert_eq!(h.version, v);
        assert_eq!(h.services.contains(&Service::Spool), v >= 3, "version {v}");
        assert_eq!(
            h.services.contains(&Service::Command),
            v >= 2,
            "version {v}"
        );
        r.client.sign_on("BOB", "SECRET1");
        for op in spool_ops {
            let e = match op {
                Op::ListSpooledFiles => r.client.call(op, &ListSpooledFiles::default()),
                Op::ReadSpooledFile => r.client.call(
                    op,
                    &ReadSpooledFile {
                        first_page: 1,
                        ..ReadSpooledFile::default()
                    },
                ),
                Op::HoldSpooledFile => r.client.call(op, &HoldSpooledFile::default()),
                Op::MoveSpooledFile => r.client.call(op, &MoveSpooledFile::default()),
                Op::ListOutputQueues => r.client.call(op, &ListOutputQueues::default()),
                _ => r.client.call(op, &SpooledFileRef::default()),
            };
            if v < 3 {
                // Not this version's: refused, nothing sent it does not know.
                assert_eq!(
                    (e.status, e.body.is_empty()),
                    (Status::InvalidRequest, true),
                    "{op:?} on {v}"
                );
            } else {
                // Known: answered, here for a file that is not there.
                let want = if matches!(op, Op::ListSpooledFiles | Op::ListOutputQueues) {
                    Status::Ok
                } else {
                    Status::NotFound
                };
                assert_eq!(e.status, want, "{op:?}");
            }
        }
    }
    // Before sign-on, version 3's operations are NotSignedOn.
    let mut r = start_with(cfg(), Arc::clone(&c));
    r.client.hello_range(3, 3);
    assert_eq!(
        r.client
            .call(Op::ListOutputQueues, &ListOutputQueues::default())
            .status,
        Status::NotSignedOn
    );
}
