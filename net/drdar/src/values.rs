// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Values and SQLCAs between the DRDA client's forms and the SQL
//! engine's.

/// A value from a server as the engine holds it.
#[must_use]
pub fn value(v: &ddm::sql::Value) -> sql::Value {
    match v {
        ddm::sql::Value::Null => sql::Value::Null,
        ddm::sql::Value::Int(i) => sql::Value::int(*i),
        ddm::sql::Value::Dec(d) => sql::Value::Num(d.clone()),
        ddm::sql::Value::Double(f) => sql::Value::Double(*f),
        ddm::sql::Value::Text(t) => sql::Value::Str(t.clone()),
    }
}

/// A server's SQLCA as the engine's.
#[must_use]
pub fn sqlca(c: &ddm::sql::Sqlca) -> sql::Sqlca {
    sql::Sqlca {
        sqlcode: c.sqlcode,
        sqlstate: c.sqlstate.clone(),
        sqlerrd: c.sqlerrd,
        sqlwarn: c.sqlwarn.map(|b| if b == 0 { ' ' } else { char::from(b) }),
        sqlerrmc: c.sqlerrmc.clone(),
        sqlerrp: c.sqlerrproc.clone(),
    }
}

/// An SQLCA of `code` and `state`, the message tokens `what`.
#[must_use]
pub fn coded(code: i32, state: &str, what: &str) -> sql::Sqlca {
    sql::Sqlca {
        sqlcode: code,
        sqlstate: state.into(),
        sqlerrmc: what.into(),
        ..sql::Sqlca::default()
    }
}
