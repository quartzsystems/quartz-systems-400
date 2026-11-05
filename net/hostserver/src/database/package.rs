// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Extended dynamic packages (`*SQLPKG`), as the primary keeps them for
//! `JTOpen` (`tools/conformance/hostsvr/traces/database/sqlpkg.trace`):
//! create package answers SQL0601 when the package exists; a statement
//! prepared with the package's name goes into it the second time the
//! connection prepares it; return package sends the statements (name,
//! type, UTF-16 text, the columns and markers as SQLDAs); a client that
//! has the package opens or executes a statement by its package name
//! without preparing it. The package lives in the store through the
//! job's SQL session (`Op::Package`).

use hostsvr::cp::{db_reply, db_request};
use hostsvr::frame::Param;
use sql::embed::{Desc, Embedded, Op, Reply};
use sql::sqlca::Sqlca;
use sql::value::Value;

use super::types::{ColType, sqltype};
use super::{Answer, Database, Req, code, db_text};
use crate::exits::Exits;
use crate::session::Auth;
use crate::text;

/// The statement types the host server names (the request's 0x3812).
const TYPE_SELECT: u8 = 2;

/// An SQLDA as IBM lays it out: `SQLDA`, SQLDABC, SQLN, SQLD, then per
/// entry the type, length, 12 reserved bytes, two null pointers and the
/// name (length and 30 EBCDIC bytes). Empty for no entries.
pub fn sqlda(cols: &[ColType], markers: bool) -> Vec<u8> {
    if cols.is_empty() {
        return Vec::new();
    }
    let n = u16::try_from(cols.len()).unwrap_or(u16::MAX);
    let mut b = text::ebcdic_fixed("SQLDA", 8);
    b.extend((16 + 80 * u32::from(n)).to_be_bytes());
    b.extend(n.to_be_bytes());
    b.extend(n.to_be_bytes());
    for (i, c) in cols.iter().enumerate() {
        let len = match c.base {
            sqltype::DECIMAL | sqltype::NUMERIC => u32::from(c.precision) * 256 + u32::from(c.scale),
            sqltype::CHAR | sqltype::VARCHAR | sqltype::LONGVARCHAR | sqltype::GRAPHIC | sqltype::VARGRAPHIC => u32::from(c.scale),
            _ => c.len,
        };
        b.extend((c.base | u16::from(c.null)).to_be_bytes());
        b.extend(u16::try_from(len).unwrap_or(u16::MAX).to_be_bytes());
        b.extend([0u8; 44]);
        // A marker is named by its number, as the primary names it.
        let name = if markers || c.name.is_empty() { format!("{:05}", i + 1) } else { c.name.clone() };
        let name: String = name.chars().take(30).collect();
        b.extend(u16::try_from(name.len()).unwrap_or(30).to_be_bytes());
        b.extend(text::ebcdic_fixed(&name, 30));
    }
    b
}

/// The package information reply (0x380B) of `rows` (the session's
/// `RETURN`): the length, CCSID 13488, the default collection (18 bytes,
/// left zero), the count, 16 reserved bytes, a 64-byte entry per
/// statement (needs the collection, type, EBCDIC name, 19 reserved
/// bytes, offsets and lengths of the columns, the text and the markers
/// from the parameter's start), then the parts.
pub fn package_info(rows: &[Vec<Value>]) -> Vec<u8> {
    let stmts = rows.get(1..).unwrap_or(&[]);
    let mut entries = Vec::new();
    let mut parts = Vec::new();
    // Offsets count from the parameter's length field (6 bytes before the data).
    let base = 6 + 42 + 64 * stmts.len();
    let bin = |v: Option<&Value>| match v {
        Some(Value::Bin(b)) => b.clone(),
        _ => Vec::new(),
    };
    let off = |at: usize| u32::try_from(base + at).unwrap_or(0).to_be_bytes();
    let len = |n: usize| u32::try_from(n).unwrap_or(0).to_be_bytes();
    for s in stmts {
        let int = |i: usize| s.get(i).and_then(|v| v.as_i64().ok()).unwrap_or(0);
        let name = s.first().map(Value::as_text).unwrap_or_default();
        let utf16: Vec<u8> = s.get(2).map(Value::as_text).unwrap_or_default().encode_utf16().flat_map(u16::to_be_bytes).collect();
        let (cols, markers) = (bin(s.get(3)), bin(s.get(4)));
        entries.push(u8::try_from(int(5)).unwrap_or(0));
        entries.extend(u16::try_from(int(1)).unwrap_or(0).to_be_bytes());
        entries.extend(text::ebcdic_fixed(&name, 18));
        entries.extend([0u8; 19]);
        let t = parts.len();
        let f = t + utf16.len();
        let m = f + cols.len();
        entries.extend(off(f));
        entries.extend(len(cols.len()));
        entries.extend(off(t));
        entries.extend(len(utf16.len()));
        entries.extend(off(m));
        entries.extend(len(markers.len()));
        parts.extend(utf16);
        parts.extend(cols);
        parts.extend(markers);
    }
    let total = 42 + entries.len() + parts.len();
    let mut b = u32::try_from(total).unwrap_or(0).to_be_bytes().to_vec();
    b.extend(13488u16.to_be_bytes());
    b.extend([0u8; 18]);
    b.extend(u16::try_from(stmts.len()).unwrap_or(0).to_be_bytes());
    b.extend([0u8; 16]);
    b.extend(entries);
    b.extend(parts);
    b
}

impl<A: Auth, E: Embedded, X: Exits> Database<A, E, X> {
    /// Run a package action on `lib/name`.
    fn pkg_action(&mut self, action: &str, lib: &str, name: &str, more: Vec<Value>) -> Reply {
        let mut inputs = vec![Value::Str(lib.into()), Value::Str(name.into())];
        inputs.extend(more);
        let d = Desc {
            name: action.into(),
            ..self.desc(Op::Package)
        };
        self.run(d, inputs)
    }

    /// The package a request names: its 0x3804, and the library its
    /// 0x3801 gives or the package request last named.
    fn pkg_named(&self, params: &[Param]) -> Option<(String, String)> {
        let name = params.iter().find(|p| p.cp == db_request::PACKAGE_NAME).map(db_text)?;
        let lib = params
            .iter()
            .find(|p| p.cp == db_request::LIBRARY_NAME)
            .map(db_text)
            .or_else(|| self.package.as_ref().map(|p| p.0.clone()))
            .unwrap_or_else(|| "QGPL".into());
        Some((lib, name))
    }

    /// Create, return, clear or delete package.
    pub(super) fn package_request(&mut self, r: &Req, params: &[Param]) -> Answer {
        let Some((lib, name)) = self.pkg_named(params) else {
            return self.finish(r, code(-204, "42704"), Vec::new());
        };
        self.package = Some((lib.clone(), name.clone()));
        let (ca, extra) = match r.function {
            hostsvr::database::id::CREATE_PACKAGE => {
                let commit = if self.autocommit { 0 } else { [0, 2, 1, 3, 4].get(usize::from(self.commit)).copied().unwrap_or(0) };
                let attrs = vec![
                    Value::int(0),
                    Value::int(i64::from(self.sql_naming)),
                    Value::int(commit),
                    Value::int(i64::from(self.date_format)),
                    Value::Str("/".into()),
                    Value::int(0),
                    Value::Str(":".into()),
                    Value::Str(".".into()),
                    Value::Str(self.default_lib.clone()),
                    Value::Str(String::new()),
                ];
                (self.pkg_action("CREATE", &lib, &name, attrs).sqlca, Vec::new())
            }
            hostsvr::database::id::RETURN_PACKAGE => {
                let rep = self.pkg_action("RETURN", &lib, &name, Vec::new());
                if rep.sqlca.is_error() {
                    (rep.sqlca, Vec::new())
                } else {
                    self.pkg_stmts = rep.rows.iter().skip(1).map(|s| (s[0].as_text(), s[2].as_text())).collect();
                    (Sqlca::default(), vec![Param::new(db_reply::PACKAGE_INFO, package_info(&rep.rows))])
                }
            }
            hostsvr::database::id::CLEAR_PACKAGE => (self.pkg_action("CLEAR", &lib, &name, Vec::new()).sqlca, Vec::new()),
            _ => {
                self.package = None;
                (self.pkg_action("DELETE", &lib, &name, Vec::new()).sqlca, Vec::new())
            }
        };
        self.finish(r, ca, extra)
    }

    /// After a prepare that names the package: the second prepare of the
    /// same text on this connection puts the statement in it.
    pub(super) fn package_prepared(&mut self, rpb: &super::Rpb, params: &[Param]) {
        let Some((lib, name)) = self.pkg_named(params) else { return };
        let n = self.prepares.entry(rpb.text.clone()).or_insert(0);
        *n += 1;
        if *n != 2 {
            return;
        }
        let stmt_type = params
            .iter()
            .find(|p| p.cp == db_request::STATEMENT_TYPE)
            .and_then(Param::as_u16)
            .and_then(|t| u8::try_from(t).ok())
            .unwrap_or(if rpb.select { TYPE_SELECT } else { 1 });
        let more = vec![
            Value::int(i64::from(stmt_type)),
            Value::Str(rpb.text.clone()),
            Value::Bin(sqlda(&rpb.cols, false)),
            Value::Bin(sqlda(&rpb.markers, true)),
            Value::int(0),
        ];
        let rep = self.pkg_action("ADD", &lib, &name, more);
        if let Some(stmt) = rep.rows.first().and_then(|r| r.first()) {
            self.pkg_stmts.push((stmt.as_text(), rpb.text.clone()));
        }
    }

    /// An open or execute that names a package statement (0x3806 with
    /// 0x3804) the block has not prepared: its text, prepared now.
    pub(super) fn package_statement(&mut self, r: &Req, params: &[Param]) {
        if !params.iter().any(|p| p.cp == db_request::PACKAGE_NAME) {
            return;
        }
        let Some(stmt) = params.iter().find(|p| p.cp == db_request::STATEMENT_NAME).map(db_text) else { return };
        let mut text = self.pkg_stmts.iter().find(|(n, _)| n.eq_ignore_ascii_case(&stmt)).map(|(_, t)| t.clone());
        if text.is_none()
            && let Some((lib, name)) = self.pkg_named(params)
        {
            let rep = self.pkg_action("RETURN", &lib, &name, Vec::new());
            self.pkg_stmts = rep.rows.iter().skip(1).map(|s| (s[0].as_text(), s[2].as_text())).collect();
            text = self.pkg_stmts.iter().find(|(n, _)| n.eq_ignore_ascii_case(&stmt)).map(|(_, t)| t.clone());
        }
        let Some(text) = text else { return };
        let rpb = self.rpb(r);
        if rpb.text == text && (rpb.select || !rpb.markers.is_empty() || rpb.call.is_some()) {
            return;
        }
        if let Some(x) = self.rpbs.get_mut(&r.rpb) {
            x.text = text;
        }
        let _ = self.prepare(r, &[], false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_info_is_laid_out_as_the_primary_sends_it() {
        // The primary's reply to return package for one statement
        // (sqlpkg.trace): offsets count from the parameter's start.
        let n = ColType::from_describe(497, 4, "N");
        let cols = sqlda(std::slice::from_ref(&n), false);
        let markers = sqlda(&[ColType::from_describe(497, 4, "")], true);
        assert_eq!(cols.len(), 96);
        let text = "SELECT CAST(? AS INTEGER) + 1 AS N FROM SYSIBM.SYSDUMMY1";
        let rows = vec![
            Vec::new(),
            vec![
                Value::Str("QZB027CC62A9389001".into()),
                Value::int(2),
                Value::Str(text.into()),
                Value::Bin(cols),
                Value::Bin(markers),
                Value::int(0),
                Value::int(0),
            ],
        ];
        let b = package_info(&rows);
        assert_eq!(b.len(), 0x19A);
        assert_eq!(&b[..6], &[0, 0, 1, 0x9A, 0x34, 0xB0]);
        assert_eq!(&b[24..26], &[0, 1]);
        let e = &b[42..106];
        assert_eq!(&e[..3], &[0, 0, 2]);
        let words: Vec<u32> = e[40..].chunks(4).map(|c| u32::from_be_bytes([c[0], c[1], c[2], c[3]])).collect();
        assert_eq!(words, [0xE0, 0x60, 0x70, 0x70, 0x140, 0x60]);
        assert_eq!(&b[0xE0 - 6..0xE0 - 6 + 8], text::ebcdic("SQLDA   ").as_slice());
        // The column's entry: INTEGER null-capable, 4 bytes, named N.
        assert_eq!(&b[0xE0 - 6 + 16..0xE0 - 6 + 20], &[0x01, 0xF1, 0, 4]);
        assert_eq!(&b[0x140 - 6 + 64..0x140 - 6 + 66], &[0, 5]);
    }
}
