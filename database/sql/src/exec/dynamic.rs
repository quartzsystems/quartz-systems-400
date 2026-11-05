// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a dynamic interface may run: the text of `PREPARE` and `EXECUTE
//! IMMEDIATE`, a JDBC or DRDA statement, a `RUNSQLSTM` script, STRSQL.
//! The statements only a precompiled program or a routine takes (cursor
//! statements, `DESCRIBE`, `WHENEVER`, locators, host variables) are
//! refused as the primary refuses them
//! (`tools/conformance/sql/cases/dyn/NOPREP.sql`).

use super::{Prepared, prepare};
use crate::ast::{Naming, Statement};
use crate::error::{Result, SqlError};

/// `SQL0312`: a host variable in a dynamic statement.
fn host_variable(name: &str) -> SqlError {
    SqlError::Coded {
        code: -312,
        state: "42618",
        what: format!("host variable :{name} has no meaning in a dynamic statement"),
    }
}

fn cannot_prepare(what: &str) -> SqlError {
    SqlError::Coded {
        code: -84,
        state: "42612",
        what: format!("{what} cannot be prepared: only a precompiled program or a routine runs it"),
    }
}

/// The first host variable of `text` (`:name` outside strings, delimited
/// names and comments).
fn first_host(text: &str) -> Option<String> {
    let b: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            q @ ('\'' | '"') => {
                i += 1;
                while i < b.len() {
                    if b[i] == q {
                        if b.get(i + 1) == Some(&q) {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
            }
            '-' if b.get(i + 1) == Some(&'-') => {
                while i < b.len() && b[i] != '\n' {
                    i += 1;
                }
            }
            '/' if b.get(i + 1) == Some(&'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                    i += 1;
                }
                i += 1;
            }
            ':' if b.get(i + 1).is_some_and(|c| c.is_alphabetic() || matches!(c, '_' | '$' | '#' | '@')) => {
                let name: String = b[i + 1..]
                    .iter()
                    .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '#' | '@' | '.'))
                    .collect();
                return Some(name.to_ascii_uppercase());
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The leading words of `text`, upper case.
fn words(text: &str, n: usize) -> Vec<String> {
    text.split(|c: char| c.is_whitespace() || c == '(' || c == ';')
        .filter(|w| !w.is_empty())
        .take(n)
        .map(str::to_ascii_uppercase)
        .collect()
}

/// Refuse what a dynamic interface cannot run.
///
/// # Errors
/// `SQL0312` for a host variable, `SQL0104` for `PREPARE` and `EXECUTE
/// IMMEDIATE`, `SQL0199` for `DEALLOCATE`, `SQL0084` for the other
/// embedded-only statements.
pub fn check(text: &str) -> Result<()> {
    let w = words(text, 3);
    let w0 = w.first().map_or("", String::as_str);
    let w1 = w.get(1).map_or("", String::as_str);
    // A routine's or trigger's body keeps its own variables.
    if !matches!(w0, "CREATE" | "ALTER")
        && let Some(h) = first_host(text)
    {
        return Err(host_variable(&h));
    }
    match (w0, w1) {
        ("PREPARE", _) | ("EXECUTE", "IMMEDIATE") => Err(SqlError::Coded {
            code: -104,
            state: "42601",
            what: format!("{w0} is not a statement a dynamic interface runs"),
        }),
        ("DEALLOCATE", _) => Err(SqlError::Coded {
            code: -199,
            state: "42601",
            what: "DEALLOCATE is not a statement a dynamic interface runs".into(),
        }),
        ("DECLARE", "GLOBAL") => Ok(()),
        ("EXECUTE" | "DESCRIBE" | "DECLARE" | "OPEN" | "FETCH" | "CLOSE" | "WHENEVER" | "INCLUDE" | "ALLOCATE" | "ASSOCIATE", _)
        | ("BEGIN" | "END", "DECLARE")
        | ("FREE" | "HOLD", "LOCATOR")
        | ("GET", "DIAGNOSTICS") => Err(cannot_prepare(format!("{w0} {w1}").trim_end())),
        _ => Ok(()),
    }
}

/// Parse a statement a dynamic interface was given.
///
/// # Errors
/// What [`check`] refuses, and the parser's errors.
pub fn prepare_dynamic(text: &str, naming: Naming) -> Result<Prepared> {
    check(text)?;
    let p = prepare(text, naming)?;
    check_statement(&p.statement)?;
    Ok(p)
}

/// Refuse a parsed statement a dynamic interface cannot run: the
/// cursor statements.
///
/// # Errors
/// `SQL0084`.
pub fn check_statement(st: &Statement) -> Result<()> {
    match st {
        Statement::DeclareCursor(_) => Err(cannot_prepare("DECLARE CURSOR")),
        Statement::Open { .. } => Err(cannot_prepare("OPEN")),
        Statement::Fetch(_) => Err(cannot_prepare("FETCH")),
        Statement::Close(_) => Err(cannot_prepare("CLOSE")),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(t: &str) -> i32 {
        check(t).err().map_or(0, |e| e.sqlcode())
    }

    #[test]
    fn embedded_statements_are_refused_as_the_primary_refuses_them() {
        assert_eq!(code("PREPARE S1 FROM 'VALUES 1'"), -104);
        assert_eq!(code("EXECUTE IMMEDIATE 'VALUES 1'"), -104);
        assert_eq!(code("EXECUTE S1"), -84);
        assert_eq!(code("DESCRIBE S1 INTO :D"), -312);
        assert_eq!(code("DECLARE C1 CURSOR FOR SELECT 1 FROM T"), -84);
        assert_eq!(code("FETCH C1"), -84);
        assert_eq!(code("BEGIN DECLARE SECTION"), -84);
        assert_eq!(code("DEALLOCATE PREPARE S1"), -199);
        assert_eq!(code("SET :X = 1"), -312);
        assert_eq!(code("DECLARE GLOBAL TEMPORARY TABLE T (A INT)"), 0);
        assert_eq!(code("SELECT ':X', \"A:B\" FROM T -- :Y"), 0);
        assert_eq!(code("CREATE PROCEDURE P () BEGIN L1:LOOP LEAVE L1; END LOOP; END"), 0);
    }
}
