// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Column statistics, as `QSYS2.SYSCOLUMNSTAT` shows them: per member
//! and column the number of distinct values, of nulls, the lengths, a
//! histogram of equal-depth ranges and the most frequent values. They
//! are collected when an index is made over the columns, when the
//! planner asked for them and the collection job (`QDBFSTCCOL`) next
//! runs, and again when the member has changed by a tenth since.
//!
//! They are kept on the file's `*SQLINF` (`COLSTATS` entries,
//! `statsave`), so they outlive an IPL, and in the storage service's
//! memory, one table for every job (the service runs SQL on one
//! thread), which reads a file's entries the first time it is weighed
//! ([`prime`]).

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use db::file::{self, FileKind};
use db::member::Member;
use store::obj::ObjectId;
use store::txn::Txn;

use crate::error::Result;
use crate::value::Value;

/// Rows a histogram and the most frequent values are worked out from.
const SAMPLE: usize = 10_000;
/// Distinct values counted exactly; past it the count is extrapolated.
const EXACT: usize = 200_000;
/// Ranges of a histogram.
const RANGES: usize = 100;
/// Most frequent values kept.
const FREQUENT: usize = 100;
/// The share of a member's rows that may change before its statistics
/// are collected again.
const STALE: f64 = 0.10;
/// Tables with fewer rows than this are never asked for: any plan
/// reads them alike.
pub const ASK_FLOOR: u64 = 1_000;
/// Requests the collection job holds at most.
const REQUESTS: usize = 1024;

/// The statistics of one column of one member.
#[derive(Clone, Debug)]
pub struct ColumnStats {
    /// The column's SQL name.
    pub column: String,
    /// Its system name.
    pub system_column: String,
    /// Distinct non-null values.
    pub distinct: u64,
    /// Null values.
    pub nulls: u64,
    /// Average length in bytes of the values' text.
    pub avg_len: u32,
    /// Longest.
    pub max_len: u32,
    /// The upper bound of each equal-depth range, ascending: range `i`
    /// holds the values up to `bounds[i]`.
    pub bounds: Vec<Value>,
    /// The most frequent values and the share of rows each has.
    pub frequent: Vec<(Value, f64)>,
    /// Rows, deleted rows and slots of the member when collected.
    pub rows: u64,
    /// See `rows`.
    pub deleted: u64,
    /// See `rows`.
    pub slots: u64,
    /// When first collected and last updated (Unix seconds).
    pub created: u64,
    /// See `created`.
    pub updated: u64,
}

impl ColumnStats {
    /// `QDBST_` and 32 hex digits, as IBM names statistics.
    #[must_use]
    pub fn name(&self, member: ObjectId) -> String {
        format!("QDBST_{}", hex(&self.internal_id(member)))
    }

    /// The 16-byte internal identifier.
    #[must_use]
    pub fn internal_id(&self, member: ObjectId) -> [u8; 16] {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(member.to_string().as_bytes());
        h.update(self.system_column.as_bytes());
        let d = h.finalize();
        let mut out = [0u8; 16];
        out.copy_from_slice(&d[..16]);
        out
    }

    /// The share of rows with `v` (not null).
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn equal(&self, v: &Value) -> f64 {
        if self.rows == 0 {
            return 0.0;
        }
        if let Some((_, f)) = self.frequent.iter().find(|(x, _)| same(x, v)) {
            return *f;
        }
        let kept: f64 = self.frequent.iter().map(|(_, f)| f).sum();
        let others = self.distinct.saturating_sub(self.frequent.len() as u64).max(1);
        let nonnull = 1.0 - self.nulls as f64 / self.rows as f64;
        ((nonnull - kept).max(0.0) / others as f64).min(1.0)
    }

    /// The share of rows with a value below `v` (`or_equal`: up to it).
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn below(&self, v: &Value, or_equal: bool) -> f64 {
        if self.bounds.is_empty() || self.rows == 0 {
            return 0.0;
        }
        let n = self.bounds.iter().take_while(|b| b.compare(v).ok().flatten() == Some(Ordering::Less)).count();
        let nonnull = 1.0 - self.nulls as f64 / self.rows as f64;
        let mut f = n as f64 / self.bounds.len() as f64 * nonnull;
        if or_equal {
            f += self.equal(v);
        }
        f.clamp(0.0, 1.0)
    }

    /// Whether the member has changed enough since they were collected.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn stale(&self, rows: u64, deleted: u64, slots: u64) -> bool {
        let changed = rows.abs_diff(self.rows) + deleted.abs_diff(self.deleted) + slots.abs_diff(self.slots);
        changed as f64 > self.rows.max(1) as f64 * STALE
    }
}

fn same(a: &Value, b: &Value) -> bool {
    a.compare(b).ok().flatten() == Some(Ordering::Equal)
}

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02X}");
        s
    })
}

/// A member's statistics, by member and the column's system name.
type Key = (ObjectId, String);

/// The table and the collection job's requests.
#[derive(Default)]
struct Registry {
    /// Statistics by member and column, with the file they belong to.
    stats: HashMap<Key, (ObjectId, ColumnStats)>,
    /// Columns the planner asked for: file, member, system column name.
    requests: Vec<(ObjectId, ObjectId, String)>,
    /// Files whose kept statistics have been read.
    loaded: HashSet<ObjectId>,
}

thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());
}

/// The statistics of `column` of `member`, when collected.
#[must_use]
pub fn get(member: ObjectId, column: &str) -> Option<ColumnStats> {
    REGISTRY.with(|r| r.borrow().stats.get(&(member, column.to_string())).map(|(_, s)| s.clone()))
}

/// Read the statistics kept on `file` the first time it is asked for:
/// after an IPL they come back from its `*SQLINF`. One that cannot be
/// read is left to be collected again.
pub fn prime(txn: &mut Txn<'_>, file: ObjectId) {
    if REGISTRY.with(|r| !r.borrow_mut().loaded.insert(file)) {
        return;
    }
    let Ok(kept) = db::sqlinf::colstats::load(txn, file) else { return };
    REGISTRY.with(|r| {
        let mut r = r.borrow_mut();
        for c in &kept {
            if let Some(s) = super::statsave::from_stored(c) {
                r.stats.entry((c.member, s.system_column.clone())).or_insert((file, s));
            }
        }
    });
}

/// Ask the collection job for `column` of `member` of `file`.
pub fn request(file: ObjectId, member: ObjectId, column: &str) {
    REGISTRY.with(|r| {
        let mut r = r.borrow_mut();
        let key = (file, member, column.to_string());
        if r.requests.len() < REQUESTS && !r.requests.contains(&key) && !r.stats.contains_key(&(member, column.to_string())) {
            r.requests.push(key);
        }
    });
}

/// Statistics of a file gone, or all of them.
pub fn forget(file: Option<ObjectId>) {
    REGISTRY.with(|r| {
        let mut r = r.borrow_mut();
        match file {
            Some(f) => {
                r.stats.retain(|_, (x, _)| *x != f);
                r.requests.retain(|(x, _, _)| *x != f);
                r.loaded.remove(&f);
            }
            None => *r = Registry::default(),
        }
    });
}

/// Collect `columns` (system names) of the first member of `file`.
/// Logical files and files without members have none.
pub fn collect(txn: &mut Txn<'_>, file_id: ObjectId, columns: &[String], now: u64) -> Result<usize> {
    let desc = file::describe(txn, file_id)?;
    if desc.attr.kind == FileKind::Logical {
        return Ok(0);
    }
    let (Some(entry), Some(fmt)) = (desc.members.first().copied(), desc.formats.first().cloned()) else {
        return Ok(0);
    };
    let fields: Vec<(usize, dds::FieldDef)> = fmt
        .format
        .fields
        .iter()
        .enumerate()
        .filter(|(_, f)| columns.iter().any(|c| *c == f.name.to_string()) && !f.data_type.is_lob())
        .map(|(i, f)| (i, f.clone()))
        .collect();
    if fields.is_empty() {
        return Ok(0);
    }
    prime(txn, file_id);
    let m = Member::open(txn, &desc, &entry)?;
    let attr = m.attr().clone();
    let stride = usize::try_from(attr.record_count / SAMPLE as u64).unwrap_or(usize::MAX).max(1);
    let mut acc: Vec<Acc> = fields.iter().map(|_| Acc::default()).collect();
    let mut from = 1;
    let mut seen = 0usize;
    loop {
        let slots = m.scan(txn, from, 512)?;
        let Some(end) = slots.last().map(|s| s.rrn) else { break };
        for s in &slots {
            let sampled = seen.is_multiple_of(stride);
            seen += 1;
            for ((i, f), a) in fields.iter().zip(&mut acc) {
                let v = db::record::read_field(f, &s.record, &s.null_map, fmt.null_index(*i))?;
                a.add(Value::from_field(f, &v)?, sampled);
            }
        }
        from = end + 1;
    }
    let n = fields.len();
    let mut kept = Vec::with_capacity(n);
    REGISTRY.with(|r| {
        let mut r = r.borrow_mut();
        for ((_, f), a) in fields.iter().zip(acc) {
            let key = (entry.object, f.name.to_string());
            let created = r.stats.get(&key).map_or(now, |(_, s)| s.created);
            let mut s = a.finish(attr.record_count);
            s.column = f.alias.clone().unwrap_or_else(|| f.name.to_string());
            s.system_column = f.name.to_string();
            s.deleted = attr.deleted_count;
            s.slots = attr.slots;
            s.created = created;
            s.updated = now;
            kept.extend(super::statsave::to_stored(entry.object, &s));
            r.stats.insert(key, (file_id, s));
            r.requests.retain(|(_, m, c)| !(*m == entry.object && *c == f.name.to_string()));
        }
    });
    let members: Vec<ObjectId> = desc.members.iter().map(|e| e.object).collect();
    db::sqlinf::colstats::store(txn, file_id, &members, &kept, now)?;
    Ok(n)
}

/// Rows past which `CREATE INDEX` leaves its columns to the job.
const AT_CREATE: u64 = 100_000;

/// `CREATE INDEX` over `columns` of `t`: their statistics collected
/// now when the table is not large, else asked of the collection job.
pub fn on_index(txn: &mut Txn<'_>, t: &crate::exec::TableInfo, columns: &[String], now: u64) -> Result<()> {
    let rows = Member::open(txn, &t.desc, &t.member).map_or(0, |m| m.attr().record_count);
    if rows <= AT_CREATE {
        collect(txn, t.id, columns, now)?;
    } else {
        for c in columns {
            request(t.id, t.member.object, c);
        }
    }
    Ok(())
}

/// The collection job's pass: every requested column, then every
/// column whose member changed enough. Lines for the job's log.
pub fn collect_due(txn: &mut Txn<'_>, now: u64) -> Vec<String> {
    let (requests, kept) = REGISTRY.with(|r| {
        let r = r.borrow();
        let kept: Vec<(ObjectId, ObjectId, String, u64, u64, u64)> =
            r.stats.iter().map(|((m, c), (f, s))| (*f, *m, c.clone(), s.rows, s.deleted, s.slots)).collect();
        (r.requests.clone(), kept)
    });
    let mut due: Vec<(ObjectId, Vec<String>)> = Vec::new();
    let mut add = |f: ObjectId, c: String| match due.iter_mut().find(|(x, _)| *x == f) {
        Some((_, cs)) if !cs.contains(&c) => cs.push(c),
        Some(_) => {}
        None => due.push((f, vec![c])),
    };
    for (f, _, c) in requests {
        add(f, c);
    }
    for (f, m, c, rows, deleted, slots) in kept {
        let Ok(desc) = file::describe(txn, f) else {
            forget(Some(f));
            continue;
        };
        let Some(entry) = desc.members.iter().find(|e| e.object == m) else { continue };
        let Ok(mem) = Member::open(txn, &desc, entry) else { continue };
        let a = mem.attr();
        let probe = ColumnStats {
            column: String::new(),
            system_column: String::new(),
            distinct: 0,
            nulls: 0,
            avg_len: 0,
            max_len: 0,
            bounds: Vec::new(),
            frequent: Vec::new(),
            rows,
            deleted,
            slots,
            created: 0,
            updated: 0,
        };
        if probe.stale(a.record_count, a.deleted_count, a.slots) {
            add(f, c);
        }
    }
    let mut lines = Vec::new();
    for (f, cols) in due {
        match collect(txn, f, &cols, now) {
            Ok(n) if n > 0 => lines.push(format!("statistics of {n} columns of file {f} collected")),
            Ok(_) => forget(Some(f)),
            Err(e) => {
                lines.push(format!("statistics of file {f} not collected: {e}"));
                forget(Some(f));
            }
        }
    }
    lines
}

/// One column's values as they are read.
#[derive(Default)]
struct Acc {
    nulls: u64,
    count: u64,
    len_sum: u64,
    max_len: u32,
    exact: HashMap<String, u64>,
    overflow: bool,
    overflow_at: u64,
    sample: Vec<Value>,
}

impl Acc {
    fn add(&mut self, v: Value, sampled: bool) {
        self.count += 1;
        if v.is_null() {
            self.nulls += 1;
            return;
        }
        let len = u32::try_from(v.to_string().len()).unwrap_or(u32::MAX);
        self.len_sum += u64::from(len);
        self.max_len = self.max_len.max(len);
        if !self.overflow {
            *self.exact.entry(v.key()).or_default() += 1;
            if self.exact.len() > EXACT {
                self.overflow = true;
                self.overflow_at = self.count;
            }
        }
        if sampled && self.sample.len() < SAMPLE {
            self.sample.push(v);
        }
    }

    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn finish(mut self, rows: u64) -> ColumnStats {
        let nonnull = self.count - self.nulls;
        let distinct = if self.overflow {
            // Extrapolated from where the exact count stopped.
            ((self.exact.len() as f64) * (self.count as f64 / self.overflow_at.max(1) as f64)) as u64
        } else {
            self.exact.len() as u64
        };
        self.sample.sort_by(|a, b| a.compare(b).ok().flatten().unwrap_or(Ordering::Equal));
        let mut bounds = Vec::new();
        if !self.sample.is_empty() {
            let ranges = RANGES.min(self.sample.len());
            for i in 1..=ranges {
                let at = (i * self.sample.len()).div_ceil(ranges) - 1;
                bounds.push(self.sample[at].clone());
            }
        }
        // The most frequent values: exact counts when kept, else the sample's.
        let total = rows.max(1) as f64;
        let mut frequent: Vec<(Value, f64)> = Vec::new();
        let mut seen = HashSet::new();
        let mut counts: Vec<(&Value, u64)> = Vec::new();
        for v in &self.sample {
            if seen.insert(v.key()) {
                let c = if self.overflow { 0 } else { self.exact.get(&v.key()).copied().unwrap_or(0) };
                counts.push((v, c));
            }
        }
        if self.overflow {
            let mut by: HashMap<String, u64> = HashMap::new();
            for v in &self.sample {
                *by.entry(v.key()).or_default() += 1;
            }
            let scale = nonnull as f64 / self.sample.len().max(1) as f64;
            for c in &mut counts {
                c.1 = (by[&c.0.key()] as f64 * scale) as u64;
            }
        }
        counts.sort_by_key(|c| std::cmp::Reverse(c.1));
        for (v, c) in counts.into_iter().take(FREQUENT) {
            if c > 1 {
                frequent.push((v.clone(), c as f64 / total));
            }
        }
        ColumnStats {
            column: String::new(),
            system_column: String::new(),
            distinct,
            nulls: self.nulls,
            avg_len: self.len_sum.checked_div(nonnull).map_or(0, |x| u32::try_from(x).unwrap_or(u32::MAX)),
            max_len: self.max_len,
            bounds,
            frequent,
            rows,
            deleted: 0,
            slots: 0,
            created: 0,
            updated: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_from_the_values() {
        let mut a = Acc::default();
        for i in 0..1000 {
            a.add(Value::int(i % 100), true);
        }
        for _ in 0..1000 {
            a.add(Value::Null, true);
        }
        let s = a.finish(2000);
        assert_eq!(s.distinct, 100);
        assert_eq!(s.nulls, 1000);
        assert!((s.equal(&Value::int(5)) - 0.005).abs() < 1e-9, "{}", s.equal(&Value::int(5)));
        let below = s.below(&Value::int(50), false);
        assert!((below - 0.25).abs() < 0.02, "{below}");
        assert!(!s.stale(2000, 0, 0));
        assert!(s.stale(2500, 0, 0));
    }
}
