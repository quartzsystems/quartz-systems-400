// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The catalog functions: `SQLTables`, `SQLColumns` and
//! `SQLPrimaryKeys` run a query over the `SYSIBM` metadata views on the
//! statement, its result described as the oracle's CLI describes it
//! (pub400, `tools/conformance/oracle/sqlcli/`). Schema, table and
//! column arguments are `LIKE` patterns, except `SQLPrimaryKeys`'s.

use super::Cli;
use super::host::{Host, get_str};
use crate::consts::{rc, ty};
use crate::data::ColDesc;
use crate::model::handles::Ptr;

/// A string literal.
fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// One column of a catalog result.
fn col(name: &str, sql_type: i16, size: i32, nullable: bool) -> ColDesc {
    ColDesc { name: name.into(), sql_type, size, scale: 0, nullable, ccsid: 0 }
}

fn names(cols: &[ColDesc]) -> String {
    cols.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
}

/// `SQLTables`' result.
fn tables_cols() -> Vec<ColDesc> {
    vec![
        col("TABLE_CAT", ty::VARCHAR, 128, false),
        col("TABLE_SCHEM", ty::VARCHAR, 128, false),
        col("TABLE_NAME", ty::VARCHAR, 128, false),
        col("TABLE_TYPE", ty::VARCHAR, 128, false),
        col("REMARKS", ty::WVARCHAR, 2000, true),
    ]
}

/// `SQLColumns`' result.
fn columns_cols() -> Vec<ColDesc> {
    let s = |n: &str, null: bool| col(n, ty::SMALLINT, 2, null);
    let i = |n: &str, null: bool| col(n, ty::INTEGER, 4, null);
    vec![
        col("TABLE_CAT", ty::VARCHAR, 128, false),
        col("TABLE_SCHEM", ty::VARCHAR, 128, false),
        col("TABLE_NAME", ty::VARCHAR, 128, false),
        col("COLUMN_NAME", ty::VARCHAR, 128, false),
        s("DATA_TYPE", false),
        col("TYPE_NAME", ty::VARCHAR, 261, false),
        i("COLUMN_SIZE", true),
        i("BUFFER_LENGTH", false),
        s("DECIMAL_DIGITS", true),
        s("NUM_PREC_RADIX", true),
        s("NULLABLE", false),
        col("REMARKS", ty::WVARCHAR, 2000, true),
        col("COLUMN_DEF", ty::WVARCHAR, 2000, true),
        s("SQL_DATA_TYPE", false),
        s("SQL_DATETIME_SUB", true),
        i("CHAR_OCTET_LENGTH", true),
        i("ORDINAL_POSITION", false),
        col("IS_NULLABLE", ty::VARCHAR, 3, false),
    ]
}

/// `SQLPrimaryKeys`' result.
fn keys_cols() -> Vec<ColDesc> {
    vec![
        col("TABLE_CAT", ty::VARCHAR, 128, false),
        col("TABLE_SCHEM", ty::VARCHAR, 128, false),
        col("TABLE_NAME", ty::VARCHAR, 128, false),
        col("COLUMN_NAME", ty::VARCHAR, 128, false),
        col("KEY_SEQ", ty::SMALLINT, 2, false),
        col("PK_NAME", ty::VARCHAR, 128, false),
    ]
}

/// The conditions on `(column, pattern)`: a pattern that is null or
/// empty matches everything.
fn filter(conds: &[(&str, Option<String>, bool)]) -> String {
    let parts: Vec<String> = conds
        .iter()
        .filter_map(|(c, v, like)| {
            let v = v.as_ref().filter(|v| !v.is_empty())?;
            Some(if *like { format!("{c} LIKE {}", lit(v)) } else { format!("{c} = {}", lit(v)) })
        })
        .collect();
    if parts.is_empty() { String::new() } else { format!(" WHERE {}", parts.join(" AND ")) }
}

impl Cli {
    /// Run a catalog query on statement `h` with the oracle's columns.
    fn catalog(&mut self, host: &mut dyn Host, h: i32, text: &str, cols: Vec<ColDesc>) -> i32 {
        let Some(s) = self.handles.stmt(h) else { return rc::INVALID_HANDLE };
        if s.state == crate::model::handles::StmtState::Open {
            return self.fail(h, "HY010", "The statement has a cursor open.");
        }
        let dbc = s.dbc;
        let code = self.prepare_text(host, h, dbc, text);
        if code == rc::ERROR {
            return code;
        }
        if let Some(s) = self.handles.stmt(h)
            && s.cols.len() == cols.len()
        {
            s.cols = cols;
        }
        self.execute(host, h)
    }

    /// `SQLTables`: the catalog argument is not a filter; the types are
    /// a comma-separated list, quoted or not.
    #[allow(clippy::too_many_arguments)]
    pub fn tables(&mut self, host: &mut dyn Host, h: i32, _cat: Ptr, _catl: i16, sch: Ptr, schl: i16, tab: Ptr, tabl: i16, typ: Ptr, typl: i16) -> i32 {
        if !self.handles.is(h, crate::consts::htype::STMT) || !self.begin(h) {
            return rc::INVALID_HANDLE;
        }
        let sch = get_str(host, sch, i32::from(schl));
        let tab = get_str(host, tab, i32::from(tabl));
        let types = get_str(host, typ, i32::from(typl)).unwrap_or_default();
        let mut w = filter(&[("TABLE_SCHEM", sch, true), ("TABLE_NAME", tab, true)]);
        let list: Vec<String> = types.split(',').map(|t| t.trim().trim_matches('\'').trim().to_ascii_uppercase()).filter(|t| !t.is_empty()).collect();
        if !list.is_empty() {
            let inl = list.iter().map(|t| lit(t)).collect::<Vec<_>>().join(", ");
            w = if w.is_empty() { format!(" WHERE TABLE_TYPE IN ({inl})") } else { format!("{w} AND TABLE_TYPE IN ({inl})") };
        }
        let cols = tables_cols();
        let text = format!("SELECT {} FROM SYSIBM.SQLTABLES{w} ORDER BY TABLE_TYPE, TABLE_SCHEM, TABLE_NAME", names(&cols));
        self.catalog(host, h, &text, cols)
    }

    /// `SQLColumns`.
    #[allow(clippy::too_many_arguments)]
    pub fn columns(&mut self, host: &mut dyn Host, h: i32, _cat: Ptr, _catl: i16, sch: Ptr, schl: i16, tab: Ptr, tabl: i16, colp: Ptr, coll: i16) -> i32 {
        if !self.handles.is(h, crate::consts::htype::STMT) || !self.begin(h) {
            return rc::INVALID_HANDLE;
        }
        let w = filter(&[
            ("TABLE_SCHEM", get_str(host, sch, i32::from(schl)), true),
            ("TABLE_NAME", get_str(host, tab, i32::from(tabl)), true),
            ("COLUMN_NAME", get_str(host, colp, i32::from(coll)), true),
        ]);
        let cols = columns_cols();
        let text = format!("SELECT {} FROM SYSIBM.SQLCOLUMNS{w} ORDER BY TABLE_SCHEM, TABLE_NAME, ORDINAL_POSITION", names(&cols));
        self.catalog(host, h, &text, cols)
    }

    /// `SQLPrimaryKeys`: the table is named, not a pattern; without one,
    /// every key of the schema.
    #[allow(clippy::too_many_arguments)]
    pub fn primary_keys(&mut self, host: &mut dyn Host, h: i32, _cat: Ptr, _catl: i16, sch: Ptr, schl: i16, tab: Ptr, tabl: i16) -> i32 {
        if !self.handles.is(h, crate::consts::htype::STMT) || !self.begin(h) {
            return rc::INVALID_HANDLE;
        }
        let tab = get_str(host, tab, i32::from(tabl));
        let w = filter(&[("TABLE_SCHEM", get_str(host, sch, i32::from(schl)), false), ("TABLE_NAME", tab, false)]);
        let cols = keys_cols();
        let text = format!("SELECT {} FROM SYSIBM.SQLPRIMARYKEYS{w} ORDER BY TABLE_SCHEM, TABLE_NAME, KEY_SEQ", names(&cols));
        self.catalog(host, h, &text, cols)
    }
}
