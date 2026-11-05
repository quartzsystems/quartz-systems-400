// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One connection to a server over the DRDA client: a query read to its
//! end, a statement run at once, the unit of work ended. A server that
//! wants the package bound (`-805`) gets it bound once, with enough
//! sections for one statement at a time.

use std::io::{Read, Write};

use ddm::client::Connection;

use crate::session::Server;
use crate::values::{coded, sqlca, value};

/// Sections bound in the package when the server asks for it.
pub const SECTIONS: u16 = 4;

/// A connection and whether its package is bound.
pub struct Remote<S> {
    conn: Connection<S>,
    bound: bool,
}

impl<S> Remote<S> {
    /// Over an open connection.
    pub fn new(conn: Connection<S>) -> Self {
        Self { conn, bound: false }
    }
}

fn failed(e: &ddm::Error) -> sql::Sqlca {
    coded(-30_080, "08001", &e.to_string())
}

/// The server has no package for the statement.
fn no_package(c: &sql::Sqlca) -> bool {
    matches!(c.sqlcode, -805 | -514 | -518)
}

impl<S: Read + Write> Remote<S> {
    fn bind(&mut self) -> Result<(), sql::Sqlca> {
        let ca = self.conn.bind_dynamic(SECTIONS).map_err(|e| failed(&e))?;
        let ca = sqlca(&ca);
        if ca.is_error() {
            return Err(ca);
        }
        self.bound = true;
        Ok(())
    }

    fn query_once(&mut self, text: &str) -> Result<Vec<Vec<sql::Value>>, sql::Sqlca> {
        self.conn.reuse_sections(1);
        let p = self.conn.prepare(text).map_err(|e| failed(&e))?;
        if let Some(ca) = p.sqlca.as_ref().map(sqlca).filter(sql::Sqlca::is_error) {
            return Err(ca);
        }
        let rows = self.conn.query(&p).map_err(|e| failed(&e))?;
        let _ = self.conn.close(&p, &rows);
        if let Some(ca) = rows.end.as_ref().map(sqlca).filter(sql::Sqlca::is_error) {
            return Err(ca);
        }
        Ok(rows.rows.iter().map(|r| r.iter().map(value).collect()).collect())
    }

    fn execute_once(&mut self, text: &str) -> sql::Sqlca {
        self.conn.reuse_sections(1);
        match self.conn.execute_immediate(text) {
            Ok(c) => sqlca(&c),
            Err(e) => failed(&e),
        }
    }
}

impl<S: Read + Write> Server for Remote<S> {
    fn query(&mut self, text: &str) -> Result<Vec<Vec<sql::Value>>, sql::Sqlca> {
        match self.query_once(text) {
            Err(c) if no_package(&c) && !self.bound => {
                self.bind()?;
                self.query_once(text)
            }
            r => r,
        }
    }

    fn execute(&mut self, text: &str) -> sql::Sqlca {
        let c = self.execute_once(text);
        if no_package(&c) && !self.bound {
            if let Err(e) = self.bind() {
                return e;
            }
            return self.execute_once(text);
        }
        c
    }

    fn commit(&mut self) -> sql::Sqlca {
        self.conn.commit().map_or_else(|e| failed(&e), |c| sqlca(&c))
    }

    fn rollback(&mut self) -> sql::Sqlca {
        self.conn.rollback().map_or_else(|e| failed(&e), |c| sqlca(&c))
    }
}
