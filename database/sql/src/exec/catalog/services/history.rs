// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSYS2.HISTORY_LOG_INFO(START_TIME, END_TIME, GENERATE_SYSLOG,
//! EOF_DELAY)`: the history log, read from its files `QSYS/QHSTnnnnnn`
//! (the layout `DSPLOG` reads) with the asking user's authority.
//! `GENERATE_SYSLOG` and `EOF_DELAY` are refused (SQL0443).

use db::odp::{Odp, OpenOpts};
use db::record::Value as Field;
use store::obj::{ops, types};
use store::txn::Txn;

use super::super::super::SqlContext;
use super::{Args, Out};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// IBM's message type names by the two-character code.
fn type_name(code: &str) -> &'static str {
    match code.trim() {
        "01" => "COMPLETION",
        "02" => "DIAGNOSTIC",
        "05" => "INQUIRY",
        "06" => "SENDER",
        "08" | "10" => "REQUEST",
        "14" => "NOTIFY",
        "15" | "17" => "ESCAPE",
        "21" | "22" | "23" | "24" | "25" | "26" => "REPLY",
        // 04 and anything unknown.
        _ => "INFORMATIONAL",
    }
}

/// `CYYMMDD` and `HHMMSS` as Unix seconds (UTC, as the log writes them).
fn when(date: &str, time: &str) -> Option<u64> {
    let d: u32 = date.trim().parse().ok()?;
    let t: u32 = time.trim().parse().ok()?;
    let year = 1900 + (d / 1_000_000) * 100 + d / 10_000 % 100;
    Some(secs(i64::from(year), d / 100 % 100, d % 100, t / 10_000, t / 100 % 100, t % 100))
}

/// Unix seconds of a UTC date and time (days from the civil calendar).
fn secs(y: i64, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> u64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + i64::from(hh) * 3600 + i64::from(mm) * 60 + i64::from(ss)).unwrap_or(0)
}

fn limit(v: &Value) -> Option<u64> {
    match v {
        Value::Timestamp(t) => Some(secs(
            i64::from(t.date.year),
            u32::from(t.date.month),
            u32::from(t.date.day),
            u32::from(t.time.hour),
            u32::from(t.time.minute),
            u32::from(t.time.second),
        )),
        _ => None,
    }
}

/// The rows.
pub fn rows(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, out: &mut Out) -> Result<()> {
    if !args.get("GENERATE_SYSLOG").is_null() || !args.get("EOF_DELAY").is_null() {
        return Err(SqlError::Coded { code: -443, state: "38501", what: "GENERATE_SYSLOG and EOF_DELAY are not supported".into() });
    }
    // The default start, CURRENT DATE - 1 DAY, when none is given.
    let start = limit(args.get("START_TIME")).unwrap_or_else(|| (ctx.clock() / 86_400).saturating_sub(1) * 86_400);
    let end = limit(args.get("END_TIME")).unwrap_or(u64::MAX);
    let mut files: Vec<(String, store::obj::ObjectId)> = ops::list(txn, ctx.qsys, Some(types::FILE))?
        .into_iter()
        .filter_map(|(k, id)| {
            let n = k.name.to_string();
            (n.len() == 10 && n.starts_with("QHST") && n[4..].bytes().all(|b| b.is_ascii_digit())).then_some((n, id))
        })
        .collect();
    files.sort();
    let mut n = 0;
    for (_, id) in files {
        let mut o = Odp::open(txn, ctx.locks.clone(), id, OpenOpts::input(ctx.job)).map_err(|e| match e {
            db::DbError::Store(store::StoreError::NotAuthorized { .. }) => {
                SqlError::Coded { code: -443, state: "42501", what: "not authorized to the history log".into() }
            }
            e => e.into(),
        })?;
        while let Some(r) = o.read(txn, false)? {
            let text = |f: &str| match o.field(&r, f) {
                Ok(Field::Char(b)) => ccsid::from_ebcdic(&b).trim_end_matches([' ', '\0']).to_string(),
                Ok(Field::Num(d)) => d.to_string(),
                _ => String::new(),
            };
            let Some(at) = when(&text("QHDATE"), &text("QHTIME")) else { continue };
            if at < start || at > end {
                continue;
            }
            n += 1;
            out.row();
            out.int("ORDINAL_POSITION", n);
            out.text_or_null("MESSAGE_ID", &text("QHMSGID"));
            out.text("MESSAGE_TYPE", type_name(&text("QHTYPE")));
            out.int("SEVERITY", text("QHSEV").parse::<i64>().unwrap_or(0));
            out.stamp("MESSAGE_TIMESTAMP", at);
            let (job, user, nbr) = (text("QHJOB"), text("QHUSER"), text("QHNBR"));
            out.text_or_null("FROM_USER", &user);
            if !job.is_empty() {
                out.text("FROM_JOB", format!("{nbr}/{user}/{job}"));
                out.text("FROM_JOB_NAME", job.as_str());
                out.text("FROM_JOB_USER", user.as_str());
                out.text("FROM_JOB_NUMBER", nbr.as_str());
            }
            out.text_or_null("MESSAGE_LIBRARY", &text("QHMSGL"));
            out.text_or_null("MESSAGE_FILE", &text("QHMSGF"));
            out.text("MESSAGE_TEXT", text("QHTEXT"));
        }
        o.close(txn);
    }
    Ok(())
}
