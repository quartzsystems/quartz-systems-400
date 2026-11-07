// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSSECCHK`: the running system against the security model of
//! `docs/SECURITY.md`. Each check gives pass, warning or failure: a
//! failure is an exposure, a warning a value weaker than Quartz
//! recommends. The exit point and TLS facts come from the caller (the
//! registration facility and the TLS policy are not this crate's).

use std::fmt::Write as _;

use qsname::Name;
use store::obj::header::{Authority, ObjectHeader, text_of};
use store::obj::{ops, types};
use store::txn::Txn;

use crate::Result;
use crate::cmd::{Env, civil};
use crate::password;
use crate::profile::spcaut::SpecialAuth;
use crate::profile::{index, objects, shipped};

/// A check's outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    /// As the model wants.
    Pass,
    /// Weaker than recommended.
    Warn,
    /// An exposure.
    Fail,
}

impl Status {
    /// The report's word.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Status::Pass => "PASS",
            Status::Warn => "WARN",
            Status::Fail => "FAIL",
        }
    }
}

/// One line of the report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// The area: `SYSVAL`, `PROFILE`, `PUBLIC`, `ADOPT`, `EXITPNT`,
    /// `AUDIT`, `TLS`.
    pub area: &'static str,
    /// What was checked.
    pub item: String,
    /// The outcome.
    pub status: Status,
    /// Why.
    pub detail: String,
}

fn f(
    area: &'static str,
    item: impl Into<String>,
    status: Status,
    detail: impl Into<String>,
) -> Finding {
    Finding {
        area,
        item: item.into(),
        status,
        detail: detail.into(),
    }
}

/// A registered exit program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExitReg {
    /// Exit point.
    pub point: String,
    /// Its format.
    pub format: String,
    /// Program library.
    pub lib: String,
    /// Program.
    pub pgm: String,
}

/// The TLS configuration as the policy reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TlsFacts {
    /// Protocols enabled (`*TLSV1.3` ...).
    pub enabled: Vec<String>,
    /// Protocols `QSSLPCL` lists that the engine never enables.
    pub refused: Vec<String>,
    /// Suites usable with the enabled protocols, by `QSSLCSL` name.
    pub suites: Vec<String>,
    /// `QSSLCSL` entries not recognized or not implemented.
    pub unmapped: Vec<String>,
}

/// What the caller supplies besides the store.
#[derive(Clone, Debug, Default)]
pub struct Inputs<'a> {
    /// Every registered exit program.
    pub exits: Vec<ExitReg>,
    /// The TLS policy.
    pub tls: TlsFacts,
    /// Commands shipped `*PUBLIC *EXCLUDE`.
    pub exclude_cmds: &'a [&'a str],
}

/// A value's judge.
type Judge = fn(&str) -> Status;

/// The security values checked one by one: name, Quartz's recommended
/// value, and the judge of a current value.
const VALUES: &[(&str, &str, Judge)] = &[
    ("QSECURITY", "50", |v| {
        match v.trim().parse::<u8>().unwrap_or(0) {
            0..=29 => Status::Fail,
            30..=39 => Status::Warn,
            _ => Status::Pass,
        }
    }),
    ("QALWUSRDMN", "QTEMP", |v| {
        warn_if(v.split_whitespace().any(|w| w == "*ALL"))
    }),
    ("QCRTAUT", "*EXCLUDE", |v| match v.trim() {
        "*ALL" => Status::Fail,
        "*CHANGE" => Status::Warn,
        _ => Status::Pass,
    }),
    ("QDSPSGNINF", "1", |v| warn_if(v.trim() == "0")),
    ("QINACTITV", "60", |v| warn_if(v.trim() == "*NONE")),
    ("QLMTDEVSSN", "1", |v| warn_if(v.trim() == "0")),
    ("QLMTSECOFR", "1", |v| warn_if(v.trim() == "0")),
    ("QMAXSIGN", "3", |v| match v.trim() {
        "*NOMAX" => Status::Fail,
        n => warn_if(n.parse::<u32>().unwrap_or(0) > 5),
    }),
    ("QPWDEXPITV", "60", |v| match v.trim() {
        "*NOMAX" => Status::Warn,
        n => warn_if(n.parse::<u32>().unwrap_or(0) > 90),
    }),
    ("QPWDLVL", "3", |v| {
        warn_if(v.trim().parse::<u8>().unwrap_or(0) < 2)
    }),
    ("QPWDMINLEN", "8", |v| {
        match v.trim().parse::<u32>().unwrap_or(0) {
            0..=5 => Status::Fail,
            6 | 7 => Status::Warn,
            _ => Status::Pass,
        }
    }),
    ("QPWDRQDDIF", "1", |v| warn_if(v.trim() == "0")),
    ("QPWDRQDDGT", "1", |v| warn_if(v.trim() == "0")),
    ("QRETSVRSEC", "0", |v| warn_if(v.trim() == "1")),
    ("QRMTSIGN", "*VERIFY", |v| warn_if(v.trim() == "*SAMEPRF")),
    ("QVFYOBJRST", "3", |v| warn_if(v.trim() == "1")),
];

fn warn_if(b: bool) -> Status {
    if b { Status::Warn } else { Status::Pass }
}

/// The audit levels the model wants when auditing is on.
const AUDIT_LEVELS: &[&str] = &["*AUTFAIL", "*SECURITY"];

/// Device, controller and line descriptions.
const CONFIG_TYPES: &[u16] = &[types::DEVD, 0x001C, 0x001D];

/// Rows past this many in one area are counted, not listed.
const MAX_ROWS: usize = 40;

/// Run every check. Needs `*ALLOBJ` and `*AUDIT`.
pub fn run(txn: &mut Txn<'_>, env: &Env<'_>, inputs: &Inputs<'_>) -> Result<Vec<Finding>> {
    env.actor.require(
        SpecialAuth::ALLOBJ.union(SpecialAuth::AUDIT),
        "check system security",
    )?;
    let mut out = sysvals(env);
    out.extend(profiles(txn, env)?);
    out.extend(public(txn, env, inputs.exclude_cmds)?);
    out.extend(adopting(txn, env)?);
    out.extend(exits(txn, env, &inputs.exits)?);
    out.extend(audit(txn, env)?);
    out.extend(tls(&inputs.tls));
    Ok(out)
}

/// The security system values.
#[must_use]
pub fn sysvals(env: &Env<'_>) -> Vec<Finding> {
    VALUES
        .iter()
        .map(|(n, rec, judge)| {
            let v = (env.sysval)(n);
            let s = judge(&v);
            let detail = if s == Status::Pass {
                v.trim().to_string()
            } else {
                format!("{}, recommended {rec}", v.trim())
            };
            f("SYSVAL", *n, s, detail)
        })
        .collect()
}

/// Profiles whose password is their name.
fn profiles(txn: &mut Txn<'_>, env: &Env<'_>) -> Result<Vec<Finding>> {
    let pwdlvl = super::usrprf::policy(env).pwdlvl;
    let names = objects::list(txn, env.qsys)?;
    let mut out = Vec::new();
    for n in &names {
        let (_, p) = objects::read(txn, env.qsys, n)?;
        if p.pwd_none || !password::matches(&p, n, pwdlvl) {
            continue;
        }
        let (s, why) = if p.disabled {
            (Status::Warn, "default password, profile disabled")
        } else if p.pwdexp {
            (
                Status::Warn,
                "default password, expired: the next sign-on sets it",
            )
        } else {
            (Status::Fail, "default password, sign-on allowed")
        };
        out.push(f("PROFILE", n.as_str(), s, why));
    }
    if out.is_empty() {
        out.push(f(
            "PROFILE",
            "*ALL",
            Status::Pass,
            format!("{} profiles, none with a default password", names.len()),
        ));
    }
    Ok(out)
}

fn type_name(code: u16) -> &'static str {
    types::type_by_code(code).map_or("*UNKNOWN", |t| t.name)
}

fn public_text(a: Authority) -> String {
    crate::auth::bits::Auth::of(a).display(false).to_string()
}

/// `*PUBLIC` of the objects in `QSYS` and of the system libraries
/// beyond what the system ships them with (`shipped::public_for`, and
/// `*EXCLUDE` for the commands `exclude_cmds` names).
fn public(txn: &mut Txn<'_>, env: &Env<'_>, exclude_cmds: &[&str]) -> Result<Vec<Finding>> {
    let lib = store::obj::library::Library::load(txn, env.qsys)?;
    let mut ids = vec![env.qsys];
    ids.extend(
        lib.list(txn, None)?
            .into_iter()
            .map(|e| e.1)
            .filter(|id| *id != env.qsys),
    );
    let mut out = Vec::new();
    let mut extra = 0;
    let mut checked = 0;
    for id in ids {
        let h = ops::read_header(txn, id)?.1;
        let name = h.name.to_string();
        if h.public_authority.0 & Authority::FROM_AUTL != 0 {
            continue;
        }
        if id.type_code == types::LIB && !shipped::SYSTEM_LIBRARIES.contains(&name.as_str()) {
            continue;
        }
        checked += 1;
        let want = if id.type_code == types::CMD && exclude_cmds.contains(&name.as_str()) {
            Authority::EXCLUDE
        } else if CONFIG_TYPES.contains(&id.type_code) {
            // Configuration objects (made by automatic configuration, as
            // IBM's) carry *CHANGE: using a device needs it.
            Authority::CHANGE
        } else {
            shipped::public_for(id.type_code, &name)
        };
        let have = h.public_authority.0 & Authority::ALL.0;
        if have & !want.0 == 0 {
            continue;
        }
        if out.len() < MAX_ROWS {
            out.push(f(
                "PUBLIC",
                format!("QSYS/{name} {}", type_name(id.type_code)),
                Status::Fail,
                format!(
                    "*PUBLIC {}, shipped {}",
                    public_text(h.public_authority),
                    public_text(want)
                ),
            ));
        } else {
            extra += 1;
        }
    }
    if extra > 0 {
        out.push(f(
            "PUBLIC",
            "...",
            Status::Fail,
            format!("{extra} more objects"),
        ));
    }
    if out.is_empty() {
        out.push(f(
            "PUBLIC",
            "QSYS",
            Status::Pass,
            format!("{checked} objects, none beyond the shipped *PUBLIC"),
        ));
    }
    Ok(out)
}

fn library_of(txn: &mut Txn<'_>, h: &ObjectHeader) -> Result<String> {
    Ok(if h.id == h.library || h.id.type_code == types::LIB {
        "QSYS".to_string()
    } else {
        ops::read_header(txn, h.library)?.1.name.to_string()
    })
}

/// Programs that adopt authority: one whose owner has `*ALLOBJ` must
/// not be usable by `*PUBLIC`, and none may be changed by it.
fn adopting(txn: &mut Txn<'_>, env: &Env<'_>) -> Result<Vec<Finding>> {
    let mut out = Vec::new();
    let mut n = 0;
    for id in index::all_objects(txn, env.qsys)? {
        if id.type_code != types::PGM && id.type_code != types::SRVPGM {
            continue;
        }
        let h = ops::read_header(txn, id)?.1;
        if h.flags & ObjectHeader::ADOPTS == 0 {
            continue;
        }
        n += 1;
        let owner = text_of(&h.owner);
        let powerful = objects::read(txn, env.qsys, &owner)
            .is_ok_and(|(_, p)| p.spcaut.has(SpecialAuth::ALLOBJ));
        let public = h.public_authority.0 & Authority::ALL.0;
        let changeable = public
            & (Authority::OBJMGT | Authority::OBJEXIST | Authority::OBJALTER | Authority::UPD)
            != 0;
        let s = if changeable {
            Status::Fail
        } else if powerful && public != 0 {
            Status::Warn
        } else {
            continue;
        };
        let lib = library_of(txn, &h)?;
        out.push(f(
            "ADOPT",
            format!("{lib}/{} {}", h.name, type_name(id.type_code)),
            s,
            format!(
                "adopts {owner}, *PUBLIC {}",
                public_text(h.public_authority)
            ),
        ));
    }
    if out.is_empty() {
        out.push(f(
            "ADOPT",
            "*ALL",
            Status::Pass,
            format!("{n} adopting programs, none open to *PUBLIC"),
        ));
    }
    Ok(out)
}

/// Each registered exit program must exist (a missing one can be
/// created by whoever may add to its library) and must not be changeable
/// by `*PUBLIC`.
fn exits(txn: &mut Txn<'_>, env: &Env<'_>, regs: &[ExitReg]) -> Result<Vec<Finding>> {
    let mut out = Vec::new();
    for r in regs {
        let item = format!("{} {}", r.point, r.format);
        let found = match (Name::parse(&r.lib), Name::parse(&r.pgm)) {
            (Ok(l), Ok(p)) => match ops::find_library(txn, env.qsys, l) {
                Ok(lib) => {
                    store::obj::library::Library::load(txn, lib.id)?.lookup(txn, types::PGM, p)?
                }
                Err(_) => None,
            },
            _ => None,
        };
        let Some(id) = found else {
            out.push(f(
                "EXITPNT",
                item,
                Status::Fail,
                format!("{}/{} is registered and does not exist", r.lib, r.pgm),
            ));
            continue;
        };
        let h = ops::read_header(txn, id)?.1;
        let public = h.public_authority.0 & Authority::ALL.0;
        let s = if public & !Authority::USE.0 != 0
            && h.public_authority.0 & Authority::FROM_AUTL == 0
        {
            Status::Fail
        } else {
            Status::Pass
        };
        out.push(f(
            "EXITPNT",
            item,
            s,
            format!(
                "{}/{}, *PUBLIC {}",
                r.lib,
                r.pgm,
                public_text(h.public_authority)
            ),
        ));
    }
    if out.is_empty() {
        out.push(f(
            "EXITPNT",
            "*ALL",
            Status::Pass,
            "no exit programs registered",
        ));
    }
    Ok(out)
}

/// `QAUDCTL`, `QAUDLVL` and the audit journal.
fn audit(txn: &mut Txn<'_>, env: &Env<'_>) -> Result<Vec<Finding>> {
    let ctl = (env.sysval)("QAUDCTL");
    let lvl = (env.sysval)("QAUDLVL");
    let words = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
    let ctl_w = words(&ctl);
    let lvl_w = words(&lvl);
    let journal = crate::audit::write::journal(txn, env.qsys)?.is_some();
    let mut out = Vec::new();
    if ctl_w.is_empty() || ctl_w.iter().all(|w| w == "*NONE") {
        out.push(f(
            "AUDIT",
            "QAUDCTL",
            Status::Warn,
            "*NONE: security auditing is off",
        ));
        return Ok(out);
    }
    out.push(f("AUDIT", "QAUDCTL", Status::Pass, ctl.trim()));
    out.push(if journal {
        f("AUDIT", "QAUDJRN", Status::Pass, "QSYS/QAUDJRN exists")
    } else {
        f(
            "AUDIT",
            "QAUDJRN",
            Status::Fail,
            "auditing is on and QSYS/QAUDJRN does not exist",
        )
    });
    if ctl_w.iter().any(|w| w == "*AUDLVL") {
        let missing: Vec<&str> = AUDIT_LEVELS
            .iter()
            .copied()
            .filter(|l| !lvl_w.iter().any(|w| w == l))
            .collect();
        out.push(if missing.is_empty() {
            f("AUDIT", "QAUDLVL", Status::Pass, lvl.trim())
        } else {
            f(
                "AUDIT",
                "QAUDLVL",
                Status::Warn,
                format!("{}, without {}", lvl.trim(), missing.join(" ")),
            )
        });
    } else {
        out.push(f(
            "AUDIT",
            "QAUDLVL",
            Status::Warn,
            "QAUDCTL has no *AUDLVL: action auditing is off",
        ));
    }
    Ok(out)
}

/// Whether a suite name is weak: no forward secrecy or an old cipher
/// or MAC.
fn weak_suite(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    ["RC4", "DES", "NULL", "EXPORT", "MD5"]
        .iter()
        .any(|w| n.contains(w))
        || (n.ends_with("_SHA") && !n.contains("GCM"))
}

/// `QSSLPCL`, `QSSLCSL` and `QSSLCSLCTL` as the TLS policy reads them.
#[must_use]
pub fn tls(t: &TlsFacts) -> Vec<Finding> {
    let mut out = Vec::new();
    let old: Vec<&str> = t
        .enabled
        .iter()
        .map(String::as_str)
        .filter(|p| !matches!(*p, "*TLSV1.2" | "*TLSV1.3"))
        .collect();
    out.push(if !old.is_empty() {
        f(
            "TLS",
            "QSSLPCL",
            Status::Fail,
            format!("enables {}", old.join(" ")),
        )
    } else if t.enabled.is_empty() {
        f(
            "TLS",
            "QSSLPCL",
            Status::Warn,
            "no protocol enabled: TLS servers cannot start",
        )
    } else {
        f("TLS", "QSSLPCL", Status::Pass, t.enabled.join(" "))
    });
    if !t.refused.is_empty() {
        out.push(f(
            "TLS",
            "QSSLPCL",
            Status::Warn,
            format!("lists {}, which are never enabled", t.refused.join(" ")),
        ));
    }
    let weak: Vec<&str> = t
        .suites
        .iter()
        .map(String::as_str)
        .filter(|s| weak_suite(s))
        .collect();
    out.push(if weak.is_empty() {
        f(
            "TLS",
            "QSSLCSL",
            Status::Pass,
            format!("{} suites, all AEAD with forward secrecy", t.suites.len()),
        )
    } else {
        f(
            "TLS",
            "QSSLCSL",
            Status::Fail,
            format!("weak suites usable: {}", weak.join(" ")),
        )
    });
    if !t.unmapped.is_empty() {
        out.push(f(
            "TLS",
            "QSSLCSL",
            Status::Warn,
            format!("not implemented, ignored: {}", t.unmapped.join(" ")),
        ));
    }
    out
}

/// How many of each status.
#[must_use]
pub fn counts(findings: &[Finding]) -> (usize, usize, usize) {
    let n = |s| findings.iter().filter(|x| x.status == s).count();
    (n(Status::Pass), n(Status::Warn), n(Status::Fail))
}

/// The printed report: a heading, the column titles, one line per
/// finding and the totals. `product` is the product and release
/// (`QS00SS1 V1R1M0`).
#[must_use]
pub fn report(findings: &[Finding], system: &str, product: &str, now: u64) -> Vec<String> {
    let secs = now % 86_400;
    let time = format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60);
    let mut out = vec![
        format!(
            " {product:<16}  System Security Check (QSSECCHK)  {system:<8} {} {time}",
            civil(now)
        ),
        String::new(),
        format!(" {:<8} {:<34} {:<6} {}", "Area", "Item", "Status", "Detail"),
    ];
    for x in findings {
        let mut l = String::new();
        let _ = write!(
            l,
            " {:<8} {:<34} {:<6} {}",
            x.area,
            x.item,
            x.status.word(),
            x.detail
        );
        out.push(l);
    }
    let (p, w, fl) = counts(findings);
    out.push(String::new());
    out.push(format!(
        " {} checks: {p} passed, {w} warnings, {fl} failures.",
        findings.len()
    ));
    out.push(String::new());
    out.push(" * * * * *   E N D   O F   L I S T I N G   * * * * *".to_string());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weak_suites_and_protocols() {
        assert!(weak_suite("*RSA_AES_128_CBC_SHA"));
        assert!(weak_suite("*RSA_RC4_128_SHA"));
        assert!(!weak_suite("*ECDHE_RSA_AES_128_GCM_SHA256"));
        assert!(!weak_suite("*AES_128_GCM_SHA256"));
        let t = TlsFacts {
            enabled: vec!["*TLSV1.3".into(), "*TLSV1".into()],
            ..TlsFacts::default()
        };
        assert_eq!(tls(&t)[0].status, Status::Fail);
    }

    #[test]
    fn value_judges() {
        let judge = |n: &str, v: &str| VALUES.iter().find(|x| x.0 == n).map(|x| (x.2)(v)).unwrap();
        assert_eq!(judge("QSECURITY", "20"), Status::Fail);
        assert_eq!(judge("QSECURITY", "40"), Status::Pass);
        assert_eq!(judge("QPWDMINLEN", "8"), Status::Pass);
        assert_eq!(judge("QPWDMINLEN", "4"), Status::Fail);
        assert_eq!(judge("QCRTAUT", "*CHANGE"), Status::Warn);
        assert_eq!(judge("QMAXSIGN", "*NOMAX"), Status::Fail);
        assert_eq!(judge("QPWDEXPITV", "90"), Status::Pass);
    }
}
