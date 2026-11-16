// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Events. The managed side's caller keeps one [`EventLog`]; a session
//! subscribed through [`Session::subscribe`] is sent what it missed,
//! then each event [`Session::publish`] finds after its cursor. The
//! cursor makes the order reply, replayed, live, with no gap and no
//! event twice.

use qsi::base::{self, Replay, Subscribed};
use qsi::msgid;

use super::{Role, Session};
use crate::wire::{self, kind};
use crate::{Error, Event, EventLog, Incoming};

impl Session {
    /// Answer a base `Subscribe` (request `id`, with its `since`) from
    /// `log`: the `Subscribed` reply, then the events replayed, in
    /// order. From then on [`Self::publish`] sends the new ones.
    ///
    /// `Resumed` when `since` is within the log (it may replay none);
    /// `ReadAgain` when the log no longer reaches back that far, or
    /// `since` is newer than anything it has (it is another log).
    ///
    /// # Errors
    /// Closed; no such request; the peer not pinned.
    pub fn subscribe(&mut self, id: u32, log: &EventLog, since: Option<u64>) -> Result<(), Error> {
        self.ready()?;
        if !self.pinned {
            return Err(Error::NotPinned);
        }
        let next = log.next();
        let (replay, from) = match since {
            None => (Replay::Fresh, next),
            Some(s) if s < next && s.saturating_add(1) >= log.first() => (Replay::Resumed, s + 1),
            Some(_) => (Replay::ReadAgain, next),
        };
        let replayed = log.from(from).filter(|e| self.may_send(e)).count();
        let replayed = u32::try_from(replayed).unwrap_or(u32::MAX);
        let answer = base::Reply::Subscribe(Subscribed {
            replay,
            replayed,
            next,
        });
        self.reply_msg(id, &answer)?;
        self.cursor = Some(from);
        self.publish(log)?;
        Ok(())
    }

    /// Answer a base `Unsubscribe` (request `id`): no more events.
    ///
    /// # Errors
    /// Closed; no such request.
    pub fn unsubscribe(&mut self, id: u32) -> Result<(), Error> {
        self.reply_msg(id, &base::Reply::Unsubscribe(qsi::Empty))?;
        self.cursor = None;
        Ok(())
    }

    /// Send the events of `log` this session has not been sent; how
    /// many. Nothing when not subscribed. Only the events of what the
    /// two agreed on go ([`qsi::caps::of_event`]): the numbers are the
    /// log's, so a console that did not agree on every family the
    /// managed side has sees them skip. A session that fell behind the
    /// log's reach skips to its oldest.
    ///
    /// # Errors
    /// The session is closed.
    pub fn publish(&mut self, log: &EventLog) -> Result<usize, Error> {
        self.ready()?;
        if self.role != Role::Managed {
            return Err(Error::WrongRole("the managed side"));
        }
        let Some(mut cursor) = self.cursor.filter(|_| self.pinned) else {
            return Ok(0);
        };
        let mut sent = 0;
        for e in log.from(cursor) {
            cursor = e.seq + 1;
            if !self.may_send(e) {
                continue;
            }
            // The log bounds an event below the message limit.
            self.out
                .extend(wire::message_frames(kind::EVENT, &ipc::encode(e)));
            sent += 1;
        }
        self.cursor = Some(cursor.max(log.next()));
        Ok(sent)
    }

    /// Whether the peer agreed on what `e` needs. An event of a family
    /// this crate does not know is the caller's, and goes.
    fn may_send(&self, e: &Event) -> bool {
        let need = qsi::caps::of_event(e.family, e.op);
        need == 0 || self.agreed().is_some_and(|a| a.has(need))
    }

    pub(super) fn on_event(&mut self, env: &[u8]) {
        if self.role != Role::Console {
            return self.fail(msgid::OUT_OF_ORDER, "an event to the managed side");
        }
        let Some(e) = wire::read::<Event>(env) else {
            return self.fail(msgid::BAD_FRAME, "an event that does not read");
        };
        if self.last_event.is_some_and(|l| e.seq <= l) || e.seq == 0 {
            return self.fail(msgid::OUT_OF_ORDER, "an event numbered out of order");
        }
        if !self.pinned {
            return;
        }
        self.last_event = Some(e.seq);
        self.incoming.push_back(Incoming::Event(e));
    }
}
