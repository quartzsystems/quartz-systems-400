// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSYS2.JOBLOG_INFO(JOB_NAME, IGNORE_ERRORS, MESSAGE_ORDER,
//! MESSAGE_LIMIT, MESSAGE_TIMESTAMP)`: an active job's log as the
//! storage service keeps it (`DSPJOBLOG`'s). Message keys are the
//! messages' order in the log; tokens are not kept, only the text.

use store::txn::Txn;

use super::super::super::SqlContext;
use super::{Args, Out, ask};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// IBM's message type names.
fn type_name(kind: &str) -> String {
    match kind {
        "Information" => "INFORMATIONAL".into(),
        other => other.to_ascii_uppercase(),
    }
}

/// The rows.
pub fn rows(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, out: &mut Out) -> Result<()> {
    let job = args.word("JOB_NAME").unwrap_or_else(|| "*".into());
    let ignore = args.word("IGNORE_ERRORS").is_some_and(|v| v == "YES");
    let descending = args.word("MESSAGE_ORDER").is_some_and(|v| v == "DESCENDING");
    let limit = args.int("MESSAGE_LIMIT").and_then(|n| usize::try_from(n).ok());
    let since = match args.get("MESSAGE_TIMESTAMP") {
        Value::Timestamp(t) => Some(*t),
        _ => None,
    };
    let super::host::JobLog::Found(qualified, entries) = ask(txn, ctx, "the job log", |h| h.job_log(&job))? else {
        if ignore {
            return Ok(());
        }
        return Err(SqlError::Coded { code: -443, state: "38501", what: format!("job {job} is not active") });
    };
    let mut numbered: Vec<(usize, &wm::joblog::Entry)> = entries.iter().enumerate().map(|(i, e)| (i + 1, e)).collect();
    if descending {
        numbered.reverse();
    }
    let mut n = 0;
    for (key, e) in numbered {
        let (secs, micros) = (e.time / 1_000_000, u32::try_from(e.time % 1_000_000).unwrap_or(0));
        if let Some(t) = &since {
            let at = crate::value::Timestamp::new(
                db::record::datetime::Date::from_unix(secs),
                db::record::datetime::Time::from_unix(secs),
                u64::from(micros) * 1_000_000,
            );
            let cmp = Value::Timestamp(at).compare(&Value::Timestamp(*t))?;
            let before = cmp == Some(std::cmp::Ordering::Less);
            if before != descending && cmp != Some(std::cmp::Ordering::Equal) {
                continue;
            }
        }
        if limit.is_some_and(|l| n >= l) {
            break;
        }
        n += 1;
        out.row();
        out.int("ORDINAL_POSITION", n);
        out.text_or_null("MESSAGE_ID", &e.msgid);
        out.text("MESSAGE_TYPE", type_name(&e.kind));
        out.int("SEVERITY", e.severity);
        out.stamp_micros("MESSAGE_TIMESTAMP", secs, micros);
        out.text_or_null("FROM_LIBRARY", &e.from.1);
        out.text_or_null("FROM_PROGRAM", &e.from.0);
        out.text_or_null("FROM_INSTRUCTION", &e.from_inst);
        out.text_or_null("TO_LIBRARY", &e.to.1);
        out.text_or_null("TO_PROGRAM", &e.to.0);
        out.text_or_null("TO_INSTRUCTION", &e.to_inst);
        if !e.msgid.trim().is_empty() {
            let file = match &e.msgid[..e.msgid.len().min(3)] {
                "SQL" | "SQ" => "QSQLMSG",
                "CEE" => "QCEEMSG",
                _ => "QCPFMSG",
            };
            out.text("MESSAGE_LIBRARY", "QSYS");
            out.text("MESSAGE_FILE", file);
        }
        out.text("MESSAGE_TEXT", e.text.as_str());
        out.text_or_null("MESSAGE_SECOND_LEVEL_TEXT", &e.help);
        out.set("MESSAGE_KEY", Value::Bin(u32::try_from(key).unwrap_or(u32::MAX).to_be_bytes().to_vec()));
        out.text("QUALIFIED_JOB_NAME", qualified.as_str());
    }
    Ok(())
}
