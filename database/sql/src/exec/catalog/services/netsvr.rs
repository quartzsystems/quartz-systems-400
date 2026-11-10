// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SERVER_SHARE_INFO`: the file and print shares, from the `QZLS*`
//! configuration (`docs/NETSERVER.md` section 4). The other kind's
//! columns are null.

use store::txn::Txn;

use super::super::super::SqlContext;
use super::{Out, ask};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// `SERVER_SHARE_INFO`.
pub fn shares(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let list = ask(txn, ctx, "the NetServer shares", |h| h.server_shares())?.map_err(|e| {
        SqlError::Coded {
            code: -443,
            state: "38501",
            what: e,
        }
    })?;
    for s in &list {
        out.row();
        out.text("SERVER_SHARE_NAME", s.name.as_str());
        out.text("SHARE_TYPE", if s.print { "PRINT" } else { "FILE" });
        out.text_or_null("TEXT_DESCRIPTION", &s.text);
        out.yes("ENCRYPTION_REQUIRED", s.encryption);
        out.text_or_null("SHARE_AUTHORIZATION_LIST", &s.autl);
        if s.print {
            out.text("SPOOLED_FILE_TYPE", s.spool_type.as_str());
            out.text_or_null("OUTPUT_QUEUE_LIBRARY", &s.outq.0);
            out.text_or_null("OUTPUT_QUEUE", &s.outq.1);
            out.text_or_null("PRINT_DRIVER", &s.driver);
            out.text_or_null("PRINTER_FILE_LIBRARY", &s.printer_file.0);
            out.text_or_null("PRINTER_FILE", &s.printer_file.1);
            out.yes("PUBLISH_PRINT_SHARE", s.publish);
            out.int("CURRENT_CONNECTIONS", s.current);
        } else {
            out.text("PATH_NAME", s.path.as_str());
            out.text("PERMISSIONS", if s.read_only { "RO" } else { "RW" });
            if s.max_users >= 0 {
                out.int("MAXIMUM_CONNECTIONS", s.max_users);
            }
            out.int("CURRENT_CONNECTIONS", s.current);
            out.yes("TEXT_CONVERSION_ENABLED", s.text_conversion);
            out.int("TEXT_CONVERSION_CCSID", s.text_ccsid);
            out.int("FILE_EXTENSION_COUNT", s.extension_count);
            if s.extension_count > 0 {
                out.set("FILE_EXTENSIONS", Value::Bin(s.extensions.clone()));
            }
        }
    }
    Ok(())
}
