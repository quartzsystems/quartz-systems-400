// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Directory watches over the IFS (`ifs::lease::notify`): the library's
//! `DirWatch` of a watched directory, fed with every job's changes there
//! and below it as the job's lease poll brings them
//! ([`super::lease::LeaseLink`]). Each watcher holds at most [`BATCHES`]
//! batches; past it its next answer is `Lost` (the client lists the
//! directory again, `STATUS_NOTIFY_ENUM_DIR`), as when the IFS's own queue
//! overflows.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use ifs::lease::notify::{ChangeKind, DirEvent};
use smb_server::{DirChange, DirChangeKind, DirWatch};
use tokio::sync::mpsc;

use super::worker::IfsWorker;

/// Batches one watcher holds before its next answer is `Lost`.
pub const BATCHES: usize = 64;

struct Feed {
    tx: mpsc::Sender<Vec<DirChange>>,
    lost: Arc<AtomicBool>,
}

#[derive(Default)]
struct Inner {
    feeds: HashMap<u64, Feed>,
    /// Changes for a watch whose number the job has not taken yet.
    early: HashMap<u64, Vec<DirChange>>,
}

/// The job's watches, by the IFS's number.
#[derive(Default)]
pub struct Watches {
    inner: Mutex<Inner>,
}

impl Watches {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The library's watch over the IFS's watch `id`; the IFS watch ends
    /// with it (through `io`).
    pub fn watcher(self: &Arc<Self>, id: u64, io: Arc<IfsWorker>) -> Watcher {
        let (tx, rx) = mpsc::channel(BATCHES);
        let lost = Arc::new(AtomicBool::new(false));
        let mut g = self.lock();
        if let Some(early) = g.early.remove(&id) {
            let _ = tx.try_send(early);
        }
        g.feeds.insert(
            id,
            Feed {
                tx,
                lost: lost.clone(),
            },
        );
        Watcher {
            id,
            rx,
            lost,
            watches: self.clone(),
            io,
        }
    }

    /// The poll brought `events` for the watch `id`.
    pub fn deliver(&self, id: u64, events: Vec<DirEvent>) {
        let changes: Vec<DirChange> = events.into_iter().map(change).collect();
        let mut g = self.lock();
        let Some(feed) = g.feeds.get(&id) else {
            // Kept only for a watch being taken now; a few at most.
            if g.early.len() >= BATCHES {
                g.early.clear();
            }
            g.early.entry(id).or_default().extend(changes);
            return;
        };
        if feed.tx.try_send(changes).is_err() {
            feed.lost.store(true, Ordering::SeqCst);
        }
    }

    fn end(&self, id: u64) {
        let mut g = self.lock();
        g.feeds.remove(&id);
        g.early.remove(&id);
    }
}

/// One IFS change as the library takes it: `/` becomes `\`.
fn change(e: DirEvent) -> DirChange {
    let name = e.name.replace('/', "\\");
    let kind = match e.kind {
        ChangeKind::Added => DirChangeKind::Added,
        ChangeKind::Removed => DirChangeKind::Removed,
        ChangeKind::Renamed { from } => DirChangeKind::Renamed {
            from: from.replace('/', "\\"),
        },
        ChangeKind::Modified => DirChangeKind::Modified,
        ChangeKind::Attributes => DirChangeKind::AttributesChanged,
        ChangeKind::Lost => DirChangeKind::Lost,
        ChangeKind::Moved => DirChangeKind::Moved,
        ChangeKind::Deleted => DirChangeKind::Deleted,
    };
    DirChange::new(name, kind, e.dir)
}

/// The library's watch of one directory.
pub struct Watcher {
    id: u64,
    rx: mpsc::Receiver<Vec<DirChange>>,
    lost: Arc<AtomicBool>,
    watches: Arc<Watches>,
    io: Arc<IfsWorker>,
}

fn lost() -> Vec<DirChange> {
    vec![DirChange::new("", DirChangeKind::Lost, true)]
}

#[async_trait]
impl DirWatch for Watcher {
    async fn next(&mut self) -> Option<Vec<DirChange>> {
        if self.lost.swap(false, Ordering::SeqCst) {
            return Some(lost());
        }
        let batch = self.rx.recv().await?;
        if self.lost.swap(false, Ordering::SeqCst) {
            return Some(lost());
        }
        Some(batch)
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.watches.end(self.id);
        let id = self.id;
        self.io.post(move |c| {
            let _ = c.watch_end(id);
        });
    }
}
