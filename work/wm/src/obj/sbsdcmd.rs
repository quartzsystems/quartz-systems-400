// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The commands that build and change a subsystem description, applied
//! to the description in memory: `CRTSBSD`, `CHGSBSD`, and the add,
//! change and remove commands of each kind of entry.

#![allow(clippy::many_single_char_names)] // s, p, e, r, n, v: description, params, entry

use super::params::qualified;
use super::sbsd::{Aje, Jobqe, Pje, Pool, PoolKind, Rtge, Sbsd, Wse};
use crate::cmd::Params;
use crate::{Error, Result};

fn bad(kw: &str, v: &str) -> Error {
    Error::value("CPD0084", format!("{kw}({v}) is not valid"))
}

fn num<T: std::str::FromStr>(kw: &str, v: &str) -> Result<T> {
    v.trim().parse().map_err(|_| bad(kw, v))
}

/// A limit: `*NOMAX` is 0.
fn limit<T: std::str::FromStr + Default>(kw: &str, v: &str) -> Result<T> {
    if v.trim().eq_ignore_ascii_case("*NOMAX") {
        Ok(T::default())
    } else {
        num(kw, v)
    }
}

/// `LIB/NAME` as `(name, lib)`; an unqualified name gets `dft`.
fn obj(v: &str, dft: &str) -> (String, String) {
    let (l, n) = qualified(v);
    (n, if l == "*LIBL" { dft.to_string() } else { l })
}

/// `POOLS((1 *BASE) (2 2048 5) ...)`.
pub fn pools(v: &str) -> Result<Vec<Pool>> {
    let mut out: Vec<Pool> = Vec::new();
    for item in Params::list(v) {
        let w: Vec<&str> = item.split_whitespace().collect();
        let id: u8 = match w.first() {
            Some(n) => num("POOLS", n)?,
            None => continue,
        };
        if !(1..=10).contains(&id) || out.iter().any(|p| p.id == id) {
            return Err(bad("POOLS", &item));
        }
        let pool = match w.as_slice() {
            [_, special] => Pool {
                id,
                kind: PoolKind::parse(special).ok_or_else(|| bad("POOLS", &item))?,
                size_kb: 0,
                actlvl: 0,
            },
            [_, size, act] => Pool {
                id,
                kind: PoolKind::Private,
                size_kb: num("POOLS", size)?,
                actlvl: num("POOLS", act)?,
            },
            _ => return Err(bad("POOLS", &item)),
        };
        out.push(pool);
    }
    if out.is_empty() {
        return Err(bad("POOLS", v));
    }
    Ok(out)
}

/// `CRTSBSD` (over the defaults) and `CHGSBSD` (over the description).
pub fn change(mut s: Sbsd, p: &Params) -> Result<Sbsd> {
    for (kw, v) in &p.kw {
        match kw.as_str() {
            "SBSD" => {}
            "POOLS" => s.pools = pools(v)?,
            "MAXJOBS" => s.maxjobs = limit(kw, v)?,
            "SGNDSPF" => s.sgndspf = obj(v, "*LIBL"),
            "SYSLIBLE" => s.syslible = v.trim().to_ascii_uppercase(),
            "TEXT" => s.text = p.text("TEXT"),
            _ => return Err(unknown(kw)),
        }
    }
    Ok(s)
}

fn unknown(kw: &str) -> Error {
    Error::value(
        "CPD0043",
        format!("keyword {kw} not valid for this command"),
    )
}

fn seq(p: &Params) -> Result<u16> {
    let v = p
        .kw("SEQNBR")
        .ok_or_else(|| Error::value("CPD0071", "SEQNBR is needed"))?;
    let n: u16 = num("SEQNBR", v)?;
    if !(1..=9999).contains(&n) {
        return Err(bad("SEQNBR", v));
    }
    Ok(n)
}

/// `ADDRTGE` / `CHGRTGE`.
pub fn rtge(s: &mut Sbsd, p: &Params, add: bool) -> Result<()> {
    let n = seq(p)?;
    let at = s.rtges.iter().position(|r| r.seq == n);
    let mut r = match (add, at) {
        (true, Some(_)) => {
            return Err(Error::value(
                "CPF1697",
                format!("routing entry {n} already exists"),
            ));
        }
        (true, None) => Rtge {
            seq: n,
            cmpval: "*ANY".into(),
            start: 1,
            pgm: ("QCMD".into(), "QSYS".into()),
            cls: ("*SBSD".into(), String::new()),
            maxact: 0,
            pool: 1,
        },
        (false, Some(i)) => s.rtges[i].clone(),
        (false, None) => return Err(missing("routing entry", n)),
    };
    for (kw, v) in &p.kw {
        match kw.as_str() {
            "SBSD" | "SEQNBR" => {}
            "CMPVAL" => {
                let w = Params::list(v);
                let first = w.first().cloned().unwrap_or_default();
                let (val, start) = match first.strip_prefix('\'') {
                    Some(q) => {
                        let (t, rest) = q.split_once('\'').unwrap_or((q, ""));
                        (t.to_string(), rest.trim().to_string())
                    }
                    None => (first.clone(), w.get(1).cloned().unwrap_or_default()),
                };
                r.cmpval = if val.eq_ignore_ascii_case("*ANY") {
                    "*ANY".into()
                } else {
                    val
                };
                r.start = if start.is_empty() {
                    1
                } else {
                    num(kw, &start)?
                };
            }
            "PGM" => r.pgm = obj(v, "*LIBL"),
            "CLS" => r.cls = obj(v, "*LIBL"),
            "MAXACT" => r.maxact = limit(kw, v)?,
            "POOLID" => r.pool = num(kw, v)?,
            _ => return Err(unknown(kw)),
        }
    }
    if s.pool(r.pool).is_none() {
        return Err(Error::value(
            "CPF1699",
            format!("pool {} is not defined in the subsystem", r.pool),
        ));
    }
    if let Some(i) = at {
        s.rtges[i] = r;
    } else {
        s.rtges.push(r);
    }
    s.rtges.sort_by_key(|r| r.seq);
    Ok(())
}

fn missing(what: &str, n: impl std::fmt::Display) -> Error {
    Error::value("CPF1698", format!("{what} {n} does not exist"))
}

/// `RMVRTGE`.
pub fn rmvrtge(s: &mut Sbsd, p: &Params) -> Result<()> {
    let n = seq(p)?;
    let before = s.rtges.len();
    s.rtges.retain(|r| r.seq != n);
    if before == s.rtges.len() {
        return Err(missing("routing entry", n));
    }
    Ok(())
}

/// `ADDWSE WRKSTN(name)|WRKSTNTYPE(type) [JOBD() MAXACT() AT()]`.
pub fn addwse(s: &mut Sbsd, p: &Params) -> Result<()> {
    let (by_name, name) = match (p.upper("WRKSTN"), p.upper("WRKSTNTYPE")) {
        (Some(n), None) => (true, n),
        (None, Some(t)) => (false, t),
        _ => {
            return Err(Error::value(
                "CPD0071",
                "WRKSTN or WRKSTNTYPE is needed, not both",
            ));
        }
    };
    let list = if by_name { &mut s.wsns } else { &mut s.wsts };
    if list.iter().any(|w| w.name == name) {
        return Err(Error::value(
            "CPF1697",
            format!("work station entry {name} already exists"),
        ));
    }
    let mut w = Wse {
        name,
        jobd: ("*USRPRF".into(), String::new()),
        maxact: 0,
        at_signon: true,
    };
    for (kw, v) in &p.kw {
        match kw.as_str() {
            "SBSD" | "WRKSTN" | "WRKSTNTYPE" => {}
            "JOBD" => {
                w.jobd = if v.trim().starts_with('*') {
                    (v.trim().to_ascii_uppercase(), String::new())
                } else {
                    obj(v, "*LIBL")
                };
            }
            "MAXACT" => w.maxact = limit(kw, v)?,
            "AT" => {
                w.at_signon = match v.trim().to_ascii_uppercase().as_str() {
                    "*SIGNON" => true,
                    "*ENTER" => false,
                    _ => return Err(bad(kw, v)),
                };
            }
            _ => return Err(unknown(kw)),
        }
    }
    list.push(w);
    Ok(())
}

/// `RMVWSE WRKSTN()|WRKSTNTYPE()`.
pub fn rmvwse(s: &mut Sbsd, p: &Params) -> Result<()> {
    let (list, name) = match (p.upper("WRKSTN"), p.upper("WRKSTNTYPE")) {
        (Some(n), None) => (&mut s.wsns, n),
        (None, Some(t)) => (&mut s.wsts, t),
        _ => return Err(Error::value("CPD0071", "WRKSTN or WRKSTNTYPE is needed")),
    };
    let before = list.len();
    list.retain(|w| w.name != name);
    if before == list.len() {
        return Err(missing("work station entry", name));
    }
    Ok(())
}

/// `ADDJOBQE` / `CHGJOBQE JOBQ() [SEQNBR() MAXACT() MAXPTYn()]`.
pub fn jobqe(s: &mut Sbsd, p: &Params, add: bool) -> Result<()> {
    let q = obj(
        p.kw("JOBQ")
            .ok_or_else(|| Error::value("CPD0071", "JOBQ is needed"))?,
        "*LIBL",
    );
    let at = s.jobqes.iter().position(|e| e.jobq.0 == q.0);
    let mut e = match (add, at) {
        (true, Some(_)) => {
            return Err(Error::value(
                "CPF1697",
                format!("job queue entry {} already exists", q.0),
            ));
        }
        (true, None) => Jobqe {
            jobq: q,
            seq: 10,
            maxact: 1,
            maxpty: [0; 9],
        },
        (false, Some(i)) => s.jobqes[i].clone(),
        (false, None) => return Err(missing("job queue entry", q.0)),
    };
    for (kw, v) in &p.kw {
        match kw.as_str() {
            "SBSD" | "JOBQ" => {}
            "SEQNBR" => e.seq = num(kw, v)?,
            "MAXACT" => e.maxact = limit(kw, v)?,
            k if k.starts_with("MAXPTY") => {
                let i: usize = k[6..].parse().map_err(|_| unknown(k))?;
                if !(1..=9).contains(&i) {
                    return Err(unknown(k));
                }
                e.maxpty[i - 1] = limit(kw, v)?;
            }
            _ => return Err(unknown(kw)),
        }
    }
    if s.jobqes
        .iter()
        .enumerate()
        .any(|(i, x)| Some(i) != at && x.seq == e.seq)
    {
        return Err(Error::value(
            "CPF1697",
            format!(
                "sequence number {} is used by another job queue entry",
                e.seq
            ),
        ));
    }
    if let Some(i) = at {
        s.jobqes[i] = e;
    } else {
        s.jobqes.push(e);
    }
    s.jobqes.sort_by_key(|e| e.seq);
    Ok(())
}

/// `RMVJOBQE JOBQ()`.
pub fn rmvjobqe(s: &mut Sbsd, p: &Params) -> Result<()> {
    let q = obj(
        p.kw("JOBQ")
            .ok_or_else(|| Error::value("CPD0071", "JOBQ is needed"))?,
        "*LIBL",
    );
    let before = s.jobqes.len();
    s.jobqes.retain(|e| e.jobq.0 != q.0);
    if before == s.jobqes.len() {
        return Err(missing("job queue entry", q.0));
    }
    Ok(())
}

/// `ADDAJE JOB() JOBD()`.
pub fn addaje(s: &mut Sbsd, p: &Params) -> Result<()> {
    let job = p
        .upper("JOB")
        .ok_or_else(|| Error::value("CPD0071", "JOB is needed"))?;
    if s.ajes.iter().any(|a| a.job == job) {
        return Err(Error::value(
            "CPF1697",
            format!("autostart job entry {job} already exists"),
        ));
    }
    let jobd = obj(
        p.kw("JOBD")
            .ok_or_else(|| Error::value("CPD0071", "JOBD is needed"))?,
        "*LIBL",
    );
    s.ajes.push(Aje { job, jobd });
    Ok(())
}

/// `RMVAJE JOB()`.
pub fn rmvaje(s: &mut Sbsd, p: &Params) -> Result<()> {
    let job = p
        .upper("JOB")
        .ok_or_else(|| Error::value("CPD0071", "JOB is needed"))?;
    let before = s.ajes.len();
    s.ajes.retain(|a| a.job != job);
    if before == s.ajes.len() {
        return Err(missing("autostart job entry", job));
    }
    Ok(())
}

/// `ADDPJE` / `CHGPJE PGM() [...]`.
pub fn pje(s: &mut Sbsd, p: &Params, add: bool) -> Result<()> {
    let pgm = obj(
        p.kw("PGM")
            .ok_or_else(|| Error::value("CPD0071", "PGM is needed"))?,
        "*LIBL",
    );
    let at = s.pjes.iter().position(|e| e.pgm.0 == pgm.0);
    let mut e = match (add, at) {
        (true, Some(_)) => {
            return Err(Error::value(
                "CPF1697",
                format!("prestart job entry {} already exists", pgm.0),
            ));
        }
        (true, None) => Pje {
            job: pgm.0.clone(),
            pgm,
            user: "QUSER".into(),
            jobd: ("*USRPRF".into(), String::new()),
            cls: ("QGPL".into(), "QBATCH".into()),
            strjobs: true,
            wait: true,
            pool: 1,
            inljobs: 3,
            threshold: 2,
            adljobs: 2,
            maxjobs: 0,
            maxuse: 200,
        },
        (false, Some(i)) => s.pjes[i].clone(),
        (false, None) => return Err(missing("prestart job entry", pgm.0)),
    };
    for (kw, v) in &p.kw {
        let yes = |v: &str| v.trim().eq_ignore_ascii_case("*YES");
        match kw.as_str() {
            "SBSD" | "PGM" => {}
            "USER" => e.user = v.trim().to_ascii_uppercase(),
            "JOB" => e.job = v.trim().to_ascii_uppercase(),
            "JOBD" => e.jobd = obj(v, "*LIBL"),
            "CLS" => e.cls = obj(v, "*LIBL"),
            "STRJOBS" => e.strjobs = yes(v),
            "WAIT" => e.wait = yes(v),
            "POOLID" => e.pool = num(kw, v)?,
            "INLJOBS" => e.inljobs = num(kw, v)?,
            "THRESHOLD" => e.threshold = num(kw, v)?,
            "ADLJOBS" => e.adljobs = num(kw, v)?,
            "MAXJOBS" => e.maxjobs = limit(kw, v)?,
            "MAXUSE" => e.maxuse = limit(kw, v)?,
            _ => return Err(unknown(kw)),
        }
    }
    if e.maxjobs != 0 && e.inljobs > e.maxjobs {
        return Err(Error::value("CPD1688", "INLJOBS is more than MAXJOBS"));
    }
    if let Some(i) = at {
        s.pjes[i] = e;
    } else {
        s.pjes.push(e);
    }
    Ok(())
}

/// `RMVPJE PGM()`.
pub fn rmvpje(s: &mut Sbsd, p: &Params) -> Result<()> {
    let pgm = obj(
        p.kw("PGM")
            .ok_or_else(|| Error::value("CPD0071", "PGM is needed"))?,
        "*LIBL",
    );
    let before = s.pjes.len();
    s.pjes.retain(|e| e.pgm.0 != pgm.0);
    if before == s.pjes.len() {
        return Err(missing("prestart job entry", pgm.0));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits() {
        let s = change(
            Sbsd::default(),
            &Params::parse("crtsbsd QGPL/T POOLS((1 *BASE) (2 4096 3)) MAXJOBS(20) TEXT('T')"),
        )
        .unwrap();
        assert_eq!(s.pools.len(), 2);
        assert_eq!(s.pools[1].kind, PoolKind::Private);
        let mut s = s;
        rtge(
            &mut s,
            &Params::parse("addrtge T SEQNBR(10) CMPVAL(PAY 1) POOLID(2)"),
            true,
        )
        .unwrap();
        assert!(rtge(&mut s, &Params::parse("addrtge T SEQNBR(10)"), true).is_err());
        assert!(
            rtge(
                &mut s,
                &Params::parse("addrtge T SEQNBR(20) POOLID(5)"),
                true
            )
            .is_err()
        );
        rtge(
            &mut s,
            &Params::parse("chgrtge T SEQNBR(10) MAXACT(3)"),
            false,
        )
        .unwrap();
        assert_eq!(s.rtges[0].maxact, 3);
        assert_eq!(s.route("PAYROLL").unwrap().pool, 2);
        jobqe(
            &mut s,
            &Params::parse("addjobqe T JOBQ(QGPL/QBATCH) MAXACT(2) MAXPTY1(1)"),
            true,
        )
        .unwrap();
        assert_eq!(s.jobqes[0].maxpty[0], 1);
        addwse(
            &mut s,
            &Params::parse("addwse T WRKSTNTYPE(*ALL) AT(*ENTER)"),
        )
        .unwrap();
        assert!(!s.wsts[0].at_signon);
        addaje(
            &mut s,
            &Params::parse("addaje T JOB(START) JOBD(QGPL/QDFTJOBD)"),
        )
        .unwrap();
        pje(
            &mut s,
            &Params::parse("addpje T PGM(QGPL/SRV) INLJOBS(1)"),
            true,
        )
        .unwrap();
        rmvpje(&mut s, &Params::parse("rmvpje T PGM(QGPL/SRV)")).unwrap();
        rmvaje(&mut s, &Params::parse("rmvaje T JOB(START)")).unwrap();
        rmvwse(&mut s, &Params::parse("rmvwse T WRKSTNTYPE(*ALL)")).unwrap();
        rmvjobqe(&mut s, &Params::parse("rmvjobqe T JOBQ(QBATCH)")).unwrap();
        rmvrtge(&mut s, &Params::parse("rmvrtge T SEQNBR(10)")).unwrap();
        assert!(rmvrtge(&mut s, &Params::parse("rmvrtge T SEQNBR(10)")).is_err());
    }
}
