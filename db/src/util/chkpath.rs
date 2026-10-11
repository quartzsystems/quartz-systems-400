// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Access path verification for `chkstg`: every keyed member's path
//! must hold one entry per record it should index.

use std::fmt::Write as _;

use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::{Order, each_record};
use crate::error::Result;
use crate::file::{self, FileKind};
use crate::member::Member;

/// What the sweep found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathReport {
    /// Keyed members checked.
    pub checked: u32,
    /// Members whose path and records disagree, with what was seen.
    pub problems: Vec<String>,
}

impl PathReport {
    /// The report as text.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = format!(
            "checked {} access path(s): {} problem(s)\n",
            self.checked,
            self.problems.len()
        );
        for p in &self.problems {
            let _ = writeln!(out, "  {p}");
        }
        out
    }
}

/// Walk every file of every library.
pub fn verify_all(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<PathReport> {
    let mut libraries: Vec<ObjectId> = ops::list(txn, qsys, Some(types::LIB))?
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    if !libraries.contains(&qsys) {
        libraries.insert(0, qsys);
    }
    let mut report = PathReport::default();
    for lib in libraries {
        for (_, id) in ops::list(txn, lib, Some(types::FILE))? {
            verify_file(txn, id, &mut report)?;
        }
    }
    Ok(report)
}

/// Check one file's members.
pub fn verify_file(txn: &mut Txn<'_>, file: ObjectId, report: &mut PathReport) -> Result<()> {
    let Ok(desc) = file::describe(txn, file) else {
        return Ok(());
    };
    for e in &desc.members {
        let mut m = match Member::open(txn, &desc, e) {
            Ok(m) => m,
            Err(err) => {
                report.problems.push(format!(
                    "{}/{} member {}: {err}",
                    desc.library_name,
                    desc.name(),
                    e.name
                ));
                continue;
            }
        };
        if m.attr().key_len == 0 {
            continue;
        }
        report.checked += 1;
        let label = format!("{}/{} member {}", desc.library_name, desc.name(), e.name);
        if m.attr().path_invalid {
            if desc.attr.maint == file::Maint::Immed {
                report
                    .problems
                    .push(format!("{label}: path marked invalid under MAINT(*IMMED)"));
            }
            continue;
        }
        let Some(p) = m.path() else {
            report
                .problems
                .push(format!("{label}: keyed member without a path"));
            continue;
        };
        let entries = p.count(txn)?;
        let expected = if desc.attr.kind == FileKind::Logical {
            // Every record the format derives and select/omit keeps.
            let mut n = 0usize;
            each_record(txn, &mut m, &Order::default(), &mut |_, _| {
                n += 1;
                Ok(true)
            })?;
            n
        } else {
            usize::try_from(m.attr().record_count).expect("fits")
        };
        if entries != expected {
            report.problems.push(format!(
                "{label}: {entries} path entries for {expected} records"
            ));
        }
        if u64::try_from(entries).expect("fits") != m.attr().path_entries {
            report.problems.push(format!(
                "{label}: path holds {entries} entries, attributes say {}",
                m.attr().path_entries
            ));
        }
    }
    Ok(())
}
