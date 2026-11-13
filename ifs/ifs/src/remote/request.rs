// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One request across its runs. A request that defers is run again
//! from the start; its [`Token`] is the same on every run, so a change
//! queued by the first run is found by the next one (exactly once),
//! steps a composite operation finished are not done again, and data
//! fetched since the request began counts as fresh for it however
//! short the cache's times are (every run makes progress).
//!
//! [`crate::lease::defer::deferring`] opens a run: with the token the
//! caller hands over ([`rerun`]) or a new one. A run that does not
//! defer ends its token; one that defers leaves it for the next run
//! ([`last`] says which it was). A token nobody runs again is dropped
//! after [`IDLE`]; [`abandon`] drops it at once (its job ended) and
//! [`interrupt_owner`] marks a job's waiting tokens interrupted, which
//! their next run answers `EINTR` on an `intr` mount.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use super::types::{Cred, RResult};
use super::work::Answer;
use crate::lock::Owner;

/// How long a token nobody runs again is kept.
pub const IDLE: Duration = Duration::from_secs(600);

/// A request, the same on every run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Token {
    /// Its number.
    pub id: u64,
    /// When its first run began.
    pub since: Instant,
}

impl Token {
    /// A new request.
    #[must_use]
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            since: Instant::now(),
        }
    }
}

impl Default for Token {
    fn default() -> Self {
        Self::new()
    }
}

/// A queued change's slot.
enum Slot {
    /// Queued under this ticket.
    InFlight(u64),
    /// Done.
    Done(RResult<Answer>),
    /// Done, with a value taken once (a mounted file system).
    Any(Option<Box<dyn Any + Send>>),
}

struct State {
    owner: Option<Owner>,
    seen: Instant,
    interrupted: bool,
    slots: HashMap<String, Slot>,
    steps: HashMap<String, Box<dyn Any + Send>>,
    /// Names the request changed: the slot of the latest change.
    names: HashMap<String, String>,
    runs: u32,
}

static TOKENS: LazyLock<Mutex<HashMap<u64, State>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn tokens() -> MutexGuard<'static, HashMap<u64, State>> {
    TOKENS.lock().unwrap_or_else(PoisonError::into_inner)
}

thread_local! {
    static CURRENT: Cell<Option<Token>> = const { Cell::new(None) };
    static NEXT_RUN: Cell<Option<Token>> = const { Cell::new(None) };
    static LAST: Cell<Option<Token>> = const { Cell::new(None) };
    static ALLOWED: Cell<bool> = const { Cell::new(false) };
    static SEEN: RefCell<HashMap<String, u32>> = RefCell::new(HashMap::new());
    static CRED: RefCell<Option<Cred>> = const { RefCell::new(None) };
    static OWNER: Cell<Option<Owner>> = const { Cell::new(None) };
}

/// What [`begin`] saved, for [`end`].
pub(crate) struct Run {
    outer: Option<Token>,
    allowed: bool,
    nested: bool,
}

/// Open a run (from `deferring`): `allow` lets remote work defer it.
pub(crate) fn begin(allow: bool) -> Run {
    let outer = CURRENT.with(Cell::get);
    let allowed = ALLOWED.with(|a| a.replace(allow));
    if outer.is_some() {
        return Run {
            outer,
            allowed,
            nested: true,
        };
    }
    let t = NEXT_RUN.with(Cell::take).unwrap_or_default();
    CURRENT.with(|c| c.set(Some(t)));
    SEEN.with(|s| s.borrow_mut().clear());
    CRED.with(|c| *c.borrow_mut() = None);
    OWNER.with(|o| o.set(None));
    let mut g = tokens();
    let now = Instant::now();
    g.retain(|_, s| now.duration_since(s.seen) < IDLE);
    let s = g.entry(t.id).or_insert_with(|| State {
        owner: None,
        seen: now,
        interrupted: false,
        slots: HashMap::new(),
        steps: HashMap::new(),
        names: HashMap::new(),
        runs: 0,
    });
    s.seen = now;
    s.runs += 1;
    Run {
        outer,
        allowed,
        nested: false,
    }
}

/// Close a run: a run that did not defer ends its request.
#[allow(clippy::needless_pass_by_value)] // the run ends here
pub(crate) fn end(r: Run, deferred: bool) {
    ALLOWED.with(|a| a.set(r.allowed));
    if r.nested {
        return;
    }
    let t = CURRENT.with(Cell::take);
    LAST.with(|l| l.set(t));
    CRED.with(|c| *c.borrow_mut() = None);
    if let Some(t) = t
        && !deferred
    {
        tokens().remove(&t.id);
    }
    CURRENT.with(|c| c.set(r.outer));
}

/// Whether remote work may defer the run in progress.
#[must_use]
pub fn allowed() -> bool {
    ALLOWED.with(Cell::get)
}

/// The request whose run is in progress.
#[must_use]
pub fn current() -> Option<Token> {
    CURRENT.with(Cell::get)
}

/// The token of the run that ended last on this thread.
#[must_use]
pub fn last() -> Option<Token> {
    LAST.with(Cell::get)
}

/// The next run on this thread continues `t`.
pub fn rerun(t: Token) {
    NEXT_RUN.with(|n| n.set(Some(t)));
}

/// Forget a [`rerun`] no run took (the request did not reach the IFS).
pub fn clear_rerun() {
    NEXT_RUN.with(Cell::take);
}

/// Drop `t`: what it queued is done or dropped when it arrives.
pub fn abandon(t: Token) {
    tokens().remove(&t.id);
}

/// How many times `t` has run.
#[must_use]
pub fn runs(t: Token) -> u32 {
    tokens().get(&t.id).map_or(0, |s| s.runs)
}

/// Note who the request in progress is for.
pub(crate) fn note_owner(o: Owner) {
    if let Some(t) = current()
        && let Some(s) = tokens().get_mut(&t.id)
    {
        s.owner = Some(o);
    }
}

/// Mark every waiting request of `o` interrupted; true when one was.
pub fn interrupt_owner(o: Owner) -> bool {
    let mut any = false;
    for s in tokens().values_mut() {
        if s.owner == Some(o) {
            s.interrupted = true;
            any = true;
        }
    }
    if any {
        super::tickets::wake();
    }
    any
}

/// Whether `t` was interrupted.
#[must_use]
pub fn interrupted(t: Token) -> bool {
    tokens().get(&t.id).is_some_and(|s| s.interrupted)
}

/// Whether the request with this number was interrupted.
#[must_use]
pub fn interrupted_id(id: u64) -> bool {
    tokens().get(&id).is_some_and(|s| s.interrupted)
}

/// A key for the `n`th occurrence of `what` in this run, so the same
/// change made twice by one request is two changes.
pub(crate) fn occurrence(what: &str) -> String {
    SEEN.with(|s| {
        let mut s = s.borrow_mut();
        let n = s.entry(what.to_string()).or_insert(0);
        *n += 1;
        format!("{what}#{n}")
    })
}

/// What a queued change's slot holds.
pub(crate) enum Found {
    /// Nothing queued yet.
    None,
    /// Queued under this ticket.
    InFlight(u64),
    /// Its result.
    Done(RResult<Answer>),
}

/// The slot `key` of request `t`.
pub(crate) fn slot(t: Token, key: &str) -> Found {
    match tokens().get(&t.id).and_then(|s| s.slots.get(key)) {
        None => Found::None,
        Some(Slot::InFlight(k)) => Found::InFlight(*k),
        Some(Slot::Done(r)) => Found::Done(r.clone()),
        Some(Slot::Any(_)) => Found::Done(Ok(Answer::Unit)),
    }
}

/// Record that `key` of `t` is queued under `ticket`.
pub(crate) fn queued(t: Token, key: &str, ticket: u64) {
    if let Some(s) = tokens().get_mut(&t.id) {
        s.slots.insert(key.to_string(), Slot::InFlight(ticket));
    }
}

/// The request in progress changes the name `name` with slot `key`.
pub(crate) fn note_name(name: &str, key: &str) {
    if let Some(t) = current()
        && let Some(s) = tokens().get_mut(&t.id)
    {
        s.names.insert(name.to_string(), key.to_string());
    }
}

/// The slot of the request's latest change of `name`.
pub(crate) fn named(name: &str) -> Option<String> {
    let t = current()?;
    tokens().get(&t.id)?.names.get(name).cloned()
}

/// A worker's result for `key` of request `id`; dropped when nobody
/// waits for it any more.
pub(crate) fn post(id: u64, key: &str, r: RResult<Answer>) {
    if let Some(s) = tokens().get_mut(&id)
        && let Some(slot) = s.slots.get_mut(key)
    {
        *slot = Slot::Done(r);
    }
}

/// A worker's result that is taken once.
pub(crate) fn post_any(id: u64, key: &str, v: Box<dyn Any + Send>) {
    if let Some(s) = tokens().get_mut(&id)
        && let Some(slot) = s.slots.get_mut(key)
    {
        *slot = Slot::Any(Some(v));
    }
}

/// Take what [`post_any`] left.
pub(crate) fn take_any(t: Token, key: &str) -> Option<Box<dyn Any + Send>> {
    match tokens().get_mut(&t.id)?.slots.get_mut(key)? {
        Slot::Any(v) => v.take(),
        _ => None,
    }
}

/// A step's state kept across runs.
pub(crate) fn step<T: Clone + 'static>(key: &str) -> Option<T> {
    let t = current()?;
    tokens()
        .get(&t.id)?
        .steps
        .get(key)?
        .downcast_ref::<T>()
        .cloned()
}

/// Take a step's state (it is kept again with [`set_step`]).
pub(crate) fn take_step<T: 'static>(key: &str) -> Option<T> {
    let t = current()?;
    let b = tokens().get_mut(&t.id)?.steps.remove(key)?;
    b.downcast::<T>().ok().map(|b| *b)
}

/// Keep a step's state for the next runs (nothing outside a run, and
/// nothing once the run waits: what it does then is not the request's).
pub(crate) fn set_step<T: Send + 'static>(key: &str, v: T) {
    if !crate::lease::defer::pending() {
        keep_step(key, v);
    }
}

/// Keep a step's state for the next runs, also when the run waits.
pub(crate) fn keep_step<T: Send + 'static>(key: &str, v: T) {
    if let Some(t) = current()
        && let Some(s) = tokens().get_mut(&t.id)
    {
        s.steps.insert(key.to_string(), Box::new(v));
    }
}

/// The caller of the operation in progress, as resolution set it.
#[must_use]
pub(crate) fn cred() -> Cred {
    CRED.with(|c| c.borrow().clone())
        .unwrap_or_else(Cred::nobody)
}

/// Set the caller of the operation in progress, and whose it is.
pub(crate) fn set_cred(c: Cred, o: Owner) {
    CRED.with(|x| *x.borrow_mut() = Some(c));
    OWNER.with(|x| x.set(Some(o)));
}

/// The request in progress waits: note whose it is, so its job's end
/// or interruption finds it.
pub(crate) fn note_waiting() {
    if let Some(o) = OWNER.with(Cell::get) {
        note_owner(o);
    }
}
