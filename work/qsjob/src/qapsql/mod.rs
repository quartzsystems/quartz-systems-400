// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QAP's `Sql` service over the system's database engine
//! (`sql::embed`), as the job's embedded SQL reaches it: [`Lane`] is the
//! connection's statements, cursors, options and transaction, run in
//! the lane on the job's own thread; [`catalog`] the lists, run on a
//! secondary thread. Tested on any host over an engine in memory.
//!
//! - Statements and cursors live in the embedded scope [`SCOPE`], so the
//!   commitment definition is the connection's own (`*SQLQAP`); cursors
//!   are opened `WITH HOLD` and stay open across commits.
//! - A commit level other than `None` is the engine's lock level
//!   (`*CHG`, `*CS`, `*ALL`; `Serializable` is `*ALL` too); the
//!   definition takes each statement's level between units of work, so
//!   `SetOptions` changes it once the unit has ended.
//! - `Fetch` reads in batches of [`BATCH`], looking at the cancel
//!   between them; the engine looks at it between rows read.

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

pub mod catalog;
pub mod values;

use std::collections::HashMap;

use qap_proto::ops::sql::{
    CommitLevel, Execute, Executed, Fetch, Fetched, Naming, Prepared, SetOptions, SqlFailure,
};
use qap_proto::{ServerMessage, Status};
use qapd::Refusal;
use qapd::sql::{CANCELLED, SqlResult, SqlStop};
use sql::embed::{Desc, Op, Reply, Request};
use sql::sqlca::Sqlca;
use sql::value::Value;

use values::{Described, from_sql, to_sql};

/// The embedded scope of the connection's statements.
pub const SCOPE: &str = "QAP:LANE";
/// Every scope of the connection, for its end.
const SCOPES: &str = "QAP:";
/// Rows a fetch reads from the engine at once.
pub const BATCH: usize = 100;

/// The engine and the job's messages.
pub trait Engine {
    /// Run one statement.
    fn run(&mut self, req: &Request) -> Reply;
    /// Message `id` with `data`, in the language of the job's user.
    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage;
}

/// The SQL message of an SQLCA: the message description's severity and
/// help, the engine's own text.
pub fn sql_message(e: &mut dyn Engine, ca: &Sqlca) -> ServerMessage {
    let mut m = e.message(&ca.msgid(), &[]);
    if !ca.sqlerrmc.is_empty() {
        let mut t = ca.sqlerrmc.clone();
        if let Some(c) = t.get(..1) {
            t.replace_range(..1, &c.to_uppercase());
        }
        if !t.ends_with('.') {
            t.push('.');
        }
        m.text = t;
        m.data = ca.sqlerrmc.as_bytes().to_vec();
    }
    m
}

/// How a failed SQLCA ends an operation: `NotAuthorized` for an
/// authority failure, else `SqlError` with `row`.
pub fn stop<T>(e: &mut dyn Engine, ca: &Sqlca, row: Option<u32>) -> SqlStop<T> {
    let message = sql_message(e, ca);
    if ca.sqlcode == -551 {
        return SqlStop::Refused(Refusal {
            status: Status::NotAuthorized,
            message,
        });
    }
    SqlStop::failed(
        SqlFailure {
            sqlstate: ca.sqlstate.clone(),
            sqlcode: ca.sqlcode,
            row,
        },
        message,
    )
}

/// The engine's SQLCODE for a statement a cancel ended.
const CANCEL_CODE: i32 = -952;

/// A prepared statement.
#[derive(Debug, Clone)]
struct Stmt {
    name: String,
    columns: Vec<Described>,
    parameters: Vec<Described>,
    /// A `CALL` with `OUT` or `INOUT` parameters: QAP has no way to
    /// give their values back.
    outputs: bool,
}

/// An open cursor.
#[derive(Debug, Clone)]
struct Cursor {
    name: String,
    columns: Vec<Described>,
    end: bool,
}

/// The connection's statements, cursors, options and transaction.
#[derive(Debug, Default)]
pub struct Lane {
    options: SetOptions,
    statements: HashMap<u32, Stmt>,
    cursors: HashMap<u32, Cursor>,
    next: u32,
    /// A change ran under a commit level since the last commit or
    /// rollback.
    pending: bool,
    /// Any statement ran: there is something to end.
    used: bool,
}

impl Lane {
    fn id(&mut self) -> u32 {
        self.next += 1;
        self.next
    }

    /// A statement descriptor under the connection's options.
    fn desc(&self, op: Op) -> Desc {
        Desc {
            op,
            sql_naming: self.options.naming == Naming::Sql,
            commit: match self.options.commit {
                CommitLevel::None => None,
                CommitLevel::ReadUncommitted => sql::embed::commit_level("*CHG"),
                CommitLevel::ReadCommitted => sql::embed::commit_level("*CS"),
                CommitLevel::RepeatableRead | CommitLevel::Serializable => {
                    sql::embed::commit_level("*ALL")
                }
            },
            ..Desc::default()
        }
    }

    fn run(&mut self, e: &mut dyn Engine, desc: Desc, inputs: Vec<Value>) -> Reply {
        self.used = true;
        e.run(&Request {
            scope: SCOPE.into(),
            desc,
            inputs,
        })
    }

    fn not_found(e: &mut dyn Engine, id: &str, name: &str) -> Refusal {
        Refusal {
            status: Status::NotFound,
            message: e.message(id, &[name]),
        }
    }

    /// `SetOptions`: `InUse` while a transaction is open; the default
    /// schema is `SET SCHEMA`.
    pub fn set_options(&mut self, e: &mut dyn Engine, o: &SetOptions) -> Result<(), Refusal> {
        if self.pending {
            return Err(Refusal {
                status: Status::InUse,
                message: e.message("SQL0752", &[]),
            });
        }
        let schema = o
            .default_schema
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let text = schema.map_or_else(
            || "SET SCHEMA DEFAULT".to_string(),
            |s| format!("SET SCHEMA \"{}\"", s.replace('"', "\"\"")),
        );
        let previous = std::mem::replace(&mut self.options, o.clone());
        let r = self.run(
            e,
            Desc {
                text,
                ..self.desc(Op::Exec)
            },
            Vec::new(),
        );
        if r.sqlca.is_error() {
            self.options = previous;
            return Err(Refusal {
                status: Status::Error,
                message: sql_message(e, &r.sqlca),
            });
        }
        Ok(())
    }

    /// `Prepare`: the statement, its result columns and markers.
    pub fn prepare(&mut self, e: &mut dyn Engine, text: &str) -> SqlResult<Prepared> {
        let statement = self.id();
        let name = format!("QAPS{statement}");
        let r = self.run(
            e,
            Desc {
                name: name.clone(),
                into: true,
                text: text.to_string(),
                ..self.desc(Op::Prepare)
            },
            Vec::new(),
        );
        if r.sqlca.is_error() {
            return Err(stop(e, &r.sqlca, None));
        }
        let columns: Vec<Described> = r.rows.iter().map(|row| values::described(row)).collect();
        let m = self.run(
            e,
            Desc {
                name: name.clone(),
                orient: "INPUT".into(),
                ..self.desc(Op::Describe)
            },
            Vec::new(),
        );
        if m.sqlca.is_error() {
            return Err(stop(e, &m.sqlca, None));
        }
        let outputs = m
            .rows
            .iter()
            .any(|row| matches!(row.get(9).and_then(|v| v.as_i64().ok()), Some(2 | 3)));
        let parameters: Vec<Described> = m
            .rows
            .iter()
            .map(|row| {
                let mut d = values::described(row);
                d.column.name.clear();
                d.column.label.clear();
                d
            })
            .collect();
        let reply = Prepared {
            statement,
            columns: columns.iter().map(|c| c.column.clone()).collect(),
            parameters: parameters.iter().map(|c| c.column.clone()).collect(),
        };
        self.statements.insert(
            statement,
            Stmt {
                name,
                columns,
                parameters,
                outputs,
            },
        );
        Ok(reply)
    }

    /// A parameter row as the engine's values; the failure for the
    /// first that is not one.
    fn inputs(
        e: &mut dyn Engine,
        s: &Stmt,
        row: &[qap_proto::ops::sql::SqlValue],
        at: Option<u32>,
    ) -> Result<Vec<Value>, SqlStop<Executed>> {
        if row.len() != s.parameters.len() {
            let ca = Sqlca {
                sqlcode: -313,
                sqlstate: "07001".into(),
                ..Sqlca::default()
            };
            return Err(stop(e, &ca, at));
        }
        row.iter()
            .zip(&s.parameters)
            .map(|(v, p)| {
                from_sql(p.kind, v).ok_or_else(|| {
                    let ca = Sqlca {
                        sqlcode: -302,
                        sqlstate: "22018".into(),
                        ..Sqlca::default()
                    };
                    stop(e, &ca, at)
                })
            })
            .collect()
    }

    /// `Execute`: a query opens a cursor; anything else runs once per
    /// parameter row, stopping at the first that fails, and looks at the
    /// cancel before each.
    pub fn execute(
        &mut self,
        e: &mut dyn Engine,
        req: &Execute,
        cancelled: &dyn Fn() -> bool,
    ) -> SqlResult<Executed> {
        let Some(s) = self.statements.get(&req.statement).cloned() else {
            return Err(SqlStop::Refused(Self::not_found(
                e,
                "SQL0518",
                &format!("QAPS{}", req.statement),
            )));
        };
        if !s.columns.is_empty() {
            let row = req.rows.first().cloned().unwrap_or_default();
            let inputs = Self::inputs(e, &s, &row, None)?;
            let cursor = self.id();
            let name = format!("QAPC{cursor}");
            let r = self.run(
                e,
                Desc {
                    name: name.clone(),
                    stmt: s.name.clone(),
                    hold: true,
                    ..self.desc(Op::Open)
                },
                inputs,
            );
            if r.sqlca.sqlcode == CANCEL_CODE {
                return Err(SqlStop::cancelled(
                    Executed {
                        updated: 0,
                        cursor: None,
                    },
                    sql_message(e, &r.sqlca),
                ));
            }
            if r.sqlca.is_error() {
                return Err(stop(e, &r.sqlca, None));
            }
            self.cursors.insert(
                cursor,
                Cursor {
                    name,
                    columns: s.columns.clone(),
                    end: false,
                },
            );
            return Ok(Executed {
                updated: 0,
                cursor: Some(cursor),
            });
        }
        if s.outputs {
            // BLOCKED(rel): QAP's Executed has no output values (a later
            // protocol version); refused rather than dropped.
            let ca = Sqlca {
                sqlcode: -84,
                sqlstate: "42612".into(),
                sqlerrmc: "CALL with OUT or INOUT parameters through QAP: the protocol gives no output values back".into(),
                ..Sqlca::default()
            };
            return Err(stop(e, &ca, None));
        }
        let several = !req.rows.is_empty();
        let rows = if several {
            req.rows.clone()
        } else {
            vec![Vec::new()]
        };
        let mut updated = 0i64;
        for (n, row) in rows.iter().enumerate() {
            let at = several.then(|| u32::try_from(n).unwrap_or(u32::MAX));
            if cancelled() {
                return Err(SqlStop::cancelled(
                    Executed {
                        updated,
                        cursor: None,
                    },
                    e.message(CANCELLED, &[]),
                ));
            }
            let inputs = Self::inputs(e, &s, row, at)?;
            if self.options.commit != CommitLevel::None {
                self.pending = true;
            }
            let r = self.run(
                e,
                Desc {
                    name: s.name.clone(),
                    ..self.desc(Op::Execute)
                },
                inputs,
            );
            if r.sqlca.sqlcode == 466 && rows.len() == 1 {
                // A CALL's first result set is the statement's cursor.
                return self.result_set(e, &r);
            }
            match r.sqlca.sqlcode {
                0 => updated += i64::from(r.sqlca.sqlerrd[2]),
                100 => {}
                CANCEL_CODE => {
                    return Err(SqlStop::cancelled(
                        Executed {
                            updated,
                            cursor: None,
                        },
                        sql_message(e, &r.sqlca),
                    ));
                }
                c if c < 0 => return Err(stop(e, &r.sqlca, at)),
                _ => updated += i64::from(r.sqlca.sqlerrd[2]),
            }
        }
        Ok(Executed {
            updated,
            cursor: None,
        })
    }

    /// The first result set a `CALL` left (`RS#1`) as a cursor.
    /// BLOCKED(rel): `Executed` names one cursor, so a second set is not
    /// reachable through QAP.
    fn result_set(&mut self, e: &mut dyn Engine, called: &Reply) -> SqlResult<Executed> {
        let name = called
            .rows
            .first()
            .and_then(|r| r.first())
            .map_or_else(|| "RS#1".to_string(), Value::as_text);
        let d = self.run(
            e,
            Desc {
                name: name.clone(),
                orient: "CURSOR".into(),
                ..self.desc(Op::Describe)
            },
            Vec::new(),
        );
        if d.sqlca.is_error() {
            return Err(stop(e, &d.sqlca, None));
        }
        let cursor = self.id();
        self.cursors.insert(
            cursor,
            Cursor {
                name,
                columns: d.rows.iter().map(|row| values::described(row)).collect(),
                end: false,
            },
        );
        Ok(Executed {
            updated: 0,
            cursor: Some(cursor),
        })
    }

    /// `Fetch`: up to `max_rows` (0 for all), in batches.
    pub fn fetch(
        &mut self,
        e: &mut dyn Engine,
        req: &Fetch,
        cancelled: &dyn Fn() -> bool,
    ) -> SqlResult<Fetched> {
        let Some(c) = self.cursors.get(&req.cursor).cloned() else {
            return Err(SqlStop::Refused(Self::not_found(
                e,
                "SQL0501",
                &format!("QAPC{}", req.cursor),
            )));
        };
        let want = if req.max_rows == 0 {
            usize::MAX
        } else {
            usize::try_from(req.max_rows).unwrap_or(usize::MAX)
        };
        let mut rows = Vec::new();
        let mut end = c.end;
        while !end && rows.len() < want {
            if cancelled() {
                let _ = self.close_cursor(e, req.cursor);
                return Err(SqlStop::cancelled(
                    Fetched { rows, end: false },
                    e.message(CANCELLED, &[]),
                ));
            }
            let n = (want - rows.len()).min(BATCH);
            let r = self.run(
                e,
                Desc {
                    name: c.name.clone(),
                    rows: true,
                    ..self.desc(Op::Fetch)
                },
                vec![Value::int(i64::try_from(n).unwrap_or(1))],
            );
            if r.sqlca.sqlcode == CANCEL_CODE {
                let _ = self.close_cursor(e, req.cursor);
                return Err(SqlStop::cancelled(
                    Fetched { rows, end: false },
                    sql_message(e, &r.sqlca),
                ));
            }
            if r.sqlca.is_error() {
                return Err(stop(e, &r.sqlca, None));
            }
            end = r.sqlca.sqlcode == 100 || r.rows.len() < n || r.sqlca.sqlerrd[4] == 100;
            rows.extend(r.rows.iter().map(|row| {
                row.iter()
                    .zip(&c.columns)
                    .map(|(v, d)| to_sql(d.kind, v))
                    .collect()
            }));
        }
        if let Some(c) = self.cursors.get_mut(&req.cursor) {
            c.end = end;
        }
        Ok(Fetched { rows, end })
    }

    /// `CloseCursor`.
    pub fn close_cursor(&mut self, e: &mut dyn Engine, cursor: u32) -> Result<(), Refusal> {
        let Some(c) = self.cursors.remove(&cursor) else {
            return Err(Self::not_found(e, "SQL0501", &format!("QAPC{cursor}")));
        };
        let _ = self.run(
            e,
            Desc {
                name: c.name,
                ..self.desc(Op::Close)
            },
            Vec::new(),
        );
        Ok(())
    }

    /// `Release`: the engine's prepared statement goes too; cursors
    /// opened from it stay.
    pub fn release(&mut self, e: &mut dyn Engine, statement: u32) -> Result<(), Refusal> {
        let Some(s) = self.statements.remove(&statement) else {
            return Err(Self::not_found(e, "SQL0518", &format!("QAPS{statement}")));
        };
        let _ = self.run(
            e,
            Desc {
                name: s.name,
                ..self.desc(Op::Deallocate)
            },
            Vec::new(),
        );
        Ok(())
    }

    /// `Commit` or `Rollback`; nothing to do without commitment control.
    pub fn end_unit(&mut self, e: &mut dyn Engine, commit: bool) -> SqlResult<()> {
        let text = if commit { "COMMIT" } else { "ROLLBACK" };
        let r = self.run(
            e,
            Desc {
                text: text.into(),
                ..self.desc(Op::Exec)
            },
            Vec::new(),
        );
        // -7007: the connection never ran under commitment control.
        if r.sqlca.is_error() && r.sqlca.sqlcode != -7007 {
            return Err(stop(e, &r.sqlca, None));
        }
        self.pending = false;
        Ok(())
    }

    /// The connection ends or runs as another profile: changes not
    /// committed roll back, statements and cursors go, options as at
    /// the start.
    pub fn end(&mut self, e: &mut dyn Engine) {
        if !self.used {
            return;
        }
        let _ = self.end_unit(e, false);
        let _ = e.run(&Request {
            scope: SCOPES.into(),
            desc: Desc {
                op: Op::End,
                ..Desc::default()
            },
            inputs: Vec::new(),
        });
        let _ = e.run(&Request {
            scope: SCOPE.into(),
            desc: Desc {
                op: Op::Exec,
                text: "SET SCHEMA DEFAULT".into(),
                ..Desc::default()
            },
            inputs: Vec::new(),
        });
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests;
