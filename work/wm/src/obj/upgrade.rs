// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped work management objects across a release change. They
//! are the system's own: [`super::shipped::install`] makes the missing
//! ones at every IPL, and a slip install or upgrade, which restores the
//! `QS00SS1` load over them, keeps the system's copies ([`keep`] before
//! the restore, [`put_back`] after it): subsystem entries, job queue and
//! output queue contents, messages, schedule and reply list entries.
//!
//! A release that adds entries to a shipped subsystem lists them in
//! [`ADDITIONS`] with a marker object it ships at the same time. A
//! system without the marker never had the entries, so they are added
//! to its description once, after a slip install or, when the code came
//! by PTF, at the next IPL. Entries a customer removes later stay
//! removed; the customer's own entries are never touched.

use qsname::Name;
use store::obj::library::Library;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::rpyl;
use super::sbsd::{Sbsd, shipped as shipped_sbsds};
use super::{read, shipped, write};
use crate::Result;

/// Entries a release added to a shipped subsystem description in `QSYS`.
#[derive(Debug)]
pub struct Addition {
    /// What they are for, in the log.
    pub what: &'static str,
    /// The subsystem description.
    pub sbsd: &'static str,
    /// The object shipped with them: `(library, name, type)`.
    pub marker: (&'static str, &'static str, u16),
    /// Routing entries by sequence number.
    pub rtges: &'static [u16],
    /// Job queue entries by queue (name, library).
    pub jobqes: &'static [(&'static str, &'static str)],
    /// Prestart job entries by program (name, library).
    pub pjes: &'static [(&'static str, &'static str)],
}

/// Every release's additions.
pub const ADDITIONS: &[Addition] = &[Addition {
    // V1R2M0 (docs/NETSERVER.md section 3).
    what: "NetServer",
    sbsd: "QSERVER",
    marker: ("QSYS", "QZLSSERVER", types::JOBD),
    rtges: &[800],
    jobqes: &[("QPWFSERVER", "QSYS")],
    pjes: &[("QZLSFILE", "QSYS"), ("QZLSFILET", "QSYS")],
}];

fn find(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    lib: &str,
    name: &str,
    ty: u16,
) -> Result<Option<ObjectId>> {
    let l = if lib.eq_ignore_ascii_case("QSYS") {
        qsys
    } else {
        let Ok(n) = Name::parse(lib) else {
            return Ok(None);
        };
        match ops::find_library(txn, qsys, n) {
            Ok(l) => l.id,
            Err(_) => return Ok(None),
        }
    };
    let Ok(n) = Name::parse(name) else {
        return Ok(None);
    };
    Ok(Library::load(txn, l)?.lookup(txn, ty, n)?)
}

/// Add `a`'s entries that `have` lacks, from the shipped description.
/// One line saying what was added or left out; `None` when nothing was.
#[must_use]
pub fn merge(have: &mut Sbsd, a: &Addition) -> Option<String> {
    let shipped = shipped_sbsds()
        .into_iter()
        .find(|(n, _)| *n == a.sbsd)
        .map(|(_, s)| s)?;
    let mut added = Vec::new();
    let mut left = Vec::new();
    for seq in a.rtges {
        let Some(r) = shipped.rtges.iter().find(|r| r.seq == *seq) else {
            continue;
        };
        if have.rtges.iter().any(|x| x == r) {
            continue;
        }
        if have.rtges.iter().any(|x| x.seq == *seq) {
            left.push(format!(
                "routing entry {seq} (the sequence number is in use)"
            ));
        } else if have.pool(r.pool).is_none() {
            left.push(format!("routing entry {seq} (no pool {})", r.pool));
        } else {
            have.rtges.push(r.clone());
            have.rtges.sort_by_key(|r| r.seq);
            added.push(format!("routing entry {seq}"));
        }
    }
    for (name, lib) in a.jobqes {
        let q = (name.to_string(), lib.to_string());
        let Some(e) = shipped.jobqes.iter().find(|e| e.jobq == q) else {
            continue;
        };
        if have.jobqes.iter().any(|x| x.jobq == q) {
            continue;
        }
        if have.jobqes.iter().any(|x| x.seq == e.seq) {
            left.push(format!(
                "job queue entry {lib}/{name} (sequence number {} is in use)",
                e.seq
            ));
        } else {
            have.jobqes.push(e.clone());
            have.jobqes.sort_by_key(|e| e.seq);
            added.push(format!("job queue entry {lib}/{name}"));
        }
    }
    for (name, lib) in a.pjes {
        let p = (name.to_string(), lib.to_string());
        let Some(e) = shipped.pjes.iter().find(|e| e.pgm == p) else {
            continue;
        };
        if have.pjes.iter().any(|x| x.pgm == p) {
            continue;
        }
        if have.pool(e.pool).is_none() {
            left.push(format!("prestart job entry {name} (no pool {})", e.pool));
        } else {
            have.pjes.push(e.clone());
            added.push(format!("prestart job entry {name}"));
        }
    }
    if added.is_empty() && left.is_empty() {
        return None;
    }
    let mut line = format!("QSYS/{}: {}", a.sbsd, a.what);
    if !added.is_empty() {
        line.push_str(": added ");
        line.push_str(&added.join(", "));
    }
    if !left.is_empty() {
        line.push_str("; not added: ");
        line.push_str(&left.join(", "));
    }
    Some(line)
}

/// The additions whose marker is missing: entries this system never had.
fn missing(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Vec<&'static Addition>> {
    let mut v = Vec::new();
    for a in ADDITIONS {
        let (l, n, t) = a.marker;
        if find(txn, qsys, l, n, t)?.is_none() {
            v.push(a);
        }
    }
    Ok(v)
}

/// At an IPL, before the shipped objects are made: the additions a
/// subsystem the system has never had, added to it. A line for each.
pub fn add_missing(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    for a in missing(txn, qsys)? {
        let Some(id) = find(txn, qsys, "QSYS", a.sbsd, types::SBSD)? else {
            continue;
        };
        let mut s = Sbsd::decode(&read(txn, id)?)?;
        if let Some(line) = merge(&mut s, a) {
            write(txn, id, &s.encode())?;
            lines.push(line);
        }
    }
    Ok(lines)
}

/// The system's shipped work management objects, as they were before
/// the operating system's load was restored over them.
#[derive(Debug, Default)]
pub struct Kept {
    /// `(library, name, type, payload)`.
    objects: Vec<(&'static str, &'static str, u16, Vec<u8>)>,
    /// The reply list.
    rpyl: Option<rpyl::RpyList>,
    /// The additions the system never had.
    missing: Vec<&'static Addition>,
}

impl Kept {
    /// How many objects are kept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.objects.len() + usize::from(self.rpyl.is_some())
    }

    /// Whether nothing is kept (a fresh install).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Before the operating system's load is restored over an installed
/// system: the shipped work management objects it has.
pub fn keep(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Kept> {
    let mut k = Kept {
        missing: missing(txn, qsys)?,
        ..Kept::default()
    };
    for (lib, name, ty) in shipped::objects() {
        if let Some(id) = find(txn, qsys, lib, name, ty)? {
            k.objects.push((lib, name, ty, read(txn, id)?));
        }
    }
    if rpyl::find(txn, qsys)?.is_some() {
        k.rpyl = Some(rpyl::load(txn, qsys)?);
    }
    Ok(k)
}

/// After the restore: each kept object's payload back where the load's
/// differs, with the additions the system never had merged into its
/// subsystems. A line for each object put back.
pub fn put_back(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    kept: &Kept,
    stamp: &super::Stamp,
) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    for (lib, name, ty, old) in &kept.objects {
        let Some(id) = find(txn, qsys, lib, name, *ty)? else {
            continue;
        };
        let mut want = old.clone();
        let mut note = None;
        if *ty == types::SBSD {
            let mut s = Sbsd::decode(old)?;
            let notes: Vec<String> = kept
                .missing
                .iter()
                .filter(|a| a.sbsd == *name)
                .filter_map(|a| merge(&mut s, a))
                .collect();
            if !notes.is_empty() {
                want = s.encode();
                note = Some(notes.join("; "));
            }
        }
        if read(txn, id)? != want {
            write(txn, id, &want)?;
            let tname = types::type_by_code(*ty).map_or("*UNKNOWN", |t| t.name);
            lines.push(note.unwrap_or_else(|| format!("{lib}/{name} {tname}: the system's kept")));
        }
    }
    if let Some(list) = &kept.rpyl
        && rpyl::find(txn, qsys)?.is_some()
        && rpyl::load(txn, qsys)? != *list
    {
        rpyl::save(txn, qsys, list, stamp)?;
        lines.push("the system reply list: the system's kept".into());
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obj::sbsd::Pje;

    fn qserver() -> Sbsd {
        shipped_sbsds()
            .into_iter()
            .find(|(n, _)| *n == "QSERVER")
            .unwrap()
            .1
    }

    /// QSERVER as V1R1M0 shipped it: the two file server prestart entries.
    fn v1r1m0() -> Sbsd {
        let mut s = qserver();
        s.rtges.clear();
        s.jobqes.clear();
        s.pjes.retain(|p| p.pgm.0.starts_with("QPWFSERV"));
        s
    }

    #[test]
    fn merge_adds_netserver_to_a_v1r1m0_qserver() {
        let mut s = v1r1m0();
        let line = merge(&mut s, &ADDITIONS[0]).unwrap();
        assert!(line.contains("added routing entry 800"), "{line}");
        assert!(line.contains("QZLSFILET"), "{line}");
        assert_eq!(s, qserver());
        // Again: nothing to do.
        assert_eq!(merge(&mut s, &ADDITIONS[0]), None);
    }

    #[test]
    fn merge_keeps_the_customers_entries() {
        let mut s = v1r1m0();
        let mine = Pje {
            pgm: ("MYSRV".into(), "MYLIB".into()),
            job: "MYSRV".into(),
            ..s.pjes[0].clone()
        };
        s.pjes.push(mine.clone());
        // The customer's routing entry 800 stays; NetServer's is not added.
        let mut r = qserver().rtges[0].clone();
        r.cmpval = "MINE".into();
        s.rtges.push(r.clone());
        let line = merge(&mut s, &ADDITIONS[0]).unwrap();
        assert!(line.contains("not added: routing entry 800"), "{line}");
        assert!(s.pjes.contains(&mine));
        assert_eq!(s.rtges, vec![r]);
        assert_eq!(s.jobqes, qserver().jobqes);
    }
}
