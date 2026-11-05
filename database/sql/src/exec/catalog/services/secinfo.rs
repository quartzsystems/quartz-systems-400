// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SECURITY_INFO`: the security system values, the audit journal and
//! the next user and group IDs. QS/400 has no pending security or
//! password level (a change applies at the next IPL from the value
//! itself), and no service-tools security locks: those are null.

use std::collections::HashMap;

use store::txn::Txn;

use super::super::super::SqlContext;
use super::{Args, Out, ask};
use crate::error::Result;

/// The `NOT NULL` columns with no value here: the service tools locks.
pub const NO_VALUE: &[&str] = &[
    "ALLOW_DIGITAL_CERTIFICATE_ADD",
    "ALLOW_SECURITY_SYSVAL_CHANGE",
    "ALLOW_SERVICE_TOOLS_PASSWORD_CHANGE",
    "ALLOW_PASSWORD_EXIT_PROGRAM_ADD_REMOVE",
    "OBJECT_AUTHORITY_COLLECTION_ACTIVE",
];

/// Integer columns and their system values.
const NUMBERS: &[(&str, &str)] = &[
    ("SECURITY_LEVEL", "QSECURITY"),
    ("PASSWORD_LEVEL", "QPWDLVL"),
    ("PASSWORD_EXPIRATION_WARNING", "QPWDEXPWRN"),
    ("PASSWORD_LIMIT_DIGITS", "QPWDLMTAJC"),
    ("PASSWORD_LIMIT_REPEAT", "QPWDLMTREP"),
    ("PASSWORD_LIMIT_POSITIONS", "QPWDPOSDIF"),
    ("PASSWORD_REQUIRE_DIGIT", "QPWDRQDDGT"),
    ("PASSWORD_MAXIMUM_LENGTH", "QPWDMAXLEN"),
    ("PASSWORD_MINIMUM_LENGTH", "QPWDMINLEN"),
    ("PASSWORD_DUPLICATION", "QPWDRQDDIF"),
    ("MAXIMUM_SIGNON_ACTION", "QMAXSGNACN"),
    ("VERIFY_OBJECT_RESTORE", "QVFYOBJRST"),
    ("LIMIT_SECOFR_ACCESS", "QLMTSECOFR"),
    ("AUTOCONFIGURE_DEVICES", "QAUTOCFG"),
    ("AUTOCONFIGURE_REMOTE_CONTROLLERS", "QAUTORMT"),
];

/// Text columns and their system values.
const TEXTS: &[(&str, &str)] = &[
    ("PASSWORD_CHANGE_BLOCK", "QPWDCHGBLK"),
    ("PASSWORD_EXPIRATION_INTERVAL", "QPWDEXPITV"),
    ("PASSWORD_LIMIT_CHARACTERS", "QPWDLMTCHR"),
    ("PASSWORD_RULES", "QPWDRULES"),
    ("PASSWORD_VALIDATION_PROGRAM", "QPWDVLDPGM"),
    ("CREATE_PUBLIC_AUTHORITY", "QCRTAUT"),
    ("CREATE_OBJECT_AUDITING", "QCRTOBJAUD"),
    ("MAXIMUM_SIGNON_ATTEMPTS", "QMAXSIGN"),
    ("ALLOW_OBJECT_RESTORE", "QALWOBJRST"),
    ("USE_ADOPTED_AUTHORITY", "QUSEADPAUT"),
    ("ALLOW_USER_DOMAIN", "QALWUSRDMN"),
    ("INACTIVE_JOB_TIMEOUT", "QINACTITV"),
    ("INACTIVE_JOB_MESSAGE_QUEUE", "QINACTMSGQ"),
    ("DISCONNECTED_JOB_INTERVAL", "QDSCJOBITV"),
    ("AUDITING_CONTROL", "QAUDCTL"),
    ("AUDITING_LEVEL", "QAUDLVL"),
    ("AUDITING_LEVEL_EXTENSION", "QAUDLVL2"),
];

/// The view.
pub fn view(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let vals = ask(txn, ctx, "the system values", |h| h.system_values())?;
    let by: HashMap<&str, &super::host::SysValue> = vals.iter().map(|v| (v.name.as_str(), v)).collect();
    let text = |n: &str| {
        by.get(n).map(|v| match (&v.text, v.number) {
            (Some(t), _) => t.trim().to_string(),
            (None, Some(n)) => n.to_string(),
            (None, None) => String::new(),
        })
    };
    out.row();
    // The table function has the levels and journal only.
    for (col, sv) in NUMBERS {
        if !out.has(col) {
            continue;
        }
        if let Some(n) = text(sv).and_then(|t| t.trim().parse::<i64>().ok()) {
            out.int(col, n);
        }
    }
    for (col, sv) in TEXTS {
        if !out.has(col) {
            continue;
        }
        if let Some(t) = text(sv) {
            out.text(col, t);
        }
    }
    let audit = sec::audit::write::journal(txn, ctx.qsys).ok().flatten();
    out.yes("AUDIT_JOURNAL_EXISTS", audit.is_some());
    if let Some(id) = audit
        && let Ok(j) = jrn::journal::Journal::load(txn, id)
        && let Ok((_, h)) = store::obj::ops::read_header(txn, j.attr.attached)
        && let Ok((_, lh)) = store::obj::ops::read_header(txn, h.library)
    {
        out.text("AUDIT_JOURNAL_RECEIVER", h.name.to_string());
        out.text("AUDIT_JOURNAL_RECEIVER_LIBRARY", lh.name.to_string());
    }
    if let Ok(n) = sec::profile::groups::next_id(txn, ctx.qsys, false) {
        out.int("NEXT_USER_ID", n);
    }
    if let Ok(n) = sec::profile::groups::next_id(txn, ctx.qsys, true) {
        out.int("NEXT_GROUP_ID", n);
    }
    Ok(())
}

/// The table function: `IGNORE_ERRORS` changes nothing here.
pub fn function(txn: &mut Txn<'_>, ctx: &SqlContext, _args: &Args, out: &mut Out) -> Result<()> {
    view(txn, ctx, out)
}
