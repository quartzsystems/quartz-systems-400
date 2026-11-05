// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL names and system names: the system name an identifier has, the
//! names IBM generates for long ones, and the SQL names objects keep in
//! their `*SQLINF` and their library's name index. The rules are the
//! primary oracle's (`tools/conformance/oracle/sql-ddl/README.md`).

use db::sqlinf::names::{self, Space, Target};
use db::sqlinf::{self, Kind, SqlInfo, tag};
use qsname::Name;
use store::obj::library::Library;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use crate::ast::Ident;
use crate::error::{Result, SqlError};

/// The most characters of an SQL name.
pub const SQL_NAME_MAX: usize = 128;

/// The system name an identifier is as it stands: a regular name of ten
/// characters or fewer, or a delimited one that fits with its quotes.
#[must_use]
pub fn direct(id: &Ident) -> Option<Name> {
    let v = &id.value;
    if v.is_empty() || v.len() > 10 && !id.delimited {
        return None;
    }
    if let Ok(n) = Name::parse(v)
        && n.to_string() == *v
    {
        return Some(n);
    }
    if id.delimited {
        return Name::parse(&format!("\"{v}\"")).ok();
    }
    None
}

/// Whether a character may follow the first of an unquoted name.
fn name_char(c: char) -> bool {
    c.is_ascii_uppercase() || c.is_ascii_digit() || "$#@_".contains(c)
}

/// The candidates of a generated object name: the first five characters
/// and five digits, or for a name that cannot start an unquoted one,
/// four characters (blanks as `_`) and four digits in quotes.
fn object_candidate(sql: &str, n: u32) -> Option<Name> {
    let five: String = sql.chars().take(5).collect();
    let first = five.chars().next()?;
    if (first.is_ascii_uppercase() || "$#@".contains(first)) && five.chars().all(name_char) {
        return Name::parse(&format!("{five}{n:05}")).ok();
    }
    let four: String = sql.chars().take(4).map(|c| if c == ' ' { '_' } else { c }).collect();
    // Quoted only when the name could not stand unquoted (`"D4 M"` is `D4_M0001`).
    let first = four.chars().next()?;
    if (first.is_ascii_uppercase() || "$#@".contains(first)) && four.chars().all(name_char) {
        return Name::parse(&format!("{four}{n:04}")).ok();
    }
    Name::parse(&format!("\"{four}{n:04}\"")).ok()
}

/// Whether `sql` takes the four-character form of a generated name.
fn four_form(sql: &str) -> bool {
    let five: String = sql.chars().take(5).collect();
    !five.chars().next().is_some_and(|c| c.is_ascii_uppercase() || "$#@".contains(c)) || !five.chars().all(name_char)
}

/// A system name for a long SQL name: the lowest number whose name no
/// object of `type_code` in `lib` has and `taken` does not refuse.
pub fn generate(
    txn: &mut Txn<'_>,
    lib: ObjectId,
    type_code: u16,
    sql: &str,
    taken: &dyn Fn(&Name) -> bool,
) -> Result<Name> {
    let l = Library::load(txn, lib)?;
    let limit = if four_form(sql) { 9999 } else { 99_999 };
    for n in 1..=limit {
        let Some(cand) = object_candidate(sql, n) else {
            break;
        };
        if !taken(&cand) && l.lookup(txn, type_code, cand)?.is_none() {
            return Ok(cand);
        }
    }
    Err(SqlError::Unsupported { what: format!("a system name for {sql}: the names it could have are taken") })
}

/// A column's generated system name: the SQL name upper cased, its
/// first five characters and five digits, the lowest `taken` refuses
/// not. A character no system name may have is `_`, a first one `Q`
/// (no oracle case has one).
#[must_use]
pub fn generate_column(sql: &str, taken: &dyn Fn(&Name) -> bool) -> Name {
    let upper: String = sql.to_uppercase().chars().take(5).collect();
    let mut five: String = upper.chars().map(|c| if name_char(c) { c } else { '_' }).collect();
    if !five.starts_with(|c: char| c.is_ascii_uppercase() || "$#@".contains(c)) {
        five.replace_range(..five.chars().next().map_or(0, char::len_utf8), "Q");
    }
    if five.is_empty() {
        five.push('Q');
    }
    (1..=99_999)
        .filter_map(|n| Name::parse(&format!("{five}{n:05}")).ok())
        .find(|c| !taken(c))
        .unwrap_or_else(|| Name::parse("Q").expect("valid"))
}

/// Refuse an SQL name over 128 characters (`SQL0107`).
pub fn check_length(id: &Ident) -> Result<()> {
    if id.value.chars().count() > SQL_NAME_MAX {
        return Err(SqlError::NameTooLong { name: id.value.clone(), max: SQL_NAME_MAX });
    }
    Ok(())
}

/// `FOR SYSTEM NAME` is only for an SQL name that is not a system name
/// itself (`SQL7029`).
pub fn check_for_system(id: &Ident, system: Option<&Ident>) -> Result<()> {
    if let Some(s) = system
        && direct(id).is_some()
    {
        return Err(SqlError::SystemName { name: s.value.clone() });
    }
    Ok(())
}

/// The system name of a new object in `lib`: `FOR SYSTEM NAME`, the
/// name itself, or one generated; and the SQL name to keep when it is
/// not the system name.
pub fn new_object(
    txn: &mut Txn<'_>,
    lib: ObjectId,
    id: &Ident,
    system: Option<&Ident>,
) -> Result<(Name, Option<String>)> {
    check_length(id)?;
    check_for_system(id, system)?;
    if let Some(s) = system {
        let n = direct(s).ok_or_else(|| SqlError::SystemName { name: s.value.clone() })?;
        return Ok((n, Some(id.value.clone())));
    }
    if let Some(n) = direct(id) {
        return Ok((n, None));
    }
    if names::find(txn, lib, Space::Table, &id.value)?.is_some_and(|t| alive(txn, t))
        || names::find(txn, lib, Space::Index, &id.value)?.is_some_and(|t| alive(txn, t))
    {
        return Err(SqlError::Exists { name: id.value.clone(), kind: "*FILE" });
    }
    Ok((generate(txn, lib, types::FILE, &id.value, &|_| false)?, Some(id.value.clone())))
}

/// Whether an index entry still names an object of that name: an object
/// deleted outside SQL leaves its entry behind.
fn alive(txn: &mut Txn<'_>, t: Target) -> bool {
    ops::read_header(txn, t.id).is_ok()
}

/// The object an SQL name stands for in `lib`.
pub fn find(txn: &mut Txn<'_>, lib: ObjectId, space: Space, sql: &str) -> Result<Option<ObjectId>> {
    match names::find(txn, lib, space, sql)? {
        Some(t) if alive(txn, t) => Ok(Some(t.id)),
        _ => Ok(None),
    }
}

/// Keep what SQL knows of a new object: its kind, and its SQL name when
/// that is not its system name (in its `*SQLINF` and the name index).
pub fn record(
    txn: &mut Txn<'_>,
    lib: ObjectId,
    id: ObjectId,
    kind: Kind,
    sql: Option<&str>,
    now: u64,
) -> Result<SqlInfo> {
    let mut info = sqlinf::load(txn, id)?.unwrap_or_else(|| SqlInfo::new(kind));
    info.kind = kind;
    if let Some(s) = sql {
        info.set(tag::NAME, s.as_bytes().to_vec());
        let (_, h) = ops::read_header(txn, id)?;
        names::put(txn, lib, kind.space(), s, Target { id, system: h.name })?;
    }
    sqlinf::save(txn, id, &info, now)?;
    Ok(info)
}

/// The SQL name of an object: its `NAME` entry, else its system name.
pub fn sql_name(txn: &mut Txn<'_>, id: ObjectId) -> String {
    if let Ok(Some(info)) = sqlinf::load(txn, id)
        && let Some(n) = info.name()
    {
        return n;
    }
    ops::read_header(txn, id).map(|(_, h)| unquoted(h.name)).unwrap_or_default()
}

/// A system name as an SQL name: a quoted one without its quotes.
#[must_use]
pub fn unquoted(n: Name) -> String {
    let s = n.to_string();
    match s.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
        Some(inner) => inner.to_string(),
        None => s,
    }
}

/// What SQL kind an object was made as, `None` for one SQL did not make.
pub fn kind_of(txn: &mut Txn<'_>, id: ObjectId) -> Option<Kind> {
    sqlinf::load(txn, id).ok().flatten().map(|i| i.kind)
}

/// Give an object a new SQL name (`RENAME`): the old entry goes, the new
/// one is kept unless the new name is the system name.
pub fn rename(
    txn: &mut Txn<'_>,
    lib: ObjectId,
    id: ObjectId,
    kind: Kind,
    sql: Option<&str>,
    now: u64,
) -> Result<()> {
    names::forget(txn, id)?;
    let mut info = sqlinf::load(txn, id)?.unwrap_or_else(|| SqlInfo::new(kind));
    info.remove(tag::NAME);
    sqlinf::save(txn, id, &info, now)?;
    record(txn, lib, id, info.kind, sql, now)?;
    Ok(())
}

/// The library of a schema by its SQL name: a system name, else QSYS's
/// index of schema names.
pub fn schema(txn: &mut Txn<'_>, qsys: ObjectId, id: &Ident) -> Result<Option<ObjectId>> {
    if let Some(n) = direct(id) {
        return Ok(ops::find_library(txn, qsys, n).ok().map(|f| f.id));
    }
    find(txn, qsys, Space::Schema, &id.value)
}

/// The SQL name of a library: its schema name in QSYS's index, else its
/// system name. (A `*LIB` owns no `*SQLINF`: the index is all there is.)
pub fn schema_name(txn: &mut Txn<'_>, lib: ObjectId) -> String {
    let Ok((_, h)) = ops::read_header(txn, lib) else {
        return String::new();
    };
    if h.library != lib
        && let Ok(all) = names::list(txn, h.library, Space::Schema)
        && let Some((n, _)) = all.into_iter().find(|(_, t)| t.id == lib)
    {
        return n;
    }
    h.name.to_string()
}

/// Keep a schema's SQL name in QSYS's index.
pub fn record_schema(txn: &mut Txn<'_>, qsys: ObjectId, lib: ObjectId, sql: &str) -> Result<()> {
    let (_, h) = ops::read_header(txn, lib)?;
    names::put(txn, qsys, Space::Schema, sql, Target { id: lib, system: h.name })?;
    Ok(())
}

/// The record format name of a new table: its system name, or for a
/// quoted one (which DDS cannot name a format) the name inside upper
/// cased when that is a system name, else `QSQFMT`.
#[must_use]
pub fn format_name(name: Name) -> Name {
    if !name.is_quoted() {
        return name;
    }
    let inner = name.to_string().trim_matches('"').to_uppercase().replace(' ', "_");
    Name::parse(&inner).unwrap_or_else(|_| Name::parse("QSQFMT").expect("valid"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str, delimited: bool) -> Ident {
        Ident { value: s.into(), delimited }
    }

    #[test]
    fn direct_names() {
        assert_eq!(direct(&id("CUSTOMER", false)).unwrap().to_string(), "CUSTOMER");
        assert!(direct(&id("CUSTOMER_MASTER", false)).is_none());
        assert_eq!(direct(&id("lowtab", true)).unwrap().to_string(), "\"lowtab\"");
        assert_eq!(direct(&id("ABC", true)).unwrap().to_string(), "ABC");
        assert!(direct(&id("a long lower name", true)).is_none());
    }

    #[test]
    fn generated_names_follow_the_oracle() {
        assert_eq!(object_candidate("CUSTOMER_MASTER_FILE", 1).unwrap().to_string(), "CUSTO00001");
        assert_eq!(object_candidate("A_LONGER_NAME_X", 1).unwrap().to_string(), "A_LON00001");
        assert_eq!(object_candidate("a long lower name", 1).unwrap().to_string(), "\"a_lo0001\"");
        assert_eq!(object_candidate("D4 Monthly Totals", 1).unwrap().to_string(), "D4_M0001");
        let none = |_: &Name| false;
        assert_eq!(generate_column("lower", &none).to_string(), "LOWER00001");
        assert_eq!(generate_column("Mixed Case Column", &none).to_string(), "MIXED00001");
        assert_eq!(generate_column("A_B_C_D_E_F", &none).to_string(), "A_B_C00001");
        let one = |n: &Name| n.to_string() == "CUSTO00001";
        assert_eq!(generate_column("CUSTOMER_NAME_LONG", &one).to_string(), "CUSTO00002");
    }
}
