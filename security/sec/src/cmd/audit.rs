// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The auditing commands: `CHGSECAUD`, `DSPSECAUD`, `CHGOBJAUD`,
//! `CHGUSRAUD`, `DSPAUDJRNE`, `CPYAUDJRNE`; and the security reports
//! whose data is the store's alone (`PRTADPOBJ`, `PRTPVTAUT`,
//! `PRTPUBAUT`, `PRTSYSSECA`, `PRTUSROBJ`). Reports come back as lines
//! for the caller to print.

use std::fmt::Write as _;

use qsname::Name;
use store::obj::header::{ObjectHeader, text, text_of};
use store::obj::{ObjectId, auth, ops, types};
use store::txn::Txn;

use crate::audit::layout;
use crate::audit::write::{self as awrite, JOURNAL};
use crate::audit::{Event, Kind};
use crate::auth::bits::Auth;
use crate::cmd::{Env, Keywords, bad_value, civil};
use crate::profile::index::{self, Kind as Idx};
use crate::profile::objects;
use crate::profile::spcaut::SpecialAuth;
use crate::{Result, SecError};

/// IBM's default action auditing set, `QAUDLVL(*DFTSET)`.
pub const DFTSET: &[&str] = &["*AUTFAIL", "*CREATE", "*DELETE", "*SECURITY", "*SAVRST"];

/// The receiver `CHGSECAUD` makes first.
pub const FIRST_RECEIVER: &str = "AUDRCV0001";

#[allow(clippy::needless_pass_by_value)] // used as map_err(jrn_err)
fn jrn_err(e: jrn::JrnError) -> SecError {
    SecError::Corrupt(e.to_string())
}

/// `CHGSECAUD QAUDCTL() QAUDLVL() INLJRNRCV()`: the audit journal and
/// its first receiver when missing; the system values to set.
pub fn chgsecaud(
    txn: &mut Txn<'_>,
    env: &Env<'_>,
    k: &Keywords,
) -> Result<(String, Vec<(String, String)>)> {
    env.actor.require(
        SpecialAuth::ALLOBJ.union(SpecialAuth::AUDIT),
        "change security auditing",
    )?;
    let mut out = String::new();
    if awrite::journal(txn, env.qsys)?.is_none() {
        let rcv_name = k.upper("INLJRNRCV").map_or_else(
            || FIRST_RECEIVER.to_string(),
            |v| crate::cmd::qualified(&v).0,
        );
        let rcv_name = if rcv_name == "*GEN" {
            FIRST_RECEIVER.to_string()
        } else {
            rcv_name
        };
        let rn = Name::parse(&rcv_name).map_err(|_| bad_value("INLJRNRCV", &rcv_name))?;
        let rcv = jrn::rcv::crtjrnrcv(
            txn,
            env.qsys,
            rn,
            &jrn::rcv::CrtJrnRcvOpts {
                text: "Audit journal receiver".into(),
                owner: env.actor.user.clone(),
                now: env.now,
                ..jrn::rcv::CrtJrnRcvOpts::default()
            },
        )
        .map_err(jrn_err)?;
        jrn::journal::crtjrn(
            txn,
            env.qsys,
            Name::parse(JOURNAL).expect("valid"),
            rcv,
            &jrn::CrtJrnOpts {
                text: "Security audit journal".into(),
                mngrcv_system: true,
                owner: env.actor.user.clone(),
                now: env.now,
                ..jrn::CrtJrnOpts::default()
            },
        )
        .map_err(jrn_err)?;
        let _ = writeln!(
            out,
            "Journal QAUDJRN created in QSYS with receiver {rcv_name}."
        );
    }
    let mut changes = Vec::new();
    match k.upper("QAUDCTL").as_deref() {
        None | Some("*SAME") => {}
        Some(v) => changes.push(("QAUDCTL".to_string(), v.to_string())),
    }
    match k.upper("QAUDLVL").as_deref() {
        None | Some("*SAME") => {}
        Some("*DFTSET") => changes.push(("QAUDLVL".to_string(), DFTSET.join(" "))),
        Some(v) => changes.push(("QAUDLVL".to_string(), v.to_string())),
    }
    Ok((out, changes))
}

/// `DSPSECAUD`: the auditing values and the journal's state.
pub fn dspsecaud(txn: &mut Txn<'_>, env: &Env<'_>) -> Result<String> {
    let mut o = String::from(" Security auditing\n");
    let (jrn_state, rcv) = match awrite::journal(txn, env.qsys)? {
        Some(id) => {
            let j = jrn::Journal::load(txn, id).map_err(jrn_err)?;
            let r = j
                .attached(txn)
                .map_or_else(|_| "*NONE".to_string(), |r| r.name().to_string());
            ("QSYS/QAUDJRN".to_string(), r)
        }
        None => ("*NONE".to_string(), String::new()),
    };
    let _ = writeln!(o, " Security journal  . . . . . . . . . :   {jrn_state}");
    if !rcv.is_empty() {
        let _ = writeln!(o, " Current receiver  . . . . . . . . . :   {rcv}");
    }
    for n in [
        "QAUDCTL",
        "QAUDLVL",
        "QAUDLVL2",
        "QAUDENDACN",
        "QAUDFRCLVL",
        "QCRTOBJAUD",
    ] {
        let _ = writeln!(
            o,
            " {n:<10}  . . . . . . . . . . . . :   {}",
            (env.sysval)(n)
        );
    }
    Ok(o)
}

/// `CHGOBJAUD OBJAUD()` on object `id`.
pub fn chgobjaud(txn: &mut Txn<'_>, env: &Env<'_>, id: ObjectId, value: &str) -> Result<String> {
    env.actor
        .require(SpecialAuth::AUDIT, "change object auditing")?;
    let v = value.to_ascii_uppercase();
    if !["*NONE", "*USRPRF", "*CHANGE", "*ALL"].contains(&v.as_str()) {
        return Err(bad_value("OBJAUD", value));
    }
    let (page, mut h) = auth::load(txn, id)?;
    let old = text_of(&h.audit);
    h.audit = text(&v)?;
    ops::write_header(txn, page, &h)?;
    let e = crate::guard::object_event(txn, Kind::Ad, &h)
        .set("ETYP", "O")
        .set(
            "OVAL",
            if old.is_empty() {
                "*NONE".to_string()
            } else {
                old
            },
        )
        .set("NVAL", v.as_str())
        .set("CMDN", "CHGOBJAUD");
    crate::audit::record(e);
    Ok(format!("Object auditing for {} changed to {v}.", h.name))
}

/// `CHGUSRAUD USRPRF() OBJAUD() AUDLVL()`.
pub fn chgusraud(txn: &mut Txn<'_>, env: &Env<'_>, k: &Keywords) -> Result<String> {
    env.actor
        .require(SpecialAuth::AUDIT, "change user auditing")?;
    let users = k
        .list("USRPRF")
        .ok_or_else(|| SecError::msg("CPD0071", "USRPRF is required."))?;
    for u in &users {
        let (id, mut p) = objects::read(txn, env.qsys, u)?;
        let old = p.objaud.clone();
        if let Some(v) = k.upper("OBJAUD") {
            if !["*SAME", "*NONE", "*CHANGE", "*ALL"].contains(&v.as_str()) {
                return Err(bad_value("OBJAUD", &v));
            }
            if v != "*SAME" {
                p.objaud = v;
            }
        }
        if let Some(l) = k.list("AUDLVL")
            && l != ["*SAME"]
        {
            p.audlvl = l;
        }
        objects::write(txn, id, &p)?;
        crate::audit::record(
            Event::new(Kind::Ad)
                .set("ETYP", "U")
                .set("USRP", p.name.as_str())
                .set("OVAL", old)
                .set("NVAL", p.objaud.as_str())
                .set("CMDN", "CHGUSRAUD")
                .set(
                    "ALVL",
                    p.audlvl.iter().fold(String::new(), |mut acc, v| {
                        let _ = write!(acc, "{v:<10}");
                        acc
                    }),
                ),
        );
    }
    crate::auth::cache::bump_epoch();
    Ok(format!(
        "Auditing changed for {} user profile(s).",
        users.len()
    ))
}

/// What `DSPAUDJRNE` and `CPYAUDJRNE` select.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// `ENTTYP`: the types, empty for every implemented one.
    pub types: Vec<Kind>,
    /// `USRPRF`, `None` for `*ALL`.
    pub user: Option<String>,
    /// `FROMTIME`, Unix seconds.
    pub from: Option<u64>,
    /// `TOTIME`, Unix seconds.
    pub to: Option<u64>,
}

impl Selection {
    /// From `ENTTYP`, `USRPRF`, `FROMTIME`, `TOTIME` (dates `YYYY-MM-DD`).
    pub fn from_keywords(k: &Keywords) -> Result<Self> {
        let mut s = Self::default();
        if let Some(t) = k.list("ENTTYP")
            && t != ["*ALL"]
        {
            for c in t {
                s.types
                    .push(Kind::by_code(&c).ok_or_else(|| bad_value("ENTTYP", &c))?);
            }
        }
        s.user = k.upper("USRPRF").filter(|u| u != "*ALL");
        if let Some(d) = k.get("FROMTIME") {
            s.from = Some(crate::cmd::parse_date("FROMTIME", &d)?);
        }
        if let Some(d) = k.get("TOTIME") {
            s.to = Some(crate::cmd::parse_date("TOTIME", &d)? + 86_399);
        }
        Ok(s)
    }

    fn filter(&self) -> jrn::Filter {
        jrn::Filter {
            codes: vec![jrn::code::AUDIT],
            types: self
                .types
                .iter()
                .map(|k| {
                    let c = k.code().as_bytes();
                    [c[0], c[1]]
                })
                .collect(),
            from_time: self.from,
            to_time: self.to,
            ..jrn::Filter::default()
        }
    }
}

/// The entries a selection takes.
pub fn selected(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    sel: &Selection,
) -> Result<Vec<(jrn::Entry, Event)>> {
    let all = awrite::entries(txn, qsys, &sel.filter())?;
    Ok(all
        .into_iter()
        .filter(|(e, _)| sel.user.as_ref().is_none_or(|u| e.job.user == *u))
        .collect())
}

fn stamp(e: &jrn::Entry) -> String {
    let s = e.time % 86_400;
    format!(
        "{} {:02}:{:02}:{:02}.{:06}",
        civil(e.time),
        s / 3600,
        s / 60 % 60,
        s % 60,
        e.micros
    )
}

/// `DSPAUDJRNE`: per type a heading and a line per entry, the columns
/// the type's fields.
pub fn dspaudjrne(txn: &mut Txn<'_>, env: &Env<'_>, sel: &Selection) -> Result<Vec<String>> {
    env.actor
        .require(SpecialAuth::AUDIT, "display audit entries")?;
    let entries = selected(txn, env.qsys, sel)?;
    let mut kinds: Vec<Kind> = entries.iter().map(|(_, ev)| ev.kind).collect();
    kinds.sort();
    kinds.dedup();
    let mut out = vec![" Display Audit Journal Entries".to_string(), String::new()];
    for k in kinds {
        let Some(l) = layout::of(k) else {
            continue;
        };
        out.push(format!(" Entry type {}", k.code()));
        let mut head = format!(
            " {:<10} {:<26} {:<10} {:<10} {:<6}",
            "SEQNBR", "TIMESTAMP", "JOB", "USER", "NUMBER"
        );
        for f in l.fields {
            let w = f.size.unwrap_or(10).clamp(4, 24);
            let _ = write!(head, " {:<w$}", &f.name[2..]);
        }
        out.push(head);
        for (e, ev) in entries.iter().filter(|(_, ev)| ev.kind == k) {
            let mut row = format!(
                " {:<10} {:<26} {:<10} {:<10} {:06}",
                e.seq,
                stamp(e),
                e.job.name,
                e.job.user,
                e.job.number
            );
            for f in l.fields {
                let w = f.size.unwrap_or(10).clamp(4, 24);
                let v: String = ev.get(&f.name[2..]).chars().take(w).collect();
                let _ = write!(row, " {v:<w$}");
            }
            out.push(row);
        }
        out.push(String::new());
    }
    if out.len() == 2 {
        out.push(" No audit entries were selected.".into());
    }
    Ok(out)
}

/// The heading fields of an output file record, before the type's own:
/// IBM's `*TYPE5` heading names with the type's prefix.
pub const HEADING: &[(&str, u32)] = &[
    ("ENTL", 5),
    ("SEQN", 10),
    ("CODE", 1),
    ("ENTT", 2),
    ("TSTP", 26),
    ("JOB", 10),
    ("USER", 10),
    ("NBR", 6),
    ("PGM", 10),
    ("USPF", 10),
    ("SYNM", 8),
];

/// The fields of output file `QAUDITxx`: the heading, then the type's.
#[must_use]
pub fn outfile_fields(k: Kind) -> Vec<(String, u32)> {
    let code = k.code();
    let mut v: Vec<(String, u32)> = HEADING
        .iter()
        .map(|(n, l)| (format!("{code}{n}"), *l))
        .collect();
    if let Some(l) = layout::of(k) {
        for f in l.fields {
            v.push((
                f.name.to_string(),
                u32::try_from(f.size.unwrap_or(0)).unwrap_or(0),
            ));
        }
    }
    v
}

/// One output file record as text, every field fixed width.
#[must_use]
pub fn outfile_record(e: &jrn::Entry, ev: &Event, system_name: &str) -> String {
    let code = ev.kind.code();
    let data = ev.encode();
    let mut r = String::new();
    let mut put = |v: &str, n: usize| {
        let s: String = v.chars().take(n).collect();
        let _ = write!(r, "{s:<n$}");
    };
    put(&format!("{:05}", data.len() + 104), 5);
    put(&format!("{:010}", e.seq), 10);
    put("T", 1);
    put(code, 2);
    put(&stamp(e), 26);
    put(&e.job.name, 10);
    put(&e.job.user, 10);
    put(&format!("{:06}", e.job.number), 6);
    put(&e.program, 10);
    put(&e.job.user, 10);
    put(system_name, 8);
    if let Some(l) = layout::of(ev.kind) {
        for f in l.fields {
            put(ev.get(&f.name[2..]), f.size.unwrap_or(0));
        }
    }
    r
}

/// The output file name for type `k`: `prefix` and the type's code.
#[must_use]
pub fn outfile_name(prefix: &str, k: Kind) -> String {
    let p: String = prefix.chars().take(8).collect();
    format!("{p}{}", k.code())
}

/// `PRTADPOBJ USRPRF()`: the programs a user owns that adopt.
pub fn prtadpobj(txn: &mut Txn<'_>, env: &Env<'_>, user: &str) -> Result<Vec<String>> {
    env.actor.require(
        SpecialAuth::ALLOBJ.union(SpecialAuth::AUDIT),
        "print adopting objects",
    )?;
    let mut out = vec![
        format!(" Adopting objects owned by {user}"),
        format!(
            " {:<10} {:<10} {:<8} {}",
            "OBJECT", "LIBRARY", "TYPE", "PUBLIC"
        ),
    ];
    for (id, _) in index::list(txn, env.qsys, user, Idx::Owned)? {
        let h = ops::read_header(txn, id)?.1;
        if h.flags & ObjectHeader::ADOPTS != 0 {
            out.push(object_row(txn, &h)?);
        }
    }
    Ok(out)
}

fn type_name(h: &ObjectHeader) -> &'static str {
    types::type_by_code(h.id.type_code).map_or("*UNKNOWN", |t| t.name)
}

fn object_row(txn: &mut Txn<'_>, h: &ObjectHeader) -> Result<String> {
    let lib = if h.id == h.library || h.id.type_code == types::LIB {
        "QSYS".to_string()
    } else {
        ops::read_header(txn, h.library)?.1.name.to_string()
    };
    let public = if h.public_authority.0 & store::obj::header::Authority::FROM_AUTL != 0 {
        "*AUTL"
    } else {
        Auth::of(h.public_authority).display(h.id.type_code == types::AUTL)
    };
    Ok(format!(
        " {:<10} {:<10} {:<8} {public}",
        h.name,
        lib,
        type_name(h)
    ))
}

fn type_code(t: &str) -> Result<u16> {
    types::type_by_name(t)
        .map(|i| i.code)
        .ok_or_else(|| bad_value("OBJTYPE", t))
}

/// `PRTPVTAUT OBJTYPE()`: every private authority to objects of a type.
pub fn prtpvtaut(txn: &mut Txn<'_>, env: &Env<'_>, obj_type: &str) -> Result<Vec<String>> {
    env.actor.require(
        SpecialAuth::ALLOBJ.union(SpecialAuth::AUDIT),
        "print private authorities",
    )?;
    let code = type_code(obj_type)?;
    let mut out = vec![
        format!(" Private authorities, objects of type {obj_type}"),
        format!(
            " {:<10} {:<10} {:<10} {:<10} {}",
            "OBJECT", "LIBRARY", "OWNER", "USER", "AUTHORITY"
        ),
    ];
    for id in index::all_objects(txn, env.qsys)? {
        if id.type_code != code {
            continue;
        }
        let h = ops::read_header(txn, id)?.1;
        let lib = ops::read_header(txn, h.library)?.1.name.to_string();
        for pa in auth::private_list(txn, &h)? {
            out.push(format!(
                " {:<10} {:<10} {:<10} {:<10} {}",
                h.name,
                lib,
                text_of(&h.owner),
                text_of(&pa.profile),
                Auth::of(pa.bits).display(code == types::AUTL)
            ));
        }
    }
    Ok(out)
}

/// `PRTPUBAUT OBJTYPE()`: objects of a type `*PUBLIC` may use.
pub fn prtpubaut(txn: &mut Txn<'_>, env: &Env<'_>, obj_type: &str) -> Result<Vec<String>> {
    env.actor.require(
        SpecialAuth::ALLOBJ.union(SpecialAuth::AUDIT),
        "print public authorities",
    )?;
    let code = type_code(obj_type)?;
    let mut out = vec![
        format!(" Objects of type {obj_type} whose *PUBLIC is not *EXCLUDE"),
        format!(
            " {:<10} {:<10} {:<8} {}",
            "OBJECT", "LIBRARY", "TYPE", "PUBLIC"
        ),
    ];
    for id in index::all_objects(txn, env.qsys)? {
        if id.type_code != code {
            continue;
        }
        let h = ops::read_header(txn, id)?.1;
        if h.public_authority.0 != 0 {
            out.push(object_row(txn, &h)?);
        }
    }
    Ok(out)
}

/// `PRTUSROBJ USRPRF()`: what a user owns.
pub fn prtusrobj(txn: &mut Txn<'_>, env: &Env<'_>, user: &str) -> Result<Vec<String>> {
    env.actor.require(
        SpecialAuth::ALLOBJ.union(SpecialAuth::AUDIT),
        "print user objects",
    )?;
    let mut out = vec![
        format!(" Objects owned by {user}"),
        format!(
            " {:<10} {:<10} {:<8} {}",
            "OBJECT", "LIBRARY", "TYPE", "PUBLIC"
        ),
    ];
    for (id, _) in index::list(txn, env.qsys, user, Idx::Owned)? {
        let h = ops::read_header(txn, id)?.1;
        out.push(object_row(txn, &h)?);
    }
    Ok(out)
}

/// The security system values `PRTSYSSECA` lists, with Quartz's
/// recommended values.
pub const RECOMMENDED: &[(&str, &str)] = &[
    ("QALWUSRDMN", "QTEMP"),
    ("QAUDCTL", "*AUDLVL *OBJAUD"),
    ("QAUDENDACN", "*NOTIFY"),
    ("QAUDLVL", "*AUTFAIL *CREATE *DELETE *SECURITY *SAVRST"),
    ("QCRTAUT", "*EXCLUDE"),
    ("QCRTOBJAUD", "*NONE"),
    ("QDSPSGNINF", "1"),
    ("QINACTITV", "60"),
    ("QINACTMSGQ", "*ENDJOB"),
    ("QLMTDEVSSN", "1"),
    ("QLMTSECOFR", "1"),
    ("QMAXSGNACN", "3"),
    ("QMAXSIGN", "3"),
    ("QPWDEXPITV", "60"),
    ("QPWDLVL", "3"),
    ("QPWDMINLEN", "8"),
    ("QPWDRQDDIF", "1"),
    ("QPWDRQDDGT", "1"),
    ("QRETSVRSEC", "0"),
    ("QSECURITY", "50"),
    ("QUSEADPAUT", "*NONE"),
];

/// `PRTSYSSECA`: each security value, its value and the recommended one.
#[must_use]
pub fn prtsysseca(env: &Env<'_>) -> Vec<String> {
    let mut out = vec![
        " System security attributes".to_string(),
        format!(" {:<12} {:<40} {}", "VALUE", "CURRENT", "RECOMMENDED"),
    ];
    for (n, r) in RECOMMENDED {
        out.push(format!(" {:<12} {:<40} {r}", n, (env.sysval)(n)));
    }
    out
}
