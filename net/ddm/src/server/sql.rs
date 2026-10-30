// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL commands: binding (accepted; sections are dynamic), prepare
//! and describe, open, continue and close a query, execute and execute
//! immediately, commit and roll back. Replies follow the pub400 traces:
//! a failed prepare answers `SQLCARD` alone; `OPNQRY` answers `OPNQRYRM`
//! and `QRYDSC`; the rows come with `CNTQRY`, and the last block ends
//! with the `+100` row, `ENDQRYRM` and `SQLCARD`.

// The SQLCA is the error DRDA returns; it is not boxed.
#![allow(clippy::result_large_err)]

use super::{Section, Session};
use crate::cp::{cmd, obj, param, rm, svrcod};
use crate::fdoca::desc::{self, Field};
use crate::fdoca::sqlda::{self, Sqldard};
use crate::fdoca::{Reader, Writer, rows, sqlca};
use crate::msg::sqlcmd::sqlstt_bytes;
use crate::msg::{Pkgnamcsn, reply};
use crate::sql::{Sqlca, Value};
use crate::wire::{DssType, Object};

/// Limited block query protocol.
pub const LMTBLKPRC: u16 = 0x2417;

/// An open query.
#[derive(Debug)]
pub struct Query {
    fields: Vec<Field>,
}

/// Whether the server's SQL side takes this command.
pub fn handles(code: u16) -> bool {
    matches!(
        code,
        cmd::BGNBND
            | cmd::BNDSQLSTT
            | cmd::ENDBND
            | cmd::PRPSQLSTT
            | cmd::DSCSQLSTT
            | cmd::OPNQRY
            | cmd::CNTQRY
            | cmd::CLSQRY
            | cmd::EXCSQLIMM
            | cmd::EXCSQLSTT
            | cmd::RDBCMM
            | cmd::RDBRLLBCK
            | cmd::EXCSQLSET
    )
}

type Out = Vec<(DssType, Object)>;

fn section_of(p: &Pkgnamcsn) -> Section {
    Section {
        collection: p.collection.clone(),
        package: p.package.clone(),
        number: p.section,
    }
}

impl Session {
    fn sqlcard(&self, ca: Option<&Sqlca>) -> (DssType, Object) {
        let mut ca = ca.cloned();
        if let Some(c) = ca.as_mut().filter(|c| c.rdbname.is_empty()) {
            c.rdbname.clone_from(&self.cfg.rdb);
        }
        (
            DssType::Object,
            Object::data(obj::SQLCARD, sqlca::sqlcard(ca.as_ref(), crate::fdoca::Repr::default(), self.sqlam)),
        )
    }

    fn statement(&self, data: &[Object]) -> Option<String> {
        let o = data.iter().find(|o| o.cp == obj::SQLSTT)?;
        let (b, _) = sqlstt_bytes(o.bytes()).ok()?;
        Some(
            ccsid::conv::to_string(self.their_repr.ccsid, &b)
                .unwrap_or_else(|_| crate::wire::text(&b)),
        )
    }

    fn params(&self, data: &[Object]) -> Result<Vec<Value>, Sqlca> {
        let Some(sqldta) = data.iter().find(|o| o.cp == obj::SQLDTA) else {
            return Ok(Vec::new());
        };
        let bad = |_| Sqlca::error(-30_000, "58008", "SQLDTA");
        let descriptor = sqldta.get(obj::FDODSC).map(Object::bytes).unwrap_or_default();
        let values = sqldta.get(obj::FDODTA).map(Object::bytes).unwrap_or_default();
        let fields = desc::fields(descriptor).map_err(bad)?;
        let mut r = Reader::new(values, self.their_repr);
        if r.peek() == Some(rows::PRESENT) {
            r.u8().map_err(bad)?;
        }
        fields.iter().map(|f| rows::read_value(&mut r, f).map_err(bad)).collect()
    }

    #[allow(clippy::too_many_lines)] // one arm per command
    pub(super) fn sql_command(&mut self, c: &Object, data: &[Object]) -> Out {
        let rpy = |o: Object| (DssType::Reply, o);
        let pkg = Pkgnamcsn::from_command(c);
        match c.cp {
            cmd::BGNBND | cmd::BNDSQLSTT | cmd::ENDBND | cmd::EXCSQLSET => vec![self.sqlcard(None)],
            cmd::PRPSQLSTT | cmd::DSCSQLSTT => {
                let Ok(pkg) = pkg else {
                    return vec![rpy(reply::syntaxrm(0x0E, param::PKGNAMCSN))];
                };
                let sec = section_of(&pkg);
                let res = if c.cp == cmd::PRPSQLSTT {
                    match self.statement(data) {
                        Some(text) => self.sql.prepare(&sec, &text),
                        None => return vec![rpy(reply::syntaxrm(0x0E, obj::SQLSTT))],
                    }
                } else {
                    self.sql.describe(&sec)
                };
                let wants_da = c.cp == cmd::DSCSQLSTT
                    || c.get(param::RTNSQLDA).and_then(|x| x.bytes().first().copied()) == Some(0xF1);
                match res {
                    Ok(columns) if wants_da => {
                        let mut w = Writer::new(crate::fdoca::Repr::default());
                        sqlda::write(&mut w, &Sqldard { sqlca: None, columns }, self.sqlam);
                        vec![(DssType::Object, Object::data(obj::SQLDARD, w.out))]
                    }
                    Ok(_) => vec![self.sqlcard(None)],
                    Err(ca) => vec![self.sqlcard(Some(&ca))],
                }
            }
            cmd::OPNQRY => {
                let Ok(pkg) = pkg else {
                    return vec![rpy(reply::syntaxrm(0x0E, param::PKGNAMCSN))];
                };
                let sec = section_of(&pkg);
                if self.queries.contains_key(&sec) {
                    return vec![rpy(reply::message(rm::QRYPOPRM, svrcod::ERROR, vec![]))];
                }
                let params = match self.params(data) {
                    Ok(p) => p,
                    Err(ca) => return vec![rpy(reply::opnqflrm(&self.cfg.rdb)), self.sqlcard(Some(&ca))],
                };
                match self.sql.open(&sec, &params) {
                    Ok(columns) => {
                        let fields: Vec<Field> = columns.iter().map(|c| Field::of(&c.ty)).collect();
                        let id = self.next_qryinsid.to_be_bytes();
                        self.next_qryinsid += 1;
                        let dsc = desc::qrydsc(&fields);
                        self.queries.insert(sec, Query { fields });
                        vec![
                            rpy(reply::opnqryrm(LMTBLKPRC, &id)),
                            (DssType::Object, Object::data(obj::QRYDSC, dsc)),
                        ]
                    }
                    Err(ca) => vec![rpy(reply::opnqflrm(&self.cfg.rdb)), self.sqlcard(Some(&ca))],
                }
            }
            cmd::CNTQRY => {
                let Ok(pkg) = pkg else {
                    return vec![rpy(reply::syntaxrm(0x0E, param::PKGNAMCSN))];
                };
                let sec = section_of(&pkg);
                let Some(q) = self.queries.get(&sec) else {
                    return vec![rpy(reply::qrynoprm(&self.cfg.rdb, &pkg.to_object()))];
                };
                let fields = q.fields.clone();
                let max = self.cfg.block_rows.max(1);
                let mut w = Writer::new(crate::fdoca::Repr::default());
                match self.sql.fetch(&sec, max) {
                    Ok(batch) => {
                        let n = batch.len();
                        for row in &batch {
                            if let Err(e) = rows::write_row(&mut w, &fields, row) {
                                let ca = Sqlca::error(-30_000, "58008", &e.to_string());
                                return vec![self.sqlcard(Some(&ca))];
                            }
                        }
                        if n == max {
                            return vec![(DssType::Object, Object::data(obj::QRYDTA, w.out))];
                        }
                        let mut end = Sqlca::no_data();
                        end.rdbname.clone_from(&self.cfg.rdb);
                        rows::write_end(&mut w, &end, self.sqlam);
                        self.queries.remove(&sec);
                        let _ = self.sql.close(&sec);
                        vec![
                            (DssType::Object, Object::data(obj::QRYDTA, w.out)),
                            rpy(reply::endqryrm(&self.cfg.rdb)),
                            self.sqlcard(Some(&end)),
                        ]
                    }
                    Err(ca) => {
                        self.queries.remove(&sec);
                        let _ = self.sql.close(&sec);
                        vec![rpy(reply::endqryrm(&self.cfg.rdb)), self.sqlcard(Some(&ca))]
                    }
                }
            }
            cmd::CLSQRY => {
                let Ok(pkg) = pkg else {
                    return vec![rpy(reply::syntaxrm(0x0E, param::PKGNAMCSN))];
                };
                let sec = section_of(&pkg);
                if self.queries.remove(&sec).is_none() {
                    return vec![rpy(reply::qrynoprm(&self.cfg.rdb, &pkg.to_object()))];
                }
                match self.sql.close(&sec) {
                    Ok(()) => vec![self.sqlcard(None)],
                    Err(ca) => vec![self.sqlcard(Some(&ca))],
                }
            }
            cmd::EXCSQLIMM => match self.statement(data) {
                Some(text) => match self.sql.execute_immediate(&text) {
                    Ok(ca) => vec![self.sqlcard(Some(&ca).filter(|c| c.sqlcode != 0 || c.sqlerrd[2] != 0))],
                    Err(ca) => vec![self.sqlcard(Some(&ca))],
                },
                None => vec![rpy(reply::syntaxrm(0x0E, obj::SQLSTT))],
            },
            cmd::EXCSQLSTT => {
                let Ok(pkg) = pkg else {
                    return vec![rpy(reply::syntaxrm(0x0E, param::PKGNAMCSN))];
                };
                let params = match self.params(data) {
                    Ok(p) => p,
                    Err(ca) => return vec![self.sqlcard(Some(&ca))],
                };
                match self.sql.execute(&section_of(&pkg), &params) {
                    Ok(ca) => vec![self.sqlcard(Some(&ca).filter(|c| c.sqlcode != 0 || c.sqlerrd[2] != 0))],
                    Err(ca) => vec![self.sqlcard(Some(&ca))],
                }
            }
            cmd::RDBCMM | cmd::RDBRLLBCK => {
                let commit = c.cp == cmd::RDBCMM;
                let res = if commit { self.sql.commit() } else { self.sql.rollback() };
                for (sec, _) in self.queries.drain() {
                    let _ = self.sql.close(&sec);
                }
                match res {
                    Ok(()) => vec![
                        rpy(reply::enduowrm(&self.cfg.rdb, if commit { 1 } else { 2 })),
                        self.sqlcard(None),
                    ],
                    Err(ca) => vec![self.sqlcard(Some(&ca))],
                }
            }
            other => vec![rpy(reply::cmdnsprm(other))],
        }
    }
}
