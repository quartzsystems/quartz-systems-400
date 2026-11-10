// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Directory watches across owners (`docs/NETSERVER.md` section 6): a
//! `NetServer` job watches a directory (and, for a tree watch, everything
//! below it) for its clients' `CHANGE_NOTIFY`; every owner's change to an
//! entry there, and the service's own, is queued for it and goes out with
//! its break orders ([`super::Leases::orders`]). Directories are named by
//! their canonical paths, which follow renames. A watcher's queue holds
//! at most [`QUEUE`] changes: past it the changes are dropped and one
//! [`ChangeKind::Lost`] tells the watcher to list the directory again.
//! Changes below a tree watch's directory that the watcher made itself
//! are left out (its server reports those); changes to the directory's
//! own entries are not (its server matches them with its own).

use std::collections::VecDeque;

use crate::lock::Owner;

/// Most changes one watch holds between polls.
pub const QUEUE: usize = 256;

/// What happened to an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    /// It appeared.
    Added,
    /// It went away.
    Removed,
    /// It was renamed within the same directory from `from` (a path
    /// relative to the watched directory, as `name` is).
    Renamed {
        /// The old name.
        from: String,
    },
    /// Its data was written or truncated.
    Modified,
    /// Its attributes, times or authorities changed.
    Attributes,
    /// Changes were dropped: list the directory again.
    Lost,
    /// The watched directory itself was renamed or moved.
    Moved,
    /// The watched directory itself was removed; nothing follows.
    Deleted,
}

/// One change, as a watcher hears it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEvent {
    /// The entry's path relative to the watched directory, components
    /// separated by `/`; empty for `Lost`, `Moved` and `Deleted`.
    pub name: String,
    /// What happened.
    pub kind: ChangeKind,
    /// The entry is a directory.
    pub dir: bool,
}

/// One owner's watch.
#[derive(Debug)]
struct Watch {
    id: u64,
    owner: Owner,
    /// The watched directory's canonical path.
    dir: String,
    tree: bool,
    queue: VecDeque<DirEvent>,
    lost: bool,
    /// Its directory was removed: nothing more comes.
    ended: bool,
}

impl Watch {
    fn push(&mut self, e: DirEvent) {
        if self.ended || self.lost {
            return;
        }
        if self.queue.len() >= QUEUE {
            self.queue.clear();
            self.lost = true;
            return;
        }
        self.queue.push_back(e);
    }

    fn has_news(&self) -> bool {
        !self.queue.is_empty() || self.lost
    }

    fn take(&mut self) -> Vec<DirEvent> {
        let mut v: Vec<DirEvent> = self.queue.drain(..).collect();
        if std::mem::take(&mut self.lost) {
            v = vec![DirEvent {
                name: String::new(),
                kind: ChangeKind::Lost,
                dir: true,
            }];
        }
        v
    }
}

/// The relative path of `path` below `dir`, or `None` when it is not
/// below.
fn below<'a>(path: &'a str, dir: &str) -> Option<&'a str> {
    if dir == "/" {
        return path.strip_prefix('/').filter(|r| !r.is_empty());
    }
    path.strip_prefix(dir)?
        .strip_prefix('/')
        .filter(|r| !r.is_empty())
}

/// Every watch.
#[derive(Debug, Default)]
pub(super) struct Watches {
    list: Vec<Watch>,
    next: u64,
}

impl Watches {
    pub(super) fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub(super) fn add(&mut self, owner: Owner, dir: String, tree: bool) -> u64 {
        self.next += 1;
        self.list.push(Watch {
            id: self.next,
            owner,
            dir,
            tree,
            queue: VecDeque::new(),
            lost: false,
            ended: false,
        });
        self.next
    }

    pub(super) fn end(&mut self, owner: Owner, id: u64) {
        self.list.retain(|w| w.owner != owner || w.id != id);
    }

    pub(super) fn release_owner(&mut self, owner: Owner) {
        self.list.retain(|w| w.owner != owner);
    }

    /// `kind` happened to the entry at `path` (canonical), by `by`.
    pub(super) fn changed(&mut self, by: Owner, path: &str, kind: &ChangeKind, dir: bool) {
        for w in &mut self.list {
            let Some(rel) = below(path, &w.dir) else {
                continue;
            };
            let deep = rel.contains('/');
            if deep && (!w.tree || w.owner == by) {
                continue;
            }
            w.push(DirEvent {
                name: rel.to_owned(),
                kind: kind.clone(),
                dir,
            });
        }
    }

    /// `from` was renamed to `to`, by `by`: the entry's change in the
    /// directories that watch it, and the watches of the directory and
    /// below it follow it.
    pub(super) fn renamed(&mut self, by: Owner, from: &str, to: &str, dir: bool) {
        let parent = |p: &str| p.rfind('/').map_or_else(String::new, |i| p[..i].to_owned());
        let same_dir = parent(from) == parent(to);
        for w in &mut self.list {
            let (old, new) = (below(from, &w.dir), below(to, &w.dir));
            let deep = |r: &str| r.contains('/');
            let (tree, owner) = (w.tree, w.owner);
            let hears = |r: &str| !deep(r) || (tree && owner != by);
            match (old, new) {
                (Some(o), Some(n)) if same_dir && hears(n) => w.push(DirEvent {
                    name: n.to_owned(),
                    kind: ChangeKind::Renamed { from: o.to_owned() },
                    dir,
                }),
                (o, n) => {
                    if let Some(o) = o.filter(|o| hears(o)) {
                        w.push(DirEvent {
                            name: o.to_owned(),
                            kind: ChangeKind::Removed,
                            dir,
                        });
                    }
                    if let Some(n) = n.filter(|n| hears(n)) {
                        w.push(DirEvent {
                            name: n.to_owned(),
                            kind: ChangeKind::Added,
                            dir,
                        });
                    }
                }
            }
        }
        if !dir {
            return;
        }
        for w in &mut self.list {
            if w.dir == from {
                to.clone_into(&mut w.dir);
                w.push(DirEvent {
                    name: String::new(),
                    kind: ChangeKind::Moved,
                    dir: true,
                });
            } else if let Some(rest) = below(&w.dir, from) {
                w.dir = format!("{to}/{rest}");
            }
        }
    }

    /// The directory at `path` was removed: its watches end.
    pub(super) fn removed_dir(&mut self, path: &str) {
        for w in &mut self.list {
            if w.dir == path {
                w.push(DirEvent {
                    name: String::new(),
                    kind: ChangeKind::Deleted,
                    dir: true,
                });
                w.ended = true;
            }
        }
    }

    pub(super) fn has_news(&self, owner: Owner) -> bool {
        self.list.iter().any(|w| w.owner == owner && w.has_news())
    }

    /// `owner`'s queued changes, by watch.
    pub(super) fn take(&mut self, owner: Owner) -> Vec<(u64, Vec<DirEvent>)> {
        self.list
            .iter_mut()
            .filter(|w| w.owner == owner && w.has_news())
            .map(|w| (w.id, w.take()))
            .collect()
    }
}
