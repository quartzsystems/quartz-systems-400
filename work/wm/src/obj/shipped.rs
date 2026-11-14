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
use super::prtf::Prtf;
use super::{Stamp, create, library};
use crate::Result;

/// `(library, name, description)`.
#[allow(clippy::too_many_lines)] // one entry per shipped description
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
        (
            "QSYS",
            "QSTRUPJD",
            JobD {
                user: "QPGMR".into(),
                jobq: ("QSYS".into(), "QCTL".into()),
                rtgdta: "QSTRUP".into(),
                text: "Startup job description".into(),
                ..d()
            },
        ),
        (
            "QSYS",
            "QZBSJOBD",
            JobD {
                user: "QUSER".into(),
                jobq: ("QSYS".into(), "QUSRNOMAX".into()),
                text: "Host server prestart job description".into(),
                ..d()
            },
        ),
        (
            "QGPL",
            "QDFTSVR",
            JobD {
                user: "QUSER".into(),
                jobq: ("QSYS".into(), "QUSRNOMAX".into()),
                text: "Default server job description".into(),
                ..d()
            },
        ),
        (
            "QSYS",
            "QPWFSPJ",
            JobD {
                user: "QUSER".into(),
                jobq: ("QSYS".into(), "QUSRNOMAX".into()),
                text: "File server prestart job description".into(),
                ..d()
            },
        ),
        (
            "QSYS",
            "QZLSPJ",
            JobD {
                rtgdta: "QZLSSTART".into(),
                inqmsgrpy: "*DFT".into(),
                logoutput: "*PND".into(),
                jobmsgqmx: Some(4),
                jobmsgqfl: "*WRAP".into(),
                text: "NetServer file prestart job description".into(),
                ..d()
            },
        ),
        (
            "QSYS",
            "QZLSSERVER",
            JobD {
                user: "QPGMR".into(),
                rtgdta: "QZLSSTART".into(),
                inqmsgrpy: "*DFT".into(),
                logoutput: "*PND".into(),
                jobmsgqmx: Some(8),
                jobmsgqfl: "*WRAP".into(),
                text: "NetServer server job description".into(),
                ..d()
            },
        ),
        // The NFS daemons' (the primary's, oracle/nfs/jobd-primary.txt).
        (
            "QSYS",
            "QP0LNFSD",
            JobD {
                user: "QUSER".into(),
                jobq: ("QSYS".into(), "QSYSNOMAX".into()),
                rtgdta: "QNFSNFSD".into(),
                inqmsgrpy: "*DFT".into(),
                text: "QP0LNFSD job description".into(),
                ..d()
            },
        ),
        (
            "QSYS",
            "QP0LRPCD",
            JobD {
                user: "QUSER".into(),
                jobq: ("QSYS".into(), "QSYSNOMAX".into()),
                rtgdta: "QNFSRPCD".into(),
                inqmsgrpy: "*DFT".into(),
                text: "QP0LRPCD job description".into(),
                ..d()
            },
        ),
        // The directory server's (docs/DIRSRV.md section 4): the
        // instance jobs, the two publishing agents, its utilities.
        (
            "QSYS",
            "QDIRSRV",
            JobD {
                user: "QDIRSRV".into(),
                jobq: ("QSYS".into(), "QSYSNOMAX".into()),
                text: "Directory server job description".into(),
                ..d()
            },
        ),
        (
            "QSYS",
            "QGLDPUBE",
            JobD {
                user: "QDIRSRV".into(),
                jobq: ("QSYS".into(), "QSYSNOMAX".into()),
                rtgdta: "QGLDPUBE".into(),
                text: "Directory publishing agent QGLDPUBE".into(),
                ..d()
            },
        ),
        (
            "QSYS",
            "QGLDPUBA",
            JobD {
                user: "QDIRSRV".into(),
                jobq: ("QSYS".into(), "QSYSNOMAX".into()),
                rtgdta: "QGLDPUBA".into(),
                text: "Directory publishing agent QGLDPUBA".into(),
                ..d()
            },
        ),
        (
            "QSYS",
            "QDIRJOBD",
            JobD {
                user: "QDIRSRV".into(),
                jobq: ("QSYS".into(), "QSYSNOMAX".into()),
                rtgdta: "QCMDB".into(),
                text: "Directory services job description".into(),
                ..d()
            },
        ),
        (
            "QSYS",
            "QSYSSCD",
            JobD {
                user: "QSYS".into(),
                jobq: ("QSYS".into(), "QSYSNOMAX".into()),
                rtgdta: "QSYSSCD".into(),
                text: "Job scheduler job description".into(),
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
        ("QGPL", "QCASERVR", c(20, 2000, 30, "Host server class")),
        (
            "QSYS",
            "QPWFSERVER",
            c(20, 2000, 30, "File and database server class"),
        ),
        (
            "QSYS",
            "QSYSCLS50",
            c(50, 500, 30, "System class, priority 50"),
        ),
        // The NFS daemons' routing entries name these (QSYSWRK 2120 to
        // 2127); the primary's QSYS has QP0LNFSD and QP0LRPCD. Their
        // values are QSYSCLS20's (NEEDS-CODY: DSPCLS of the primary's).
        ("QSYS", "QP0LNFSD", c(20, 500, 30, "Class for QP0LNFSD job")),
        ("QSYS", "QP0LRPCD", c(20, 500, 30, "Class for QP0LRPCD job")),
        ("QSYS", "QP0LBIOD", c(20, 500, 30, "Class for QP0LBIOD job")),
        ("QSYS", "QP0LMNTD", c(20, 500, 30, "Class for QP0LMNTD job")),
        ("QSYS", "QP0LLCKD", c(20, 500, 30, "Class for QP0LLCKD job")),
        (
            "QSYS",
            "QP0LSTATD",
            c(20, 500, 30, "Class for QP0LSTATD job"),
        ),
        ("QSYS", "QP0LGSSD", c(20, 500, 30, "Class for QP0LGSSD job")),
        ("QSYS", "QP0LRGYD", c(20, 500, 30, "Class for QP0LRGYD job")),
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

/// The printer files in `QSYS`.
fn printer_files() -> Vec<(&'static str, Prtf)> {
    let f = |text: &str, outq: &str| Prtf {
        text: text.into(),
        outq: outq.into(),
        ..Prtf::default()
    };
    vec![
        ("QSYSPRT", f("System printer file", "*JOB")),
        ("QPRINT", f("Program-described printer file", "*JOB")),
        ("QPJOBLOG", f("Job log", "QUSRSYS/QEZJOBLOG")),
        ("QPSRVDMP", f("Service dump", "QUSRSYS/QEZDEBUG")),
        ("QPPGMDMP", f("RPG formatted dump", "*JOB")),
        ("QPDSPJOB", f("Job information", "*JOB")),
        ("QPDSPLOG", f("History log", "*JOB")),
        ("QPDSPSBS", f("Subsystem information", "*JOB")),
    ]
}

/// `(library, name, text)` of the job queues.
fn job_queues() -> [(&'static str, &'static str, &'static str); 10] {
    [
        ("QGPL", "QBATCH", "Batch job queue"),
        ("QGPL", "QBASE", "Base subsystem job queue"),
        ("QGPL", "QINTER", "Interactive job queue"),
        ("QGPL", "QPGMR", "Programmer job queue"),
        ("QGPL", "QS36EVOKE", "System/36 evoke job queue"),
        ("QSYS", "QCTL", "Controlling subsystem job queue"),
        ("QSYS", "QSPL", "Spooling job queue"),
        ("QSYS", "QSYSNOMAX", "System jobs, no limit"),
        ("QSYS", "QUSRNOMAX", "User jobs, no limit"),
        ("QSYS", "QPWFSERVER", "File server job queue"),
    ]
}

/// `(library, name, text)` of the output queues.
fn output_queues() -> [(&'static str, &'static str, &'static str); 6] {
    [
        ("QGPL", "QPRINT", "Default printer output queue"),
        ("QGPL", "QPRINTS", "Special forms output queue"),
        ("QGPL", "QPRINT2", "Second printer output queue"),
        ("QUSRSYS", "QEZJOBLOG", "Job logs"),
        ("QUSRSYS", "QEZDEBUG", "Service dumps and debug output"),
        ("QUSRSYS", "QSCAPTURE", "Capture printer output queue"),
    ]
}

/// Every object [`install`] makes but the reply list (a `*USRSPC`,
/// [`super::rpyl`]): `(library, name, type)`.
#[must_use]
pub fn objects() -> Vec<(&'static str, &'static str, u16)> {
    let mut v = Vec::new();
    v.extend(jobds().into_iter().map(|(l, n, _)| (l, n, types::JOBD)));
    v.extend(classes().into_iter().map(|(l, n, _)| (l, n, types::CLS)));
    v.extend(
        job_queues()
            .into_iter()
            .map(|(l, n, _)| (l, n, types::JOBQ)),
    );
    v.extend(
        output_queues()
            .into_iter()
            .map(|(l, n, _)| (l, n, types::OUTQ)),
    );
    v.extend(
        printer_files()
            .into_iter()
            .map(|(n, _)| ("QSYS", n, types::FILE)),
    );
    v.push(("QSYS", super::devd::CONSOLE, types::DEVD));
    v.push(("QSYS", super::devd::CAPTURE, types::DEVD));
    let (slib, sname) = super::jobscd::SCHEDULE;
    v.push((slib, sname, types::JOBSCD));
    v.extend(
        super::sbsd::shipped()
            .into_iter()
            .map(|(n, _)| ("QSYS", n, types::SBSD)),
    );
    v.extend(queues().into_iter().map(|(l, n, _)| (l, n, types::MSGQ)));
    v
}

/// Whether `lib` holds `name` of a type.
fn exists(txn: &mut Txn<'_>, lib: ObjectId, name: Name, type_code: u16) -> Result<bool> {
    Ok(Library::load(txn, lib)?
        .lookup(txn, type_code, name)?
        .is_some())
}

/// Create the shipped objects that do not exist; how many were made.
#[allow(clippy::too_many_lines)]
pub fn install(txn: &mut Txn<'_>, qsys: ObjectId, stamp: &Stamp) -> Result<usize> {
    // A release's entries for a subsystem the system already has, before
    // the objects that mark them are made.
    let mut made = super::upgrade::add_missing(txn, qsys)?.len();
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
    for (lib, name, text) in job_queues() {
        let l = library(txn, qsys, lib)?;
        let n = Name::parse(name).expect("valid");
        if !exists(txn, l, n, types::JOBQ)? {
            let q = super::jobq::JobQ::new(text);
            create(txn, l, n, types::JOBQ, text, stamp, &q.encode())?;
            made += 1;
        }
    }
    for (lib, name, text) in output_queues() {
        let l = library(txn, qsys, lib)?;
        let n = Name::parse(name).expect("valid");
        if !exists(txn, l, n, types::OUTQ)? {
            let q = super::outq::OutQ::new(text);
            create(txn, l, n, types::OUTQ, text, stamp, &q.encode())?;
            made += 1;
        }
    }
    for (name, f) in printer_files() {
        let n = Name::parse(name).expect("valid");
        if !exists(txn, qsys, n, types::FILE)? {
            let id = (n, types::FILE, "PRTF");
            super::create_attr(txn, qsys, id, &f.text, stamp, &f.encode())?;
            made += 1;
        }
    }
    let n = Name::parse(super::devd::CONSOLE).expect("valid");
    if !exists(txn, qsys, n, types::DEVD)? {
        let d = super::devd::Devd::console();
        let id = (n, types::DEVD, "DSP");
        super::create_attr(txn, qsys, id, &d.text, stamp, &d.encode())?;
        made += 1;
    }
    let n = Name::parse(super::devd::CAPTURE).expect("valid");
    if !exists(txn, qsys, n, types::DEVD)? {
        let d = super::devd::Devd::capture();
        let id = (n, types::DEVD, "PRT");
        super::create_attr(txn, qsys, id, &d.text, stamp, &d.encode())?;
        made += 1;
    }
    let (slib, sname) = super::jobscd::SCHEDULE;
    let l = library(txn, qsys, slib)?;
    let n = Name::parse(sname).expect("valid");
    if !exists(txn, l, n, types::JOBSCD)? {
        let text = "Default job schedule";
        let s = super::jobscd::JobScd {
            next_entry: 1,
            entries: Vec::new(),
        };
        create(txn, l, n, types::JOBSCD, text, stamp, &s.encode())?;
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
