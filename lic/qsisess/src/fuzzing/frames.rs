// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The frame decoder: bytes in pieces into a console and a managed
//! session that already have a good hello.

use super::{answer, drain, look};
use crate::loopback::{console_hello, host_hello};
use crate::wire::{frame, kind};
use crate::{EventLog, Limits, Role, Session};

/// One input. The first byte chooses the piece size and whether the
/// managed side's peer is pinned.
pub fn frames(data: &[u8]) {
    let Some((&first, rest)) = data.split_first() else {
        return;
    };
    let piece = usize::from(first % 61) + 1;
    let pinned = first & 0x80 == 0;
    let limits = Limits {
        max_message: 256 * 1024,
        ..Limits::default()
    };
    let (Ok(mut console), Ok(mut managed)) = (
        Session::new(Role::Console, console_hello(), true, limits, 0),
        Session::new(Role::Managed, host_hello(), pinned, limits, 0),
    ) else {
        return;
    };
    let mut log = EventLog::new(&limits);
    let _ = log.push(qsi::family::BASE, qsi::base::event::STATE, vec![0; 3]);
    console.feed(
        &frame(kind::HELLO, &ipc::encode(&host_hello())).unwrap_or_default(),
        0,
    );
    managed.feed(
        &frame(kind::HELLO, &ipc::encode(&console_hello())).unwrap_or_default(),
        0,
    );
    let mut now = 0;
    for chunk in rest.chunks(piece) {
        now += 100;
        console.feed(chunk, now);
        managed.feed(chunk, now);
        drain(&mut console, look);
        drain(&mut managed, |s, i| answer(s, &log, i));
        if now % 1000 == 0 {
            let _ = log.push(qsi::family::BASE, qsi::base::event::REFCODE, chunk.to_vec());
            let _ = managed.publish(&log);
        }
        let _ = console.take_output();
        let _ = managed.take_output();
    }
    console.tick(now + 1_000_000);
    managed.tick(now + 1_000_000);
    drain(&mut console, look);
    drain(&mut managed, |s, i| answer(s, &log, i));
    assert!(console.is_closed() && managed.is_closed());
}
