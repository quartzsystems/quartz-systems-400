// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Streams: several at once, one stalled, credit, close and reset.

mod common;

use std::collections::BTreeMap;

use common::frames;
use qsi::base;
use qsi::msgid;
use qsisess::loopback::Loopback;
use qsisess::wire::{self, Reset, frame, kind};
use qsisess::{Error, Incoming, StreamId};

/// Open a stream: a request answered with one.
fn open(l: &mut Loopback) -> StreamId {
    let id = l
        .console
        .request_msg("U", &base::Request::Identify(qsi::Empty))
        .unwrap();
    l.pump();
    l.take_request(id).unwrap();
    let s = l.managed.reply_stream(id, b"go".to_vec()).unwrap();
    l.pump();
    let r = l.take_reply(id).unwrap();
    assert_eq!(r.stream, Some(s));
    s
}

/// Data the console received, by stream, taken.
fn received(l: &mut Loopback) -> BTreeMap<StreamId, usize> {
    let mut got = BTreeMap::new();
    l.at_console.retain(|i| match i {
        Incoming::Data { stream, bytes } => {
            *got.entry(*stream).or_default() += bytes.len();
            false
        }
        _ => true,
    });
    got
}

#[test]
fn three_at_once_one_stalled() {
    const LEN: usize = 1024 * 1024;
    let mut l = Loopback::new(true);
    l.piece = 4096;
    let streams = [open(&mut l), open(&mut l), open(&mut l)];
    assert_eq!(streams, [1, 2, 3]);
    let stalled = streams[2];
    let data = vec![0xA5u8; LEN];
    let mut sent = [0usize; 3];
    let mut got = [0usize; 3];
    let mut requests = 0;
    for _ in 0..200 {
        for (i, s) in streams.iter().enumerate() {
            sent[i] += l.managed.send(*s, &data[sent[i]..]).unwrap();
        }
        l.pump();
        for (s, n) in received(&mut l) {
            let i = streams.iter().position(|x| *x == s).unwrap();
            got[i] += n;
            // The console consumes all but the stalled stream.
            if s != stalled {
                l.console.grant(s, u32::try_from(n).unwrap()).unwrap();
            }
        }
        l.pump();
        // Requests still go through.
        let id = l
            .console
            .request_msg("U", &base::Request::Time(qsi::Empty))
            .unwrap();
        l.pump();
        l.take_request(id).unwrap();
        l.managed
            .reply_msg(
                id,
                &base::Reply::Time(base::Time {
                    unix_ns: 0,
                    offset_s: 0,
                    zone: String::new(),
                }),
            )
            .unwrap();
        l.pump();
        assert!(l.take_reply(id).is_some());
        requests += 1;
        if got[0] == LEN && got[1] == LEN {
            break;
        }
    }
    assert_eq!((got[0], got[1]), (LEN, LEN));
    assert_eq!(got[2], 256 * 1024);
    assert_eq!(sent[2], 256 * 1024);
    assert_eq!(l.managed.credit(stalled), Some(0));
    assert!(requests > 1);

    // Uploads the other way, within the managed side's window.
    let up = l.console.send(streams[0], &data).unwrap();
    assert_eq!(up, 256 * 1024);
    l.pump();
    assert!(
        l.at_managed
            .iter()
            .any(|i| matches!(i, Incoming::Data { stream, .. } if *stream == streams[0]))
    );

    // Close one way, then the other: the stream is gone at both.
    l.managed.close_stream(streams[0]).unwrap();
    assert_eq!(
        l.managed.send(streams[0], b"x").unwrap_err(),
        Error::StreamClosed(streams[0])
    );
    l.pump();
    assert!(
        l.at_console
            .contains(&Incoming::StreamClosed { stream: streams[0] })
    );
    // The console may still send.
    assert!(l.console.send(streams[0], b"").is_ok());
    l.console.close_stream(streams[0]).unwrap();
    l.pump();
    assert!(
        l.at_managed
            .contains(&Incoming::StreamClosed { stream: streams[0] })
    );
    assert_eq!(l.console.open_streams(), 2);
    assert_eq!(l.managed.open_streams(), 2);

    // Reset by the console, then by the managed side.
    l.console
        .reset_stream(stalled, "QSI0399", "no more")
        .unwrap();
    l.pump();
    assert!(l.at_managed.iter().any(|i| matches!(i,
        Incoming::StreamReset { stream, msgid, by_peer: true, .. } if *stream == stalled && msgid == "QSI0399")));
    l.managed
        .reset_stream(streams[1], "QSI0398", "gone")
        .unwrap();
    l.pump();
    assert!(l.at_console.iter().any(|i| matches!(i,
        Incoming::StreamReset { stream, by_peer: true, .. } if *stream == streams[1])));
    assert_eq!(l.console.open_streams(), 0);
    assert_eq!(l.managed.open_streams(), 0);
    assert_eq!(
        l.console.send(stalled, b"x").unwrap_err(),
        Error::NoStream(stalled)
    );
    assert!(l.console.is_open() && l.managed.is_open());
}

#[test]
fn data_beyond_the_credit_resets() {
    let mut l = Loopback::new(true);
    let s = open(&mut l);
    // A peer that ignores its credit: nine full frames, 288 KiB.
    let chunk = vec![0u8; qsipart::frame::MAX_BODY - 4];
    for _ in 0..9 {
        l.console
            .feed(&frame(kind::DATA, &wire::data_body(s, &chunk)).unwrap(), 0);
    }
    let resets: Vec<Reset> = frames(&l.console.take_output())
        .into_iter()
        .filter(|f| f.kind == kind::RESET)
        .map(|f| wire::read(&f.body).unwrap())
        .collect();
    assert_eq!(resets.len(), 1);
    assert_eq!(
        (resets[0].stream, resets[0].msgid.as_str()),
        (s, msgid::NO_CREDIT)
    );
    let items: Vec<_> = std::iter::from_fn(|| l.console.poll()).collect();
    assert!(items.iter().any(|i| matches!(i,
        Incoming::StreamReset { msgid, by_peer: false, .. } if msgid == msgid::NO_CREDIT)));
    // Data still in flight for it afterwards is passed over.
    l.console
        .feed(&frame(kind::DATA, &wire::data_body(s, b"late")).unwrap(), 0);
    assert!(l.console.take_output().is_empty());
    assert!(l.console.is_open());
}

#[test]
fn a_stream_never_opened() {
    let mut l = Loopback::new(true);
    l.console
        .feed(&frame(kind::DATA, &wire::data_body(99, b"x")).unwrap(), 0);
    let out = frames(&l.console.take_output());
    assert_eq!(out.len(), 1);
    let r: Reset = wire::read(&out[0].body).unwrap();
    assert_eq!((r.stream, r.msgid.as_str()), (99, msgid::NO_STREAM));
    assert!(l.console.poll().is_none());
    // Credit and close for it too; a reset for it is passed over.
    l.managed.feed(
        &frame(
            kind::CREDIT,
            &ipc::encode(&wire::Credit {
                stream: 5,
                grant: 9,
            }),
        )
        .unwrap(),
        0,
    );
    assert_eq!(frames(&l.managed.take_output()).len(), 1);
    l.managed
        .feed(&frame(kind::RESET, &ipc::encode(&r)).unwrap(), 0);
    assert!(l.managed.take_output().is_empty());
    assert!(l.console.is_open() && l.managed.is_open());
    assert_eq!(l.console.grant(99, 1).unwrap_err(), Error::NoStream(99));
}

#[test]
fn the_session_end_resets_open_streams() {
    let mut l = Loopback::new(true);
    let s = open(&mut l);
    l.managed.close("going down");
    l.pump();
    for items in [&l.at_console, &l.at_managed] {
        assert!(items.iter().any(|i| matches!(i,
            Incoming::StreamReset { stream, msgid, .. } if *stream == s && msgid == msgid::CLOSING)));
    }
}
