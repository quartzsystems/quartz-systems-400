// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The handshake: bytes from the first one on into a fresh console and
//! a fresh managed session.

use super::{answer, drain, look};
use crate::loopback::{console_hello, host_hello};
use crate::{EventLog, Incoming, Limits, Role, Session};

/// One input. The first byte chooses the piece size and the pin.
pub fn handshake(data: &[u8]) {
    let Some((&first, rest)) = data.split_first() else {
        return;
    };
    let piece = usize::from(first % 31) + 1;
    let pinned = first & 0x80 == 0;
    let limits = Limits::default();
    let (Ok(mut console), Ok(mut managed)) = (
        Session::new(Role::Console, console_hello(), pinned, limits, 0),
        Session::new(Role::Managed, host_hello(), pinned, limits, 0),
    ) else {
        return;
    };
    let log = EventLog::new(&limits);
    let mut ready = 0;
    for chunk in rest.chunks(piece) {
        console.feed(chunk, 1);
        managed.feed(chunk, 1);
        drain(&mut console, |s, i| {
            if let Incoming::Ready { agreed, .. } = &i {
                assert!(
                    agreed.version >= 1 && agreed.caps & !s.agreed().map_or(0, |a| a.caps) == 0
                );
                ready += 1;
            }
            look(s, i);
        });
        drain(&mut managed, |s, i| {
            if matches!(i, Incoming::Ready { .. }) {
                ready += 1;
            }
            answer(s, &log, i);
        });
        let _ = console.take_output();
        let _ = managed.take_output();
    }
    assert!(ready <= 2, "a session was ready twice");
}
