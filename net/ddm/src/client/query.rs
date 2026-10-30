// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Statements on a connection: prepare (`PRPSQLSTT` with `SQLSTT`),
//! open (`OPNQRY`), continue (`CNTQRY`) and close (`CLSQRY`) a query,
//! run immediately (`EXCSQLIMM`) or a prepared statement
//! (`EXCSQLSTT`).

use std::io::{Read, Write};

use super::{Connection, reply_error};
use crate::cp::{obj, param, rm};
use crate::fdoca::desc::{self, Field};
use crate::fdoca::rows::{self, Row};
use crate::fdoca::{Reader, sqlca, sqlda};
use crate::msg::sqlcmd::{self, QRYBLKSZ};
use crate::sql::{Column, Sqlca, Value};
use crate::wire::Dss;
use crate::Error;

/// A prepared statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prepared {
    /// Its package and section.
    pub pkg: crate::msg::Pkgnamcsn,
    /// Its result columns (empty for a statement that is not a query).
    pub columns: Vec<Column>,
    /// The SQLCA of the prepare.
    pub sqlca: Option<Sqlca>,
}

/// What a query returned.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rows {
    /// The answer set's columns as `QRYDSC` describes them.
    pub fields: Vec<Field>,
    /// The rows.
    pub rows: Vec<Vec<Value>>,
    /// The SQLCA that ended the data.
    pub end: Option<Sqlca>,
    /// The query instance (`QRYINSID`) from `OPNQRYRM`.
    pub qryinsid: Option<Vec<u8>>,
    /// Whether the server closed the query (`ENDQRYRM`).
    pub closed: bool,
}

/// The SQLCA the replies carry, an error for a reply message that is
/// not one of the ordinary ones.
pub fn sqlca_of<S>(c: &Connection<S>, replies: &[Dss]) -> Result<Sqlca, Error> {
    let mut out = None;
    for d in replies {
        let Some(o) = d.first() else { continue };
        match o.cp {
            obj::SQLCARD => {
                let mut r = Reader::new(o.bytes(), c.repr);
                out = sqlca::read(&mut r, c.sqlam)?;
            }
            rm::ENDUOWRM | rm::RDBUPDRM | rm::SQLERRRM | rm::ENDQRYRM | rm::OPNQRYRM => {}
            _ => return Err(reply_error(o)),
        }
    }
    Ok(out.unwrap_or_default())
}

impl<S: Read + Write> Connection<S> {
    /// Prepare `text` at a fresh section, describing its output.
    pub fn prepare(&mut self, text: &str) -> Result<Prepared, Error> {
        let section = self.next_section();
        let pkg = self.pkg(section);
        let corr = self.next_corr();
        let replies = self.exchange(vec![
            Dss::request(corr, sqlcmd::prpsqlstt(&pkg, true)),
            Dss::object(corr, sqlcmd::sqlstt(text)),
        ])?;
        let mut p = Prepared {
            pkg,
            columns: Vec::new(),
            sqlca: None,
        };
        for d in &replies {
            let Some(o) = d.first() else { continue };
            match o.cp {
                obj::SQLDARD => {
                    let mut r = Reader::new(o.bytes(), self.repr);
                    let da = sqlda::read(&mut r, self.sqlam)?;
                    p.columns = da.columns;
                    p.sqlca = da.sqlca;
                }
                obj::SQLCARD => {
                    let mut r = Reader::new(o.bytes(), self.repr);
                    p.sqlca = sqlca::read(&mut r, self.sqlam)?;
                }
                rm::SQLERRRM => {}
                _ => return Err(reply_error(o)),
            }
        }
        Ok(p)
    }

    /// Open a prepared query and fetch every row.
    pub fn query(&mut self, p: &Prepared) -> Result<Rows, Error> {
        let corr = self.next_corr();
        let replies = self.exchange(vec![Dss::request(corr, sqlcmd::opnqry(&p.pkg, QRYBLKSZ))])?;
        let mut out = Rows::default();
        let mut qryinsid = None;
        let mut ended = false;
        self.absorb(&replies, &mut out, &mut qryinsid, &mut ended)?;
        while !ended {
            let corr = self.next_corr();
            let replies = self.exchange(vec![Dss::request(
                corr,
                sqlcmd::cntqry(&p.pkg, QRYBLKSZ, qryinsid.as_deref()),
            )])?;
            let before = out.rows.len();
            self.absorb(&replies, &mut out, &mut qryinsid, &mut ended)?;
            if !ended && out.rows.len() == before {
                break;
            }
        }
        out.qryinsid = qryinsid;
        Ok(out)
    }

    /// Close a query (`CLSQRY`) unless the server ended it already.
    pub fn close(&mut self, p: &Prepared, rows: &Rows) -> Result<Sqlca, Error> {
        if rows.closed {
            return Ok(Sqlca::default());
        }
        let corr = self.next_corr();
        let r = self.exchange(vec![Dss::request(
            corr,
            sqlcmd::clsqry(&p.pkg, rows.qryinsid.as_deref()),
        )])?;
        if r.iter().any(|d| d.code() == Some(rm::QRYNOPRM)) {
            return Ok(Sqlca::default());
        }
        sqlca_of(self, &r)
    }

    fn absorb(
        &mut self,
        replies: &[Dss],
        out: &mut Rows,
        qryinsid: &mut Option<Vec<u8>>,
        ended: &mut bool,
    ) -> Result<(), Error> {
        for d in replies {
            let Some(o) = d.first() else { continue };
            match o.cp {
                rm::OPNQRYRM => {
                    *qryinsid = o.get(param::QRYINSID).map(|x| x.bytes().to_vec());
                }
                obj::QRYDSC => out.fields.extend(desc::fields(o.bytes())?),
                obj::QRYDTA => {
                    let mut r = Reader::new(o.bytes(), self.repr);
                    for row in rows::read_rows(&mut r, &out.fields, self.sqlam)? {
                        match row {
                            Row::Values(v) => out.rows.push(v),
                            Row::End(ca) => {
                                out.end = Some(ca);
                                *ended = true;
                            }
                        }
                    }
                }
                rm::ENDQRYRM => {
                    *ended = true;
                    out.closed = true;
                }
                obj::SQLCARD => {
                    let mut r = Reader::new(o.bytes(), self.repr);
                    if let Some(ca) = sqlca::read(&mut r, self.sqlam)? {
                        if ca.sqlcode != 0 {
                            *ended = true;
                        }
                        out.end = Some(ca);
                    }
                }
                rm::OPNQFLRM | rm::SQLERRRM | rm::QRYNOPRM => *ended = true,
                _ => return Err(reply_error(o)),
            }
        }
        Ok(())
    }

    /// Bind the connection's package with `sections` cursors for the
    /// dynamic statements `S1`, `S2`, ... (`BGNBND`, `BNDSQLSTT`,
    /// `ENDBND`), then commit. Returns the SQLCA of the end.
    pub fn bind_dynamic(&mut self, sections: u16) -> Result<Sqlca, Error> {
        let pkg = self.pkg(0);
        let corr = self.next_corr();
        let mut list = vec![Dss::request(corr, sqlcmd::bgnbnd(&pkg))];
        for n in 1..=sections {
            let c = self.next_corr();
            list.push(Dss::request(c, sqlcmd::bndsqlstt(&pkg.at(n))));
            list.push(Dss::object(
                c,
                sqlcmd::sqlstt(&format!("DECLARE C{n} CURSOR FOR S{n}")),
            ));
        }
        let c = self.next_corr();
        list.push(Dss::request(c, sqlcmd::endbnd(&pkg)));
        let r = self.exchange(list)?;
        let ca = sqlca_of(self, &r)?;
        self.commit()?;
        Ok(ca)
    }

    /// Run `text` once (`EXCSQLIMM`).
    pub fn execute_immediate(&mut self, text: &str) -> Result<Sqlca, Error> {
        let section = self.next_section();
        let pkg = self.pkg(section);
        let corr = self.next_corr();
        let r = self.exchange(vec![
            Dss::request(corr, sqlcmd::excsqlimm(&pkg)),
            Dss::object(corr, sqlcmd::sqlstt(text)),
        ])?;
        sqlca_of(self, &r)
    }

    /// Run a prepared statement (`EXCSQLSTT`).
    pub fn execute(&mut self, p: &Prepared) -> Result<Sqlca, Error> {
        let corr = self.next_corr();
        let r = self.exchange(vec![Dss::request(corr, sqlcmd::excsqlstt(&p.pkg))])?;
        sqlca_of(self, &r)
    }
}
