// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Services that change the system: `SYSTOOLS.CHANGE_USER_PROFILE`
//! (a `CHGUSRPRF` built from its arguments, shown or run) and the
//! procedures QS/400 refuses or keeps as no-ops.

use std::fmt::Write as _;

use store::obj::{ops, types};
use store::txn::Txn;

use super::super::super::SqlContext;
use super::{Args, Out, ask};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// `CHGUSRPRF` keywords and the arguments that give them.
const KEYWORDS: &[(&str, &str)] = &[
    ("PASSWORD", "P_PASSWORD"),
    ("PWDEXP", "P_PASSWORD_EXPIRED"),
    ("STATUS", "P_STATUS"),
    ("INLPGM", "P_INITIAL_PROGRAM"),
    ("LMTCPB", "P_LIMIT_CAPABILITIES"),
    ("TEXT", "P_TEXT"),
    ("PWDEXPITV", "P_PASSWORD_EXPIRATION_INTERVAL"),
    ("JOBD", "P_JOB_DESCRIPTION"),
    ("GRPPRF", "P_GROUP_PROFILE"),
    ("USREXPDATE", "P_USER_EXPIRATION_DATE"),
];

/// The command `CHANGE_USER_PROFILE` builds: only the keywords whose
/// argument is not `*SAME`.
fn command(args: &Args) -> Result<String> {
    let user = args.word("P_USER_NAME").unwrap_or_default();
    let mut cmd = format!("CHGUSRPRF USRPRF({user})");
    for (kw, arg) in KEYWORDS {
        let Some(v) = args.text(arg) else { continue };
        let v = v.trim();
        if v.eq_ignore_ascii_case("*SAME") {
            continue;
        }
        let v = if *kw == "TEXT" { format!("'{}'", v.replace('\'', "''")) } else { v.to_ascii_uppercase() };
        let _ = write!(cmd, " {kw}({v})");
    }
    if let Some(n) = args.int("P_USER_EXPIRATION_INTERVAL") {
        let _ = write!(cmd, " USREXPITV({n})");
    }
    let action = args.word("P_SPECIAL_AUTHORITY_ACTION").unwrap_or_else(|| "*SAME".into());
    if action != "*SAME" {
        // BLOCKED(sql): adding or removing one special authority needs
        // the profile's current list; only a whole list is given here.
        return Err(SqlError::Coded {
            code: -84,
            state: "42612",
            what: "P_SPECIAL_AUTHORITY_ACTION other than *SAME is not supported".into(),
        });
    }
    Ok(cmd)
}

/// `SYSTOOLS.CHANGE_USER_PROFILE`: with `PREVIEW => 'YES'` the
/// command only, else the command run.
pub fn change_user_profile(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, out: &mut Out) -> Result<()> {
    let cmd = command(args)?;
    let preview = args.word("PREVIEW").is_none_or(|p| p != "NO");
    out.row();
    out.text("USER_NAME", args.word("P_USER_NAME").unwrap_or_default());
    out.text("CHGUSRPRF_COMMAND", cmd.as_str());
    out.yes("CHANGE_ATTEMPTED", !preview);
    if preview {
        return Ok(());
    }
    match ask(txn, ctx, "the command processor", |h| h.command(&cmd))? {
        Ok(()) => out.yes("CHANGE_SUCCESSFUL", true),
        Err((id, text)) => {
            out.yes("CHANGE_SUCCESSFUL", false);
            out.text("FAILURE_MESSAGE_ID", id);
            out.text("FAILURE_MESSAGE_TEXT", text);
        }
    }
    Ok(())
}

/// A procedure QS/400 refuses: SQL0084 with what is missing.
fn refused(what: &str) -> SqlError {
    SqlError::Coded { code: -84, state: "42612", what: what.into() }
}

/// `SET_SERVER_SBS_ROUTING`: the host servers' prestart jobs run in
/// `QUSRWRK` for every user; routing by user is not kept.
pub fn set_server_sbs_routing(_txn: &mut Txn<'_>, _ctx: &mut SqlContext, _args: &Args) -> Result<Vec<Value>> {
    Err(refused("QSYS2.SET_SERVER_SBS_ROUTING: host server routing by user is not supported"))
}

/// `SET_PASE_SHELL_INFO`: the SSH server starts QSH for every user; a
/// user's PASE shell is not kept.
pub fn set_pase_shell_info(_txn: &mut Txn<'_>, _ctx: &mut SqlContext, _args: &Args) -> Result<Vec<Value>> {
    Err(refused("QSYS2.SET_PASE_SHELL_INFO: a user's PASE shell is not supported; SSH sessions run QSH"))
}

/// `SYSTOOLS.LPRINTF`: the job log is the job's, not the storage
/// service's.
pub fn lprintf(_txn: &mut Txn<'_>, _ctx: &mut SqlContext, _args: &Args) -> Result<Vec<Value>> {
    // BLOCKED(sql): a statement runs in the storage service, which
    // cannot write the asking job's log.
    Err(refused("SYSTOOLS.LPRINTF: writing the job log from a statement is not supported"))
}

/// `OVERRIDE_TABLE(TABLE_SCHEMA, TABLE_NAME, BLOCK_SIZE)`: the table
/// must exist and the size be `*DEFAULT`, `*BUF32KB` ... or a number of
/// bytes; QS/400 reads records unblocked, so it changes nothing else.
pub fn override_table(txn: &mut Txn<'_>, ctx: &mut SqlContext, args: &Args) -> Result<Vec<Value>> {
    let schema = args.word("TABLE_SCHEMA").unwrap_or_default();
    let table = args.word("TABLE_NAME").unwrap_or_default();
    let size = args.word("BLOCK_SIZE").unwrap_or_default();
    let valid = matches!(size.as_str(), "*DEFAULT" | "*BUF32KB" | "*BUF64KB" | "*BUF128KB" | "*BUF256KB" | "*BUF512KB" | "*BUF1MB" | "*BUF2MB" | "*BUF4MB" | "*BUF8MB" | "*BUF16MB")
        || size.parse::<u64>().is_ok_and(|n| n > 0);
    if !valid {
        return Err(SqlError::Coded { code: -171, state: "42815", what: format!("BLOCK_SIZE {size}") });
    }
    let found = super::objects::libraries(txn, ctx, &schema)?.into_iter().any(|l| {
        ops::list(txn, l, Some(types::FILE)).is_ok_and(|v| v.iter().any(|(k, _)| k.name.to_string() == table))
    });
    if !found {
        return Err(SqlError::Coded { code: -204, state: "42704", what: format!("{schema}.{table} *FILE") });
    }
    Ok(Vec::new())
}
