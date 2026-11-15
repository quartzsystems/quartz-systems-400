// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Bytes in: frames out of the reader, each to its handler. Over TLS no
//! junk is expected, so a byte the reader passes over closes the
//! session, as does a frame of a kind the session does not know.

use qsi::msgid;
use qsipart::frame::{HEADER, MAX_BODY};

use super::{Session, State};
use crate::wire::parts::Step;
use crate::wire::{self, Goodbye, kind};

impl Session {
    /// Bytes from the connection, in whatever pieces they came.
    pub fn feed(&mut self, bytes: &[u8], now_ms: u64) {
        if self.is_closed() {
            return;
        }
        if !bytes.is_empty() {
            self.last_rx = now_ms;
        }
        // A frame's worth at a time, so the reader's buffer never cuts.
        for chunk in bytes.chunks(HEADER + MAX_BODY) {
            self.reader.push(chunk);
            loop {
                let f = self.reader.next_frame();
                if self.reader.skipped() > 0 {
                    return self.fail(msgid::BAD_FRAME, "bytes that are not a frame");
                }
                let Some(f) = f else {
                    break;
                };
                self.on_frame(f.kind, &f.body);
                if self.is_closed() {
                    return;
                }
            }
        }
    }

    fn on_frame(&mut self, kind_: u16, body: &[u8]) {
        if self.state == State::Waiting {
            if kind_ == kind::HELLO {
                return self.on_hello(body);
            }
            return self.fail(msgid::OUT_OF_ORDER, "a message before the hello");
        }
        if self.parts.busy() && kind_ != kind::PART {
            return self.fail(msgid::BAD_FRAME, "a frame inside a message's parts");
        }
        match kind_ {
            kind::HELLO => self.fail(msgid::OUT_OF_ORDER, "a second hello"),
            kind::REQUEST | kind::REPLY | kind::EVENT => {
                let step = self.parts.first(kind_, body, self.limits.message());
                self.on_step(step);
            }
            kind::PART => {
                let step = self.parts.part(body);
                self.on_step(step);
            }
            kind::PING => match wire::read::<u64>(body) {
                Some(token) => self.frame(kind::PONG, &token.to_le_bytes()),
                None => self.fail(msgid::BAD_FRAME, "a ping that does not read"),
            },
            kind::PONG => {}
            kind::GOODBYE => {
                let g = wire::read::<Goodbye>(body).unwrap_or(Goodbye {
                    msgid: msgid::BAD_FRAME.into(),
                    text: "a goodbye that does not read".into(),
                });
                let why = (!g.msgid.is_empty()).then_some(g.msgid);
                self.shutdown(why, &g.text, true);
            }
            kind::DATA => self.on_data(body),
            kind::CREDIT => self.on_credit(body),
            kind::CLOSE => self.on_close(body),
            kind::RESET => self.on_reset(body),
            _ => self.fail(
                msgid::BAD_FRAME,
                "a frame of a kind the session does not know",
            ),
        }
    }

    fn on_step(&mut self, step: Step) {
        match step {
            Step::More => {}
            Step::Bad(why) => self.fail(msgid::BAD_FRAME, why),
            Step::TooLong(_) => self.fail(msgid::TOO_LONG, "a message longer than the limit"),
            Step::Done(kind::REQUEST, env) => self.on_request(&env),
            Step::Done(kind::REPLY, env) => self.on_reply(&env),
            Step::Done(_, env) => self.on_event(&env),
        }
    }
}
