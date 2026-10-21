// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*JOBQ`: jobs waiting to start, by priority then submission time.
//! Each keeps what `SBMJOB` resolved for it: its number, name and user,
//! its routing and request data, its attributes and `*LDA`, where its
//! completion message goes. How many may start at once is the
//! subsystem's job queue entry's business, not the queue's.

use ipc::wire_struct;

use crate::job::JobAttrs;
use crate::rec::{Layout, Rec, field, text_bytes, text_of};
use crate::{Error, Result};

/// The fixed part.
pub const LAYOUT: Layout = Layout {
    name: "JobQueue",
    size: Some(80),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("Held", 10, 1),
        field("OprCtl", 11, 1),
        field("Count", 12, 4),
        field("AutChk", 16, 10),
        field("Reserved", 26, 4),
        field("Text", 30, 50),
    ],
};

/// One queued job's fixed part.
pub const ENTRY_LAYOUT: Layout = Layout {
    name: "JobQueueJob",
    size: Some(176),
    fields: &[
        field("Number", 0, 4),
        field("JobPty", 4, 1),
        field("Held", 5, 1),
        field("Reserved", 6, 2),
        field("Submitted", 8, 8),
        field("Scheduled", 16, 8),
        field("Name", 24, 10),
        field("User", 34, 10),
        field("JobdName", 44, 10),
        field("JobdLib", 54, 10),
        field("MsgqName", 64, 10),
        field("MsgqLib", 74, 10),
        field("SbmName", 84, 10),
        field("SbmUser", 94, 10),
        field("SbmNumber", 104, 4),
        field("RtgDta", 108, 48),
        field("RqsLen", 156, 4),
        field("AttrsLen", 160, 4),
        field("LdaLen", 164, 4),
        field("Reserved2", 168, 8),
    ],
};

const MAGIC: &[u8; 8] = b"QSWMJOBQ";
const VERSION: u64 = 1;

/// A job on a queue.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QJob {
    /// Job number, given at submission.
    pub number: u32,
    /// Job name.
    pub name: String,
    /// User.
    pub user: String,
    /// `JOBPTY` 0-9 (1 is first).
    pub jobpty: u8,
    /// Held (`HOLD(*YES)`, `HLDJOB`).
    pub held: bool,
    /// When it was submitted, microseconds since the epoch.
    pub submitted: u64,
    /// Not before this Unix time (`SCDDATE`/`SCDTIME`); 0 for none.
    pub scheduled: u64,
    /// The job description, `lib/name`.
    pub jobd: String,
    /// Where the completion message goes, `lib/name`; empty for none.
    pub msgq: String,
    /// Who submitted it: name, user, number.
    pub submitter: (String, String, u32),
    /// Routing data.
    pub rtgdta: String,
    /// Request data: the command to run.
    pub rqsdta: String,
    /// The attributes the job starts with.
    pub attrs: JobAttrs,
    /// The `*LDA` it starts with.
    pub lda: Vec<u8>,
}

wire_struct!(QJob {
    number,
    name,
    user,
    jobpty,
    held,
    submitted,
    scheduled,
    jobd,
    msgq,
    submitter,
    rtgdta,
    rqsdta,
    attrs,
    lda
});

impl QJob {
    /// `number/user/name`.
    #[must_use]
    pub fn qualified(&self) -> String {
        format!("{:06}/{}/{}", self.number, self.user, self.name)
    }

    /// Whether it may start now: released and its time come.
    #[must_use]
    pub fn ready(&self, now: u64) -> bool {
        !self.held && self.scheduled <= now
    }

    /// `HLD`, `SCD` or `RLS`, as `WRKJOBQ` shows it.
    #[must_use]
    pub fn status(&self, now: u64) -> &'static str {
        if self.held {
            "HLD"
        } else if self.scheduled > now {
            "SCD"
        } else {
            "RLS"
        }
    }
}

/// A job queue.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobQ {
    /// `HLDJOBQ`.
    pub held: bool,
    /// `OPRCTL(*YES)`.
    pub oprctl: bool,
    /// `AUTCHK`: `*OWNER` or `*DTAAUT`.
    pub autchk: String,
    /// `TEXT`.
    pub text: String,
    /// Jobs in starting order.
    pub jobs: Vec<QJob>,
}

fn split<'a>(tail: &mut &'a [u8], n: usize) -> Result<&'a [u8]> {
    if tail.len() < n {
        return Err(Error::Corrupt("job queue entry truncated".into()));
    }
    let (a, b) = tail.split_at(n);
    *tail = b;
    Ok(a)
}

impl JobQ {
    /// A new queue.
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            oprctl: true,
            autchk: "*OWNER".into(),
            text: text.into(),
            ..Self::default()
        }
    }

    /// The payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_bytes("Magic", MAGIC);
        r.set_uint("Version", VERSION);
        r.set_flag("Held", self.held);
        r.set_flag("OprCtl", self.oprctl);
        r.set_uint("Count", self.jobs.len() as u64);
        r.set_text("AutChk", &self.autchk);
        r.set_text("Text", &self.text);
        let mut out = r.buf;
        for j in &self.jobs {
            let mut e = Rec::new(&ENTRY_LAYOUT);
            let (jl, jn) = j.jobd.split_once('/').unwrap_or(("", &j.jobd));
            let (ml, mn) = j.msgq.split_once('/').unwrap_or(("", &j.msgq));
            let rqs = text_bytes(&j.rqsdta, j.rqsdta.chars().count());
            let attrs = ipc::encode(&j.attrs);
            e.set_uint("Number", u64::from(j.number));
            e.set_uint("JobPty", u64::from(j.jobpty));
            e.set_flag("Held", j.held);
            e.set_uint("Submitted", j.submitted);
            e.set_uint("Scheduled", j.scheduled);
            e.set_text("Name", &j.name);
            e.set_text("User", &j.user);
            e.set_text("JobdName", jn);
            e.set_text("JobdLib", jl);
            e.set_text("MsgqName", mn);
            e.set_text("MsgqLib", ml);
            e.set_text("SbmName", &j.submitter.0);
            e.set_text("SbmUser", &j.submitter.1);
            e.set_uint("SbmNumber", u64::from(j.submitter.2));
            e.set_text("RtgDta", &j.rtgdta);
            e.set_uint("RqsLen", rqs.len() as u64);
            e.set_uint("AttrsLen", attrs.len() as u64);
            e.set_uint("LdaLen", j.lda.len() as u64);
            out.extend(e.buf);
            out.extend(rqs);
            out.extend(attrs);
            out.extend(&j.lda);
        }
        out
    }

    /// Read a payload back.
    #[allow(clippy::many_single_char_names)] // record, entry, lengths
    pub fn decode(b: &[u8]) -> Result<Self> {
        let r = Rec::read(&LAYOUT, b)?;
        if r.bytes("Magic") != MAGIC || r.uint("Version") != VERSION {
            return Err(Error::Corrupt("not a job queue".into()));
        }
        let mut q = Self {
            held: r.flag("Held"),
            oprctl: r.flag("OprCtl"),
            autchk: r.text("AutChk"),
            text: r.text("Text"),
            jobs: Vec::new(),
        };
        let mut rest = r.tail();
        let size = ENTRY_LAYOUT.size.unwrap_or(0);
        for _ in 0..r.uint("Count") {
            let head = split(&mut rest, size)?;
            let e = Rec::read(&ENTRY_LAYOUT, head)?;
            let n = |f: &str| usize::try_from(e.uint(f)).unwrap_or(usize::MAX);
            let rqs = text_of(split(&mut rest, n("RqsLen"))?);
            let attrs = ipc::decode(split(&mut rest, n("AttrsLen"))?)
                .map_err(|x| Error::Corrupt(format!("queued job attributes: {x}")))?;
            let lda = split(&mut rest, n("LdaLen"))?.to_vec();
            let q2 = |lib: String, name: String| {
                if name.is_empty() {
                    String::new()
                } else {
                    format!("{lib}/{name}")
                }
            };
            q.jobs.push(QJob {
                number: u32::try_from(e.uint("Number")).unwrap_or(0),
                name: e.text("Name"),
                user: e.text("User"),
                jobpty: u8::try_from(e.uint("JobPty")).unwrap_or(5),
                held: e.flag("Held"),
                submitted: e.uint("Submitted"),
                scheduled: e.uint("Scheduled"),
                jobd: q2(e.text("JobdLib"), e.text("JobdName")),
                msgq: q2(e.text("MsgqLib"), e.text("MsgqName")),
                submitter: (
                    e.text("SbmName"),
                    e.text("SbmUser"),
                    u32::try_from(e.uint("SbmNumber")).unwrap_or(0),
                ),
                rtgdta: e.text("RtgDta"),
                rqsdta: rqs,
                attrs,
                lda,
            });
        }
        Ok(q)
    }

    /// Put a job in its place: after every job of a better or the same
    /// priority.
    pub fn add(&mut self, j: QJob) {
        let at = self
            .jobs
            .iter()
            .position(|x| x.jobpty > j.jobpty)
            .unwrap_or(self.jobs.len());
        self.jobs.insert(at, j);
    }

    /// The first job that may start now, of a priority `allowed` lets
    /// start; taken off the queue. A held queue starts nothing.
    pub fn take(&mut self, now: u64, allowed: impl Fn(u8) -> bool) -> Option<QJob> {
        if self.held {
            return None;
        }
        let i = self
            .jobs
            .iter()
            .position(|j| j.ready(now) && allowed(j.jobpty))?;
        Some(self.jobs.remove(i))
    }

    /// A job by number.
    pub fn job_mut(&mut self, number: u32) -> Option<&mut QJob> {
        self.jobs.iter_mut().find(|j| j.number == number)
    }

    /// Take a job off the queue (`ENDJOB` of a queued job).
    pub fn remove(&mut self, number: u32) -> Option<QJob> {
        let i = self.jobs.iter().position(|j| j.number == number)?;
        Some(self.jobs.remove(i))
    }

    /// Change a queued job's priority; it moves to its new place.
    pub fn set_priority(&mut self, number: u32, pty: u8) -> bool {
        let Some(mut j) = self.remove(number) else {
            return false;
        };
        j.jobpty = pty;
        self.add(j);
        true
    }
}

/// `SBMJOB`'s parameters, as the submitting job sends them. `None` and
/// empty values take the defaults: the job description's (`*JOBD`), the
/// submitting job's (`*CURRENT`), the user profile's (`*USRPRF`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SbmJob {
    /// `CMD` or `RQSDTA`: what the job runs.
    pub cmd: String,
    /// `JOB`; empty is `*JOBD`.
    pub job: String,
    /// `JOBD`, `lib/name`; empty is `*USRPRF` (`QGPL/QDFTJOBD`).
    pub jobd: String,
    /// `JOBQ`, `lib/name`; empty is `*JOBD`.
    pub jobq: String,
    /// `JOBPTY`.
    pub jobpty: Option<u8>,
    /// `USER`; empty is `*CURRENT`.
    pub user: String,
    /// `RTGDTA`; empty is `QCMDB`.
    pub rtgdta: String,
    /// `HOLD`.
    pub hold: Option<bool>,
    /// `SCDDATE`/`SCDTIME` as Unix seconds; 0 for `*CURRENT`.
    pub scheduled: u64,
    /// `MSGQ`, `lib/name`; empty is `*USRPRF`, `*NONE` for none.
    pub msgq: String,
    /// `INLLIBL`: `None` is `*CURRENT`, `Some(empty)` `*JOBD` ... the
    /// names otherwise.
    pub inllibl: Option<Vec<String>>,
    /// `CURLIB`: empty is `*CURRENT`.
    pub curlib: String,
    /// The other keywords (`LOG`, `INQMSGRPY`, `OUTPTY`, `SWS` ...),
    /// applied to the job's attributes as `CHGJOB` would.
    pub changes: Vec<(String, String)>,
    /// The submitting job's `*LDA`.
    pub lda: Vec<u8>,
}

wire_struct!(SbmJob {
    cmd,
    job,
    jobd,
    jobq,
    jobpty,
    user,
    rtgdta,
    hold,
    scheduled,
    msgq,
    inllibl,
    curlib,
    changes,
    lda
});

#[cfg(test)]
mod tests {
    use super::*;

    fn job(number: u32, pty: u8) -> QJob {
        QJob {
            number,
            name: format!("J{number}"),
            user: "QSECOFR".into(),
            jobpty: pty,
            submitted: u64::from(number),
            jobd: "QGPL/QDFTJOBD".into(),
            msgq: "QUSRSYS/QSECOFR".into(),
            rtgdta: "QCMDB".into(),
            rqsdta: "CALL QGPL/X PARM('a b')".into(),
            lda: vec![0x40; 1024],
            ..QJob::default()
        }
    }

    #[test]
    fn order_take_hold() {
        let mut q = JobQ::new("Test");
        q.add(job(1, 5));
        q.add(job(2, 3));
        q.add(job(3, 5));
        q.add(job(4, 9));
        assert_eq!(
            q.jobs.iter().map(|j| j.number).collect::<Vec<_>>(),
            [2, 1, 3, 4]
        );
        q.job_mut(2).unwrap().held = true;
        assert_eq!(q.take(0, |_| true).unwrap().number, 1);
        q.jobs[1].scheduled = 100;
        assert_eq!(q.take(50, |p| p != 5).unwrap().number, 4);
        assert!(q.take(50, |_| true).is_none(), "held, scheduled later");
        assert_eq!(q.take(100, |_| true).unwrap().number, 3);
        assert!(q.set_priority(2, 1));
        q.held = true;
        assert!(q.take(1000, |_| true).is_none());
    }

    #[test]
    fn round_trip() {
        let mut q = JobQ::new("Batch");
        q.add(job(7, 5));
        q.add(job(8, 1));
        q.jobs[0].attrs.libl.usr = vec!["QGPL".into()];
        let back = JobQ::decode(&q.encode()).unwrap();
        assert_eq!(back, q);
        assert_eq!(back.jobs[0].qualified(), "000008/QSECOFR/J8");
    }
}
