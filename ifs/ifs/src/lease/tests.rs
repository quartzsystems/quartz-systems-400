// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

use std::time::{Duration, Instant};

use super::defer::{Deferral, deferring};
use super::state::{H, R, RWH, Trigger, W, break_to, candidates};
use super::{Leases, Others};
use crate::errno::Errno;
use crate::lock::{ObjKey, Owner};

const F: ObjKey = ObjKey { fs: 1, ino: 7 };
const A: Owner = Owner(1);
const B: Owner = Owner(2);
const K: [u8; 32] = [9; 32];

fn held(l: &Leases) -> Vec<(Owner, u8)> {
    l.on(F).into_iter().map(|(o, _, s)| (o, s)).collect()
}

#[test]
fn break_table_matches_ms_fsa() {
    assert_eq!(
        break_to(Trigger::Open { overwrite: false }, RWH),
        Some((R | H, true))
    );
    assert_eq!(break_to(Trigger::Open { overwrite: false }, R | H), None);
    assert_eq!(
        break_to(Trigger::Open { overwrite: true }, R | H),
        Some((0, false))
    );
    assert_eq!(break_to(Trigger::Write, RWH), Some((0, true)));
    assert_eq!(break_to(Trigger::Write, R), Some((0, false)));
    assert_eq!(break_to(Trigger::Handle, R | H), Some((R, true)));
    assert_eq!(break_to(Trigger::Sharing, R), None);
    assert_eq!(candidates(RWH).collect::<Vec<_>>(), [RWH, R | H, R]);
    assert_eq!(candidates(R | W).collect::<Vec<_>>(), [R | W, R]);
}

#[test]
fn grants_are_capped_by_other_owners() {
    let l = Leases::default();
    assert_eq!(l.grant(A, F, 5, K, RWH, Others::default()), RWH);
    // Another owner's lease leaves B neither W nor, while A writes, R.
    assert_eq!(l.grant(B, F, 6, [1; 32], RWH, Others::default()), 0);
    let l = Leases::default();
    let open = Others {
        open: true,
        locked: false,
    };
    assert_eq!(l.grant(A, F, 5, K, RWH, open), R | H);
    let locked = Others {
        open: true,
        locked: true,
    };
    assert_eq!(l.grant(B, F, 6, [1; 32], RWH, locked), 0);
    // A wants no lease without R.
    assert_eq!(l.grant(B, F, 6, [2; 32], H, Others::default()), 0);
}

#[test]
fn a_write_waits_for_the_write_back_and_the_report_ends_it() {
    let l = Leases::default();
    l.grant(A, F, 5, K, RWH, Others::default());
    let (r, d) = deferring(true, || l.gate(B, F, Trigger::Write));
    assert_eq!(r, Err(Errno::EAGAIN));
    let d = d.expect("deferred");
    assert!(!l.ready(&d));
    assert_eq!(l.orders(A, Duration::ZERO).unwrap(), [(5, 0)]);
    // The holder's own operations break nothing.
    assert_eq!(
        deferring(true, || l.gate(A, F, Trigger::Write)),
        (Ok(()), None)
    );
    l.report(A, 5, &[(K, 0)]);
    assert!(l.ready(&d));
    assert!(held(&l).is_empty());
    assert_eq!(
        deferring(true, || l.gate(B, F, Trigger::Write)),
        (Ok(()), None)
    );
}

#[test]
fn a_read_lease_breaks_at_once_and_nothing_waits() {
    let l = Leases::default();
    l.grant(A, F, 5, K, R, Others::default());
    assert_eq!(
        deferring(true, || l.gate(B, F, Trigger::Write)),
        (Ok(()), None)
    );
    assert_eq!(held(&l), [(A, 0)]);
    assert_eq!(l.orders(A, Duration::ZERO).unwrap(), [(5, 0)]);
}

#[test]
fn an_open_keeps_read_and_handle_caching() {
    let l = Leases::default();
    l.grant(A, F, 5, K, RWH, Others::default());
    let (_, d) = deferring(true, || l.gate(B, F, Trigger::Open { overwrite: false }));
    let d = d.expect("deferred");
    // A second trigger while the break runs lowers the order.
    let _ = deferring(true, || l.gate(B, F, Trigger::Write));
    assert_eq!(l.orders(A, Duration::ZERO).unwrap(), [(5, 0)]);
    l.report(A, 5, &[(K, R | H)]);
    assert!(l.ready(&d));
    assert_eq!(held(&l), [(A, R | H)]);
}

#[test]
fn without_permission_the_operation_goes_on() {
    let l = Leases::default();
    l.grant(A, F, 5, K, RWH, Others::default());
    assert_eq!(
        deferring(false, || l.gate(B, F, Trigger::Handle)),
        (Ok(()), None)
    );
    assert_eq!(l.orders(A, Duration::ZERO).unwrap(), [(5, R | W)]);
}

#[test]
fn an_unanswered_break_expires() {
    let l = Leases::default();
    l.grant(A, F, 5, K, RWH, Others::default());
    let (_, d) = deferring(true, || l.gate(B, F, Trigger::Write));
    let d = d.expect("deferred");
    l.expire(Instant::now() + super::BREAK_TIMEOUT);
    assert!(l.ready(&d));
    assert_eq!(held(&l), [(A, 0)]);
}

#[test]
fn an_ended_owner_frees_its_waiters() {
    let l = Leases::default();
    l.grant(A, F, 5, K, RWH, Others::default());
    let (_, d) = deferring(true, || l.gate(B, F, Trigger::Write));
    l.release_owner(A);
    assert!(l.ready(&d.expect("deferred")));
    assert!(l.is_empty());
    assert!(l.orders(A, Duration::ZERO).unwrap().is_empty());
}

#[test]
fn a_poll_defers_until_an_order_comes() {
    let l = std::sync::Arc::new(Leases::default());
    l.grant(A, F, 5, K, RWH, Others::default());
    let (r, d) = deferring(true, || l.orders(A, Duration::from_secs(5)));
    assert_eq!(r, Err(Errno::EAGAIN));
    let d = d.expect("deferred");
    assert!(matches!(d, Deferral::Orders { .. }));
    let l2 = l.clone();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        let _ = l2.gate(B, F, Trigger::Write);
    });
    assert!(l.wait(&d));
    t.join().unwrap();
    assert_eq!(l.orders(A, Duration::ZERO).unwrap(), [(5, 0)]);
}

#[test]
fn reports_replace_what_the_holder_had_and_ignore_unknown_files() {
    let l = Leases::default();
    l.grant(A, F, 5, K, R | H, Others::default());
    l.grant(A, F, 5, [3; 32], R, Others::default());
    l.report(A, 5, &[([3; 32], R)]);
    assert_eq!(l.on(F).len(), 1);
    l.report(A, 99, &[(K, RWH)]);
    l.report(B, 5, &[(K, RWH)]);
    assert_eq!(held(&l), [(A, R)]);
}

#[test]
fn a_lease_being_granted_survives_a_report_that_keeps_it() {
    let l = Leases::default();
    l.grant(A, F, 5, K, RWH, Others::default());
    l.grant(A, F, 5, [4; 32], R, Others::default());
    l.report(A, 5, &[(K, super::state::KEEP)]);
    assert_eq!(l.on(F), [(A, K, RWH)]);
}
