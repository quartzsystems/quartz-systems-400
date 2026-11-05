// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `IFS_OBJECT_STATISTICS`, `IFS_READ`, `IFS_READ_BINARY` and
//! `IFS_READ_UTF8`: the integrated file system as the asking job sees
//! it, with its authority, through the storage service's IFS.

use store::txn::Txn;

use super::super::super::SqlContext;
use super::host::IfsObject;
use super::{Args, Out, ask};
use crate::error::{Result, SqlError};
use crate::value::Value;

fn failed(what: String) -> SqlError {
    SqlError::Coded { code: -443, state: "38501", what }
}

fn ignore(args: &Args) -> bool {
    args.word("IGNORE_ERRORS").is_none_or(|v| v == "YES")
}

fn object(out: &mut Out, path: &str, o: &IfsObject) {
    out.row();
    out.text("PATH_NAME", path);
    out.text("OBJECT_TYPE", o.kind.as_str());
    out.text_or_null("SYMBOLIC_LINK", &o.link_target);
    out.int("ASP_NUMBER", 1);
    out.text_or_null("TEXT_DESCRIPTION", &o.text);
    out.int("FILE_IDENTIFIER_NUMBER", o.file_id);
    out.stamp("CREATE_TIMESTAMP", o.created);
    out.stamp("ACCESS_TIMESTAMP", o.accessed);
    out.stamp("DATA_CHANGE_TIMESTAMP", o.data_changed);
    out.stamp("OBJECT_CHANGE_TIMESTAMP", o.changed);
    out.int("ALLOCATED_SIZE", o.allocated);
    out.int("DATA_SIZE", o.size);
    if o.ccsid != 0 {
        out.int("CCSID", o.ccsid);
    }
    out.int("HARD_LINK_COUNT", o.links);
    out.yes("OBJECT_READ_ONLY", o.mode & 0o222 == 0);
    out.text("OBJECT_OWNER", o.owner.as_str());
    out.text_or_null("PRIMARY_GROUP", &o.group);
    out.text_or_null("AUTHORIZATION_LIST", &o.autl);
    out.yes("SET_EFFECTIVE_USER_ID", o.mode & 0o4000 != 0);
    out.yes("SET_EFFECTIVE_GROUP_ID", o.mode & 0o2000 != 0);
    out.text("OBJECT_DOMAIN", "*SYSTEM");
}

/// `IFS_OBJECT_STATISTICS(START_PATH_NAME, SUBTREE_DIRECTORIES,
/// OBJECT_TYPE_LIST, OMIT_LIST, IGNORE_ERRORS)`.
pub fn statistics(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, out: &mut Out) -> Result<()> {
    let Some(start) = args.text("START_PATH_NAME") else {
        return Ok(());
    };
    let subtree = args.word("SUBTREE_DIRECTORIES").is_none_or(|v| v == "YES");
    let types: Vec<String> = args.word("OBJECT_TYPE_LIST").unwrap_or_default().split_whitespace().map(str::to_string).collect();
    let omit: Vec<String> = args.text("OMIT_LIST").unwrap_or_default().split_whitespace().map(str::to_string).collect();
    let list = match ask(txn, ctx, "the IFS", |h| h.ifs_objects(start.trim(), subtree))? {
        Ok(l) => l,
        Err(_) if ignore(args) => return Ok(()),
        Err(e) => return Err(failed(e)),
    };
    for (path, o) in list {
        if omit.iter().any(|p| path == *p || path.starts_with(&format!("{p}/"))) {
            continue;
        }
        let o = match o {
            Ok(o) => o,
            Err(_) if ignore(args) => continue,
            Err(e) => return Err(failed(format!("{path}: {e}"))),
        };
        if !types.is_empty() && !types.iter().any(|t| *t == o.kind || (t == "*ALLDIR" && o.kind == "*DIR") || (t == "*ALLSTMF" && o.kind == "*STMF")) {
            continue;
        }
        object(out, &path, &o);
    }
    Ok(())
}

/// What a read breaks lines at.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Eol {
    Any,
    Crlf,
    Lf,
    Cr,
    Lfcr,
    None,
}

fn eol(args: &Args) -> Result<Eol> {
    Ok(match args.word("END_OF_LINE").as_deref() {
        None | Some("ANY") => Eol::Any,
        Some("CRLF") => Eol::Crlf,
        Some("LF") => Eol::Lf,
        Some("CR") => Eol::Cr,
        Some("LFCR") => Eol::Lfcr,
        Some("NONE") => Eol::None,
        Some(o) => return Err(SqlError::Coded { code: -171, state: "42815", what: format!("END_OF_LINE {o}") }),
    })
}

/// The lines of `data`: broken at `eol`, then at most `max` bytes each.
fn lines(data: &[u8], eol: Eol, max: usize, nl: u8) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut cur = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        let next = data.get(i + 1).copied();
        let brk = match eol {
            Eol::None => 0,
            Eol::Crlf => usize::from(b == b'\r' && next == Some(b'\n')) * 2,
            Eol::Lfcr => usize::from(b == b'\n' && next == Some(b'\r')) * 2,
            Eol::Lf => usize::from(b == b'\n' || b == nl),
            Eol::Cr => usize::from(b == b'\r'),
            Eol::Any => {
                if (b == b'\r' && next == Some(b'\n')) || (b == b'\n' && next == Some(b'\r')) {
                    2
                } else {
                    usize::from(b == b'\r' || b == b'\n' || b == nl)
                }
            }
        };
        if brk > 0 {
            out.push(std::mem::take(&mut cur));
            i += brk;
            continue;
        }
        cur.push(b);
        if cur.len() >= max {
            out.push(std::mem::take(&mut cur));
        }
        i += 1;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The three reads: text in the job's terms, binary, or UTF-8.
fn read(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, out: &mut Out, binary: bool) -> Result<()> {
    let Some(path) = args.text("PATH_NAME") else {
        return Ok(());
    };
    let eol = eol(args)?;
    let max = args.int("MAXIMUM_LINE_LENGTH").and_then(|n| usize::try_from(n).ok()).filter(|n| *n > 0).unwrap_or(usize::MAX);
    let (data, ccsid) = match ask(txn, ctx, "the IFS", |h| h.ifs_read(path.trim()))? {
        Ok(d) => d,
        Err(_) if ignore(args) => return Ok(()),
        Err(e) => return Err(failed(e)),
    };
    let codec = if binary { None } else { ccsid::conv::Codec::of(ccsid).ok().flatten() };
    // EBCDIC text ends lines with NL (X'15') as well.
    let nl = if codec.as_ref().is_some_and(ccsid::conv::Codec::is_ebcdic) { 0x15 } else { b'\n' };
    for (i, l) in lines(&data, eol, max, nl).into_iter().enumerate() {
        out.row();
        out.int("LINE_NUMBER", i + 1);
        let v = if binary {
            Value::Bin(l)
        } else {
            match &codec {
                Some(c) => {
                    let mut chars = Vec::new();
                    c.decode(&l, &mut chars);
                    Value::Str(chars.into_iter().collect())
                }
                None => Value::Str(String::from_utf8_lossy(&l).into_owned()),
            }
        };
        out.set("LINE", v);
    }
    Ok(())
}

/// `IFS_READ` and `IFS_READ_UTF8`.
pub fn read_text(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, out: &mut Out) -> Result<()> {
    read(txn, ctx, args, out, false)
}

/// `IFS_READ_BINARY`.
pub fn read_binary(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, out: &mut Out) -> Result<()> {
    read(txn, ctx, args, out, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_breaks() {
        let l = |d: &[u8], e| lines(d, e, usize::MAX, b'\n');
        assert_eq!(l(b"a\r\nb\nc", Eol::Any), vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
        assert_eq!(l(b"a\r\nb", Eol::Lf), vec![b"a\r".to_vec(), b"b".to_vec()]);
        assert_eq!(l(b"ab\ncd", Eol::None), vec![b"ab\ncd".to_vec()]);
        assert_eq!(lines(b"abcde", Eol::None, 2, b'\n'), vec![b"ab".to_vec(), b"cd".to_vec(), b"e".to_vec()]);
    }
}
