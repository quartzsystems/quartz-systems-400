// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The work management objects every system has, with IBM's names and
//! shapes. The storage service creates the missing ones at each start,
//! so a store from before they existed gets them too.

use qsname::Name;
use store::obj::library::Library;
use store::obj::{ObjectId, types};
use store::txn::Txn;

use super::cls::Cls;
use super::jobd::JobD;
use super::msgq::MsgQ;
use super::{Stamp, create, library};
use crate::Result;

/// `(library, name, description)`.
fn jobds() -> Vec<(&'static str, &'static str, JobD)> {
    let d = JobD::default;
    vec![
        (
            "QGPL",
            "QDFTJOBD",
            JobD {
                text: "Default job description".into(),
                ..d()
            },
        ),
        (
            "QGPL",
            "QBATCH",
            JobD {
                user: "QPGMR".into(),
                rtgdta: "QCMDB".into(),
                text: "Batch job description".into(),
                ..d()
            },
        ),
        (
            "QGPL",
            "QINTER",
            JobD {
                jobq: ("QGPL".into(), "QINTER".into()),
                text: "Interactive job description".into(),
                ..d()
            },
        ),
        (
            "QSYS",
            "QCTL",
            JobD {
                jobq: ("QSYS".into(), "QCTL".into()),
                text: "Controlling subsystem job description".into(),
                ..d()
            },
        ),
        (
            "QSYS",
            "QSPLJOBD",
            JobD {
                user: "QSPLJOB".into(),
                jobq: ("QSYS".into(), "QSPL".into()),
                rtgdta: "QCMDB".into(),
                text: "Spooling job description".into(),
                ..d()
            },
        ),
        (
            "QSYS",
            "QSYSJOBD",
            JobD {
                user: "QSYS".into(),
                jobq: ("QSYS".into(), "QSYSNOMAX".into()),
                rtgdta: "QCMDB".into(),
                text: "System job description".into(),
                ..d()
            },
        ),
    ]
}

/// `(library, name, class)`.
fn classes() -> Vec<(&'static str, &'static str, Cls)> {
    let c = |runpty, timeslice, dftwait, text: &str| Cls {
        runpty,
        timeslice,
        dftwait: Some(dftwait),
        text: text.into(),
        ..Cls::default()
    };
    vec![
        ("QGPL", "QBATCH", c(50, 5000, 120, "Batch subsystem class")),
        (
            "QGPL",
            "QINTER",
            c(20, 2000, 30, "Interactive subsystem class"),
        ),
        (
            "QSYS",
            "QCTL",
            c(10, 2000, 30, "Controlling subsystem class"),
        ),
        ("QSYS", "QSPL", c(15, 3000, 120, "Spooling subsystem class")),
        (
            "QSYS",
            "QSYSCLS20",
            c(20, 500, 30, "System class, priority 20"),
        ),
        (
            "QSYS",
            "QSYSCLS25",
            c(25, 500, 30, "System class, priority 25"),
        ),
        (
            "QSYS",
            "QSYSCLS50",
            c(50, 500, 30, "System class, priority 50"),
        ),
    ]
}

/// `(library, name, queue)`.
fn queues() -> Vec<(&'static str, &'static str, MsgQ)> {
    let q = |text: &str| MsgQ {
        text: text.into(),
        ..MsgQ::default()
    };
    vec![
        ("QSYS", "QSYSOPR", q("System operator message queue")),
        ("QSYS", "QSYSMSG", q("Severe system condition messages")),
        ("QSYS", "QHST", q("History log message queue")),
        ("QSYS", "QSCONSOLE", q("Console display message queue")),
        ("QUSRSYS", "QSECOFR", q("Security officer message queue")),
    ]
}

/// Whether `lib` holds `name` of a type.
fn exists(txn: &mut Txn<'_>, lib: ObjectId, name: Name, type_code: u16) -> Result<bool> {
    Ok(Library::load(txn, lib)?
        .lookup(txn, type_code, name)?
        .is_some())
}

/// Create the shipped objects that do not exist; how many were made.
pub fn install(txn: &mut Txn<'_>, qsys: ObjectId, stamp: &Stamp) -> Result<usize> {
    let mut made = 0;
    for (lib, name, d) in jobds() {
        let l = library(txn, qsys, lib)?;
        let n = Name::parse(name).expect("valid");
        if !exists(txn, l, n, types::JOBD)? {
            create(txn, l, n, types::JOBD, &d.text, stamp, &d.encode())?;
            made += 1;
        }
    }
    for (lib, name, c) in classes() {
        let l = library(txn, qsys, lib)?;
        let n = Name::parse(name).expect("valid");
        if !exists(txn, l, n, types::CLS)? {
            create(txn, l, n, types::CLS, &c.text, stamp, &c.encode())?;
            made += 1;
        }
    }
    if super::rpyl::find(txn, qsys)?.is_none() {
        super::rpyl::save(txn, qsys, &super::rpyl::RpyList::shipped(), stamp)?;
        made += 1;
    }
    for (name, s) in super::sbsd::shipped() {
        let n = Name::parse(name).expect("valid");
        if !exists(txn, qsys, n, types::SBSD)? {
            create(txn, qsys, n, types::SBSD, &s.text, stamp, &s.encode())?;
            made += 1;
        }
    }
    for (lib, name, q) in queues() {
        let l = library(txn, qsys, lib)?;
        let n = Name::parse(name).expect("valid");
        if !exists(txn, l, n, types::MSGQ)? {
            create(txn, l, n, types::MSGQ, &q.text, stamp, &q.encode())?;
            made += 1;
        }
    }
    Ok(made)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_values_are_valid() {
        for (_, _, d) in jobds() {
            d.validate().unwrap();
        }
        for (_, _, c) in classes() {
            c.validate().unwrap();
        }
    }
}
