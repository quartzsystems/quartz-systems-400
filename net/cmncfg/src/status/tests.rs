// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The status machine under scripted vary commands and link events.

use super::*;

fn ids(effects: &[Effect]) -> Vec<&str> {
    effects
        .iter()
        .map(|e| match e {
            Effect::Message { msgid, .. } => msgid.as_str(),
        })
        .collect()
}

fn line_with(count: u16, interval: u16) -> State {
    let mut s = State::new();
    s.add_line(
        "ETHLINE",
        LineConfig {
            rcy_count: count,
            rcy_interval: interval,
            ..LineConfig::default()
        },
    );
    let l = s.line_mut("ETHLINE").unwrap();
    l.attach_dev("ETHLINET", "ETHLITCP");
    s
}

fn statuses(s: &State) -> Vec<(String, u8, Status)> {
    s.rows(Kind::Line, &|_| true, true)
        .into_iter()
        .map(|r| (r.name, r.depth, r.status))
        .collect()
}

fn on(s: &mut State) {
    s.vary_on_begin("ETHLINE").unwrap();
    s.vary_on_done("ETHLINE", "eth0", Ok(true), true).unwrap();
}

#[test]
fn vary_on_with_carrier_is_active_down_the_tree() {
    let mut s = line_with(2, 5);
    assert_eq!(s.line("ETHLINE").unwrap().status, Status::VariedOff);
    s.vary_on_begin("ETHLINE").unwrap();
    assert_eq!(s.line("ETHLINE").unwrap().status, Status::VaryOnPending);
    assert_eq!(statuses(&s)[0].2.text(), "VARY ON PENDING");
    assert!(
        s.vary_on_begin("ETHLINE")
            .unwrap_err()
            .starts_with("CPD2645")
    );
    let e = s.vary_on_done("ETHLINE", "eth0", Ok(true), true).unwrap();
    assert_eq!(ids(&e), ["CPF5908"]);
    assert_eq!(
        statuses(&s),
        [
            ("ETHLINE".to_string(), 0, Status::Active),
            ("ETHLINET".to_string(), 1, Status::Active),
            ("ETHLITCP".to_string(), 2, Status::Active),
        ]
    );
    match &e[0] {
        Effect::Message {
            line,
            ctl,
            msgq,
            inquiry,
            data,
            ..
        } => {
            assert_eq!(line, "ETHLINE");
            assert_eq!(ctl, "ETHLINET");
            assert_eq!(msgq, "QSYS/QSYSOPR");
            assert!(!inquiry);
            assert_eq!(wm::rec::text_of(&data[..10]), "ETHLINET");
        }
    }
    assert_eq!(s.status_of(Kind::Dev, "ethlitcp"), Some(Status::Active));
    assert_eq!(Status::Active.code(), 60);
}

#[test]
fn vary_on_without_carrier_waits_for_it() {
    let mut s = line_with(2, 5);
    s.vary_on_begin("ETHLINE").unwrap();
    // Events while pending are not the line's yet.
    assert!(s.carrier("eth0", true, 99).is_empty());
    let e = s.vary_on_done("ETHLINE", "eth0", Ok(false), true).unwrap();
    assert!(e.is_empty());
    assert_eq!(s.line("ETHLINE").unwrap().status, Status::VariedOn);
    // A loss while waiting is not a failure.
    assert!(s.carrier("eth0", false, 100).is_empty());
    let e = s.carrier("eth0", true, 101);
    assert_eq!(ids(&e), ["CPF5908"]);
    assert_eq!(s.line("ETHLINE").unwrap().status, Status::Active);
    // Another interface's events are nobody's.
    assert!(s.carrier("eth9", false, 102).is_empty());
}

#[test]
fn vary_on_failure_stays_off() {
    let mut s = line_with(2, 5);
    s.vary_on_begin("ETHLINE").unwrap();
    let e = s
        .vary_on_done("ETHLINE", "", Err("no such port".into()), true)
        .unwrap_err();
    assert!(e.starts_with("CPD2659"), "{e}");
    assert_eq!(s.line("ETHLINE").unwrap().status, Status::VariedOff);
    assert!(
        s.vary_off("ETHLINE", true)
            .unwrap_err()
            .starts_with("CPD2646")
    );
}

#[test]
fn recovery_within_the_limit_then_cancelled() {
    let mut s = line_with(2, 5);
    on(&mut s);
    // First loss: recovery pending, the message to the queue.
    let e = s.carrier("eth0", false, 1000);
    assert_eq!(ids(&e), ["CPI5917"]);
    assert_eq!(s.line("ETHLINE").unwrap().status, Status::RcyPnd);
    assert_eq!(statuses(&s)[2].2, Status::RcyPnd);
    // A second down event in the same loss counts nothing.
    assert!(s.carrier("eth0", false, 1001).is_empty());
    // Carrier back: active again.
    assert_eq!(ids(&s.carrier("eth0", true, 1010)), ["CPF5908"]);
    assert_eq!(s.line("ETHLINE").unwrap().status, Status::Active);
    // Second loss within five minutes: still pending.
    assert_eq!(ids(&s.carrier("eth0", false, 1100)), ["CPI5917"]);
    s.carrier("eth0", true, 1110);
    // Third: over the count, recovery cancelled with the inquiry; the
    // carrier coming back changes nothing until a vary off and on.
    let e = s.carrier("eth0", false, 1200);
    assert_eq!(ids(&e), ["CPA58EE"]);
    match &e[0] {
        Effect::Message { inquiry, data, .. } => {
            assert!(inquiry);
            assert_eq!(wm::rec::text_of(&data[5..15]), "ETHLINE");
        }
    }
    assert_eq!(s.line("ETHLINE").unwrap().status, Status::RcyCnl);
    assert!(s.carrier("eth0", true, 1210).is_empty());
    assert_eq!(s.line("ETHLINE").unwrap().status, Status::RcyCnl);
    assert_eq!(s.vary_off("ETHLINE", true).unwrap(), "eth0");
    assert_eq!(s.line("ETHLINE").unwrap().status, Status::VariedOff);
    assert_eq!(statuses(&s)[1].2, Status::VariedOff);
    // Events for the interface are no longer the line's.
    assert!(s.carrier("eth0", true, 1220).is_empty());
    on(&mut s);
    assert!(s.line("ETHLINE").unwrap().failures.is_empty());
}

#[test]
fn old_failures_leave_the_window() {
    let mut s = line_with(1, 5);
    on(&mut s);
    s.carrier("eth0", false, 0);
    s.carrier("eth0", true, 1);
    // Six minutes later the first loss no longer counts.
    s.carrier("eth0", false, 361);
    assert_eq!(s.line("ETHLINE").unwrap().status, Status::RcyPnd);
    s.carrier("eth0", true, 362);
    s.carrier("eth0", false, 400);
    assert_eq!(s.line("ETHLINE").unwrap().status, Status::RcyCnl);
}

#[test]
fn interval_zero_counts_every_loss_since_vary_on() {
    let mut s = line_with(2, 0);
    on(&mut s);
    for t in [0, 10_000] {
        s.carrier("eth0", false, t);
        assert_eq!(s.line("ETHLINE").unwrap().status, Status::RcyPnd);
        s.carrier("eth0", true, t + 1);
    }
    s.carrier("eth0", false, 1_000_000);
    assert_eq!(s.line("ETHLINE").unwrap().status, Status::RcyCnl);
}

#[test]
fn no_recovery_fails_outright() {
    let mut s = line_with(0, 0);
    on(&mut s);
    let e = s.link_removed("eth0", 5);
    assert_eq!(ids(&e), ["CPA58EE"]);
    assert_eq!(s.line("ETHLINE").unwrap().status, Status::Failed);
    assert!(Status::Failed.passes("*FAILED") && !Status::Failed.passes("*ACTIVE"));
}

#[test]
fn controllers_and_devices_have_their_own_switch() {
    let mut s = line_with(2, 5);
    s.vary_on_begin("ETHLINE").unwrap();
    s.vary_on_done("ETHLINE", "eth0", Ok(true), false).unwrap();
    // RANGE(*OBJ): the line alone.
    assert_eq!(statuses(&s)[1].2, Status::VariedOff);
    assert_eq!(statuses(&s)[2].2, Status::VariedOff);
    assert!(
        s.vary_dev("ETHLITCP", true)
            .unwrap_err()
            .starts_with("CPF5928")
    );
    s.vary_ctl("ETHLINET", true, false).unwrap();
    assert_eq!(statuses(&s)[1].2, Status::Active);
    assert_eq!(statuses(&s)[2].2, Status::VariedOff);
    s.vary_dev("ETHLITCP", true).unwrap();
    assert_eq!(statuses(&s)[2].2, Status::Active);
    s.vary_ctl("ETHLINET", false, false).unwrap();
    assert_eq!(statuses(&s)[2].2, Status::VariedOff);
    assert!(
        s.vary_ctl("ETHLINET", false, false)
            .unwrap_err()
            .starts_with("CPD2650")
    );
    assert!(
        s.vary_ctl("NOCTL", true, true)
            .unwrap_err()
            .starts_with("CPF2602")
    );
    assert!(
        s.vary_dev("NODEV", true)
            .unwrap_err()
            .starts_with("CPF2603")
    );
    // The views by kind; RANGE(*OBJ) leaves out what hangs off.
    assert_eq!(s.rows(Kind::Ctl, &|_| true, true).len(), 2);
    assert_eq!(s.rows(Kind::Ctl, &|_| true, false).len(), 1);
    assert_eq!(s.rows(Kind::Line, &|_| true, false).len(), 1);
    let dev_rows = s.rows(Kind::Dev, &|n| n.starts_with("ETHLI"), true);
    assert_eq!(dev_rows.len(), 1);
    assert_eq!(dev_rows[0].kind, Kind::Dev);
    assert_eq!(
        s.line_of_ctl("ethlinet").map(|l| l.name.as_str()),
        Some("ETHLINE")
    );
    assert_eq!(
        s.line_of_dev("ETHLITCP").map(|(_, c)| c.name.as_str()),
        Some("ETHLINET")
    );
    assert!(s.attached("ETHLITCP") && !s.attached("OTHER"));
}

#[test]
fn controller_view_keeps_devices_under_their_controller() {
    // The capture's *CTL view: each controller in name order with its
    // devices beneath, across lines.
    let mut s = State::new();
    s.add_line("LINETH", LineConfig::default());
    s.add_line("AAALINE", LineConfig::default());
    let l = s.line_mut("LINETH").unwrap();
    l.attach_ctl("LINETNET");
    l.attach_dev("LINETNET00", "LINETTCP");
    l.attach_ctl("LINETNET01");
    l.attach_dev("LINETNET02", "LINETTCP00");
    s.line_mut("AAALINE")
        .unwrap()
        .attach_dev("ZZZNET", "ZZZTCP");
    let rows: Vec<(String, u8)> = s
        .rows(Kind::Ctl, &|_| true, true)
        .into_iter()
        .map(|r| (r.name, r.depth))
        .collect();
    let want = [
        ("LINETNET", 0),
        ("LINETNET00", 0),
        ("LINETTCP", 1),
        ("LINETNET01", 0),
        ("LINETNET02", 0),
        ("LINETTCP00", 1),
        ("ZZZNET", 0),
        ("ZZZTCP", 1),
    ];
    let want: Vec<(String, u8)> = want.iter().map(|(n, d)| ((*n).to_string(), *d)).collect();
    assert_eq!(rows, want);
}

#[test]
fn a_changed_description_keeps_status_and_attachments() {
    let mut s = line_with(2, 5);
    on(&mut s);
    s.add_line(
        "ETHLINE",
        LineConfig {
            rcy_count: 0,
            ..LineConfig::default()
        },
    );
    let l = s.line("ETHLINE").unwrap();
    assert_eq!(l.status, Status::Active);
    assert_eq!(l.ctls.len(), 1);
    assert_eq!(l.config.rcy_count, 0);
    assert_eq!(s.online_lines(), ["ETHLINE"]);
}

#[test]
fn unknown_lines_are_refused() {
    let mut s = State::new();
    assert!(s.vary_on_begin("NOPE").unwrap_err().starts_with("CPF2601"));
    assert!(s.vary_off("NOPE", true).unwrap_err().starts_with("CPF2601"));
    assert!(s.remove_line("NOPE").is_none());
    assert_eq!(Status::VaryOffPending.text(), "VARY OFF PENDING");
    assert_eq!(Kind::Ctl.text(), "*CTL");
    assert_eq!(Kind::parse("*DEV"), Some(Kind::Dev));
    assert_eq!(s.status_of(Kind::Line, "NOPE"), None);
}
