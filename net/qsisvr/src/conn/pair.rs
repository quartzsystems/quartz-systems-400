// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The pairing exchange on one connection (`docs/QSI.md` section 3):
//! QS/400's trust crate does the exchange, the keeper holds the password
//! and the pins. Every outcome, failed or not, goes to the keeper, which
//! audits it.

use qsi::{Empty, msgid, pair};
use qsitrust::pair::{Confirm, Guard, Managed, Paired, Start};

use super::Conn;
use crate::trust::{Attempt, Failure, Keeper};

/// The keeper as `qsitrust`'s guard for one attempt.
struct Guarded<'a> {
    keeper: &'a dyn Keeper,
    attempt: Attempt,
    failed: bool,
    kept: Option<bool>,
}

impl Guard for Guarded<'_> {
    fn password(&mut self) -> Option<String> {
        self.keeper.attempt()
    }

    fn failed(&mut self) {
        self.failed = true;
    }

    fn paired(&mut self, console: &Paired) {
        let a = Attempt {
            name: console.name.clone(),
            fingerprint: console.fingerprint,
            address: self.attempt.address.clone(),
        };
        self.kept = Some(self.keeper.paired(&a));
    }
}

impl Conn {
    fn attempt(&self, name: &str) -> Attempt {
        Attempt {
            name: name.to_string(),
            fingerprint: self.config.peer.fingerprint,
            address: self.config.address.clone(),
        }
    }

    pub(super) fn pair_failed(&mut self, name: &str, why: Failure) {
        let a = self.attempt(name);
        self.keeper.failed(&a, why);
        self.failed += 1;
        self.say(format!(
            "A pairing from {} by {:?} failed: {}.",
            self.config.address,
            name,
            why.text()
        ));
    }

    pub(super) fn pair(&mut self, id: u32, request: pair::Request) {
        if self.config.peer.pinned || self.paired.is_some() {
            self.refuse(
                id,
                &[qsisess::Note::new(
                    msgid::ALREADY_PAIRED,
                    "This console is paired with this system already.",
                )],
            );
            return;
        }
        match request {
            pair::Request::Start(s) => self.start(id, s),
            pair::Request::Confirm(c) => self.confirm(id, &c),
        }
    }

    fn start(&mut self, id: u32, s: pair::Start) {
        // A second start abandons the first.
        self.abandon();
        let name = s.name.clone();
        let mut guard = Guarded {
            keeper: self.keeper.as_ref(),
            attempt: self.attempt(&name),
            failed: false,
            kept: None,
        };
        let started = self.config.binding.ok_or(()).and_then(|binding| {
            let start = Start {
                spake: s.spake,
                name: s.name,
            };
            let mine = self.config.hello.name.clone();
            Managed::answer(&mut guard, binding, &start, &mine).map_err(|_| ())
        });
        if let Ok((managed, answer)) = started {
            self.pairing = Some((managed, name));
            let reply = pair::Reply::Answer(pair::Answer {
                spake: answer.spake,
                confirm: answer.confirm,
                name: answer.name,
            });
            let _ = self.session.reply_msg(id, &reply);
            self.outcome = "pairing answered".into();
        } else {
            self.pair_failed(&name, Failure::NoPassword);
            self.refuse(
                id,
                &[qsisess::Note::new(
                    msgid::NO_PAIRING,
                    "No pairing password is out at this system, or it has no attempts left.",
                )],
            );
        }
    }

    fn confirm(&mut self, id: u32, c: &pair::Confirm) {
        let Some((managed, name)) = self.pairing.take() else {
            self.refuse(
                id,
                &[qsisess::Note::new(
                    msgid::PAIRING_ORDER,
                    "A confirmation came before a start.",
                )],
            );
            return;
        };
        let mut guard = Guarded {
            keeper: self.keeper.as_ref(),
            attempt: self.attempt(&name),
            failed: false,
            kept: None,
        };
        let confirmed = managed.confirm(
            &mut guard,
            &Confirm {
                confirm: c.confirm.clone(),
            },
        );
        let (kept, _failed) = (guard.kept, guard.failed);
        match (confirmed, kept) {
            (Ok(console), Some(true)) => {
                self.paired = Some(console.fingerprint);
                self.say(format!(
                    "Console {:?} paired from {}, certificate {}.",
                    console.name,
                    self.config.address,
                    console.fingerprint.hex()
                ));
                let _ = self.session.reply_msg(id, &pair::Reply::Done(Empty));
            }
            (Ok(_), _) => {
                self.pair_failed(&name, Failure::NotKept);
                self.refuse(
                    id,
                    &[qsisess::Note::new(
                        msgid::NO_PAIRING,
                        "The console could not be pinned: two consoles are paired already.",
                    )],
                );
            }
            (Err(_), _) => {
                self.pair_failed(&name, Failure::NotConfirmed);
                self.refuse(
                    id,
                    &[qsisess::Note::new(
                        msgid::PAIRING_FAILED,
                        "The pairing did not confirm.",
                    )],
                );
            }
        }
    }
}
