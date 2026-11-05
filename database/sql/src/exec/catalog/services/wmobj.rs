// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work management objects as views: `JOB_DESCRIPTION_INFO` from the
//! `*JOBD` objects work management keeps (`wm::obj::jobd`).

use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::super::super::SqlContext;
use super::Out;
use crate::error::Result;

/// `JOB_DESCRIPTION_INFO`'s `NOT NULL` columns with no value here: a
/// QS/400 job description keeps no job switches.
pub const JOBD_NO_VALUE: &[&str] = &["JOB_SWITCHES"];

/// Every object of `type_code` the user can list: library name, object
/// name, object.
pub(super) fn all_of(txn: &mut Txn<'_>, ctx: &SqlContext, type_code: u16) -> Result<Vec<(String, String, ObjectId)>> {
    let mut libs: Vec<ObjectId> = ops::list(txn, ctx.qsys, Some(types::LIB))?.into_iter().map(|(_, id)| id).collect();
    libs.insert(0, ctx.qsys);
    let mut out = Vec::new();
    for lib in libs {
        let Ok((_, lh)) = ops::read_header(txn, lib) else { continue };
        let Ok(list) = ops::list(txn, lib, Some(type_code)) else { continue };
        for (k, id) in list {
            out.push((lh.name.to_string(), k.name.to_string(), id));
        }
    }
    Ok(out)
}

/// `JOB_DESCRIPTION_INFO`.
pub fn jobds(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    for (lib, name, id) in all_of(txn, ctx, types::JOBD)? {
        let Ok(bytes) = wm::obj::read(txn, id) else { continue };
        let Ok(d) = wm::obj::jobd::JobD::decode(&bytes) else { continue };
        out.row();
        out.text("JOB_DESCRIPTION_LIBRARY", lib);
        out.text("JOB_DESCRIPTION", name);
        out.text("AUTHORIZATION_NAME", d.user.trim());
        out.text("ACCOUNTING_CODE", d.acgcde.trim());
        out.text("ROUTING_DATA", d.rtgdta.trim());
        out.text_or_null("REQUEST_DATA", &d.rqsdta);
        if let Some(l) = &d.inllibl {
            out.int("LIBRARY_LIST_COUNT", l.len());
            out.text("LIBRARY_LIST", l.iter().map(|n| format!("{n:<10}")).collect::<Vec<_>>().concat().trim_end().to_string());
        } else {
            out.int("LIBRARY_LIST_COUNT", 0);
            out.text("LIBRARY_LIST", "*SYSVAL");
        }
        out.text_or_null("TEXT_DESCRIPTION", &d.text);
        out.text("JOB_QUEUE_LIBRARY", d.jobq.0.as_str());
        out.text("JOB_QUEUE", d.jobq.1.as_str());
        out.int("JOB_QUEUE_PRIORITY", d.jobpty);
        out.yes("HOLD_ON_JOB_QUEUE", d.hold);
        out.text_or_null("OUTPUT_QUEUE_LIBRARY", &d.outq.0);
        out.text("OUTPUT_QUEUE", d.outq.1.trim());
        out.int("OUTPUT_QUEUE_PRIORITY", d.outpty);
        out.text("SPOOLED_FILE_ACTION", d.splfacn.trim());
        out.text("PRINTER_DEVICE", d.prtdev.trim());
        out.text_or_null("PRINT_TEXT", &d.prttxt);
        if let Some(m) = d.jobmsgqmx {
            out.int("JOB_MESSAGE_QUEUE_MAXIMUM_SIZE", m);
        }
        out.text("JOB_MESSAGE_QUEUE_FULL_ACTION", d.jobmsgqfl.trim());
        if let Some(s) = d.syntax {
            out.int("SYNTAX_CHECK_SEVERITY", s);
        }
        out.int("JOB_END_SEVERITY", d.endsev);
        out.text("JOBLOG_OUTPUT", d.logoutput.trim());
        out.text("INQUIRY_MESSAGE_REPLY", d.inqmsgrpy.trim());
        out.int("MESSAGE_LOGGING_LEVEL", d.log_level);
        out.int("MESSAGE_LOGGING_SEVERITY", d.log_sev);
        out.text("MESSAGE_LOGGING_TEXT", d.log_text.trim());
        out.text("LOG_CL_PROGRAM_COMMANDS", d.logclpgm.trim());
        out.text("DEVICE_RECOVERY_ACTION", d.devrcyacn.trim());
        out.text("TIME_SLICE_END_POOL", d.tsepool.trim());
        out.text("ALLOW_MULTIPLE_THREADS", if d.alwmltthd { "*YES" } else { "*NO" });
        out.text_or_null("ASPGRP", &d.inlaspgrp);
        out.text("DDM_CONVERSATION", d.ddmcnv.trim());
    }
    Ok(())
}
