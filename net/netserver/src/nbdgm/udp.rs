// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The announcer's socket loop over [`Datagrams`] (UDP 138 is
//! `nbns::Udp::bind(service::PORT)`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::service::{Announcer, Out};
use crate::nbns::Datagrams;

/// The longest the loop waits before it looks at its stop flag.
const POLL: Duration = Duration::from_millis(200);

fn send_all(d: &mut dyn Datagrams, out: Vec<Out>) {
    let to = d.broadcast();
    for o in out {
        if let Err(e) = d.send(&o.datagram.encode(), to) {
            tracing::debug!("nbdgm: send to {to}: {e}");
        }
    }
}

/// Runs `a` on `d` until `stop`: announces once this node has an address
/// towards the broadcast, then as [`Announcer`] says, and a last time at
/// the end.
pub fn serve(d: &mut dyn Datagrams, a: &mut Announcer, stop: &AtomicBool) {
    let mut me = None;
    let mut next_try = Instant::now();
    while !stop.load(Ordering::Acquire) {
        let now = Instant::now();
        if me.is_none() && now >= next_try {
            me = d.local_ip(*d.broadcast().ip());
            match me {
                Some(ip) => send_all(d, a.start(now, ip)),
                None => next_try = now + Duration::from_secs(1),
            }
        }
        if let Some(ip) = me {
            send_all(d, a.tick(now, ip));
        }
        let wait = a.next_wake().map_or(POLL, |t| {
            t.saturating_duration_since(Instant::now()).min(POLL)
        });
        match d.recv(wait) {
            Ok(Some((bytes, from))) => a.handle(Instant::now(), from, &bytes),
            Ok(None) => {}
            Err(e) => {
                tracing::debug!("nbdgm: receive: {e}");
                std::thread::sleep(POLL);
            }
        }
    }
    if let Some(ip) = me {
        send_all(d, a.stop(ip));
    }
}
