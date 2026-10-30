// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*SBSD`: a subsystem description. A fixed part with the counts of
//! each kind of entry, then the entries in a fixed order: pools,
//! routing, workstation names, workstation types, job queues,
//! autostart jobs, prestart jobs, communications.

use std::fmt::Write as _;

use crate::rec::{Layout, Rec, field};
use crate::{Error, Result};

/// The fixed part.
pub const LAYOUT: Layout = Layout {
    name: "SubsystemDescription",
    size: Some(128),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("Pools", 10, 1),
        field("Reserved1", 11, 1),
        field("MaxJobs", 12, 4),
        field("SgnDspfName", 16, 10),
        field("SgnDspfLib", 26, 10),
        field("SysLibLe", 36, 10),
        field("Routing", 46, 2),
        field("WsNames", 48, 2),
        field("WsTypes", 50, 2),
        field("JobQueues", 52, 2),
        field("Autostart", 54, 2),
        field("Prestart", 56, 2),
        field("Comm", 58, 2),
        field("Reserved2", 60, 18),
        field("Text", 78, 50),
    ],
};

/// A pool entry.
pub const POOL_LAYOUT: Layout = Layout {
    name: "SubsystemPool",
    size: Some(16),
    fields: &[
        field("Id", 0, 1),
        field("Kind", 1, 1),
        field("Shared", 2, 1),
        field("Reserved", 3, 1),
        field("SizeKb", 4, 8),
        field("ActLvl", 12, 4),
    ],
};

/// A routing entry.
pub const RTGE_LAYOUT: Layout = Layout {
    name: "RoutingEntry",
    size: Some(84),
    fields: &[
        field("SeqNbr", 0, 2),
        field("CmpStart", 2, 2),
        field("MaxAct", 4, 2),
        field("PoolId", 6, 1),
        field("Reserved", 7, 1),
        field("PgmName", 8, 10),
        field("PgmLib", 18, 10),
        field("ClsName", 28, 10),
        field("ClsLib", 38, 10),
        field("CmpVal", 48, 32),
        field("Reserved2", 80, 4),
    ],
};

/// A workstation name or type entry.
pub const WSE_LAYOUT: Layout = Layout {
    name: "WorkstationEntry",
    size: Some(36),
    fields: &[
        field("Name", 0, 10),
        field("JobdName", 10, 10),
        field("JobdLib", 20, 10),
        field("MaxAct", 30, 2),
        field("At", 32, 1),
        field("Reserved", 33, 3),
    ],
};

/// A job queue entry.
pub const JOBQE_LAYOUT: Layout = Layout {
    name: "JobQueueEntry",
    size: Some(36),
    fields: &[
        field("Name", 0, 10),
        field("Lib", 10, 10),
        field("SeqNbr", 20, 2),
        field("MaxAct", 22, 2),
        field("MaxPty", 24, 9),
        field("Reserved", 33, 3),
    ],
};

/// An autostart job entry.
pub const AJE_LAYOUT: Layout = Layout {
    name: "AutostartEntry",
    size: Some(32),
    fields: &[
        field("Job", 0, 10),
        field("JobdName", 10, 10),
        field("JobdLib", 20, 10),
        field("Reserved", 30, 2),
    ],
};

/// A prestart job entry.
pub const PJE_LAYOUT: Layout = Layout {
    name: "PrestartEntry",
    size: Some(96),
    fields: &[
        field("PgmName", 0, 10),
        field("PgmLib", 10, 10),
        field("User", 20, 10),
        field("JobName", 30, 10),
        field("JobdName", 40, 10),
        field("JobdLib", 50, 10),
        field("ClsName", 60, 10),
        field("ClsLib", 70, 10),
        field("StrJobs", 80, 1),
        field("Wait", 81, 1),
        field("PoolId", 82, 1),
        field("Reserved", 83, 1),
        field("InlJobs", 84, 2),
        field("Threshold", 86, 2),
        field("AdlJobs", 88, 2),
        field("MaxJobs", 90, 2),
        field("MaxUse", 92, 2),
        field("Reserved2", 94, 2),
    ],
};

/// A communications entry (kept; inactive until communications exist).
pub const CMNE_LAYOUT: Layout = Layout {
    name: "CommEntry",
    size: Some(32),
    fields: &[
        field("Dev", 0, 10),
        field("Mode", 10, 10),
        field("JobdName", 20, 10),
        field("MaxAct", 30, 2),
    ],
};

const MAGIC: &[u8; 8] = b"QSWMSBSD";
const VERSION: u64 = 1;
/// `*NOMAX` in a two-byte limit.
pub const NOMAX16: u16 = 0;

/// Which storage pool a subsystem pool is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoolKind {
    /// A private pool of its own size.
    Private,
    /// `*BASE`.
    Base,
    /// `*INTERACT`.
    Interact,
    /// `*SPOOL`.
    Spool,
    /// `*SHRPOOLn`.
    Shared(u8),
    /// `*MACHINE`.
    Machine,
}

impl PoolKind {
    fn code(self) -> (u8, u8) {
        match self {
            Self::Private => (0, 0),
            Self::Base => (1, 0),
            Self::Interact => (2, 0),
            Self::Spool => (3, 0),
            Self::Shared(n) => (4, n),
            Self::Machine => (5, 0),
        }
    }

    fn of(code: u8, n: u8) -> Self {
        match code {
            1 => Self::Base,
            2 => Self::Interact,
            3 => Self::Spool,
            4 => Self::Shared(n),
            5 => Self::Machine,
            _ => Self::Private,
        }
    }

    /// `*BASE`, `*SHRPOOL3`, ...; blank for a private pool.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Private => String::new(),
            Self::Base => "*BASE".into(),
            Self::Interact => "*INTERACT".into(),
            Self::Spool => "*SPOOL".into(),
            Self::Shared(n) => format!("*SHRPOOL{n}"),
            Self::Machine => "*MACHINE".into(),
        }
    }

    /// From a special value.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_uppercase().as_str() {
            "*BASE" => Self::Base,
            "*INTERACT" => Self::Interact,
            "*SPOOL" => Self::Spool,
            "*MACHINE" => Self::Machine,
            other => {
                let n: u8 = other.strip_prefix("*SHRPOOL")?.parse().ok()?;
                if !(1..=60).contains(&n) {
                    return None;
                }
                Self::Shared(n)
            }
        })
    }
}

/// A subsystem pool: its number in the subsystem and what it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pool {
    /// 1-10.
    pub id: u8,
    /// Which pool.
    pub kind: PoolKind,
    /// A private pool's size in kilobytes.
    pub size_kb: u64,
    /// A private pool's activity level.
    pub actlvl: u32,
}

/// A routing entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rtge {
    /// `SEQNBR`.
    pub seq: u16,
    /// `CMPVAL` value; `*ANY` matches everything.
    pub cmpval: String,
    /// `CMPVAL` start position, 1-based.
    pub start: u16,
    /// `PGM`: name and library.
    pub pgm: (String, String),
    /// `CLS`: name and library.
    pub cls: (String, String),
    /// `MAXACT`; 0 is `*NOMAX`.
    pub maxact: u16,
    /// `POOLID`.
    pub pool: u8,
}

impl Rtge {
    /// Whether routing data selects this entry.
    #[must_use]
    pub fn matches(&self, rtgdta: &str) -> bool {
        if self.cmpval == "*ANY" {
            return true;
        }
        let at = usize::from(self.start.max(1) - 1);
        rtgdta
            .get(at..)
            .is_some_and(|r| r.starts_with(&self.cmpval))
    }
}

/// A workstation entry, by name or by type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wse {
    /// The device name (generic with `*`), or the type (`*ALL`, `*CONS`,
    /// `5251` ...).
    pub name: String,
    /// `JOBD`: name and library; `*USRPRF` as the name.
    pub jobd: (String, String),
    /// `MAXACT`; 0 is `*NOMAX`.
    pub maxact: u16,
    /// `AT(*SIGNON)` when true, `*ENTER` when false.
    pub at_signon: bool,
}

/// A job queue entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Jobqe {
    /// The queue: name and library.
    pub jobq: (String, String),
    /// `SEQNBR`.
    pub seq: u16,
    /// `MAXACT`; 0 is `*NOMAX`.
    pub maxact: u16,
    /// `MAXPTY1` to `MAXPTY9`; 0 is `*NOMAX`.
    pub maxpty: [u8; 9],
}

/// An autostart job entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Aje {
    /// The job name.
    pub job: String,
    /// `JOBD`: name and library.
    pub jobd: (String, String),
}

/// A prestart job entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pje {
    /// `PGM`.
    pub pgm: (String, String),
    /// `USER`.
    pub user: String,
    /// `JOB` name.
    pub job: String,
    /// `JOBD`.
    pub jobd: (String, String),
    /// `CLS`.
    pub cls: (String, String),
    /// `STRJOBS(*YES)`.
    pub strjobs: bool,
    /// `WAIT(*YES)`.
    pub wait: bool,
    /// `POOLID`.
    pub pool: u8,
    /// `INLJOBS`.
    pub inljobs: u16,
    /// `THRESHOLD`.
    pub threshold: u16,
    /// `ADLJOBS`.
    pub adljobs: u16,
    /// `MAXJOBS`; 0 is `*NOMAX`.
    pub maxjobs: u16,
    /// `MAXUSE`; 0 is `*NOMAX`.
    pub maxuse: u16,
}

/// A communications entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cmne {
    /// `DEV`.
    pub dev: String,
    /// `MODE`.
    pub mode: String,
    /// `JOBD` name.
    pub jobd: String,
    /// `MAXACT`.
    pub maxact: u16,
}

/// A subsystem description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sbsd {
    /// `POOLS`.
    pub pools: Vec<Pool>,
    /// `MAXJOBS`; 0 is `*NOMAX`.
    pub maxjobs: u32,
    /// `SGNDSPF`: name and library, `*QDSIGNON` as the name.
    pub sgndspf: (String, String),
    /// `SYSLIBLE`: a library or `*NONE`.
    pub syslible: String,
    /// `TEXT`.
    pub text: String,
    /// Routing entries by sequence number.
    pub rtges: Vec<Rtge>,
    /// Workstation name entries.
    pub wsns: Vec<Wse>,
    /// Workstation type entries.
    pub wsts: Vec<Wse>,
    /// Job queue entries by sequence number.
    pub jobqes: Vec<Jobqe>,
    /// Autostart job entries.
    pub ajes: Vec<Aje>,
    /// Prestart job entries.
    pub pjes: Vec<Pje>,
    /// Communications entries.
    pub cmnes: Vec<Cmne>,
}

impl Default for Sbsd {
    /// `CRTSBSD`'s defaults: `POOLS((1 *BASE))`.
    fn default() -> Self {
        Self {
            pools: vec![Pool {
                id: 1,
                kind: PoolKind::Base,
                size_kb: 0,
                actlvl: 0,
            }],
            maxjobs: 0,
            sgndspf: ("*QDSIGNON".into(), String::new()),
            syslible: "*NONE".into(),
            text: String::new(),
            rtges: Vec::new(),
            wsns: Vec::new(),
            wsts: Vec::new(),
            jobqes: Vec::new(),
            ajes: Vec::new(),
            pjes: Vec::new(),
            cmnes: Vec::new(),
        }
    }
}

fn count(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

fn small(r: &Rec<'_>, f: &str) -> u16 {
    u16::try_from(r.uint(f)).unwrap_or(u16::MAX)
}

fn byte(r: &Rec<'_>, f: &str) -> u8 {
    u8::try_from(r.uint(f)).unwrap_or(u8::MAX)
}

/// Take `n` entries of `layout` from `rest`, decoding each with `f`.
fn take<T>(
    rest: &mut &[u8],
    n: u64,
    layout: &'static Layout,
    f: impl Fn(&Rec<'_>) -> T,
) -> Result<Vec<T>> {
    let size = layout.size.unwrap_or(0);
    let mut out = Vec::new();
    for _ in 0..n {
        if rest.len() < size {
            return Err(Error::Corrupt(format!("{} truncated", layout.name)));
        }
        let r = Rec::read(layout, &rest[..size])?;
        out.push(f(&r));
        *rest = &rest[size..];
    }
    Ok(out)
}

fn put_wse(out: &mut Vec<u8>, w: &Wse) {
    let mut r = Rec::new(&WSE_LAYOUT);
    r.set_text("Name", &w.name);
    r.set_text("JobdName", &w.jobd.0);
    r.set_text("JobdLib", &w.jobd.1);
    r.set_uint("MaxAct", u64::from(w.maxact));
    r.set_flag("At", w.at_signon);
    out.extend(r.buf);
}

fn get_wse(r: &Rec<'_>) -> Wse {
    Wse {
        name: r.text("Name"),
        jobd: (r.text("JobdName"), r.text("JobdLib")),
        maxact: small(r, "MaxAct"),
        at_signon: r.flag("At"),
    }
}

impl Sbsd {
    /// The payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_bytes("Magic", MAGIC);
        r.set_uint("Version", VERSION);
        r.set_uint("Pools", count(self.pools.len()));
        r.set_uint("MaxJobs", u64::from(self.maxjobs));
        r.set_text("SgnDspfName", &self.sgndspf.0);
        r.set_text("SgnDspfLib", &self.sgndspf.1);
        r.set_text("SysLibLe", &self.syslible);
        r.set_uint("Routing", count(self.rtges.len()));
        r.set_uint("WsNames", count(self.wsns.len()));
        r.set_uint("WsTypes", count(self.wsts.len()));
        r.set_uint("JobQueues", count(self.jobqes.len()));
        r.set_uint("Autostart", count(self.ajes.len()));
        r.set_uint("Prestart", count(self.pjes.len()));
        r.set_uint("Comm", count(self.cmnes.len()));
        r.set_text("Text", &self.text);
        let mut out = r.buf;
        for p in &self.pools {
            let mut e = Rec::new(&POOL_LAYOUT);
            let (k, n) = p.kind.code();
            e.set_uint("Id", u64::from(p.id));
            e.set_uint("Kind", u64::from(k));
            e.set_uint("Shared", u64::from(n));
            e.set_uint("SizeKb", p.size_kb);
            e.set_uint("ActLvl", u64::from(p.actlvl));
            out.extend(e.buf);
        }
        for t in &self.rtges {
            let mut e = Rec::new(&RTGE_LAYOUT);
            e.set_uint("SeqNbr", u64::from(t.seq));
            e.set_uint("CmpStart", u64::from(t.start));
            e.set_uint("MaxAct", u64::from(t.maxact));
            e.set_uint("PoolId", u64::from(t.pool));
            e.set_text("PgmName", &t.pgm.0);
            e.set_text("PgmLib", &t.pgm.1);
            e.set_text("ClsName", &t.cls.0);
            e.set_text("ClsLib", &t.cls.1);
            e.set_text("CmpVal", &t.cmpval);
            out.extend(e.buf);
        }
        for w in self.wsns.iter().chain(&self.wsts) {
            put_wse(&mut out, w);
        }
        for q in &self.jobqes {
            let mut e = Rec::new(&JOBQE_LAYOUT);
            e.set_text("Name", &q.jobq.0);
            e.set_text("Lib", &q.jobq.1);
            e.set_uint("SeqNbr", u64::from(q.seq));
            e.set_uint("MaxAct", u64::from(q.maxact));
            e.set_bytes("MaxPty", &q.maxpty);
            out.extend(e.buf);
        }
        for a in &self.ajes {
            let mut e = Rec::new(&AJE_LAYOUT);
            e.set_text("Job", &a.job);
            e.set_text("JobdName", &a.jobd.0);
            e.set_text("JobdLib", &a.jobd.1);
            out.extend(e.buf);
        }
        for p in &self.pjes {
            let mut e = Rec::new(&PJE_LAYOUT);
            e.set_text("PgmName", &p.pgm.0);
            e.set_text("PgmLib", &p.pgm.1);
            e.set_text("User", &p.user);
            e.set_text("JobName", &p.job);
            e.set_text("JobdName", &p.jobd.0);
            e.set_text("JobdLib", &p.jobd.1);
            e.set_text("ClsName", &p.cls.0);
            e.set_text("ClsLib", &p.cls.1);
            e.set_flag("StrJobs", p.strjobs);
            e.set_flag("Wait", p.wait);
            e.set_uint("PoolId", u64::from(p.pool));
            e.set_uint("InlJobs", u64::from(p.inljobs));
            e.set_uint("Threshold", u64::from(p.threshold));
            e.set_uint("AdlJobs", u64::from(p.adljobs));
            e.set_uint("MaxJobs", u64::from(p.maxjobs));
            e.set_uint("MaxUse", u64::from(p.maxuse));
            out.extend(e.buf);
        }
        for c in &self.cmnes {
            let mut e = Rec::new(&CMNE_LAYOUT);
            e.set_text("Dev", &c.dev);
            e.set_text("Mode", &c.mode);
            e.set_text("JobdName", &c.jobd);
            e.set_uint("MaxAct", u64::from(c.maxact));
            out.extend(e.buf);
        }
        out
    }

    #[allow(clippy::similar_names)] // wsns and wsts, as the fields
    /// Read a payload back.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let r = Rec::read(&LAYOUT, b)?;
        if r.bytes("Magic") != MAGIC || r.uint("Version") != VERSION {
            return Err(Error::Corrupt("not a subsystem description".into()));
        }
        let mut rest = r.tail();
        let pools = take(&mut rest, r.uint("Pools"), &POOL_LAYOUT, |e| Pool {
            id: byte(e, "Id"),
            kind: PoolKind::of(byte(e, "Kind"), byte(e, "Shared")),
            size_kb: e.uint("SizeKb"),
            actlvl: u32::try_from(e.uint("ActLvl")).unwrap_or(u32::MAX),
        })?;
        let rtges = take(&mut rest, r.uint("Routing"), &RTGE_LAYOUT, |e| Rtge {
            seq: small(e, "SeqNbr"),
            cmpval: e.text("CmpVal"),
            start: small(e, "CmpStart"),
            pgm: (e.text("PgmName"), e.text("PgmLib")),
            cls: (e.text("ClsName"), e.text("ClsLib")),
            maxact: small(e, "MaxAct"),
            pool: byte(e, "PoolId"),
        })?;
        let wsns = take(&mut rest, r.uint("WsNames"), &WSE_LAYOUT, get_wse)?;
        let wsts = take(&mut rest, r.uint("WsTypes"), &WSE_LAYOUT, get_wse)?;
        let jobqes = take(&mut rest, r.uint("JobQueues"), &JOBQE_LAYOUT, |e| Jobqe {
            jobq: (e.text("Name"), e.text("Lib")),
            seq: small(e, "SeqNbr"),
            maxact: small(e, "MaxAct"),
            maxpty: e.bytes("MaxPty").try_into().expect("nine"),
        })?;
        let ajes = take(&mut rest, r.uint("Autostart"), &AJE_LAYOUT, |e| Aje {
            job: e.text("Job"),
            jobd: (e.text("JobdName"), e.text("JobdLib")),
        })?;
        let pjes = take(&mut rest, r.uint("Prestart"), &PJE_LAYOUT, |e| Pje {
            pgm: (e.text("PgmName"), e.text("PgmLib")),
            user: e.text("User"),
            job: e.text("JobName"),
            jobd: (e.text("JobdName"), e.text("JobdLib")),
            cls: (e.text("ClsName"), e.text("ClsLib")),
            strjobs: e.flag("StrJobs"),
            wait: e.flag("Wait"),
            pool: byte(e, "PoolId"),
            inljobs: small(e, "InlJobs"),
            threshold: small(e, "Threshold"),
            adljobs: small(e, "AdlJobs"),
            maxjobs: small(e, "MaxJobs"),
            maxuse: small(e, "MaxUse"),
        })?;
        let cmnes = take(&mut rest, r.uint("Comm"), &CMNE_LAYOUT, |e| Cmne {
            dev: e.text("Dev"),
            mode: e.text("Mode"),
            jobd: e.text("JobdName"),
            maxact: small(e, "MaxAct"),
        })?;
        Ok(Self {
            pools,
            maxjobs: u32::try_from(r.uint("MaxJobs")).unwrap_or(u32::MAX),
            sgndspf: (r.text("SgnDspfName"), r.text("SgnDspfLib")),
            syslible: r.text("SysLibLe"),
            text: r.text("Text"),
            rtges,
            wsns,
            wsts,
            jobqes,
            ajes,
            pjes,
            cmnes,
        })
    }

    /// The routing entry routing data selects: the lowest sequence
    /// number whose comparison matches.
    #[must_use]
    pub fn route(&self, rtgdta: &str) -> Option<&Rtge> {
        let mut v: Vec<&Rtge> = self.rtges.iter().collect();
        v.sort_by_key(|r| r.seq);
        v.into_iter().find(|r| r.matches(rtgdta))
    }

    /// The pool with subsystem pool number `id`.
    #[must_use]
    pub fn pool(&self, id: u8) -> Option<&Pool> {
        self.pools.iter().find(|p| p.id == id)
    }

    /// `DSPSBSD`'s menu of options, then each option's data: 1
    /// attributes, 2 pools, 3 autostart, 4 workstation names, 5
    /// workstation types, 6 job queues, 7 routing, 8 communications,
    /// 9 remote location names (none), 10 prestart.
    #[must_use]
    #[allow(clippy::too_many_lines)] // one block per DSPSBSD option
    pub fn display(&self, lib: &str, name: &str, option: Option<u8>) -> String {
        let mut out = format!("Subsystem description:   {name}     Library:   {lib}\n\n");
        let nomax = |n: u64| {
            if n == 0 {
                "*NOMAX".to_string()
            } else {
                n.to_string()
            }
        };
        let want = |n: u8| option.is_none_or(|o| o == n);
        if want(1) {
            let _ = writeln!(out, " 1. Operational attributes");
            let _ = writeln!(
                out,
                "    Maximum jobs in subsystem . . . . :   {}",
                nomax(u64::from(self.maxjobs))
            );
            let _ = writeln!(
                out,
                "    Sign-on display file  . . . . . . :   {}",
                if self.sgndspf.1.is_empty() {
                    self.sgndspf.0.clone()
                } else {
                    format!("{}/{}", self.sgndspf.1, self.sgndspf.0)
                }
            );
            let _ = writeln!(
                out,
                "    Subsystem library . . . . . . . . :   {}",
                self.syslible
            );
            let _ = writeln!(
                out,
                "    Text  . . . . . . . . . . . . . . :   {}",
                self.text
            );
        }
        if want(2) {
            let _ = writeln!(
                out,
                " 2. Pool definitions\n    Pool ID   Storage Size (K)   Activity Level"
            );
            for p in &self.pools {
                let what = match p.kind {
                    PoolKind::Private => format!("{:<18} {}", p.size_kb, p.actlvl),
                    k => k.name(),
                };
                let _ = writeln!(out, "    {:>4}      {what}", p.id);
            }
        }
        if want(3) {
            let _ = writeln!(
                out,
                " 3. Autostart job entries\n    Job         Job Description   Library"
            );
            for a in &self.ajes {
                let _ = writeln!(out, "    {:<10}  {:<16}  {}", a.job, a.jobd.0, a.jobd.1);
            }
        }
        for (n, title, list) in [
            (4, "Work station name entries", &self.wsns),
            (5, "Work station type entries", &self.wsts),
        ] {
            if want(n) {
                let _ = writeln!(
                    out,
                    " {n}. {title}\n    Name        Job Description   Max Active  At"
                );
                for w in list {
                    let _ = writeln!(
                        out,
                        "    {:<10}  {:<16}  {:<10}  {}",
                        w.name,
                        if w.jobd.1.is_empty() {
                            w.jobd.0.clone()
                        } else {
                            format!("{}/{}", w.jobd.1, w.jobd.0)
                        },
                        nomax(u64::from(w.maxact)),
                        if w.at_signon { "*SIGNON" } else { "*ENTER" }
                    );
                }
            }
        }
        if want(6) {
            let _ = writeln!(
                out,
                " 6. Job queue entries\n    Seq Nbr  Job Queue   Library     Max Active  Max by Priority 1-9"
            );
            for q in &self.jobqes {
                let ptys: Vec<String> = q
                    .maxpty
                    .iter()
                    .map(|p| if *p == 0 { "*N".into() } else { p.to_string() })
                    .collect();
                let _ = writeln!(
                    out,
                    "    {:>5}    {:<10}  {:<10}  {:<10}  {}",
                    q.seq,
                    q.jobq.0,
                    q.jobq.1,
                    nomax(u64::from(q.maxact)),
                    ptys.join(" ")
                );
            }
        }
        if want(7) {
            let _ = writeln!(
                out,
                " 7. Routing entries\n    Seq Nbr  Program     Library     Compare Value       Start  Class       Pool"
            );
            for t in &self.rtges {
                let _ = writeln!(
                    out,
                    "    {:>5}    {:<10}  {:<10}  {:<18}  {:>5}  {:<10}  {}",
                    t.seq,
                    t.pgm.0,
                    t.pgm.1,
                    t.cmpval,
                    t.start,
                    format!("{}/{}", t.cls.1, t.cls.0),
                    t.pool
                );
            }
        }
        if want(8) {
            let _ = writeln!(
                out,
                " 8. Communications entries\n    Device      Mode        Job Description   Max Active"
            );
            for c in &self.cmnes {
                let _ = writeln!(
                    out,
                    "    {:<10}  {:<10}  {:<16}  {}",
                    c.dev,
                    c.mode,
                    c.jobd,
                    nomax(u64::from(c.maxact))
                );
            }
        }
        if want(9) {
            let _ = writeln!(out, " 9. Remote location name entries\n    (none)");
        }
        if want(10) {
            let _ = writeln!(
                out,
                "10. Prestart job entries\n    Program     Library     User        Initial  Threshold  Additional  Maximum"
            );
            for p in &self.pjes {
                let _ = writeln!(
                    out,
                    "    {:<10}  {:<10}  {:<10}  {:>7}  {:>9}  {:>10}  {}",
                    p.pgm.0,
                    p.pgm.1,
                    p.user,
                    p.inljobs,
                    p.threshold,
                    p.adljobs,
                    nomax(u64::from(p.maxjobs))
                );
            }
        }
        out
    }
}

/// The shipped subsystem descriptions, all in `QSYS`, with IBM's names
/// and entry shapes.
#[must_use]
#[allow(clippy::too_many_lines)] // nine descriptions
pub fn shipped() -> Vec<(&'static str, Sbsd)> {
    let q = |n: &str, l: &str| (n.to_string(), l.to_string());
    let pool = |id, kind| Pool {
        id,
        kind,
        size_kb: 0,
        actlvl: 0,
    };
    let rt = |seq, cmp: &str, cls: (String, String), pool| Rtge {
        seq,
        cmpval: cmp.into(),
        start: 1,
        pgm: q("QCMD", "QSYS"),
        cls,
        maxact: 0,
        pool,
    };
    let jq = |name: &str, lib: &str, seq, maxact| Jobqe {
        jobq: q(name, lib),
        seq,
        maxact,
        maxpty: [0; 9],
    };
    let console = || Wse {
        name: "QSCONSOLE".into(),
        jobd: q("QCTL", "QSYS"),
        maxact: 0,
        at_signon: true,
    };
    let all_types = || Wse {
        name: "*ALL".into(),
        jobd: q("*USRPRF", ""),
        maxact: 0,
        at_signon: true,
    };
    // The startup job: routing data QSTRUP runs QWCSTRUP, which calls
    // the program QSTRUPPGM names.
    let strup = || Rtge {
        pgm: q("QWCSTRUP", "QSYS"),
        ..rt(5, "QSTRUP", q("QSYSCLS25", "QSYS"), 1)
    };
    let strupjd = || Aje {
        job: "QSTRUPJD".into(),
        jobd: q("QSTRUPJD", "QSYS"),
    };
    vec![
        (
            "QBASE",
            Sbsd {
                pools: vec![pool(1, PoolKind::Base), pool(2, PoolKind::Interact)],
                text: "Interactive, batch and spooling subsystem".into(),
                rtges: vec![
                    strup(),
                    rt(10, "QCMDB", q("QBATCH", "QGPL"), 1),
                    rt(20, "QCMDI", q("QINTER", "QGPL"), 2),
                    rt(9999, "*ANY", q("QBATCH", "QGPL"), 1),
                ],
                wsns: vec![console()],
                wsts: vec![all_types()],
                jobqes: vec![jq("QBASE", "QGPL", 10, 0)],
                ajes: vec![strupjd()],
                ..Sbsd::default()
            },
        ),
        (
            "QCTL",
            Sbsd {
                text: "Controlling subsystem".into(),
                ajes: vec![strupjd()],
                rtges: vec![
                    strup(),
                    rt(10, "QCMDI", q("QCTL", "QSYS"), 1),
                    rt(20, "QCMDB", q("QCTL", "QSYS"), 1),
                    rt(9999, "*ANY", q("QCTL", "QSYS"), 1),
                ],
                wsns: vec![console()],
                jobqes: vec![jq("QCTL", "QSYS", 10, 0)],
                ..Sbsd::default()
            },
        ),
        (
            "QINTER",
            Sbsd {
                pools: vec![pool(1, PoolKind::Base), pool(2, PoolKind::Interact)],
                text: "Interactive subsystem".into(),
                rtges: vec![
                    rt(10, "QCMDI", q("QINTER", "QGPL"), 2),
                    rt(9999, "*ANY", q("QINTER", "QGPL"), 2),
                ],
                wsts: vec![all_types()],
                jobqes: vec![jq("QINTER", "QGPL", 10, 0)],
                ..Sbsd::default()
            },
        ),
        (
            "QBATCH",
            Sbsd {
                text: "Batch subsystem".into(),
                rtges: vec![
                    rt(10, "QCMDB", q("QBATCH", "QGPL"), 1),
                    rt(9999, "*ANY", q("QBATCH", "QGPL"), 1),
                ],
                jobqes: vec![jq("QBATCH", "QGPL", 10, 1)],
                ..Sbsd::default()
            },
        ),
        (
            "QSPL",
            Sbsd {
                pools: vec![pool(1, PoolKind::Base), pool(2, PoolKind::Spool)],
                text: "Spooling subsystem".into(),
                rtges: vec![
                    Rtge {
                        pgm: q("QSPWTR", "QSYS"),
                        ..rt(10, "PRTWTR", q("QSPL", "QSYS"), 2)
                    },
                    rt(9999, "*ANY", q("QSPL", "QSYS"), 2),
                ],
                jobqes: vec![jq("QSPL", "QSYS", 10, 0)],
                ..Sbsd::default()
            },
        ),
        (
            "QSYSWRK",
            Sbsd {
                text: "System work subsystem".into(),
                rtges: vec![
                    Rtge {
                        pgm: q("QEJSCHED", "QSYS"),
                        ..rt(10, "QSYSSCD", q("QSYSCLS25", "QSYS"), 1)
                    },
                    rt(9999, "*ANY", q("QSYSCLS25", "QSYS"), 1),
                ],
                ajes: vec![Aje {
                    job: "QSYSSCD".into(),
                    jobd: q("QSYSSCD", "QSYS"),
                }],
                jobqes: vec![jq("QSYSNOMAX", "QSYS", 10, 0)],
                ..Sbsd::default()
            },
        ),
        (
            "QUSRWRK",
            Sbsd {
                text: "User work subsystem".into(),
                rtges: vec![rt(9999, "*ANY", q("QBATCH", "QGPL"), 1)],
                jobqes: vec![jq("QUSRNOMAX", "QSYS", 10, 0)],
                ..Sbsd::default()
            },
        ),
        (
            "QSERVER",
            Sbsd {
                text: "File server subsystem".into(),
                ..Sbsd::default()
            },
        ),
        (
            "QCMN",
            Sbsd {
                text: "Communications subsystem".into(),
                ..Sbsd::default()
            },
        ),
        (
            "QHTTPSVR",
            Sbsd {
                text: "HTTP server subsystem".into(),
                ..Sbsd::default()
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rtge(seq: u16, cmp: &str, pool: u8) -> Rtge {
        Rtge {
            seq,
            cmpval: cmp.into(),
            start: 1,
            pgm: ("QCMD".into(), "QSYS".into()),
            cls: ("QBATCH".into(), "QGPL".into()),
            maxact: 0,
            pool,
        }
    }

    #[test]
    fn round_trip_and_route() {
        let s = Sbsd {
            pools: vec![
                Pool {
                    id: 1,
                    kind: PoolKind::Base,
                    size_kb: 0,
                    actlvl: 0,
                },
                Pool {
                    id: 2,
                    kind: PoolKind::Shared(3),
                    size_kb: 0,
                    actlvl: 0,
                },
                Pool {
                    id: 3,
                    kind: PoolKind::Private,
                    size_kb: 2048,
                    actlvl: 4,
                },
            ],
            maxjobs: 50,
            text: "Test subsystem".into(),
            rtges: vec![
                rtge(9999, "*ANY", 1),
                rtge(10, "QCMDB", 2),
                rtge(20, "PAY", 3),
            ],
            wsns: vec![Wse {
                name: "QSCONSOLE".into(),
                jobd: ("QCTL".into(), "QSYS".into()),
                maxact: 0,
                at_signon: true,
            }],
            wsts: vec![Wse {
                name: "*ALL".into(),
                jobd: ("*USRPRF".into(), String::new()),
                maxact: 0,
                at_signon: true,
            }],
            jobqes: vec![Jobqe {
                jobq: ("QBATCH".into(), "QGPL".into()),
                seq: 10,
                maxact: 1,
                maxpty: [0; 9],
            }],
            ajes: vec![Aje {
                job: "QSYSSCD".into(),
                jobd: ("QSYSJOBD".into(), "QSYS".into()),
            }],
            pjes: vec![Pje {
                pgm: ("QZSOSIGN".into(), "QSYS".into()),
                user: "QUSER".into(),
                job: "QZSOSIGN".into(),
                jobd: ("QDFTJOBD".into(), "QGPL".into()),
                cls: ("QBATCH".into(), "QGPL".into()),
                strjobs: true,
                wait: true,
                pool: 1,
                inljobs: 2,
                threshold: 1,
                adljobs: 2,
                maxjobs: 0,
                maxuse: 200,
            }],
            cmnes: vec![Cmne {
                dev: "*ALL".into(),
                mode: "QSPWTR".into(),
                jobd: "*USRPRF".into(),
                maxact: 0,
            }],
            ..Sbsd::default()
        };
        let back = Sbsd::decode(&s.encode()).unwrap();
        assert_eq!(back, s);
        assert_eq!(s.route("QCMDB").unwrap().seq, 10);
        assert_eq!(s.route("PAYROLL").unwrap().pool, 3);
        assert_eq!(s.route("OTHER").unwrap().seq, 9999);
        assert_eq!(PoolKind::parse("*SHRPOOL60"), Some(PoolKind::Shared(60)));
        assert_eq!(PoolKind::parse("*SHRPOOL61"), None);
        let shown = s.display("QSYS", "TEST", None);
        assert!(
            shown.contains("QSCONSOLE")
                && shown.contains("*SHRPOOL3")
                && shown.contains("QZSOSIGN")
        );
    }
}
