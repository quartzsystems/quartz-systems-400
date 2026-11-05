// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `JOURNAL_INFO`, `JOURNAL_RECEIVER_INFO` and `JOURNALED_OBJECTS`:
//! the journals, their receivers and the objects journaled to them, as
//! the journal objects record them (`jrn`). Remote journals do not
//! exist on QS/400.

use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::super::super::SqlContext;
use super::Out;
use super::wmobj::all_of;
use crate::error::Result;

/// `*JRN` (the store's registry has no constant for it).
const JRN: u16 = 0x0018;
/// `*JRNRCV`.
const JRNRCV: u16 = 0x0019;

/// `JOURNAL_INFO`'s `NOT NULL` columns with no value here: QS/400
/// journals send no messages and redirect no receivers.
pub const JOURNAL_NO_VALUE: &[&str] = &["MESSAGE_QUEUE", "MESSAGE_QUEUE_LIBRARY", "REDIRECTED_RECEIVER_LIBRARY"];

/// Library and name of an object.
fn qualified(txn: &mut Txn<'_>, id: ObjectId) -> Option<(String, String)> {
    let (_, h) = ops::read_header(txn, id).ok()?;
    let (_, lh) = ops::read_header(txn, h.library).ok()?;
    Some((lh.name.to_string(), h.name.to_string()))
}

/// `JOURNAL_INFO`.
pub fn journals(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    for (lib, name, id) in all_of(txn, ctx, JRN)? {
        let Ok(j) = jrn::journal::Journal::load(txn, id) else { continue };
        let receivers = j.receivers(txn).unwrap_or_default();
        let objects = j.objects(txn).unwrap_or_default();
        let mut size = 0u64;
        for r in &receivers {
            if let Ok(rc) = jrn::rcv::Receiver::load(txn, r.receiver) {
                size += rc.attr.bytes_used;
            }
        }
        out.row();
        out.text("JOURNAL_NAME", name.as_str());
        out.text("JOURNAL_LIBRARY", lib.as_str());
        out.int("ASP_NUMBER", 1);
        out.text("JOURNAL_ASPGRP", "*SYSBAS");
        if let Some((l, n)) = qualified(txn, j.attr.attached) {
            out.text("ATTACHED_JOURNAL_RECEIVER_NAME", n);
            out.text("ATTACHED_JOURNAL_RECEIVER_LIBRARY", l);
        }
        out.text("DELETE_RECEIVER_OPTION", if j.attr.dltrcv { "YES" } else { "NO" });
        out.text("JOURNAL_TYPE", "LOCAL");
        out.text("JOURNAL_STATE", "ACTIVE");
        out.int("NUMBER_JOURNAL_RECEIVERS", receivers.len());
        out.int("TOTAL_SIZE_JOURNAL_RECEIVERS", size.div_ceil(1024));
        out.int("NUMBER_REMOTE_JOURNALS", 0);
        out.text_or_null("JOURNAL_TEXT", &ccsid::decode_trimmed(&j.header.text));
        out.text("MANAGE_RECEIVER_OPTION", if j.attr.mngrcv_system { "SYSTEM" } else { "USER" });
        out.int("JOURNALED_OBJECTS", objects.len());
        out.int("JOURNALED_FILES", objects.iter().filter(|o| o.object_type == types::FILE).count());
    }
    Ok(())
}

/// `JOURNAL_RECEIVER_INFO`.
pub fn receivers(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    for (lib, name, id) in all_of(txn, ctx, JRNRCV)? {
        let Ok(r) = jrn::rcv::Receiver::load(txn, id) else { continue };
        out.row();
        out.text("JOURNAL_RECEIVER_LIBRARY", lib.as_str());
        out.text("JOURNAL_RECEIVER_NAME", name.as_str());
        out.text_or_null("DESCRIPTIVE_TEXT", &ccsid::decode_trimmed(&r.header.text));
        out.int("JOURNAL_RECEIVER_ASP_NUMBER", 1);
        out.text("JOURNAL_RECEIVER_ASP_NAME", "*SYSBAS");
        if let Some((l, n)) = qualified(txn, r.attr.journal) {
            out.text("JOURNAL_LIBRARY", l);
            out.text("JOURNAL_NAME", n);
        }
        out.int("THRESHOLD", r.attr.threshold_kb);
        out.int("SIZE", r.attr.bytes_used.div_ceil(1024));
        out.text("STATUS", r.attr.status.name().trim_start_matches('*'));
        out.int("NUMBER_OF_JOURNAL_ENTRIES", r.attr.entry_count);
        if r.attr.entry_count > 0 {
            out.int("FIRST_SEQUENCE_NUMBER", r.attr.first_seq);
            out.int("LAST_SEQUENCE_NUMBER", r.attr.last_seq);
        }
        out.stamp("ATTACH_TIMESTAMP", r.attr.attached);
        out.stamp("DETACH_TIMESTAMP", r.attr.detached);
        out.stamp("SAVE_TIMESTAMP", r.header.last_saved);
        if let Some((l, n)) = qualified(txn, r.attr.prev) {
            out.text("PREVIOUS_JOURNAL_RECEIVER_LIBRARY", l);
            out.text("PREVIOUS_JOURNAL_RECEIVER", n);
        }
        if let Some((l, n)) = qualified(txn, r.attr.next) {
            out.text("NEXT_JOURNAL_RECEIVER_LIBRARY", l);
            out.text("NEXT_JOURNAL_RECEIVER", n);
        }
    }
    Ok(())
}

/// `JOURNALED_OBJECTS`.
pub fn objects(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    for (lib, name, id) in all_of(txn, ctx, JRN)? {
        let Ok(j) = jrn::journal::Journal::load(txn, id) else { continue };
        for o in j.objects(txn).unwrap_or_default() {
            out.row();
            out.text("JOURNAL_LIBRARY", lib.as_str());
            out.text("JOURNAL_NAME", name.as_str());
            out.int("IASP_NUMBER", 0);
            out.text("IASP_NAME", "*SYSBAS");
            out.text("OBJECT_TYPE", types::type_by_code(o.object_type).map_or("*N", |t| t.name));
            if let Some((l, n)) = qualified(txn, o.object) {
                out.text("OBJECT_LIBRARY", l);
                out.text("OBJECT_NAME", n);
            }
            if o.object_type == types::FILE {
                out.text("FILE_TYPE", "PHYSICAL");
            }
            out.text("JOURNAL_IMAGES", if o.both_images { "*BOTH" } else { "*AFTER" });
            out.text("OMIT_JOURNAL_ENTRY", if o.omit_open_close { "*OPNCLO" } else { "*NONE" });
        }
    }
    Ok(())
}
