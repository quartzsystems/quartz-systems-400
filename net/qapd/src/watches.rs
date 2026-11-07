// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A connection's watches (version 6): message queues and spool
//! targets, and the thread that looks at them every
//! [`crate::Config::watch_poll`] and sends what changed as events. A
//! watch only lists, as `ListMessages` and `ListSpooledFiles` list, with
//! the profile's authority: it never allocates a queue or changes its
//! delivery. Watches end with the connection and with a sign-off on it.
//!
//! - Messages: what is new is what comes, strictly newest first, before
//!   the first key the watch knows; sent oldest first. When the keys
//!   known outgrow the queue (messages removed), the next look lists it
//!   whole and knows only what is there.
//! - Spool: each look lists the target's files and compares them with
//!   the last look's: gone `Removed`, different `Changed`, new `Added`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, PoisonError};
use std::thread::JoinHandle;

use qap_proto::ops::event;
use qap_proto::ops::messages::{Key, ListMessages, MessageArrived, Queue, QueueRef, QueuedMessage};
use qap_proto::ops::spool::{
    ListSpooledFiles, SpoolChange, SpoolTarget, SpooledFile, SpooledFileChanged, SpooledFileId,
    WatchSpool,
};
use qap_proto::{Envelope, Service, Status};

use crate::conn::{Answer, Shared, lock};
use crate::host::{NOT_WATCHED, Refusal, ThreadHost};
use crate::lanes::ToMain;
use crate::watch;

/// Messages a look lists at a time while it looks for a key it knows.
const PAGE: u32 = 32;
/// Keys known beyond the queue's count before a look lists it whole.
const SLACK: usize = 256;

/// One queue's watch: the keys it knows.
struct QueueWatch {
    queue: Queue,
    known: HashSet<Key>,
}

/// One spool target's watch: its files at the last look.
struct SpoolWatch {
    target: SpoolTarget,
    files: HashMap<SpooledFileId, SpooledFile>,
}

/// What a connection watches.
#[derive(Default)]
pub(crate) struct Watches {
    queues: Vec<QueueWatch>,
    spool: Vec<SpoolWatch>,
    /// Up each time the watches are cleared: a look begun before is
    /// dropped.
    era: u64,
    /// The thread that looks, once made.
    looker: Option<JoinHandle<()>>,
    /// The connection is closing.
    closing: bool,
    /// A debug session is active: each look takes its news too.
    pub(crate) debugging: bool,
}

impl Watches {
    fn queue(&mut self, q: &Queue) -> Option<&mut QueueWatch> {
        self.queues.iter_mut().find(|w| &w.queue == q)
    }

    fn target(&mut self, t: &SpoolTarget) -> Option<&mut SpoolWatch> {
        self.spool.iter_mut().find(|w| &w.target == t)
    }
}

/// How a queue is named in messages.
fn queue_label(q: &Queue) -> String {
    match q {
        Queue::Own => "*OWN".into(),
        Queue::SystemOperator => "QSYSOPR".into(),
        Queue::Named { queue } => format!("{}/{}", queue.library, queue.name),
    }
}

/// How a spool target is named in messages.
fn target_label(t: &SpoolTarget) -> String {
    match t {
        SpoolTarget::Queue { outq } => format!("{}/{}", outq.library, outq.name),
        SpoolTarget::User { profile } => profile.clone(),
    }
}

/// What a target lists: a queue's files of every user, or a user's.
fn listing(t: &SpoolTarget) -> ListSpooledFiles {
    match t {
        SpoolTarget::Queue { outq } => ListSpooledFiles {
            user: "*ALL".into(),
            outq: Some(outq.clone()),
            ..ListSpooledFiles::default()
        },
        SpoolTarget::User { profile } => ListSpooledFiles {
            user: profile.clone(),
            ..ListSpooledFiles::default()
        },
    }
}

/// Every message on `q`, newest first.
fn all_messages(host: &mut dyn ThreadHost, q: &Queue) -> Result<Vec<QueuedMessage>, Refusal> {
    let r = host.list_messages(&ListMessages {
        queue: q.clone(),
        newest_first: true,
        start: 0,
        max: 0,
    })?;
    Ok(r.messages)
}

/// `WatchQueue`: the queue is listed as `ListMessages` lists it, and
/// what is on it now is known.
pub(crate) fn watch_queue(
    shared: &Arc<Shared>,
    host: &mut dyn ThreadHost,
    req: &Envelope,
) -> Answer {
    let Ok(b) = req.body_as::<QueueRef>() else {
        return Answer::status(Status::InvalidRequest);
    };
    let keys: HashSet<Key> = match all_messages(host, &b.queue) {
        Ok(v) => v.into_iter().map(|m| m.key).collect(),
        Err(r) => return Answer::refused(r),
    };
    {
        let mut w = lock(&shared.watches);
        match w.queue(&b.queue) {
            Some(q) => q.known.extend(keys),
            None => w.queues.push(QueueWatch {
                queue: b.queue,
                known: keys,
            }),
        }
    }
    start(shared);
    Answer::status(Status::Ok)
}

/// `UnwatchQueue`: `NotFound` when this connection does not watch it.
pub(crate) fn unwatch_queue(shared: &Shared, host: &mut dyn ThreadHost, req: &Envelope) -> Answer {
    let Ok(b) = req.body_as::<QueueRef>() else {
        return Answer::status(Status::InvalidRequest);
    };
    let gone = {
        let mut w = lock(&shared.watches);
        let before = w.queues.len();
        w.queues.retain(|q| q.queue != b.queue);
        before != w.queues.len()
    };
    if gone {
        Answer::status(Status::Ok)
    } else {
        not_watched(host, &queue_label(&b.queue))
    }
}

/// `WatchSpool`: the target is listed as `ListSpooledFiles` lists it,
/// and its files now are known.
pub(crate) fn watch_spool(
    shared: &Arc<Shared>,
    host: &mut dyn ThreadHost,
    req: &Envelope,
) -> Answer {
    let Ok(b) = req.body_as::<WatchSpool>() else {
        return Answer::status(Status::InvalidRequest);
    };
    if matches!(&b.target, SpoolTarget::User { profile } if profile.trim().eq_ignore_ascii_case("*ALL"))
    {
        return Answer::status(Status::InvalidRequest);
    }
    let files = match host.list_spooled_files(&listing(&b.target)) {
        Ok(r) => r.files.into_iter().map(|f| (f.id.clone(), f)).collect(),
        Err(r) => return Answer::refused(r),
    };
    {
        let mut w = lock(&shared.watches);
        match w.target(&b.target) {
            Some(t) => t.files = files,
            None => w.spool.push(SpoolWatch {
                target: b.target,
                files,
            }),
        }
    }
    start(shared);
    Answer::status(Status::Ok)
}

/// `UnwatchSpool`: `NotFound` when this connection does not watch it.
pub(crate) fn unwatch_spool(shared: &Shared, host: &mut dyn ThreadHost, req: &Envelope) -> Answer {
    let Ok(b) = req.body_as::<WatchSpool>() else {
        return Answer::status(Status::InvalidRequest);
    };
    let gone = {
        let mut w = lock(&shared.watches);
        let before = w.spool.len();
        w.spool.retain(|t| t.target != b.target);
        before != w.spool.len()
    };
    if gone {
        Answer::status(Status::Ok)
    } else {
        not_watched(host, &target_label(&b.target))
    }
}

fn not_watched(host: &mut dyn ThreadHost, what: &str) -> Answer {
    Answer::refused(Refusal {
        status: Status::NotFound,
        message: host.message(NOT_WATCHED, &[what]),
    })
}

/// Every watch ends (a sign-off): looks under way are dropped.
pub(crate) fn clear(shared: &Shared) {
    let mut w = lock(&shared.watches);
    w.queues.clear();
    w.spool.clear();
    w.era += 1;
}

/// The connection closes: the looking thread ends.
pub(crate) fn close(shared: &Shared) {
    let looker = {
        let mut w = lock(&shared.watches);
        w.closing = true;
        w.looker.take()
    };
    shared.wake.notify_all();
    if let Some(t) = looker {
        let _ = t.join();
    }
}

/// Make the looking thread, once.
pub(crate) fn start(shared: &Arc<Shared>) {
    let mut w = lock(&shared.watches);
    if w.looker.is_some() || w.closing {
        return;
    }
    let s = Arc::clone(shared);
    let report = lock(&shared.report).clone();
    w.looker = Some(watch::spawn(
        "qap-watches".into(),
        &shared.watch,
        move || run(&s),
        move |why| {
            let _ = report.send(ToMain::Closed(why));
        },
    ));
}

/// Look every `watch_poll` until the connection closes.
fn run(shared: &Arc<Shared>) {
    let mut host = (shared.threads)();
    loop {
        {
            let w = lock(&shared.watches);
            let (w, _) = shared
                .wake
                .wait_timeout(w, shared.watch_poll)
                .unwrap_or_else(PoisonError::into_inner);
            if w.closing {
                return;
            }
        }
        look(shared, &mut *host);
    }
}

/// One look at every watch.
fn look(shared: &Shared, host: &mut dyn ThreadHost) {
    let (era, queues, targets) = {
        let w = lock(&shared.watches);
        let q: Vec<Queue> = w.queues.iter().map(|q| q.queue.clone()).collect();
        let t: Vec<SpoolTarget> = w.spool.iter().map(|t| t.target.clone()).collect();
        (w.era, q, t)
    };
    for q in queues {
        messages(shared, host, &q, era);
    }
    for t in targets {
        spool(shared, host, &t, era);
    }
    crate::debug::look(shared, host);
}

/// The messages that arrived on `q` since the last look, sent oldest
/// first.
fn messages(shared: &Shared, host: &mut dyn ThreadHost, q: &Queue, era: u64) {
    let mut new: Vec<QueuedMessage> = Vec::new();
    let mut start = 0u32;
    let mut whole: Option<HashSet<Key>> = None;
    loop {
        let Ok(page) = host.list_messages(&ListMessages {
            queue: q.clone(),
            newest_first: true,
            start,
            max: PAGE,
        }) else {
            // Gone or refused now: the watch stays, quiet.
            return;
        };
        let mut w = lock(&shared.watches);
        if w.era != era {
            return;
        }
        let Some(watch) = w.queue(q) else {
            return;
        };
        let total = usize::try_from(page.total).unwrap_or(usize::MAX);
        if start == 0 && watch.known.len() > total.saturating_add(SLACK) {
            whole = Some(HashSet::new());
        }
        let n = page.messages.len();
        let mut hit = false;
        for m in page.messages {
            let seen = watch.known.contains(&m.key);
            if let Some(keys) = whole.as_mut() {
                keys.insert(m.key);
            } else if seen {
                hit = true;
                break;
            }
            // A message that arrived meanwhile shifts the pages: one
            // may come twice.
            if !seen && !new.iter().any(|n| n.key == m.key) {
                new.push(m);
            }
        }
        start = start.saturating_add(PAGE);
        if hit || n < PAGE as usize || usize::try_from(start).unwrap_or(usize::MAX) >= total {
            break;
        }
    }
    new.reverse();
    // Sent under the lock, so nothing goes out after an unwatch's `Ok`.
    let mut w = lock(&shared.watches);
    if w.era != era {
        return;
    }
    let Some(watch) = w.queue(q) else {
        return;
    };
    match whole {
        Some(keys) => watch.known = keys,
        None => watch.known.extend(new.iter().map(|m| m.key)),
    }
    for m in new {
        let body = MessageArrived {
            queue: q.clone(),
            message: m,
        };
        shared.send(vec![
            Envelope::event(Service::Messages, event::MESSAGE_ARRIVED, Status::Ok, None)
                .with_body(&body),
        ]);
    }
}

/// The changes to `t`'s files since the last look: removed, changed,
/// then added, each in the order the files were made.
fn spool(shared: &Shared, host: &mut dyn ThreadHost, t: &SpoolTarget, era: u64) {
    let Ok(list) = host.list_spooled_files(&listing(t)) else {
        return;
    };
    let now: HashMap<SpooledFileId, SpooledFile> =
        list.files.into_iter().map(|f| (f.id.clone(), f)).collect();
    let mut w = lock(&shared.watches);
    if w.era != era {
        return;
    }
    let Some(watch) = w.target(t) else {
        return;
    };
    let order = |f: &SpooledFile| (f.created, f.id.job.number.clone(), f.id.number);
    let mut removed: Vec<&SpooledFile> = watch
        .files
        .values()
        .filter(|f| !now.contains_key(&f.id))
        .collect();
    let mut changed: Vec<&SpooledFile> = now
        .values()
        .filter(|f| watch.files.get(&f.id).is_some_and(|old| old != *f))
        .collect();
    let mut added: Vec<&SpooledFile> = now
        .values()
        .filter(|f| !watch.files.contains_key(&f.id))
        .collect();
    removed.sort_by_key(|f| order(f));
    changed.sort_by_key(|f| order(f));
    added.sort_by_key(|f| order(f));
    let mut events = Vec::new();
    for (change, files) in [
        (SpoolChange::Removed, removed),
        (SpoolChange::Changed, changed),
        (SpoolChange::Added, added),
    ] {
        for f in files {
            let body = SpooledFileChanged {
                target: t.clone(),
                change,
                file: (change != SpoolChange::Removed).then(|| f.clone()),
                id: f.id.clone(),
            };
            events.push(
                Envelope::event(
                    Service::Spool,
                    event::SPOOLED_FILE_CHANGED,
                    Status::Ok,
                    None,
                )
                .with_body(&body),
            );
        }
    }
    watch.files = now;
    for e in events {
        shared.send(vec![e]);
    }
}
