// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The exchange alone, over bindings made up: the TLS tests run it
//! over real connections (`tests/pairing.rs`).

use super::*;

#[derive(Default)]
struct Box1 {
    password: Option<String>,
    attempts: u32,
    failed: u32,
    pinned: Vec<Paired>,
}

impl Guard for Box1 {
    fn password(&mut self) -> Option<String> {
        self.attempts += 1;
        (self.attempts <= 3)
            .then(|| self.password.clone())
            .flatten()
    }
    fn failed(&mut self) {
        self.failed += 1;
    }
    fn paired(&mut self, console: &Paired) {
        self.pinned.push(console.clone());
        self.password = None;
    }
}

fn fp(n: u8) -> Fingerprint {
    Fingerprint([n; 32])
}

/// The two ends' views of one connection.
fn leg(console: u8, managed: u8, exporter: u8) -> (Binding, Binding) {
    (
        Binding {
            mine: fp(console),
            peer: fp(managed),
            exporter: [exporter; 32],
        },
        Binding {
            mine: fp(managed),
            peer: fp(console),
            exporter: [exporter; 32],
        },
    )
}

fn guard() -> Box1 {
    Box1 {
        password: Some("ABCD-EFGH-JKMN-PQRS".into()),
        ..Box1::default()
    }
}

#[test]
fn the_right_password_pins_both_ways() {
    let mut g = guard();
    let (c, m) = leg(1, 2, 9);
    let (console, start) = Console::start("abcd efgh jkmn pqrs", c, "QSMC01").unwrap();
    let (managed, answer) = Managed::answer(&mut g, m, &start, "VS01").unwrap();
    let (confirm, paired) = console.answer(&answer).unwrap();
    assert_eq!(
        paired,
        Paired {
            fingerprint: fp(2),
            name: "VS01".into()
        }
    );
    let got = managed.confirm(&mut g, &confirm).unwrap();
    assert_eq!(got.fingerprint, fp(1));
    assert_eq!(got.name, "QSMC01");
    assert_eq!(g.pinned, vec![got]);
    assert_eq!(g.failed, 0);
}

#[test]
fn a_wrong_password_fails_at_the_console() {
    let mut g = guard();
    let (c, m) = leg(1, 2, 9);
    let (console, start) = Console::start("ABCD-EFGH-JKMN-PQRT", c, "QSMC01").unwrap();
    let (managed, answer) = Managed::answer(&mut g, m, &start, "VS01").unwrap();
    assert!(console.answer(&answer).is_err());
    // A console that sends something anyway fails at the managed side.
    let bogus = Confirm {
        confirm: vec![0; CONFIRM_LEN],
    };
    assert!(managed.confirm(&mut g, &bogus).is_err());
    assert_eq!((g.failed, g.pinned.len()), (1, 0));
}

#[test]
fn a_replayed_exchange_fails() {
    let mut g = guard();
    let (c, m) = leg(1, 2, 9);
    let (console, start) = Console::start("ABCD-EFGH-JKMN-PQRS", c, "QSMC01").unwrap();
    let (managed, answer) = Managed::answer(&mut g, m, &start, "VS01").unwrap();
    let (confirm, _) = console.answer(&answer).unwrap();
    managed.confirm(&mut g, &confirm).unwrap();

    // The same start and confirmation on a later connection (another
    // exported value) do not pair again, even with the password out.
    g.password = Some("ABCD-EFGH-JKMN-PQRS".into());
    let (_, m2) = leg(1, 2, 10);
    let (managed, _) = Managed::answer(&mut g, m2, &start, "VS01").unwrap();
    assert!(managed.confirm(&mut g, &confirm).is_err());
    // Nor on the same connection: the managed side's message is new.
    let (managed, answer2) = Managed::answer(&mut g, m, &start, "VS01").unwrap();
    assert_ne!(answer2.spake, answer.spake);
    assert!(managed.confirm(&mut g, &confirm).is_err());
    // An old answer to a new start does not confirm either.
    let (console, _) = Console::start("ABCD-EFGH-JKMN-PQRS", c, "QSMC01").unwrap();
    assert!(console.answer(&answer).is_err());
    assert_eq!(g.pinned.len(), 1);
}

#[test]
fn a_relay_with_its_own_certificate_fails() {
    // The console reaches a relay (certificate 7) that reaches the
    // managed side as a console of its own: each leg has its own
    // fingerprints and exported value. The relay passes the messages
    // through unchanged and knows the password, even: the
    // confirmations still do not hold, because each end's transcript
    // names what it saw.
    let mut g = guard();
    let (c, _) = leg(1, 7, 3);
    let (_, m) = leg(7, 2, 4);
    let (console, start) = Console::start("ABCD-EFGH-JKMN-PQRS", c, "QSMC01").unwrap();
    let (managed, answer) = Managed::answer(&mut g, m, &start, "VS01").unwrap();
    assert!(console.answer(&answer).is_err());
    assert!(
        managed
            .confirm(
                &mut g,
                &Confirm {
                    confirm: vec![1; 32]
                }
            )
            .is_err()
    );
    assert!(g.pinned.is_empty());
}

#[test]
fn no_password_out_and_attempts_spent_refuse() {
    let mut g = Box1::default();
    let (c, m) = leg(1, 2, 9);
    let (_, start) = Console::start("x", c, "QSMC01").unwrap();
    assert!(Managed::answer(&mut g, m, &start, "VS01").is_err());
    let mut g = guard();
    for _ in 0..3 {
        assert!(Managed::answer(&mut g, m, &start, "VS01").is_ok());
    }
    assert!(Managed::answer(&mut g, m, &start, "VS01").is_err());
    // Bad lengths and long names are refused before the password is
    // asked for.
    let mut g = guard();
    let short = Start {
        spake: vec![0; 3],
        name: "x".into(),
    };
    assert!(Managed::answer(&mut g, m, &short, "VS01").is_err());
    let long = Start {
        spake: start.spake.clone(),
        name: "n".repeat(MAX_NAME + 1),
    };
    assert!(Managed::answer(&mut g, m, &long, "VS01").is_err());
    assert_eq!(g.attempts, 0);
}
