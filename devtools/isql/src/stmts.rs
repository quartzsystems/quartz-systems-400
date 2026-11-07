// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The statements of the prompts in [`crate::screen::stmt`] built from
//! their fields, and a typed statement's name read back into them.
//! Where IBM's prompt goes on to a display the captures do not hold
//! (ALTER TABLE's action, the object COMMENT ON, LABEL ON and RENAME
//! name, GRANT's and REVOKE's privileges, CONNECT's and RELEASE's
//! server), the statement is built as far as the first display takes
//! it and left on the input lines to be finished there.

use std::fmt::Write as _;

use crate::IsqlError;
use crate::assemble::{Kind, Values, qualified};

/// Whether `kind` is one of these.
#[must_use]
pub fn has(kind: Kind) -> bool {
    crate::screen::stmt::of(kind).is_some()
}

fn get(v: &Values, k: &str) -> String {
    v.get(k).map(|s| s.trim().to_string()).unwrap_or_default()
}

fn need(v: &Values, k: &'static str) -> Result<String, IsqlError> {
    let s = get(v, k);
    if s.is_empty() {
        Err(IsqlError::Missing(k))
    } else {
        Ok(s)
    }
}

/// A choice among `words` (`1` the first); `None` when blank.
fn choice(
    v: &Values,
    k: &'static str,
    words: &[&'static str],
) -> Result<Option<&'static str>, IsqlError> {
    let s = get(v, k);
    if s.is_empty() {
        return Ok(None);
    }
    s.parse::<usize>()
        .ok()
        .and_then(|n| n.checked_sub(1))
        .and_then(|i| words.get(i).copied())
        .map(Some)
        .ok_or(IsqlError::BadValue { field: k, value: s })
}

fn chosen(v: &Values, k: &'static str, words: &[&'static str]) -> Result<&'static str, IsqlError> {
    choice(v, k, words)?.ok_or(IsqlError::Missing(k))
}

/// The lines `NAME1` .. `NAME3` typed, joined with commas.
fn lines(v: &Values, k: &'static str) -> Result<String, IsqlError> {
    let all: Vec<String> = (1..=3)
        .map(|i| get(v, &format!("{k}{i}")))
        .filter(|s| !s.is_empty())
        .collect();
    if all.is_empty() {
        Err(IsqlError::Missing(k))
    } else {
        Ok(all.join(", "))
    }
}

fn yes(v: &Values, k: &str) -> bool {
    get(v, k).eq_ignore_ascii_case("Y")
}

/// The statement from a prompt's values.
///
/// # Errors
/// A value the statement needs is missing or not one of its choices.
#[allow(clippy::too_many_lines)] // one arm per statement
pub fn build(kind: Kind, v: &Values, sql: bool) -> Result<String, IsqlError> {
    let q = |name: &'static str, lib: &str| -> Result<String, IsqlError> {
        Ok(qualified(&get(v, lib), &need(v, name)?, sql))
    };
    let drop_option = |s: String| -> Result<String, IsqlError> {
        Ok(match choice(v, "OPT", &["CASCADE", "RESTRICT"])? {
            Some(o) => format!("{s} {o}"),
            None => s,
        })
    };
    Ok(match kind {
        Kind::AlterTable => {
            let action = chosen(
                v,
                "ACTION",
                &[
                    "ADD CONSTRAINT",
                    "DROP CONSTRAINT",
                    "ADD COLUMN",
                    "ALTER COLUMN",
                    "DROP COLUMN",
                ],
            )?;
            format!("ALTER TABLE {} {action} ", q("TABLE", "LIB")?)
        }
        Kind::Call => {
            let proc = q("PROC", "LIB")?;
            let typed: Vec<String> = (1..=crate::screen::stmt::CALL_PARMS)
                .map(|i| get(v, &format!("PARM{i}")))
                .collect();
            let n = typed
                .iter()
                .rposition(|p| !p.is_empty())
                .map_or(0, |i| i + 1);
            if typed[..n].iter().any(String::is_empty) {
                return Err(IsqlError::Missing("PARM"));
            }
            if n == 0 {
                format!("CALL {proc}")
            } else {
                format!("CALL {proc}({})", typed[..n].join(", "))
            }
        }
        Kind::CommentOn => {
            let what = chosen(
                v,
                "OBJ",
                &[
                    "TABLE",
                    "COLUMN",
                    "PACKAGE",
                    "INDEX",
                    "PROCEDURE",
                    "PARAMETER",
                    "ALIAS",
                ],
            )?;
            format!("COMMENT ON {what} ")
        }
        Kind::LabelOn => format!(
            "LABEL ON {} ",
            chosen(v, "OBJ", &["TABLE", "COLUMN", "PACKAGE", "ALIAS"])?
        ),
        Kind::Rename => format!("RENAME {} ", chosen(v, "OBJ", &["TABLE", "INDEX"])?),
        Kind::Commit | Kind::Rollback => {
            let word = if kind == Kind::Commit {
                "COMMIT"
            } else {
                "ROLLBACK"
            };
            if yes(v, "HOLD") {
                format!("{word} HOLD")
            } else {
                word.to_string()
            }
        }
        Kind::Connect => match chosen(v, "CONN", &["1", "2", "3"])? {
            "1" => "CONNECT TO ".into(),
            "2" => "CONNECT RESET".into(),
            _ => "CONNECT".into(),
        },
        Kind::Disconnect | Kind::Release => {
            let word = if kind == Kind::Release {
                "RELEASE"
            } else {
                "DISCONNECT"
            };
            match chosen(v, "CONN", &["", "CURRENT", "ALL"])? {
                "" => format!("{word} "),
                which => format!("{word} {which}"),
            }
        }
        Kind::CreateAlias => {
            let mut s = format!(
                "CREATE ALIAS {} FOR {}",
                q("ALIAS", "LIB")?,
                q("TABLE", "TABLELIB")?
            );
            let m = get(v, "MEMBER");
            if !m.is_empty() {
                let _ = write!(s, " ({m})");
            }
            s
        }
        Kind::CreateCollection => {
            let mut s = format!("CREATE COLLECTION {}", need(v, "LIB")?);
            let asp = get(v, "ASP");
            if !asp.is_empty() {
                let _ = write!(s, " IN ASP {asp}");
            }
            if yes(v, "DICT") {
                s.push_str(" WITH DATA DICTIONARY");
            }
            s
        }
        Kind::CreateProcedure => {
            let mut s = format!("CREATE PROCEDURE {}", q("PROC", "LIB")?);
            let sets = get(v, "RESULT");
            if !sets.is_empty() {
                let _ = write!(s, " RESULT SETS {sets}");
            }
            let lang = get(v, "LANG").to_ascii_uppercase();
            if !lang.is_empty() {
                let _ = write!(s, " LANGUAGE {lang}");
            }
            if let Some(style) = choice(
                v,
                "STYLE",
                &["GENERAL", "GENERAL WITH NULLS", "SQL", "JAVA", "DB2GENERAL"],
            )? {
                let _ = write!(s, " PARAMETER STYLE {style}");
            }
            s
        }
        Kind::DropAlias => format!("DROP ALIAS {}", q("ALIAS", "LIB")?),
        Kind::DropIndex => format!("DROP INDEX {}", q("INDEX", "LIB")?),
        Kind::DropPackage => format!("DROP PACKAGE {}", q("PKG", "LIB")?),
        Kind::DropTable => drop_option(format!("DROP TABLE {}", q("TABLE", "LIB")?))?,
        Kind::DropView => drop_option(format!("DROP VIEW {}", q("VIEW", "LIB")?))?,
        Kind::DropCollection => drop_option(format!("DROP COLLECTION {}", need(v, "LIB")?))?,
        Kind::GrantPackage => format!(
            "GRANT  ON PACKAGE {} TO {}",
            lines(v, "PKG")?,
            lines(v, "USER")?
        ),
        Kind::RevokePackage => format!(
            "REVOKE  ON PACKAGE {} FROM {}",
            lines(v, "PKG")?,
            lines(v, "USER")?
        ),
        Kind::GrantTable => format!(
            "GRANT  ON TABLE {} TO {}",
            lines(v, "TABLE")?,
            lines(v, "USER")?
        ),
        Kind::RevokeTable => format!(
            "REVOKE  ON TABLE {} FROM {}",
            lines(v, "TABLE")?,
            lines(v, "USER")?
        ),
        Kind::LockTable => {
            let mode = chosen(v, "MODE", &["SHARE", "EXCLUSIVE"])?;
            format!("LOCK TABLE {} IN {mode} MODE", q("TABLE", "LIB")?)
        }
        Kind::SetConnection => format!("SET CONNECTION {}", need(v, "RDB")?),
        Kind::SetTransaction => {
            let level = chosen(
                v,
                "ISOL",
                &[
                    "NO COMMIT",
                    "READ UNCOMMITTED",
                    "READ COMMITTED",
                    "REPEATABLE READ",
                    "SERIALIZABLE",
                ],
            )?;
            format!("SET TRANSACTION ISOLATION LEVEL {level}")
        }
        Kind::Select
        | Kind::Insert
        | Kind::Update
        | Kind::Delete
        | Kind::CreateTable
        | Kind::CreateView
        | Kind::CreateIndex => return crate::assemble::build(kind, v, sql),
    })
}

/// The field a statement's name goes in, the library's, and the words
/// before it.
fn name_fields(kind: Kind) -> Option<(&'static str, Option<&'static str>, usize)> {
    Some(match kind {
        Kind::AlterTable | Kind::DropTable | Kind::LockTable => ("TABLE", Some("LIB"), 2),
        Kind::DropView => ("VIEW", Some("LIB"), 2),
        Kind::DropIndex => ("INDEX", Some("LIB"), 2),
        Kind::DropAlias | Kind::CreateAlias => ("ALIAS", Some("LIB"), 2),
        Kind::DropPackage => ("PKG", Some("LIB"), 2),
        Kind::CreateProcedure => ("PROC", Some("LIB"), 2),
        Kind::Call => ("PROC", Some("LIB"), 1),
        Kind::CreateCollection | Kind::DropCollection => ("LIB", None, 2),
        Kind::SetConnection => ("RDB", None, 2),
        _ => return None,
    })
}

/// A typed statement's name (and `HOLD`) in its prompt's fields.
#[must_use]
pub fn fields_of(kind: Kind, text: &str) -> Values {
    let mut v = Values::new();
    let words: Vec<&str> = text.split_whitespace().collect();
    if let Some((name, lib, skip)) = name_fields(kind)
        && let Some(w) = words.get(skip)
    {
        let w = w.split('(').next().unwrap_or("");
        match (w.split_once(['/', '.']), lib) {
            (Some((l, o)), Some(lib)) => {
                v.insert(name.to_string(), o.to_string());
                v.insert(lib.to_string(), l.to_string());
            }
            _ if !w.is_empty() => {
                v.insert(name.to_string(), w.to_string());
            }
            _ => {}
        }
    }
    if matches!(kind, Kind::Commit | Kind::Rollback)
        && words.iter().any(|w| w.eq_ignore_ascii_case("HOLD"))
    {
        v.insert("HOLD".into(), "Y".into());
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vals(p: &[(&str, &str)]) -> Values {
        p.iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn statements() {
        let b = |k, p: &[(&str, &str)]| build(k, &vals(p), false).unwrap();
        assert_eq!(
            b(
                Kind::DropTable,
                &[("TABLE", "T"), ("LIB", "QGPL"), ("OPT", "2")]
            ),
            "DROP TABLE QGPL/T RESTRICT"
        );
        assert_eq!(
            b(
                Kind::Call,
                &[("PROC", "P"), ("PARM1", "1"), ("PARM2", "'A'")]
            ),
            "CALL P(1, 'A')"
        );
        assert_eq!(b(Kind::Commit, &[("HOLD", "Y")]), "COMMIT HOLD");
        assert_eq!(b(Kind::Connect, &[("CONN", "2")]), "CONNECT RESET");
        assert_eq!(b(Kind::Release, &[("CONN", "3")]), "RELEASE ALL");
        assert_eq!(
            b(Kind::LockTable, &[("TABLE", "T"), ("MODE", "2")]),
            "LOCK TABLE T IN EXCLUSIVE MODE"
        );
        assert_eq!(
            b(Kind::SetTransaction, &[("ISOL", "3")]),
            "SET TRANSACTION ISOLATION LEVEL READ COMMITTED"
        );
        assert_eq!(
            b(
                Kind::CreateAlias,
                &[
                    ("ALIAS", "A"),
                    ("LIB", "L"),
                    ("TABLE", "T"),
                    ("MEMBER", "M2")
                ]
            ),
            "CREATE ALIAS L/A FOR T (M2)"
        );
        assert_eq!(
            b(
                Kind::GrantTable,
                &[("TABLE1", "T1"), ("TABLE2", "T2"), ("USER1", "PUBLIC")]
            ),
            "GRANT  ON TABLE T1, T2 TO PUBLIC"
        );
        assert_eq!(
            b(Kind::AlterTable, &[("TABLE", "T"), ("ACTION", "3")]),
            "ALTER TABLE T ADD COLUMN "
        );
        assert!(matches!(
            build(Kind::Call, &vals(&[("PROC", "P"), ("PARM2", "1")]), false),
            Err(IsqlError::Missing("PARM"))
        ));
        assert!(matches!(
            build(
                Kind::LockTable,
                &vals(&[("TABLE", "T"), ("MODE", "9")]),
                false
            ),
            Err(IsqlError::BadValue { .. })
        ));
        assert_eq!(
            build(Kind::DropView, &vals(&[("VIEW", "V"), ("LIB", "L")]), true).unwrap(),
            "DROP VIEW L.V"
        );
    }

    #[test]
    fn typed() {
        let v = fields_of(Kind::DropTable, "drop table qgpl/t1 cascade");
        assert_eq!(
            (
                v.get("TABLE").map(String::as_str),
                v.get("LIB").map(String::as_str)
            ),
            (Some("t1"), Some("qgpl"))
        );
        let v = fields_of(Kind::Call, "call p1(1, 2)");
        assert_eq!(v.get("PROC").map(String::as_str), Some("p1"));
        assert_eq!(
            fields_of(Kind::Commit, "commit hold")
                .get("HOLD")
                .map(String::as_str),
            Some("Y")
        );
    }
}
