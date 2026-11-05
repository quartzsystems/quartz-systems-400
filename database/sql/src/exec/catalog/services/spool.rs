// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `OUTPUT_QUEUE_ENTRIES` (and `_BASIC`), `OUTPUT_QUEUE_INFO` and
//! `SYSTOOLS.SPOOLED_FILE_DATA`: the spooled files the storage service
//! keeps (`WRKSPLF`'s), the output queue objects, and a file's lines as
//! `DSPSPLF` decodes them. AFP resources and fonts are not modelled.

use store::obj::{ops, types};
use store::txn::Txn;
use wm::obj::splf::{SplfAttrs, Status};

use super::super::super::SqlContext;
use super::{Args, Out, ask};
use crate::error::{Result, SqlError};

/// `OUTPUT_QUEUE_ENTRIES`'s `NOT NULL` columns with no value here.
pub const ENTRY_NO_VALUE: &[&str] = &[
    "SYSTEM",
    "ACCOUNTING_CODE",
    "PRINT_FIDELITY",
    "PAGE_ROTATION",
    "FILE_AVAILABLE",
    "FRONT_OVERLAY_NAME",
    "BACK_OVERLAY_NAME",
    "CHARACTER_SET_NAME",
    "CODED_FONT_NAME",
    "MULTIBYTE_DATA",
    "DBCS_CODED_FONT_NAME",
];

/// `OUTPUT_QUEUE_INFO`'s `NOT NULL` columns with no value here.
pub const QUEUE_NO_VALUE: &[&str] = &["LDAP_PUBLISHING_STATUS", "SPOOLED_FILE_ASP_ATTRIBUTE"];

fn status_text(s: Status) -> &'static str {
    match s {
        Status::Rdy => "READY",
        Status::Hld => "HELD",
        Status::Sav => "SAVED",
        Status::Wtr => "WRITING",
        Status::Opn => "OPEN",
        Status::Clo => "CLOSED",
        Status::Pnd => "PENDING",
        Status::Dfr => "DEFERRED",
        Status::Msgw => "MESSAGE",
    }
}

fn split(q: &str) -> (&str, &str) {
    q.split_once('/').map_or(("", q), |(l, n)| (l.trim(), n.trim()))
}

fn star(v: &str, none: &str) -> String {
    if v.trim().is_empty() { none.to_string() } else { v.trim().to_string() }
}

/// `OUTPUT_QUEUE_ENTRIES` and `OUTPUT_QUEUE_ENTRIES_BASIC`.
pub fn entries(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let mut files = ask(txn, ctx, "the spooled files", |h| h.spooled_files())?;
    files.sort_by(|a, b| (&a.outq, a.created, a.id).cmp(&(&b.outq, b.created, b.id)));
    for f in &files {
        entry(out, f);
    }
    Ok(())
}

fn entry(out: &mut Out, f: &SplfAttrs) {
    out.row();
    let (lib, q) = split(&f.outq);
    out.text("OUTPUT_QUEUE_NAME", q);
    out.text("OUTPUT_QUEUE_LIBRARY_NAME", lib);
    out.stamp_micros("CREATE_TIMESTAMP", f.created / 1_000_000, u32::try_from(f.created % 1_000_000).unwrap_or(0));
    out.text("SPOOLED_FILE_NAME", f.name.as_str());
    out.text("USER_NAME", f.job.1.as_str());
    out.text_or_null("USER_DATA", &f.usrdta);
    out.text("STATUS", status_text(f.status));
    out.int("SIZE", f.data_len.div_ceil(1024));
    out.int("TOTAL_PAGES", f.pages);
    out.int("COPIES", f.copies);
    out.text("FORM_TYPE", star(&f.formtype, "*STD"));
    out.text("JOB_NAME", f.job_text());
    out.text("DEVICE_TYPE", star(&f.devtype, "*SCS"));
    out.int("OUTPUT_PRIORITY", f.outpty);
    out.int("FILE_NUMBER", f.number);
    if f.expires != 0 {
        let e = f.expires;
        if let Some(d) = db::record::datetime::Date::new(u16::try_from(e / 10_000).unwrap_or(0), u8::try_from(e / 100 % 100).unwrap_or(0), u8::try_from(e % 100).unwrap_or(0)) {
            out.set("EXPIRATION_DATE", crate::value::Value::Date(d));
        }
    }
    out.text("SAVE_AFTER_WRITE", if f.save { "*YES" } else { "*NO" });
    out.int("TOTAL_RECORDS", f.records);
    out.int("PAGE_LENGTH", f.page_len);
    out.dec("LINES_PER_INCH", &format!("{}.0", f.lpi));
    out.int("PAGE_WIDTH", f.page_width);
    out.dec("CHARACTERS_PER_INCH", &format!("{}.0", f.cpi));
    out.text(
        "PRINT_BOTH_SIDES",
        match f.duplex {
            1 => "*YES",
            2 => "*TUMBLE",
            _ => "*NO",
        },
    );
    out.text("STARTING_PAGE", format!("{:>10}", 1));
    out.text("ENDING_PAGE", "*END");
    let (pl, pn) = split(&f.prtf);
    out.text("DEVICE_FILE_LIBRARY", pl);
    out.text("DEVICE_FILE_NAME", pn);
    if !f.usrdfndta.trim().is_empty() {
        out.set("USER_DEFINED_DATA", crate::value::Value::Bin(ccsid::to_ebcdic(&f.usrdfndta).unwrap_or_default()));
    }
}

/// `OUTPUT_QUEUE_INFO`: every output queue object, its files counted
/// from the spool.
pub fn queues(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let mut libs: Vec<_> = ops::list(txn, ctx.qsys, Some(types::LIB))?.into_iter().map(|(_, id)| id).collect();
    libs.insert(0, ctx.qsys);
    for lib in libs {
        let Ok((_, lh)) = ops::read_header(txn, lib) else { continue };
        let Ok(list) = ops::list(txn, lib, Some(types::OUTQ)) else { continue };
        for (key, id) in list {
            let Ok(bytes) = wm::obj::read(txn, id) else { continue };
            let Ok(q) = wm::obj::outq::OutQ::decode(&bytes) else { continue };
            out.row();
            out.text("OUTPUT_QUEUE_NAME", key.name.to_string());
            out.text("OUTPUT_QUEUE_LIBRARY_NAME", lh.name.to_string());
            out.int("NUMBER_OF_FILES", q.files.len());
            out.int("NUMBER_OF_WRITERS", usize::from(!q.writer.trim().is_empty()));
            out.int("WRITERS_TO_AUTOSTART", q.autostrwtr);
            out.text("ORDER_OF_FILES", if q.seq == wm::obj::outq::Seq::Fifo { "*FIFO" } else { "*JOBNBR" });
            out.text("DISPLAY_ANY_FILE", star(&q.dspdta, "*NO"));
            out.text("JOB_SEPARATORS", "0");
            if q.maxpages != 0 {
                out.int("MAXIMUM_PAGES", q.maxpages);
            }
            out.text("OPERATOR_CONTROLLED", if q.oprctl { "*YES" } else { "*NO" });
            out.text("AUTHORITY_TO_CHECK", star(&q.autchk, "*OWNER"));
            out.text("OUTPUT_QUEUE_STATUS", if q.held { "HELD" } else { "RELEASED" });
            out.text_or_null("WRITER_JOB_NAME", &q.writer);
            out.int("SPOOLED_FILE_ASP_NUMBER", 1);
            out.text_or_null("TEXT_DESCRIPTION", &q.text);
            out.text_or_null("REMOTE_SYSTEM_NAME", &q.rmtsys);
            out.text_or_null("REMOTE_PRINTER_QUEUE", &q.rmtprtq);
        }
    }
    Ok(())
}

/// `SYSTOOLS.SPOOLED_FILE_DATA(JOB_NAME, SPOOLED_FILE_NAME,
/// SPOOLED_FILE_NUMBER, IGNORE_ERRORS)`: the file's lines.
pub fn data(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, out: &mut Out) -> Result<()> {
    let job = args.word("JOB_NAME").unwrap_or_default();
    let name = args.word("SPOOLED_FILE_NAME").unwrap_or_else(|| "QPJOBLOG".into());
    let number = args.word("SPOOLED_FILE_NUMBER").unwrap_or_else(|| "*LAST".into());
    let ignore = args.word("IGNORE_ERRORS").is_none_or(|v| v == "YES");
    let files = ask(txn, ctx, "the spooled files", |h| h.spooled_files())?;
    let mut hits: Vec<&SplfAttrs> = files.iter().filter(|f| f.job_text() == job && f.name == name).collect();
    hits.sort_by_key(|f| f.number);
    let hit = match number.as_str() {
        "*LAST" => hits.last().copied(),
        n => n.parse::<u32>().ok().and_then(|n| hits.iter().find(|f| f.number == n).copied()),
    };
    let Some(f) = hit else {
        if ignore {
            return Ok(());
        }
        return Err(SqlError::Coded { code: -443, state: "38501", what: format!("spooled file {name} of job {job} not found") });
    };
    let id = f.id;
    let bytes = match ask(txn, ctx, "the spooled file's data", |h| h.spooled_data(id))? {
        Ok(b) => b,
        Err(_) if ignore => return Ok(()),
        Err(e) => return Err(SqlError::Coded { code: -443, state: "38501", what: e }),
    };
    let (pages, _) = spool::scs::decode(&bytes);
    let text = spool::view::text(&pages);
    // A form feed ends a line as a line end does.
    let lines: Vec<&str> = text.trim_end_matches('\n').split(['\n', '\u{c}']).collect();
    for (i, line) in lines.iter().enumerate() {
        out.row();
        out.int("ORDINAL_POSITION", i + 1);
        out.text("SPOOLED_DATA", line.trim_end());
    }
    Ok(())
}
