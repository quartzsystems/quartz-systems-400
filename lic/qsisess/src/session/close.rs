// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Keep-alive, the clean close and the close over a fault. Whatever
//! ends the session, outstanding requests come back as refusals and
//! open streams as resets, so the caller is never left waiting.

use qsi::msgid;

use super::{Role, Session, State};
use crate::wire::{self, Goodbye, MAX_TEXT, kind};
use crate::{Incoming, Outcome, Reply};

impl Session {
    /// The clock: a ping when the connection has been quiet for the
    /// keep-alive interval, a close when for the timeout.
    pub fn tick(&mut self, now_ms: u64) {
        if self.is_closed() {
            return;
        }
        let quiet = now_ms.saturating_sub(self.last_rx);
        if quiet >= self.limits.timeout_ms {
            return self.fail(msgid::TIMED_OUT, "nothing heard within the timeout");
        }
        if self.state == State::Open
            && quiet >= self.limits.keepalive_ms
            && now_ms.saturating_sub(self.last_ping) >= self.limits.keepalive_ms
        {
            self.ping_token += 1;
            self.last_ping = now_ms;
            let token = self.ping_token.to_le_bytes();
            self.frame(kind::PING, &token);
        }
    }

    /// Close cleanly: a goodbye goes after whatever is queued, and
    /// nothing more is sent.
    pub fn close(&mut self, text: &str) {
        if self.is_closed() {
            return;
        }
        self.goodbye("", text);
        self.shutdown(None, text, false);
    }

    /// Close over a fault named by `id`.
    pub(super) fn fail(&mut self, id: &str, text: &str) {
        if self.is_closed() {
            return;
        }
        self.goodbye(id, text);
        self.shutdown(Some(id.to_owned()), text, false);
    }

    fn goodbye(&mut self, id: &str, text: &str) {
        let g = Goodbye {
            msgid: wire::clip(id, 16),
            text: wire::clip(text, MAX_TEXT),
        };
        self.frame(kind::GOODBYE, &ipc::encode(&g));
    }

    pub(super) fn shutdown(&mut self, why: Option<String>, text: &str, by_peer: bool) {
        self.state = State::Closed;
        self.reader.reset();
        self.parts.reset();
        self.cursor = None;
        let id = why.clone().unwrap_or_else(|| msgid::CLOSING.to_owned());
        let text = wire::clip(text, MAX_TEXT);
        let pending = std::mem::take(&mut self.pending);
        if self.role == Role::Console {
            for (rid, (family, op)) in pending {
                self.incoming.push_back(Incoming::Reply(Reply {
                    id: rid,
                    family,
                    op,
                    stream: None,
                    outcome: Outcome::Refused {
                        msgid: id.clone(),
                        text: format!("the session closed: {text}"),
                    },
                }));
            }
        }
        for stream in std::mem::take(&mut self.streams).into_keys() {
            self.incoming.push_back(Incoming::StreamReset {
                stream,
                msgid: id.clone(),
                text: format!("the session closed: {text}"),
                by_peer: false,
            });
        }
        self.incoming.push_back(Incoming::Closed {
            msgid: why,
            text,
            by_peer,
        });
    }
}
