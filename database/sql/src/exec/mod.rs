// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Running statements: the context a job gives SQL, name resolution,
//! and `Prepared::execute`. Queries are planned in `query`, data
//! changes in `dml`, definitions in `ddl`, expressions in `eval`.

pub mod alter;
mod column;
pub mod agg;
pub mod alias;
mod answers;
pub mod cast;
mod change;
pub mod catalog;
pub mod cst;
mod cte;
pub mod datetime;
pub mod cursor;
pub mod ddl;
pub mod dml;
pub mod drop;
pub mod dynamic;
pub mod eval;
mod from;
pub use from::ORDER_COLUMN;
pub mod func;
pub mod grant;
pub mod index;
pub mod json;
pub mod locator;
mod grouping;
mod join;
pub mod mem;
pub mod names;
pub mod opnqryf;
mod olap;
pub mod query;
pub mod rename;
pub mod routine;
pub mod schema;
pub mod registers;
pub mod restype;
pub mod xml;
pub mod seq;
mod sort;
pub mod spill;
pub mod table;
pub mod view;

use db::file::{self, FileDesc, MemberEntry};
use db::format::FormatDef;
use db::odp::{self, Odp, OpenOpts, Session, SharedLocks, Usage};
use dds::FieldDef;
use qsname::{LibSpec, MemberSpec, Name};
use store::obj::ops::{self, NameContext};
use store::obj::{JobHandle, LockState, ObjectId, types};
use store::txn::Txn;

use crate::ast::{Naming, ObjectName, Statement};
use crate::error::{Result, SqlError};
use crate::parse;
use crate::value::Value;

/// A table the statement named, resolved.
#[derive(Clone, Debug)]
pub struct TableInfo {
    /// The file.
    pub id: ObjectId,
    /// Its description.
    pub desc: FileDesc,
    /// The member used.
    pub member: MemberEntry,
    /// The record format.
    pub format: FormatDef,
}

impl TableInfo {
    /// The field named, with its index.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<(usize, &FieldDef)> {
        let by_alias = || self.format.format.fields.iter().enumerate().find(|(_, f)| f.alias.as_deref() == Some(name));
        match Name::parse(name) {
            Ok(n) => self.format.format.field(&n).or_else(by_alias),
            Err(_) => by_alias(),
        }
    }

    /// The member whose LOB tree holds the table's LOB values.
    pub fn lob_owner(&self, txn: &mut Txn<'_>) -> Result<ObjectId> {
        Ok(db::member::lob::owner(txn, self.member.object)?)
    }

    /// Whether the format has LOB fields.
    #[must_use]
    pub fn has_lobs(&self) -> bool {
        self.format.format.fields.iter().any(|f| f.data_type.is_lob())
    }
}

/// Rows an `ORDER BY` sorts in memory before spilling (`sort_budget`).
pub const DEFAULT_SORT_BUDGET: usize = 100_000;

/// What a job brings to SQL: naming, the library list, the schema, the
/// job handle and its database session.
#[allow(clippy::struct_excessive_bools)]
pub struct SqlContext {
    /// Naming mode.
    pub naming: Naming,
    /// QSYS, the root of name resolution.
    pub qsys: ObjectId,
    /// Current library and library list.
    pub names: NameContext,
    /// `SET SCHEMA` or `DFTRDBCOL`: the default schema of unqualified
    /// table, view, index and alias names, under either naming.
    pub current_schema: Option<Name>,
    /// The job.
    pub job: JobHandle,
    /// The job's database session (overrides, commitment control).
    pub session: Session,
    /// The record lock table.
    pub locks: SharedLocks,
    /// The user, for `CURRENT USER`.
    pub user: String,
    /// A fixed clock, for tests; the real one when `None`.
    pub now: Option<u64>,
    /// Lines the last `EXPLAIN` produced.
    pub explain: Vec<String>,
    /// Declared cursors by name.
    pub cursors: std::collections::HashMap<String, cursor::Cursor>,
    /// Rows a sort holds in memory before it spills a run.
    pub sort_budget: usize,
    /// Where sort runs go: the job's temporary storage.
    pub spill: Box<dyn spill::SpillArea>,
    /// A column function left out a null value (SQLSTATE `01003`).
    pub nulls_eliminated: bool,
    /// `CURRENT DECFLOAT ROUNDING MODE`.
    pub decfloat_rounding: qdecimal::DfRounding,
    /// The data mapping error a result column took as null (`+802`,
    /// `+420`, `+180` ...): the statement's warning code and state.
    pub mapping_warning: Option<(i32, &'static str)>,
    /// Whether result columns take data mapping errors as null (a
    /// `SELECT` statement's select list).
    pub map_errors: bool,
    /// The unit of work's LOB locators.
    pub locators: locator::Locators,
    /// The system outside the store, for the `QSYS2` services; `None`
    /// where there is none (tools, tests).
    pub host: Option<std::rc::Rc<dyn catalog::services::host::SystemHost>>,
    /// Changes are not under commitment control even when the job has
    /// it (a program precompiled with `COMMIT(*NONE)`).
    pub no_commit: bool,
    /// `SET TRANSACTION`: the unit of work's level over the statements'
    /// own, until it commits or rolls back.
    pub isolation: Option<crate::ast::Isolation>,
    /// Asked every [`INTERRUPT_EVERY`] rows read; `None` never ends.
    pub interrupt: Option<Interrupt>,
    /// Rows read since the context was made.
    interrupt_ticks: u32,
    /// Called at a statement's safe points (between batches of a scan,
    /// between rows of the steps after it, between rows a change
    /// writes) with the store given back, so others can use it; true
    /// ends the statement. `None` never yields.
    pub yielder: Option<Yielder>,
    /// The least time between two yields.
    pub yield_every: std::time::Duration,
    /// Files and members a statement that may yield holds `*SHRRD` on,
    /// so nobody clears, reorganizes or deletes them under it.
    pub held: Vec<ObjectId>,
    /// Frames holding what they read from the store (a scan's open
    /// members, a change's open path): a yield needs none above it.
    open: u32,
    /// Yields since the context was made.
    yields: u64,
    /// The subquery answers of a change decided as of its start.
    pub(crate) answers: Option<answers::Answers>,
    /// Read locks a statement under `*ALL` holds with no commit
    /// definition to keep them: released when it ends.
    stmt_reads: Vec<(ObjectId, u64)>,
    /// Their owner number in the record lock table.
    stmt_owner: u64,
    /// The working storage a statement may hold ([`mem::STATEMENT_MEMORY`]).
    pub memory_limit: usize,
    /// What the statements of this context hold now.
    meter: mem::Meter,
    /// When the last yield ended.
    last_yield: Option<std::time::Instant>,
    /// The warning the last statement left (`SQL7905`, `SQL0569`).
    pub warning: Option<crate::sqlca::Warning>,
    /// The values the job was given by sequences and identity columns.
    pub seq: seq::JobValues,
    /// The values of the OLAP specifications of the row being
    /// projected, by slot.
    pub(crate) windows: std::collections::HashMap<usize, Value>,
    /// The rows a recursive common table expression's name stands for
    /// while its step runs, innermost last.
    pub(crate) ctes: Vec<cte::Binding>,
    /// The records an `INSERT` wrote, for a data-change table reference.
    pub(crate) captured: Option<Vec<change::Captured>>,
    /// The query being run has `SKIP LOCKED DATA`.
    pub(crate) skip_locked: bool,
    /// Special registers and built-in global variables.
    pub registers: registers::Registers,
    /// Routines: whether the requester runs the calls `CALL` plans,
    /// and the plan the last one left.
    pub routine: routine::State,
}

/// See [`SqlContext::yielder`].
pub type Yielder = std::rc::Rc<dyn Fn() -> bool>;

thread_local! {
    /// Statements yielding while they wait for a record.
    static RECORD_WAITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many statements of this thread are in a turn taken while they
/// wait for a record another job holds: when every statement in
/// progress is, the service may wait a little for requests rather than
/// spin.
#[must_use]
pub fn record_waits() -> usize {
    RECORD_WAITS.with(std::cell::Cell::get)
}

/// The default [`SqlContext::yield_every`].
pub const YIELD_EVERY: std::time::Duration = std::time::Duration::from_millis(5);

/// Rows read between two looks at [`SqlContext::interrupt`].
pub const INTERRUPT_EVERY: u32 = 64;

/// Asked while rows are read: true ends the statement with
/// [`SqlError::Cancelled`].
pub type Interrupt = std::rc::Rc<dyn Fn() -> bool>;

impl SqlContext {
    /// One row read: every [`INTERRUPT_EVERY`] rows, the first
    /// included, ask [`SqlContext::interrupt`] whether to end.
    pub fn poll_interrupt(&mut self) -> Result<()> {
        let Some(f) = &self.interrupt else {
            return Ok(());
        };
        let tick = self.interrupt_ticks;
        self.interrupt_ticks = tick.wrapping_add(1);
        if tick.is_multiple_of(INTERRUPT_EVERY) && f() {
            return Err(SqlError::Cancelled);
        }
        Ok(())
    }

    /// A context for `job` with a library list.
    #[must_use]
    pub fn new(qsys: ObjectId, job: JobHandle, names: NameContext) -> Self {
        Self::with_locks(qsys, job, names, odp::new_shared())
    }

    /// A context whose record locks live in a table other jobs share.
    #[must_use]
    pub fn with_locks(
        qsys: ObjectId,
        job: JobHandle,
        names: NameContext,
        locks: odp::SharedLocks,
    ) -> Self {
        Self {
            naming: Naming::Sys,
            qsys,
            names,
            current_schema: None,
            job,
            session: Session::new(job, locks.clone()),
            locks,
            user: "QSECOFR".into(),
            now: None,
            explain: Vec::new(),
            cursors: std::collections::HashMap::new(),
            sort_budget: DEFAULT_SORT_BUDGET,
            spill: Box::new(spill::MemSpill::default()),
            nulls_eliminated: false,
            decfloat_rounding: qdecimal::DfRounding::HalfEven,
            mapping_warning: None,
            map_errors: false,
            locators: locator::Locators::default(),
            host: None,
            no_commit: false,
            isolation: None,
            interrupt: None,
            interrupt_ticks: 0,
            yielder: None,
            yield_every: YIELD_EVERY,
            held: Vec::new(),
            open: 0,
            yields: 0,
            answers: None,
            stmt_reads: Vec::new(),
            stmt_owner: odp::locks::next_owner(),
            memory_limit: mem::STATEMENT_MEMORY,
            meter: mem::Meter::default(),
            last_yield: None,
            warning: None,
            seq: seq::JobValues::default(),
            windows: std::collections::HashMap::new(),
            ctes: Vec::new(),
            captured: None,
            skip_locked: false,
            registers: registers::Registers::default(),
            routine: routine::State::default(),
        }
    }

    /// A charge against the statement's working storage.
    pub(crate) fn charge(&self) -> mem::Charge {
        mem::Charge::new(&self.meter, self.memory_limit)
    }

    /// A frame starts holding what it read from the store: no yield
    /// under it but its own ([`SqlContext::own_point`]).
    pub(crate) fn hold_open(&mut self) {
        self.open += 1;
    }

    /// The frame of [`SqlContext::hold_open`] lets go.
    pub(crate) fn release_open(&mut self) {
        self.open = self.open.saturating_sub(1);
    }

    /// Run `f` holding the store's state: nothing under it yields.
    pub(crate) fn no_yield<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        self.hold_open();
        let r = f(self);
        self.release_open();
        r
    }

    /// Yields so far, to tell whether others ran since.
    pub(crate) fn yields(&self) -> u64 {
        self.yields
    }

    /// Under a yielder, `*SHRRD` on each object a scan reads, held until
    /// [`SqlContext::release_held`].
    pub(crate) fn hold(&mut self, txn: &mut Txn<'_>, objects: &[ObjectId], what: &str) -> Result<()> {
        if self.yielder.is_none() {
            return Ok(());
        }
        for &id in objects {
            if self.held.contains(&id) {
                continue;
            }
            odp::allocate(txn, self.job, id, LockState::SharedRead, what)?;
            self.held.push(id);
        }
        Ok(())
    }

    /// Whether the records a statement reads are read-locked: at
    /// `*ALL`, the commit definition's level, or the unit's after
    /// `SET TRANSACTION` when the job has none.
    pub(crate) fn locks_reads(&self) -> bool {
        if self.no_commit || self.isolation == Some(crate::ast::Isolation::Nc) {
            return false;
        }
        let level = match self.session.commit_def() {
            Some(d) => Some(d.borrow().lcklvl()),
            None => self.isolation.and_then(lock_level),
        };
        level == Some(db::cmt::LckLvl::All)
    }

    /// Read-lock record `rrn` of physical member `member`: by the commit
    /// definition until the unit of work ends, else until the statement
    /// ends. Another job's update lock refuses it (`SQL0913`).
    pub(crate) fn lock_read(&mut self, member: ObjectId, rrn: u64, file: &str, mbr: &str) -> Result<()> {
        let r = if let Some(d) = self.session.commit_def() {
            d.borrow_mut().lock_read(member, rrn)
        } else {
            let who = odp::Holder::new(self.job, self.stmt_owner);
            let r = self.locks.lock().expect("lock table").lock_read(member, rrn, who);
            if r == Ok(true) {
                self.stmt_reads.push((member, rrn));
            }
            r.map(|_| ())
        };
        r.map_err(|e| {
            let e = match e {
                odp::Refused::Job(h) => db::DbError::RecordLocked {
                    file: file.to_string(),
                    member: mbr.to_string(),
                    rrn,
                    holder: h.0,
                },
                odp::Refused::SameJob => db::DbError::RecordLockedInJob {
                    file: file.to_string(),
                    member: mbr.to_string(),
                    rrn,
                },
            };
            SqlError::from(e)
        })
    }

    /// Under `SKIP LOCKED DATA`: whether another job holds record `rrn`
    /// of `member` locked, so the row is left out.
    pub(crate) fn skips(&self, member: Option<ObjectId>, rrn: u64) -> bool {
        self.skip_locked
            && member.is_some_and(|m| self.locks.lock().expect("lock table").holder(m, rrn).is_some_and(|j| j != self.job))
    }

    /// The statement ended: the read locks it kept itself go.
    fn release_stmt_reads(&mut self) {
        if self.stmt_reads.is_empty() {
            return;
        }
        let who = odp::Holder::new(self.job, self.stmt_owner);
        let mut table = self.locks.lock().expect("lock table");
        for (member, rrn) in self.stmt_reads.drain(..) {
            table.unlock_read(member, rrn, who);
        }
    }

    /// A row of a step that holds nothing from the store (a filter,
    /// grouping, the select list, a join over rows read, a sort's keys):
    /// the interrupt asked, and a yield when it is time.
    pub(crate) fn tick(&mut self, txn: &mut Txn<'_>) -> Result<()> {
        self.poll_interrupt()?;
        self.safe_point(txn).map(|_| ())
    }

    /// A safe point with no frame holding the store's state: give the
    /// store back and call the yielder, at most every
    /// [`SqlContext::yield_every`], when the statement's transaction
    /// has changed nothing. True when it yielded.
    pub(crate) fn safe_point(&mut self, txn: &mut Txn<'_>) -> Result<bool> {
        self.yield_at(txn, 0, false)
    }

    /// The safe point of the one frame holding the store's state (a
    /// scan between batches): true when it yielded, and that frame
    /// reads what it held again.
    pub(crate) fn own_point(&mut self, txn: &mut Txn<'_>) -> Result<bool> {
        self.yield_at(txn, 1, false)
    }

    /// The safe point of a change between two rows: what it wrote so
    /// far is committed to the store first (its commit cycle and record
    /// locks go on as they were), then it yields as [`Self::own_point`].
    pub(crate) fn change_point(&mut self, txn: &mut Txn<'_>) -> Result<bool> {
        self.yield_at(txn, 1, true)
    }

    /// The pause of a long definition or maintenance step in the
    /// database ([`db::Pause`]): what it did so far committed, then a
    /// turn for the others as at [`Self::safe_point`]. Its locks keep
    /// what it works on as it was.
    pub(crate) fn pause(&mut self, txn: &mut Txn<'_>) -> db::Result<bool> {
        match self.turn(txn, 0, true) {
            Ok(Some(yielded)) => Ok(yielded),
            Ok(None) => Err(db::DbError::Ended),
            Err(e) => Err(e.into()),
        }
    }

    /// A change waiting for a record another job holds: what it wrote
    /// so far committed, then a turn for the others now, the holder
    /// among them. False without a yielder: nobody else can run, so
    /// there is nothing to wait for.
    pub(crate) fn wait_turn(&mut self, txn: &mut Txn<'_>) -> Result<bool> {
        let Some(y) = self.yielder.clone() else {
            return Ok(false);
        };
        txn.commit_part()?;
        if !txn.suspend() {
            return Ok(false);
        }
        RECORD_WAITS.with(|w| w.set(w.get() + 1));
        let stop = y();
        RECORD_WAITS.with(|w| w.set(w.get() - 1));
        txn.resume()?;
        self.yields += 1;
        self.last_yield = Some(std::time::Instant::now());
        if stop {
            return Err(SqlError::Cancelled);
        }
        Ok(true)
    }

    fn yield_at(&mut self, txn: &mut Txn<'_>, own: u32, commit: bool) -> Result<bool> {
        self.turn(txn, own, commit)?.ok_or(SqlError::Cancelled)
    }

    /// A turn for the others when it is time: whether it yielded, `None`
    /// when the yielder ended the statement.
    fn turn(&mut self, txn: &mut Txn<'_>, own: u32, commit: bool) -> std::result::Result<Option<bool>, store::StoreError> {
        let Some(y) = self.yielder.clone() else {
            return Ok(Some(false));
        };
        if self.open != own || self.last_yield.is_some_and(|t| t.elapsed() < self.yield_every) {
            return Ok(Some(false));
        }
        if commit {
            txn.commit_part()?;
        }
        if !txn.suspend() {
            return Ok(Some(false));
        }
        let stop = y();
        txn.resume()?;
        self.yields += 1;
        self.last_yield = Some(std::time::Instant::now());
        Ok(if stop { None } else { Some(true) })
    }

    /// A turn for the others between two statements of a run, when it
    /// is time: no transaction is open. The yielder may end the run.
    pub fn between(&mut self) -> Result<()> {
        let Some(y) = self.yielder.clone() else {
            return Ok(());
        };
        if self.last_yield.is_some_and(|t| t.elapsed() < self.yield_every) {
            return Ok(());
        }
        let stop = y();
        self.yields += 1;
        self.last_yield = Some(std::time::Instant::now());
        if stop { Err(SqlError::Cancelled) } else { Ok(()) }
    }

    /// Give back the locks a yielding statement took.
    pub fn release_held(&mut self, txn: &mut Txn<'_>) {
        self.release_stmt_reads();
        if txn.is_suspended() && txn.resume().is_err() {
            return;
        }
        for id in std::mem::take(&mut self.held) {
            odp::deallocate(txn, self.job, id, LockState::SharedRead);
        }
        self.open = 0;
        self.answers = None;
        self.last_yield = None;
    }

    /// The clock.
    #[must_use]
    pub fn clock(&self) -> u64 {
        self.now.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        })
    }

    /// The clock as seconds and the picoseconds past them, as fine as
    /// the system clock goes (a fixed clock has none).
    #[must_use]
    pub fn clock_picos(&self) -> (u64, u64) {
        match self.now {
            Some(s) => (s, 0),
            None => std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or((0, 0), |d| (d.as_secs(), u64::from(d.subsec_nanos()) * 1000)),
        }
    }

    /// An identifier as a system name.
    pub fn sys_name(ident: &crate::ast::Ident) -> Result<Name> {
        let v = &ident.value;
        if v.len() > 10 {
            return Err(SqlError::Unsupported {
                what: format!("long name {v} without a system name (FOR SYSTEM NAME / FOR COLUMN)"),
            });
        }
        Name::parse(v).map_err(|_| SqlError::ObjectNotFound {
            name: v.clone(),
            kind: "name",
        })
    }

    /// Where an unqualified name is looked up or created: the default
    /// schema when one is set, under system naming too; else the
    /// library list.
    fn default_library(&self) -> LibSpec {
        match self.current_schema {
            Some(s) => LibSpec::Name(s),
            None => LibSpec::Libl,
        }
    }

    /// The library an object name means.
    pub fn lib_spec(&self, name: &ObjectName) -> Result<LibSpec> {
        Ok(match &name.schema {
            Some(s) => LibSpec::Name(Self::sys_name(s)?),
            None => self.default_library(),
        })
    }

    /// The library a schema name means, by its system name or its SQL
    /// name.
    pub fn schema_library(&self, txn: &mut Txn<'_>, schema: &crate::ast::Ident) -> Result<ObjectId> {
        names::schema(txn, self.qsys, schema)?.ok_or_else(|| SqlError::ObjectNotFound {
            name: schema.value.clone(),
            kind: "schema",
        })
    }

    /// The system name of the library a schema name means.
    pub fn schema_system_name(&self, txn: &mut Txn<'_>, schema: &crate::ast::Ident) -> Result<Name> {
        if let Some(n) = names::direct(schema) {
            return Ok(n);
        }
        let lib = self.schema_library(txn, schema)?;
        Ok(ops::read_header(txn, lib)?.1.name)
    }

    /// The library spec of an object name, a long schema name resolved.
    pub fn lib_spec_in(&self, txn: &mut Txn<'_>, name: &ObjectName) -> Result<LibSpec> {
        match &name.schema {
            Some(s) => Ok(LibSpec::Name(self.schema_system_name(txn, s)?)),
            None => Ok(self.default_library()),
        }
    }

    /// The library to create an object in.
    pub fn library_for(&self, txn: &mut Txn<'_>, name: &ObjectName) -> Result<ObjectId> {
        match self.lib_spec_in(txn, name)? {
            // BLOCKED(sql): QTEMP is in the temporary ASP, and a statement
            // runs in one ASP's transaction.
            LibSpec::Name(n) if n.to_string() == "QTEMP" => {
                Err(SqlError::Unsupported { what: "SQL objects in QTEMP".into() })
            }
            LibSpec::Name(n) => ops::find_library(txn, self.qsys, n)
                .map(|f| f.id)
                .map_err(|_| SqlError::ObjectNotFound {
                    name: n.to_string(),
                    kind: "schema",
                }),
            LibSpec::Curlib | LibSpec::Libl => self
                .names
                .curlib
                .or_else(|| self.names.libl.first().copied())
                .ok_or_else(|| SqlError::ObjectNotFound {
                    name: "*CURLIB".into(),
                    kind: "schema",
                }),
        }
    }

    /// Resolve a table, view or index name to a file.
    pub fn resolve_file(&self, txn: &mut Txn<'_>, name: &ObjectName) -> Result<ObjectId> {
        let not_found = || SqlError::ObjectNotFound {
            name: name.to_string(),
            kind: "table",
        };
        let lib = self.lib_spec_in(txn, name).map_err(|_| not_found())?;
        if let Some(n) = names::direct(&name.name) {
            return ops::resolve(txn, self.qsys, &self.names, lib, n, types::FILE)
                .map(|f| f.id)
                .map_err(|_| not_found());
        }
        // A long SQL name: the name index of each library searched.
        names::check_length(&name.name)?;
        let libs = match lib {
            LibSpec::Name(n) => {
                let l = ops::find_library(txn, self.qsys, n).map_err(|_| not_found())?.id;
                store::obj::guard::check_id(txn, "lib.search", l).map_err(|_| not_found())?;
                vec![l]
            }
            LibSpec::Curlib => self.names.curlib.into_iter().collect(),
            LibSpec::Libl => self.names.libl.clone(),
        };
        for l in libs {
            for space in [db::sqlinf::names::Space::Table, db::sqlinf::names::Space::Index] {
                if let Some(id) = names::find(txn, l, space, &name.name.value)? {
                    return Ok(id);
                }
            }
        }
        Err(not_found())
    }

    /// Resolve a table to its first member.
    pub fn resolve_table(&self, txn: &mut Txn<'_>, name: &ObjectName) -> Result<TableInfo> {
        let mut id = self.resolve_file(txn, name)?;
        // An alias stands for the table it names, and its member.
        let mut spec = MemberSpec::First;
        if let Some(def) = file::alias_of(txn, id)? {
            id = self.resolve_file(txn, &alias::target(&def))?;
            if let Some(m) = def.member {
                spec = MemberSpec::Name(m);
            }
        }
        let desc = file::describe(txn, id).map_err(|_| SqlError::ObjectNotFound {
            name: name.to_string(),
            kind: "table",
        })?;
        let member = *desc
            .member(spec)
            .ok_or_else(|| SqlError::ObjectNotFound {
                name: format!("{name} (no member)"),
                kind: "table",
            })?;
        let format = desc
            .formats
            .first()
            .cloned()
            .ok_or_else(|| SqlError::ObjectNotFound {
                name: name.to_string(),
                kind: "table",
            })?;
        Ok(TableInfo {
            id,
            desc,
            member,
            format,
        })
    }

    /// Open a table for a data change, under commitment control when the
    /// job has it.
    pub fn open(&mut self, txn: &mut Txn<'_>, t: &TableInfo, usage: Usage) -> Result<Odp> {
        let nc = self.no_commit || self.isolation == Some(crate::ast::Isolation::Nc);
        let cmt = if nc { None } else { self.session.commit_def().cloned() };
        let opts = OpenOpts {
            member: MemberSpec::Name(t.member.name),
            usage,
            keyed: false,
            commit: cmt.is_some(),
            ..OpenOpts::input(self.job)
        };
        let mut o = Odp::open(txn, self.locks.clone(), t.id, opts)?;
        if let Some(c) = cmt {
            o.set_commit(c);
        }
        Ok(o)
    }
}

/// The lock level of an isolation; `None` for no commit. `RR` is
/// `*ALL`: the engine has no `*RR`.
#[must_use]
pub fn lock_level(iso: crate::ast::Isolation) -> Option<db::cmt::LckLvl> {
    use crate::ast::Isolation as I;
    match iso {
        I::Nc => None,
        I::Ur => Some(db::cmt::LckLvl::Chg),
        I::Cs => Some(db::cmt::LckLvl::Cs),
        I::Rs | I::Rr => Some(db::cmt::LckLvl::All),
    }
}

/// `ROLLBACK` of the job's unit of work, undone taking turns with its
/// record locks held.
fn rollback(txn: &mut Txn<'_>, ctx: &mut SqlContext) -> Result<()> {
    let now = ctx.now;
    let mut session = std::mem::replace(&mut ctx.session, Session::new(ctx.job, ctx.locks.clone()));
    let r = session.rollbck_paused(txn, now, &mut |t| ctx.pause(t));
    ctx.session = session;
    r?;
    Ok(())
}

/// `SET TRANSACTION`: refused once the unit of work changed
/// something; else the level of the rest of the unit, and of the
/// commitment definition when the job has one.
fn set_transaction(ctx: &mut SqlContext, iso: crate::ast::Isolation) -> Result<()> {
    let refused = || SqlError::UnitOfWork {
        what: "SET TRANSACTION".into(),
    };
    if let Some(def) = ctx.session.commit_def() {
        let mut def = def.borrow_mut();
        if def.cycle() != 0 {
            return Err(refused());
        }
        if let Some(l) = lock_level(iso) {
            def.set_lcklvl(l);
        }
    }
    ctx.isolation = Some(iso);
    Ok(())
}

/// A column of a result set, as the SQLDA would describe it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnDesc {
    /// Name.
    pub name: String,
    /// SQL type, e.g. `DECIMAL(7,0)`.
    pub data_type: String,
    /// Nullable.
    pub nullable: bool,
}

/// Rows with their columns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResultSet {
    /// Columns.
    pub columns: Vec<ColumnDesc>,
    /// Rows.
    pub rows: Vec<Vec<Value>>,
}

impl ResultSet {
    /// A simple text table.
    #[must_use]
    pub fn render(&self) -> String {
        let mut widths: Vec<usize> = self
            .columns
            .iter()
            .map(|c| c.name.chars().count())
            .collect();
        let texts: Vec<Vec<String>> = self
            .rows
            .iter()
            .map(|r| {
                r.iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let s = if v.is_null() {
                            "-".to_string()
                        } else {
                            v.to_string()
                        };
                        widths[i] = widths[i].max(s.chars().count());
                        s
                    })
                    .collect()
            })
            .collect();
        let line = |cells: &[String]| -> String {
            cells
                .iter()
                .enumerate()
                .map(|(i, c)| format!("{c:<w$}", w = widths[i]))
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_string()
        };
        let mut out = line(
            &self
                .columns
                .iter()
                .map(|c| c.name.clone())
                .collect::<Vec<_>>(),
        );
        out.push('\n');
        for t in &texts {
            out.push_str(&line(t));
            out.push('\n');
        }
        out
    }
}

/// What a statement produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A result set.
    Rows(ResultSet),
    /// Rows changed.
    Count(u64),
    /// Nothing to show.
    Done,
}

/// A parsed statement ready to run.
#[derive(Clone, Debug)]
pub struct Prepared {
    /// The statement.
    pub statement: Statement,
    /// Naming mode it was parsed with.
    pub naming: Naming,
}

/// Parse a statement.
pub fn prepare(text: &str, naming: Naming) -> Result<Prepared> {
    Ok(Prepared {
        statement: parse::parse(text, naming)?,
        naming,
    })
}

impl Prepared {
    /// Run the statement with `params` for the `?` markers.
    pub fn execute(
        &self,
        txn: &mut Txn<'_>,
        ctx: &mut SqlContext,
        params: &[Value],
    ) -> Result<Outcome> {
        ctx.naming = self.naming;
        ctx.warning = None;
        // Queries, changes and the long definitions take turns at their
        // safe points; any other statement runs to its end.
        let r = match &self.statement {
            Statement::Select(_)
            | Statement::Insert(_)
            | Statement::Update(_)
            | Statement::Delete(_)
            | Statement::Merge(_)
            | Statement::Truncate(_)
            | Statement::CreateIndex(_)
            | Statement::AlterTable(_)
            | Statement::Rollback { .. }
            | Statement::Open { .. } => self.dispatch(txn, ctx, params),
            _ => ctx.no_yield(|ctx| self.dispatch(txn, ctx, params)),
        };
        ctx.release_stmt_reads();
        r
    }

    #[allow(clippy::too_many_lines)]
    fn dispatch(&self, txn: &mut Txn<'_>, ctx: &mut SqlContext, params: &[Value]) -> Result<Outcome> {
        match &self.statement {
            Statement::Select(q) => {
                let rs = query::run_query(txn, ctx, params, q, &eval::Env::default())?;
                Ok(Outcome::Rows(rs))
            }
            Statement::Insert(i) => dml::insert(txn, ctx, params, i).map(Outcome::Count),
            Statement::Update(u) => dml::update(txn, ctx, params, u).map(Outcome::Count),
            Statement::Delete(d) => dml::delete(txn, ctx, params, d).map(Outcome::Count),
            Statement::CreateTable(t) => table::create(txn, ctx, t, params).map(|n| if n == 0 { Outcome::Done } else { Outcome::Count(n) }),
            Statement::CreateIndex(i) => index::create(txn, ctx, i).map(|()| Outcome::Done),
            Statement::CreateSchema(name) => schema::create(txn, ctx, name).map(|()| Outcome::Done),
            Statement::DropSchema { name, restrict, .. } => schema::drop(txn, ctx, name, *restrict).map(|()| Outcome::Done),
            Statement::AlterTable(a) => alter::alter_table(txn, ctx, a).map(|()| Outcome::Done),
            Statement::CreateSequence(s) => seq::create(txn, ctx, s).map(|()| Outcome::Done),
            Statement::AlterSequence { name, options } => seq::alter(txn, ctx, name, options).map(|()| Outcome::Done),
            Statement::DropSequence { name } => seq::drop(txn, ctx, name).map(|()| Outcome::Done),
            Statement::DropTable { name, restrict, .. } | Statement::DropView { name, restrict, .. } => {
                drop::drop(txn, ctx, name, *restrict).map(|()| Outcome::Done)
            }
            Statement::DropIndex(name) => drop::drop(txn, ctx, name, false).map(|()| Outcome::Done),
            st @ (Statement::CreateAlias { .. } | Statement::DropAlias(_)) => alias::run(txn, ctx, st).map(|()| Outcome::Done),
            Statement::Commit { hold } => {
                if ctx.session.commit_def().is_some() {
                    ctx.session.commit(txn, ctx.now)?;
                }
                ctx.unit_ended();
                if !hold {
                    cursor::end_of_unit(ctx);
                }
                Ok(Outcome::Done)
            }
            Statement::Rollback { hold } => {
                if ctx.session.commit_def().is_some() {
                    rollback(txn, ctx)?;
                }
                ctx.unit_ended();
                if !hold {
                    cursor::end_of_unit(ctx);
                }
                Ok(Outcome::Done)
            }
            Statement::DeclareCursor(d) => Ok(cursor::declare(ctx, d)),
            Statement::Open { cursor: c, using } => {
                let mut values = Vec::with_capacity(using.len());
                let mut ev = eval::Evaluator { ctx, params };
                for u in using {
                    values.push(ev.eval(txn, u, &eval::Env::default())?);
                }
                let p: Vec<Value> = if values.is_empty() {
                    params.to_vec()
                } else {
                    values
                };
                cursor::open(txn, ctx, &c.value, &p)
            }
            Statement::Fetch(f) => cursor::fetch(ctx, f, params),
            Statement::Close(c) => cursor::close(ctx, &c.value),
            Statement::CreateView(v) => view::create(txn, ctx, v).map(|()| Outcome::Done),
            Statement::LabelOn(l) | Statement::CommentOn(l) => {
                ddl::label(txn, ctx, l).map(|()| Outcome::Done)
            }
            Statement::Merge(m) => dml::merge(txn, ctx, params, m).map(Outcome::Count),
            Statement::SetSchema(target) => {
                ctx.current_schema = match target {
                    crate::ast::SchemaTarget::Name(n) => Some(ctx.schema_system_name(txn, n)?),
                    crate::ast::SchemaTarget::User => Name::parse(&ctx.user).ok(),
                    crate::ast::SchemaTarget::Default => None,
                    crate::ast::SchemaTarget::Host(_) => {
                        return Err(SqlError::Unsupported {
                            what: "SET SCHEMA from a host variable".into(),
                        });
                    }
                };
                Ok(Outcome::Done)
            }
            Statement::SetPath(entries) => {
                ctx.registers.path = Some(registers::path_text(ctx, entries));
                Ok(Outcome::Done)
            }
            Statement::SetCurrentDegree => Ok(Outcome::Done),
            Statement::SetDecfloatRounding(m) => Ok(ctx.set_decfloat_rounding(*m)),
            Statement::SetTransaction(iso) => set_transaction(ctx, *iso).map(|()| Outcome::Done),
            Statement::Grant(g) => grant::run(txn, ctx, g, false).map(|()| Outcome::Done),
            Statement::Revoke(g) => grant::run(txn, ctx, g, true).map(|()| Outcome::Done),
            Statement::Explain(inner) => {
                let Statement::Select(q) = &**inner else {
                    return Err(SqlError::Unsupported {
                        what: "EXPLAIN of a statement that is not a query".into(),
                    });
                };
                ctx.explain.clear();
                query::explain(txn, ctx, params, q)?;
                let lines = std::mem::take(&mut ctx.explain);
                Ok(Outcome::Rows(ResultSet {
                    columns: vec![ColumnDesc {
                        name: "PLAN".into(),
                        data_type: "VARCHAR(132)".into(),
                        nullable: false,
                    }],
                    rows: lines.into_iter().map(|l| vec![Value::Str(l)]).collect(),
                }))
            }
            Statement::Rename(r) => rename::run(txn, ctx, r).map(|()| Outcome::Done),
            // The count is not reported, only SQL0100 when there was nothing.
            Statement::Truncate(t) => change::truncate(txn, ctx, params, t).map(|n| if n == 0 { Outcome::Count(0) } else { Outcome::Done }),
            Statement::SetRegister { name, value } => {
                let v = eval::Evaluator { ctx, params }.eval(txn, value, &eval::Env::default())?;
                ctx.registers.set(name, &v)?;
                Ok(Outcome::Done)
            }
            st @ (Statement::CreateRoutine(_)
            | Statement::Call(_)
            | Statement::DropRoutine(_)
            | Statement::Signal(_)
            | Statement::SetResultSets(_)) => routine::run(txn, ctx, st, params),
        }
    }
}

impl Prepared {
    /// Run and report through an SQLCA, as a program would see it.
    pub fn run(
        &self,
        txn: &mut Txn<'_>,
        ctx: &mut SqlContext,
        params: &[Value],
    ) -> (crate::sqlca::Sqlca, Option<Outcome>) {
        ctx.mapping_warning = None;
        ctx.map_errors = matches!(self.statement, Statement::Select(_));
        let r = self.execute(txn, ctx, params);
        ctx.map_errors = false;
        match r {
            Ok(o) => {
                let mut ca = crate::sqlca::Sqlca::from_outcome(&o);
                if let Some((code, state)) = ctx.mapping_warning.take()
                    && ca.sqlcode >= 0
                {
                    ca.sqlcode = code;
                    ca.sqlstate = state.into();
                }
                if let Some(w) = ctx.warning.as_ref() {
                    ca = w.apply(ca);
                }
                (ca, Some(o))
            }
            Err(e) => (crate::sqlca::Sqlca::from_error(&e), None),
        }
    }
}

/// The statement's name, for messages.
#[must_use]
pub fn statement_name(s: &Statement) -> &'static str {
    match s {
        Statement::CreateTable(_) => "CREATE TABLE",
        Statement::AlterTable(_) => "ALTER TABLE",
        Statement::CreateSequence(_) => "CREATE SEQUENCE",
        Statement::AlterSequence { .. } => "ALTER SEQUENCE",
        Statement::DropSequence { .. } => "DROP SEQUENCE",
        Statement::DropTable { .. } => "DROP TABLE",
        Statement::CreateIndex(_) => "CREATE INDEX",
        Statement::DropIndex(_) => "DROP INDEX",
        Statement::CreateView(_) => "CREATE VIEW",
        Statement::DropView { .. } => "DROP VIEW",
        Statement::DropSchema { .. } => "DROP SCHEMA",
        Statement::CreateSchema(_) => "CREATE SCHEMA",
        Statement::CreateAlias { .. } => "CREATE ALIAS",
        Statement::DropAlias(_) => "DROP ALIAS",
        Statement::LabelOn(_) => "LABEL ON",
        Statement::CommentOn(_) => "COMMENT ON",
        Statement::Rename(_) => "RENAME",
        Statement::Select(_) => "SELECT",
        Statement::Insert(_) => "INSERT",
        Statement::Update(_) => "UPDATE",
        Statement::Delete(_) => "DELETE",
        Statement::Merge(_) => "MERGE",
        Statement::DeclareCursor(_) => "DECLARE CURSOR",
        Statement::Open { .. } => "OPEN",
        Statement::Fetch(_) => "FETCH",
        Statement::Close(_) => "CLOSE",
        Statement::Commit { .. } => "COMMIT",
        Statement::Rollback { .. } => "ROLLBACK",
        Statement::SetSchema(_) => "SET SCHEMA",
        Statement::SetPath(_) => "SET PATH",
        Statement::SetCurrentDegree => "SET CURRENT DEGREE",
        Statement::SetDecfloatRounding(_) => "SET CURRENT DECFLOAT ROUNDING MODE",
        Statement::SetTransaction(_) => "SET TRANSACTION",
        Statement::Explain(_) => "EXPLAIN",
        Statement::Grant(_) => "GRANT",
        Statement::Revoke(_) => "REVOKE",
        Statement::Truncate(_) => "TRUNCATE",
        Statement::SetRegister { .. } => "SET",
        Statement::CreateRoutine(r) if r.kind == crate::ast::RoutineKind::Procedure => "CREATE PROCEDURE",
        Statement::CreateRoutine(_) => "CREATE FUNCTION",
        Statement::Call(_) => "CALL",
        Statement::DropRoutine(_) => "DROP ROUTINE",
        Statement::Signal(_) => "SIGNAL",
        Statement::SetResultSets(_) => "SET RESULT SETS",
    }
}

/// A store failure is a system error to SQL.
impl From<store::StoreError> for SqlError {
    fn from(e: store::StoreError) -> Self {
        if matches!(e, store::StoreError::NotAuthorized { .. }) {
            return Self::NotAuthorized { what: e.to_string() };
        }
        Self::Database {
            what: e.to_string(),
            msgid: e.msgid().map(str::to_string),
        }
    }
}

/// `DbError` into `SqlError` with the SQLCODE the condition has.
impl From<db::DbError> for SqlError {
    fn from(e: db::DbError) -> Self {
        use db::DbError as D;
        match &e {
            D::Store(s @ store::StoreError::NotAuthorized { .. }) => Self::NotAuthorized { what: s.to_string() },
            D::DuplicateKey { .. } => Self::DuplicateKey {
                what: e.to_string(),
            },
            D::DuplicateKeys { .. } => Self::DuplicateRows {
                what: e.to_string(),
            },
            D::RecordLocked { .. } | D::RecordLockedInJob { .. } | D::FileLocked { .. } => Self::Locked {
                what: e.to_string(),
            },
            D::CommitNeedsJournal { .. } => Self::NotJournaled {
                what: e.to_string(),
            },
            D::DataMapping { .. } => Self::Conversion {
                what: e.to_string(),
            },
            D::CheckViolation { .. }
            | D::RefNoParent { .. }
            | D::RefRestricted { .. }
            | D::CheckPending { .. }
            | D::RuleNeedsJournal { .. }
            | D::IdentityExhausted { .. }
            | D::Constraint { .. } => cst::from_db(&e),
            D::Ended => Self::Cancelled,
            D::NotAFile { .. } | D::MemberNotFound { .. } => Self::ObjectNotFound {
                name: e.to_string(),
                kind: "table",
            },
            _ => Self::Database {
                what: e.to_string(),
                msgid: e.msgid().map(str::to_string),
            },
        }
    }
}
