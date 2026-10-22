// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Join logical formats at read time: the secondary records a primary
//! record joins to, one logical record per combination. A secondary is
//! searched through its own access path when the path leads with the
//! `JFLD` to-fields, else by an arrival scan; `JDUPSEQ` orders the
//! matches; `JDFTVAL` keeps an unmatched primary record with default
//! secondary fields.

use std::ops::Bound;

use dds::KeyTransform;
use qsname::{MemberSpec, Name};
use store::txn::Txn;

use super::Core;
use super::slots::Slot;
use crate::error::{DbError, Result};
use crate::file;
use crate::format::FormatDef;
use crate::lf::{self, BaseRecord, JoinDef, LogicalDef};
use crate::path;
use crate::record::{self, Value};

/// A secondary record found: its format, record and null map.
pub type JoinedRecord = (FormatDef, Vec<u8>, Vec<u8>);

/// Path entries read per batch from a secondary's path.
const BATCH: usize = 64;

/// The combinations of secondary records a primary record joins to, by
/// file index (index 0 stays `None`), one per logical record. Empty when
/// a join finds no match and `JDFTVAL` is off.
pub fn expand(
    txn: &mut Txn<'_>,
    log: &LogicalDef,
    primary: &FormatDef,
    slot: &Slot,
    jdftval: bool,
) -> Result<Vec<Vec<Option<JoinedRecord>>>> {
    let mut combos: Vec<Vec<Option<JoinedRecord>>> = vec![vec![None; log.based_on.len()]];
    for j in &log.joins {
        let to = usize::from(j.to);
        let from = usize::from(j.from);
        let (file, fmt_name) = log.based_on.get(to).ok_or_else(|| DbError::Corrupt {
            what: format!("join to file {to} not based on"),
        })?;
        let secondary = Secondary::open(txn, *file, *fmt_name)?;
        let mut next = Vec::new();
        for combo in combos {
            let from_rec: Option<BaseRecord<'_>> = if from == 0 {
                Some((primary, &slot.record, &slot.null_map))
            } else {
                combo
                    .get(from)
                    .and_then(Option::as_ref)
                    .map(|(f, r, n)| (f, r.as_slice(), n.as_slice()))
            };
            // A defaulted from-record joins to nothing.
            let matches = match from_rec {
                Some(fr) => secondary.matches(txn, j, fr)?,
                None => Vec::new(),
            };
            if matches.is_empty() {
                if jdftval {
                    next.push(combo);
                }
                continue;
            }
            for m in matches {
                let mut c = combo.clone();
                c[to] = Some((secondary.fmt.clone(), m.record, m.null_map));
                next.push(c);
            }
        }
        combos = next;
        if combos.is_empty() {
            break;
        }
    }
    Ok(combos)
}

/// A secondary file's first member and the format the join names.
struct Secondary {
    core: Core,
    fmt: FormatDef,
}

impl Secondary {
    fn open(txn: &mut Txn<'_>, file: store::obj::ObjectId, fmt_name: Name) -> Result<Self> {
        let desc = file::describe(txn, file)?;
        let entry = desc
            .member(MemberSpec::First)
            .ok_or_else(|| DbError::MemberNotFound {
                file: desc.name().to_string(),
                member: "*FIRST".into(),
            })?;
        let fmt = desc
            .formats
            .iter()
            .find(|f| f.format.name == fmt_name)
            .cloned()
            .ok_or_else(|| DbError::Corrupt {
                what: format!("format {fmt_name} missing from {}", desc.name()),
            })?;
        Ok(Self {
            core: Core::load(txn, entry.object)?,
            fmt,
        })
    }

    /// Whether the member's own path leads with the to-fields of `j`,
    /// each keyed plainly and shaped like its from-field, so the
    /// from-values make a key prefix.
    fn keyed_for(&self, j: &JoinDef, from: &FormatDef) -> bool {
        if self.core.attr.path_invalid || self.core.path().is_none() {
            return false;
        }
        let keys = &self.fmt.format.keys;
        if keys.len() < j.fields.len() {
            return false;
        }
        j.fields.iter().zip(keys).all(|((from_name, to_name), k)| {
            let tf = &self.fmt.format.fields[k.field];
            let Some((_, ff)) = from.format.field(from_name) else {
                return false;
            };
            tf.name == *to_name
                && !k.descend
                && matches!(k.transform, KeyTransform::Signed | KeyTransform::Unsigned)
                && ff.data_type == tf.data_type
                && ff.length == tf.length
                && ff.decimals == tf.decimals
        })
    }

    /// The records joining to `from`, in `JDUPSEQ` order when given,
    /// else arrival order.
    fn matches(&self, txn: &mut Txn<'_>, j: &JoinDef, from: BaseRecord<'_>) -> Result<Vec<Slot>> {
        let mut out = if self.keyed_for(j, from.0) {
            self.keyed_matches(txn, j, from)?
        } else {
            self.scanned_matches(txn, j, from)?
        };
        if let Some((field, descend)) = &j.dupseq
            && let Some((i, f)) = self.fmt.format.field(field)
        {
            let ni = self.fmt.null_index(i);
            let mut keyed: Vec<(Value, Slot)> = out
                .into_iter()
                .map(|s| record::read_field(f, &s.record, &s.null_map, ni).map(|v| (v, s)))
                .collect::<Result<_>>()?;
            keyed.sort_by(|a, b| record::compare(&a.0, &b.0));
            if *descend {
                keyed.reverse();
            }
            out = keyed.into_iter().map(|(_, s)| s).collect();
        }
        Ok(out)
    }

    fn scanned_matches(
        &self,
        txn: &mut Txn<'_>,
        j: &JoinDef,
        from: BaseRecord<'_>,
    ) -> Result<Vec<Slot>> {
        let mut out = Vec::new();
        for rrn in 1..=self.core.attr.slots {
            let Some(s) = self.core.read_slot(txn, rrn)? else {
                continue;
            };
            if s.deleted {
                continue;
            }
            if lf::join_matches(j, from, (&self.fmt, &s.record, &s.null_map))? {
                out.push(s);
            }
        }
        Ok(out)
    }

    fn keyed_matches(
        &self,
        txn: &mut Txn<'_>,
        j: &JoinDef,
        from: BaseRecord<'_>,
    ) -> Result<Vec<Slot>> {
        let mut values = Vec::with_capacity(j.fields.len());
        for (from_name, _) in &j.fields {
            let (i, f) = from
                .0
                .format
                .field(from_name)
                .ok_or_else(|| DbError::Corrupt {
                    what: format!("join field {from_name} missing from {}", from.0.format.name),
                })?;
            let v = record::read_field(f, from.1, from.2, from.0.null_index(i))?;
            if v == Value::Null {
                return Ok(Vec::new());
            }
            values.push(v);
        }
        let prefix = path::values_key(&self.fmt, &values)?;
        let p = self.core.path().expect("checked by keyed_for");
        let mut out = Vec::new();
        let mut batch = p.scan(txn, Bound::Included(&prefix), Bound::Unbounded, BATCH)?;
        while let Some(last) = batch.last().cloned() {
            for k in &batch {
                if !k.key.starts_with(&prefix) {
                    return Ok(out);
                }
                if let Some(s) = self.core.read_slot(txn, k.rrn)?
                    && !s.deleted
                    && lf::join_matches(j, from, (&self.fmt, &s.record, &s.null_map))?
                {
                    out.push(s);
                }
            }
            batch = p.scan_from(txn, Bound::Excluded(&last), BATCH)?;
        }
        Ok(out)
    }
}
