// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `MESSAGE_FILE_DATA`: the message descriptions of the `*MSGF`
//! objects (`msgf`), as `DSPMSGD` shows them. A description keeps no
//! dates or levels of its own, and no alert options: those are null.

use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::super::super::SqlContext;
use super::objects::libraries;
use super::wmobj::all_of;
use super::{Args, Out};
use crate::error::Result;

/// The `NOT NULL` columns with no value here.
pub const NO_VALUE: &[&str] = &["CREATION_DATE", "CREATION_LEVEL", "MODIFICATION_DATE", "MODIFICATION_LEVEL", "ALERT_OPTION"];

/// The view: every message file.
pub fn view(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    for (lib, name, id) in all_of(txn, ctx, types::MSGF)? {
        file(txn, &lib, &name, id, out);
    }
    Ok(())
}

/// The table function: one message file, its library a name or a list
/// (`*LIBL`, `*CURLIB` ...); the first found.
pub fn function(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, out: &mut Out) -> Result<()> {
    let lib = args.word("MESSAGE_FILE_LIBRARY").unwrap_or_else(|| "*LIBL".into());
    let Some(name) = args.word("MESSAGE_FILE") else { return Ok(()) };
    for l in libraries(txn, ctx, &lib)? {
        let Ok(list) = ops::list(txn, l, Some(types::MSGF)) else { continue };
        let Some((_, id)) = list.into_iter().find(|(k, _)| k.name.to_string() == name) else { continue };
        let Ok((_, lh)) = ops::read_header(txn, l) else { continue };
        file(txn, &lh.name.to_string(), &name, id, out);
        break;
    }
    Ok(())
}

fn file(txn: &mut Txn<'_>, lib: &str, name: &str, id: ObjectId, out: &mut Out) {
    let Ok(f) = msgf::store::load(txn, id) else { return };
    for d in &f.msgs {
        out.row();
        out.text("MESSAGE_FILE_LIBRARY", lib);
        out.text("MESSAGE_FILE", name);
        out.text("MESSAGE_ID", d.id.as_str());
        out.text("MESSAGE_TEXT", d.text.as_str());
        out.text_or_null("MESSAGE_SECOND_LEVEL_TEXT", &d.help);
        out.int("SEVERITY", d.severity);
        out.int("MESSAGE_DATA_COUNT", d.vars.len());
        let data: Vec<String> = d.vars.iter().map(msgf::desc::VarType::declaration).collect();
        out.text_or_null("MESSAGE_DATA", &data.join(" "));
        out.text("LOG_PROBLEM", "NO");
        out.int("CCSID", f.ccsid);
        let inquiry = d.dft_reply.is_some() || !d.valid_replies.is_empty();
        if inquiry {
            out.text("REPLY_TYPE", "*CHAR");
            out.int("REPLY_LENGTH", d.valid_replies.iter().map(String::len).chain(d.dft_reply.as_ref().map(String::len)).max().unwrap_or(0));
            out.int("REPLY_DECIMAL_POSITIONS", 0);
        }
        if let Some(r) = &d.dft_reply {
            out.text_or_null("DEFAULT_REPLY", r);
        }
        out.int("VALID_REPLY_VALUES_COUNT", d.valid_replies.len());
        out.text_or_null("VALID_REPLY_VALUES", &d.valid_replies.join(" "));
        out.int("SPECIAL_REPLY_VALUES_COUNT", 0);
        out.int("DUMP_LIST_COUNT", d.dump.len());
        let dump: Vec<String> = d.dump.iter().map(u16::to_string).collect();
        out.text_or_null("DUMP_LIST", &dump.join(" "));
    }
}
