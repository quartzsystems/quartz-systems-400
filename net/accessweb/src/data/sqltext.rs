// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL the table pages and transfers run, with parameter markers
//! for every value: names are delimited identifiers, so any name the
//! catalog gives works and nothing typed becomes SQL.

/// `"NAME"`, quotes doubled.
pub fn ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// `"SCHEMA"."TABLE"`.
pub fn table(schema: &str, table: &str) -> String {
    format!("{}.{}", ident(schema), ident(table))
}

/// A `LIKE` condition on a column's text, upper case, for Find.
fn find(col: &str) -> String {
    format!("UPPER(CAST({} AS VARCHAR(1000))) LIKE ?", ident(col))
}

/// The pattern Find looks for: the text anywhere, upper case.
pub fn find_pattern(q: &str) -> String {
    format!("%{}%", q.trim().to_uppercase())
}

/// The rows a page counts.
pub fn count(schema: &str, t: &str, find_col: Option<&str>) -> String {
    let w = find_col.map(|c| format!(" WHERE {}", find(c))).unwrap_or_default();
    format!("SELECT COUNT(*) FROM {}{w}", table(schema, t))
}

/// One page of rows, sorted by `sort` and then the key columns.
pub fn page(schema: &str, t: &str, cols: &[String], find_col: Option<&str>, order: &[String], offset: usize, limit: usize) -> String {
    let list = cols.iter().map(|c| ident(c)).collect::<Vec<_>>().join(", ");
    let w = find_col.map(|c| format!(" WHERE {}", find(c))).unwrap_or_default();
    let o = if order.is_empty() {
        String::new()
    } else {
        format!(" ORDER BY {}", order.iter().map(|c| ident(c)).collect::<Vec<_>>().join(", "))
    };
    let off = if offset > 0 { format!(" OFFSET {offset} ROWS") } else { String::new() };
    format!("SELECT {list} FROM {}{w}{o}{off} FETCH FIRST {limit} ROWS ONLY", table(schema, t))
}

/// Every row of a table (a download), in key order.
pub fn all(schema: &str, t: &str, cols: &[String], order: &[String], find_col: Option<&str>) -> String {
    let list = cols.iter().map(|c| ident(c)).collect::<Vec<_>>().join(", ");
    let w = find_col.map(|c| format!(" WHERE {}", find(c))).unwrap_or_default();
    let o = if order.is_empty() {
        String::new()
    } else {
        format!(" ORDER BY {}", order.iter().map(|c| ident(c)).collect::<Vec<_>>().join(", "))
    };
    format!("SELECT {list} FROM {}{w}{o}", table(schema, t))
}

/// The highest value of a column.
pub fn max(schema: &str, t: &str, col: &str) -> String {
    format!("SELECT MAX({}) FROM {}", ident(col), table(schema, t))
}

/// The row with these key values.
fn by_key(keys: &[String]) -> String {
    keys.iter().map(|k| format!("{} = ?", ident(k))).collect::<Vec<_>>().join(" AND ")
}

/// One row by its key.
pub fn one(schema: &str, t: &str, cols: &[String], keys: &[String]) -> String {
    let list = cols.iter().map(|c| ident(c)).collect::<Vec<_>>().join(", ");
    format!("SELECT {list} FROM {} WHERE {}", table(schema, t), by_key(keys))
}

/// An insert of these columns.
pub fn insert(schema: &str, t: &str, cols: &[String]) -> String {
    let list = cols.iter().map(|c| ident(c)).collect::<Vec<_>>().join(", ");
    let marks = vec!["?"; cols.len()].join(", ");
    format!("INSERT INTO {} ({list}) VALUES ({marks})", table(schema, t))
}

/// An update of these columns of the row with the key's values (the
/// markers: the columns' values, then the key's).
pub fn update(schema: &str, t: &str, cols: &[String], keys: &[String]) -> String {
    let set = cols.iter().map(|c| format!("{} = ?", ident(c))).collect::<Vec<_>>().join(", ");
    format!("UPDATE {} SET {set} WHERE {}", table(schema, t), by_key(keys))
}

/// A delete of the row with the key's values.
pub fn delete(schema: &str, t: &str, keys: &[String]) -> String {
    format!("DELETE FROM {} WHERE {}", table(schema, t), by_key(keys))
}

/// Every row of a table removed (Replace all rows).
pub fn delete_all(schema: &str, t: &str) -> String {
    format!("DELETE FROM {}", table(schema, t))
}

/// Whether a statement only reads: `SELECT`, `VALUES`, `WITH`.
pub fn reads_only(statement: &str) -> bool {
    matches!(super::script::verb(statement).as_str(), "SELECT" | "VALUES" | "WITH")
}

/// SQL `LIKE` with `%` and `_`.
pub fn like(pattern: &str, s: &str) -> bool {
    fn go(p: &[char], s: &[char]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some('%') => (0..=s.len()).any(|i| go(&p[1..], &s[i..])),
            Some('_') => !s.is_empty() && go(&p[1..], &s[1..]),
            Some(c) => s.first() == Some(c) && go(&p[1..], &s[1..]),
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = s.chars().collect();
    go(&p, &s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text() {
        let cols = vec!["ASSET_TAG".to_string(), "COST".into()];
        let key = vec!["ASSET_TAG".to_string()];
        assert_eq!(
            page("HLINV", "ASSETS", &cols, Some("MODEL"), &key, 25, 25),
            "SELECT \"ASSET_TAG\", \"COST\" FROM \"HLINV\".\"ASSETS\" WHERE UPPER(CAST(\"MODEL\" AS VARCHAR(1000))) LIKE ? ORDER BY \"ASSET_TAG\" OFFSET 25 ROWS FETCH FIRST 25 ROWS ONLY"
        );
        assert_eq!(update("S", "T", &cols, &key), "UPDATE \"S\".\"T\" SET \"ASSET_TAG\" = ?, \"COST\" = ? WHERE \"ASSET_TAG\" = ?");
        assert_eq!(insert("S", "T", &cols), "INSERT INTO \"S\".\"T\" (\"ASSET_TAG\", \"COST\") VALUES (?, ?)");
        assert_eq!(ident("A\"B"), "\"A\"\"B\"");
        assert!(reads_only(" with x as (values 1) select * from x") && !reads_only("UPDATE t SET a = 1"));
        assert_eq!(find_pattern(" dell "), "%DELL%");
    }
}
