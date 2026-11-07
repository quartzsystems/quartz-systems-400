// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `QSYS2` and `SYSTOOLS` procedures the engine runs itself, for
//! `CALL` (`routine::call`) to dispatch to ([`call`]), and `QSYS2.QCMDEXC` as a scalar
//! function. A procedure's arguments bind as a table function's do;
//! `OUT` parameters come back in order.

use store::txn::Txn;

use super::super::super::SqlContext;
use super::{Args, ask, shape};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// A procedure's body: its bound arguments, its `OUT` values.
type ProcFn = fn(&mut Txn<'_>, &mut SqlContext, &Args) -> Result<Vec<Value>>;

/// The engine's procedures, by specific name.
const PROCEDURES: &[(&str, &str, ProcFn)] = &[
    ("QSYS2", "QCMDEXC", qcmdexc_length),
    ("QSYS2", "QCMDEXC1", qcmdexc),
    ("QSYS2", "QSQIFSWRITE", ifs_write),
    ("QSYS2", "QSQIFSWRITEUTF8", ifs_write_utf8),
    ("QSYS2", "QSQIFSWRITEBINARY", ifs_write_binary),
    ("QSYS2", "OVERRIDET", super::admin::override_table),
    ("QSYS2", "SET_SERVER_SBS_ROUTING", super::admin::set_server_sbs_routing),
    ("QSYS2", "SET_PASE_SHELL_INFO", super::admin::set_pase_shell_info),
    ("SYSTOOLS", "LPRINTF", super::admin::lprintf),
    ("QSYS2", "QSQGENSQL", super::gensql::generate_sql),
    ("QSYS2", "QSQGENSQLO", super::gensql::generate_sql_objects),
    ("QSYS2", "OVERRIDE_QAQQINI", super::qaqqini::override_qaqqini),
    ("QSYS2", "DUMP_PC", super::plancache::dump),
    ("QSYS2", "CLEAR_PC", super::plancache::clear),
    ("QSYS2", "CHANGE_PC", super::plancache::change_size),
];

/// Whether `schema.name` is one of the engine's procedures.
#[must_use]
pub fn is_procedure(schema: Option<&str>, name: &str) -> bool {
    PROCEDURES.iter().any(|(s, spec, _)| {
        schema.is_none_or(|x| x == *s) && shape::routines(s, name).iter().any(|r| r.kind == 'P' && r.specific == *spec)
    })
}

/// `CALL schema.name(args)`: the `OUT` values, `None` when it is not
/// one of the engine's procedures. Of overloads, the first whose
/// parameters the arguments bind to.
pub fn call(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    schema: Option<&str>,
    name: &str,
    args: &[(Option<String>, Value)],
) -> Option<Result<Vec<Value>>> {
    let mut last = None;
    for (s, spec, f) in PROCEDURES {
        if schema.is_some_and(|x| x != *s) {
            continue;
        }
        let Some(shape) = shape::routines(s, name).iter().find(|r| r.kind == 'P' && r.specific == *spec) else {
            continue;
        };
        match Args::bind(shape, args.to_vec(), ctx) {
            Ok(bound) => return Some(f(txn, ctx, &bound)),
            Err(e) => last = Some(e),
        }
    }
    last.map(Err)
}

/// Run a command: SQL0443 with the escape message when it fails.
fn run(txn: &mut Txn<'_>, ctx: &SqlContext, cmd: &str) -> Result<()> {
    match ask(txn, ctx, "the command processor", |h| h.command(cmd))? {
        Ok(()) => Ok(()),
        Err((id, text)) => Err(SqlError::Coded { code: -443, state: "38501", what: format!("{id} {text}").trim().to_string() }),
    }
}

/// `QCMDEXC(COMMAND)`.
fn qcmdexc(txn: &mut Txn<'_>, ctx: &mut SqlContext, args: &Args) -> Result<Vec<Value>> {
    let cmd = args.text("COMMAND").unwrap_or_default();
    run(txn, ctx, &cmd)?;
    Ok(Vec::new())
}

/// The old form `QCMDEXC(command, length)`: the length is the
/// command's, as the system API takes it.
fn qcmdexc_length(txn: &mut Txn<'_>, ctx: &mut SqlContext, args: &Args) -> Result<Vec<Value>> {
    let shape = shape::index_specific("QSYS2", "QCMDEXC").map(|s| s.parms.clone()).unwrap_or_default();
    let (Some(c), Some(l)) = (shape.first(), shape.get(1)) else {
        return Ok(Vec::new());
    };
    let cmd = args.text(&c.name).unwrap_or_default();
    let len = args.int(&l.name).and_then(|n| usize::try_from(n).ok()).unwrap_or(cmd.len());
    let cmd: String = cmd.chars().take(len).collect();
    run(txn, ctx, &cmd)?;
    Ok(Vec::new())
}

/// The scalar `QCMDEXC(COMMAND, PRINT)`: 1 when the command ran, -1
/// when it failed.
pub(crate) fn qcmdexc_scalar(name: &str, args: &[Value], ctx: &SqlContext) -> Option<Result<Value>> {
    if name != "QCMDEXC" && name != "QSYS2.QCMDEXC" {
        return None;
    }
    Some((|| {
        if args.is_empty() || args.len() > 2 {
            return Err(SqlError::Coded { code: -440, state: "42884", what: format!("QSYS2.QCMDEXC with {} arguments", args.len()) });
        }
        if args.get(1).is_some_and(|p| !p.is_null() && !p.as_text().trim().eq_ignore_ascii_case("NONE")) {
            // BLOCKED(rel): PRINT => 'PRINT' writes a spooled file of the
            // command's messages; not done yet.
            return Err(SqlError::Coded { code: -171, state: "42815", what: "argument 2 of QCMDEXC: only 'NONE' is supported".into() });
        }
        if args[0].is_null() {
            return Ok(Value::Null);
        }
        let host = ctx.host.as_deref().ok_or_else(|| super::unreachable_here("the command processor"))?;
        // A scalar function holds no transaction of its own to give back:
        // the host runs the command over the store it can reach.
        let r = host.command(&args[0].as_text()).ok_or_else(|| super::unreachable_here("the command processor"))?;
        Ok(Value::int(if r.is_ok() { 1 } else { -1 }))
    })())
}

fn write(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, data: Vec<u8>, ccsid: u16) -> Result<Vec<Value>> {
    let path = args.text("PATH_NAME").unwrap_or_default();
    let mode = args.word("OVERWRITE").unwrap_or_else(|| "APPEND".into());
    let append = match mode.as_str() {
        "APPEND" => true,
        "REPLACE" => false,
        "NONE" => {
            if ask(txn, ctx, "the IFS", |h| h.ifs_read(path.trim()))?.is_ok() {
                return Err(SqlError::Coded { code: -443, state: "38501", what: format!("{path} exists and OVERWRITE is NONE") });
            }
            false
        }
        o => return Err(SqlError::Coded { code: -171, state: "42815", what: format!("OVERWRITE {o}") }),
    };
    let mut data = data;
    if args.word("END_OF_LINE").is_none_or(|v| v != "NONE") {
        match args.word("END_OF_LINE").as_deref() {
            Some("CRLF") => data.extend_from_slice(b"\r\n"),
            Some("CR") => data.push(b'\r'),
            Some("LFCR") => data.extend_from_slice(b"\n\r"),
            _ => data.push(if ccsid == 1208 || ccsid == 0 { b'\n' } else { 0x25 }),
        }
    }
    ask(txn, ctx, "the IFS", |h| h.ifs_write(path.trim(), &data, append, ccsid))?
        .map_err(|e| SqlError::Coded { code: -443, state: "38501", what: format!("{path}: {e}") })?;
    Ok(Vec::new())
}

/// `IFS_WRITE(PATH_NAME, LINE, FILE_CCSID, OVERWRITE, END_OF_LINE)`:
/// the line in the file's CCSID (the job's EBCDIC by default).
fn ifs_write(txn: &mut Txn<'_>, ctx: &mut SqlContext, args: &Args) -> Result<Vec<Value>> {
    let ccsid = args.int("FILE_CCSID").and_then(|n| u16::try_from(n).ok()).filter(|n| *n != 0).unwrap_or(37);
    let line = args.text("LINE").unwrap_or_default();
    let data = match ccsid::conv::Codec::of(ccsid).ok().flatten() {
        Some(c) if ccsid != 1208 => {
            let mut out = Vec::new();
            for ch in line.chars() {
                c.encode(ch, &mut out);
            }
            out
        }
        _ => line.into_bytes(),
    };
    write(txn, ctx, args, data, ccsid)
}

/// `IFS_WRITE_UTF8`: the line as UTF-8, a new file tagged 1208.
fn ifs_write_utf8(txn: &mut Txn<'_>, ctx: &mut SqlContext, args: &Args) -> Result<Vec<Value>> {
    let line = args.text("LINE").unwrap_or_default();
    write(txn, ctx, args, line.into_bytes(), 1208)
}

/// `IFS_WRITE_BINARY`: the bytes as they are.
fn ifs_write_binary(txn: &mut Txn<'_>, ctx: &mut SqlContext, args: &Args) -> Result<Vec<Value>> {
    let data = match args.get("LINE") {
        Value::Bin(b) => b.clone(),
        Value::Null => Vec::new(),
        v => v.as_bytes()?,
    };
    let ccsid = args.int("FILE_CCSID").and_then(|n| u16::try_from(n).ok()).unwrap_or(65535);
    write(txn, ctx, args, data, ccsid)
}
