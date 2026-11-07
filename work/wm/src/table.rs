// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The active job table: every job with a process, as the storage
//! service knows it from the job's connection. `WRKACTJOB`, `QUSLJOB`
//! and the job number counter read it.

use std::collections::BTreeMap;

use ipc::wire_struct;

use crate::job::{JobType, Status, SubType};

/// One active job, as it travels on the wire.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobInfo {
    /// Job name.
    pub name: String,
    /// User profile.
    pub user: String,
    /// Job number.
    pub number: u32,
    /// Type code (`QUSRJOBI`): `B`, `I`, `M`, `S`, `W`, `X`, ...
    pub kind: char,
    /// Subtype code, blank for none.
    pub subtype: char,
    /// The subsystem it runs in; blank for system jobs.
    pub subsystem: String,
    /// The process.
    pub pid: u32,
    /// Status byte ([`Status::byte`]).
    pub status: u8,
    /// What it is doing: `PGM-name`, `CMD-name`, `DLY-nn`, ...
    pub function: String,
    /// Unix time it started.
    pub started: u64,
    /// The lock owner handle; the job number.
    pub handle: u64,
    /// The job description it started from, `lib/name`.
    pub jobd: String,
    /// Its class, `lib/name`.
    pub class: String,
    /// The storage pool it runs in; blank for none.
    pub pool: String,
}

wire_struct!(JobInfo {
    name,
    user,
    number,
    kind,
    subtype,
    subsystem,
    pid,
    status,
    function,
    started,
    handle,
    jobd,
    class,
    pool
});

impl JobInfo {
    /// The type.
    #[must_use]
    pub fn job_type(&self) -> Option<JobType> {
        JobType::from_code(self.kind)
    }

    /// The subtype.
    #[must_use]
    pub fn sub_type(&self) -> SubType {
        SubType::from_code(self.subtype).unwrap_or_default()
    }

    /// The status.
    #[must_use]
    pub fn job_status(&self) -> Status {
        Status::from_byte(self.status)
    }

    /// `number/user/name`.
    #[must_use]
    pub fn qualified(&self) -> String {
        format!("{:06}/{}/{}", self.number, self.user, self.name)
    }
}

/// Active jobs by number.
#[derive(Debug, Default)]
pub struct JobTable {
    jobs: BTreeMap<u32, JobInfo>,
}

impl JobTable {
    /// Add a job; false when its number is taken.
    pub fn add(&mut self, job: JobInfo) -> bool {
        if self.jobs.contains_key(&job.number) {
            return false;
        }
        self.jobs.insert(job.number, job);
        true
    }

    /// Remove a job.
    pub fn remove(&mut self, number: u32) -> Option<JobInfo> {
        self.jobs.remove(&number)
    }

    /// A job by number.
    #[must_use]
    pub fn get(&self, number: u32) -> Option<&JobInfo> {
        self.jobs.get(&number)
    }

    /// A job by number, to change.
    pub fn get_mut(&mut self, number: u32) -> Option<&mut JobInfo> {
        self.jobs.get_mut(&number)
    }

    /// Whether a number is in use.
    #[must_use]
    pub fn in_use(&self, number: u32) -> bool {
        self.jobs.contains_key(&number)
    }

    /// Every job, by number.
    #[must_use]
    pub fn all(&self) -> Vec<JobInfo> {
        self.jobs.values().cloned().collect()
    }

    /// The job a `JOB()` value names: `number/user/name` (all three
    /// must match), `user/name`, or a bare name, the job of `caller`
    /// of that name first. The oldest of several wins.
    #[must_use]
    pub fn lookup(&self, q: &str, caller: &str) -> Option<&JobInfo> {
        let q = q.trim().to_ascii_uppercase();
        let parts: Vec<&str> = q.split('/').map(str::trim).collect();
        match parts.as_slice() {
            [number, user, name] => {
                let n: u32 = number.parse().ok()?;
                self.get(n).filter(|j| j.user == *user && j.name == *name)
            }
            [user, name] => self
                .jobs
                .values()
                .find(|j| j.user == *user && j.name == *name),
            [name] => {
                let mut named = self.jobs.values().filter(|j| j.name == *name);
                let first = named.clone().next();
                named.find(|j| j.user == caller).or(first)
            }
            _ => None,
        }
    }

    /// How many.
    #[must_use]
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table() {
        let mut t = JobTable::default();
        let j = JobInfo {
            name: "QSVCJOB".into(),
            user: "QSYS".into(),
            number: 7,
            kind: 'S',
            subtype: ' ',
            status: Status::Evtw.byte(),
            ..JobInfo::default()
        };
        assert!(t.add(j.clone()));
        assert!(!t.add(j.clone()));
        assert!(t.in_use(7));
        assert_eq!(t.get(7).unwrap().qualified(), "000007/QSYS/QSVCJOB");
        assert_eq!(t.get(7).unwrap().job_status(), Status::Evtw);
        let back: JobInfo = ipc::decode(&ipc::encode(&j)).unwrap();
        assert_eq!(back, j);
        assert_eq!(t.remove(7).unwrap().job_type(), Some(JobType::System));
        assert!(t.is_empty());
    }

    fn job(number: u32, user: &str, name: &str) -> JobInfo {
        JobInfo {
            name: name.into(),
            user: user.into(),
            number,
            ..JobInfo::default()
        }
    }

    #[test]
    fn lookup() {
        let mut t = JobTable::default();
        t.add(job(815, "QTMHHTTP", "ADMIN"));
        t.add(job(820, "CODY", "ADMIN"));
        t.add(job(821, "QSECOFR", "QPADEV0001"));
        let n = |q: &str, caller: &str| t.lookup(q, caller).map(|j| j.number);
        assert_eq!(n("000815/QTMHHTTP/ADMIN", "QSECOFR"), Some(815));
        assert_eq!(n("815/qtmhhttp/admin", "QSECOFR"), Some(815));
        // Every part must match.
        assert_eq!(n("000815/QSECOFR/ADMIN", "QSECOFR"), None);
        assert_eq!(n("000815/QTMHHTTP/OTHER", "QSECOFR"), None);
        assert_eq!(n("999999/QTMHHTTP/ADMIN", "QSECOFR"), None);
        assert_eq!(n("QTMHHTTP/ADMIN", "QSECOFR"), Some(815));
        // A bare name: the caller's own first, else the oldest.
        assert_eq!(n("ADMIN", "CODY"), Some(820));
        assert_eq!(n("ADMIN", "QSECOFR"), Some(815));
        assert_eq!(n("NONE", "QSECOFR"), None);
        assert_eq!(n("1/2/3/4", "QSECOFR"), None);
    }
}
