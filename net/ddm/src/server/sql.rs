// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL commands: binding (accepted; sections are dynamic), prepare
//! and describe, open, continue and close a query, execute and execute
//! immediately, commit and roll back. Replies follow the pub400 traces:
//! a failed prepare answers `SQLCARD` alone; `OPNQRY` answers `OPNQRYRM`
//! and `QRYDSC`; the rows come with `CNTQRY`, and the last block ends
//! with the `+100` row, `ENDQRYRM` and `SQLCARD`. A `CALL` that leaves
//! result sets answers as `drda-call.trace` shows: `RSLSETRM` with a
//! section per set, the `SQLCARD`, `SQLRSLRD`, then each set opened
//! with its first block.

// The SQLCA is the error DRDA returns; it is not boxed.
#![allow(clippy::result_large_err)]

use super::{Section, Session};
use crate::cp::{cmd, obj, param, rm, svrcod};
use crate::fdoca::desc::{self, Field};
use crate::fdoca::sqlda::{self, Sqldard};
use crate::fdoca::types::drda;
use crate::fdoca::{Reader, Writer, rows, sqlca};
use crate::msg::sqlcmd::sqlstt_bytes;
use crate::msg::{Pkgnamcsn, reply};
use crate::sql::{ColType, Sqlca, Value};
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

    /// A `CALL`'s output values as the primary answers `OUTEXP`
    /// (`tools/conformance/ddm/outexp-primary.txt`): every parameter by
    /// its early descriptor, nullable, the SQLCA (null when clean), then
    /// the row, an input-only parameter null.
    fn sqldtard(&self, ca: &Sqlca, outs: &[(ColType, Value)]) -> (DssType, Object) {
        let fields: Vec<Field> = outs
            .iter()
            .map(|(t, _)| {
                let mut t = *t;
                t.nullable = true;
                let mut f = Field::of(&t);
                // A varying character value goes as a long one (0x35).
                if u16::from(f.drda & !1) == drda::VARCHAR {
                    f.drda = u8::try_from(drda::LONG).unwrap_or(f.drda) | 1;
                }
                f
            })
            .collect();
        let mut w = Writer::new(crate::fdoca::Repr::default());
        let mut ca = ca.clone();
        if ca.rdbname.is_empty() {
            ca.rdbname.clone_from(&self.cfg.rdb);
        }
        let clean = ca.sqlcode == 0 && ca.sqlwarn.iter().all(|c| *c == b' ');
        sqlca::write(&mut w, (!clean).then_some(&ca), self.sqlam);
        w.u8(rows::PRESENT);
        for (f, (_, v)) in fields.iter().zip(outs) {
            let mut one = Writer::new(crate::fdoca::Repr::default());
            if let Err(e) = rows::write_value(&mut one, f, v) {
                let bad = Sqlca::error(-30_000, "58008", &e.to_string());
                return self.sqlcard(Some(&bad));
            }
            // A positive packed value is signed F, as the primary sends it.
            if u16::from(f.drda & !1) == drda::DECIMAL
                && let Some(last) = one.out.last_mut()
                && *last & 0x0F == 0x0C
            {
                *last |= 0x0F;
            }
            w.bytes(&one.out);
        }
        (
            DssType::Object,
            Object::coll(
                obj::SQLDTARD,
                vec![Object::data(obj::FDODSC, desc::qrydsc_early(&fields)), Object::data(obj::FDODTA, w.out)],
            ),
        )
    }

    /// `BGNBND` starts a package, `BNDSQLSTT` adds a section's statement,
    /// `ENDBND` makes it (`SqlBackend::bind`), as the primary keeps a
    /// bound package (`DLTSQLPKG` deletes it).
    fn bind_command(&mut self, c: &Object, data: &[Object]) -> Out {
        let named = |cp: u16| {
            c.get(cp).and_then(|o| {
                let mut b = o.bytes().to_vec();
                if cp == param::PKGNAMCT {
                    b.extend([0, 0]);
                }
                Pkgnamcsn::decode(&b).ok()
            })
        };
        match c.cp {
            cmd::BGNBND => {
                let Some(p) = named(param::PKGNAMCT) else {
                    return vec![(DssType::Reply, reply::syntaxrm(0x0E, param::PKGNAMCT))];
                };
                self.binding = Some((p, Vec::new()));
                vec![self.sqlcard(None)]
            }
            cmd::BNDSQLSTT => {
                let (Some(p), Some(text)) = (named(param::PKGNAMCSN), self.statement(data)) else {
                    return vec![(DssType::Reply, reply::syntaxrm(0x0E, param::PKGNAMCSN))];
                };
                if let Some((_, stmts)) = self.binding.as_mut() {
                    stmts.push((p.section, text));
                }
                vec![self.sqlcard(None)]
            }
            _ => {
                let Some((p, stmts)) = self.binding.take() else {
                    return vec![self.sqlcard(None)];
                };
                match self.sql.bind(&p.collection, &p.package, p.token, &stmts) {
                    Ok(()) => vec![self.sqlcard(None)],
                    Err(ca) => vec![self.sqlcard(Some(&ca))],
                }
            }
        }
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

    /// The reply to a `CALL` that left result sets, at most `max` of
    /// them: each gets a section of the procedure's own package, as IBM
    /// i gives (`CNTQRY` names it for the rest of the rows).
    fn result_sets(&mut self, pkg: &Pkgnamcsn, ca: &Sqlca, max: u16) -> Out {
        let (routine, names) = self.sql.result_sets();
        let n = names.len().min(usize::from(max));
        let (schema, name) = routine.split_once('.').unwrap_or(("", routine.as_str()));
        let base = Pkgnamcsn {
            rdb: pkg.rdb.clone(),
            collection: schema.to_string(),
            package: name.to_string(),
            token: *b"QSRSLSET",
            section: 0,
        };
        let at = |i: usize| base.at(u16::try_from(i + 1).unwrap_or(u16::MAX));
        let list: Vec<Object> = (0..n).map(|i| at(i).to_object()).collect();
        let mut out = vec![(DssType::Reply, reply::rslsetrm(&list)), self.sqlcard(Some(ca))];
        let mut w = Writer::new(crate::fdoca::Repr::default());
        w.i16(i16::try_from(n).unwrap_or(i16::MAX));
        for (i, cursor) in names.iter().take(n).enumerate() {
            // The locator, the name (no mixed part) and an unknown count.
            w.i32(0x0001_0000 + i32::try_from(i + 1).unwrap_or(0));
            w.vcs("");
            w.vcs(cursor);
            w.i32(-1);
        }
        out.push((DssType::Object, Object::data(obj::SQLRSLRD, w.out)));
        for i in 0..n {
            let sec = section_of(&at(i));
            let columns = match self.sql.open_result_set(&sec, i) {
                Ok(c) => c,
                Err(e) => {
                    out.push((DssType::Reply, reply::opnqflrm(&self.cfg.rdb)));
                    out.push(self.sqlcard(Some(&e)));
                    continue;
                }
            };
            let fields: Vec<Field> = columns.iter().map(|c| Field::of(&c.ty)).collect();
            let id = self.next_qryinsid.to_be_bytes();
            self.next_qryinsid += 1;
            out.push((DssType::Reply, reply::opnqryrm_result_set(&id)));
            if i > 0 {
                // IBM i puts an empty SQLCARD before each later set's columns.
                out.push(self.sqlcard(None));
            }
            let mut cin = Writer::new(crate::fdoca::Repr::default());
            sqlda::write_cinrd(&mut cin, &columns);
            out.push((DssType::Object, Object::data(obj::SQLCINRD, cin.out)));
            out.push((DssType::Object, Object::data(obj::QRYDSC, desc::qrydsc_early(&fields))));
            let block = self.cfg.block_rows.max(1);
            let batch = match self.sql.fetch(&sec, block) {
                Ok(b) => b,
                Err(e) => {
                    out.push(self.sqlcard(Some(&e)));
                    continue;
                }
            };
            let mut d = Writer::new(crate::fdoca::Repr::default());
            for row in &batch {
                if let Err(e) = rows::write_row(&mut d, &fields, row) {
                    let ca = Sqlca::error(-30_000, "58008", &e.to_string());
                    out.push(self.sqlcard(Some(&ca)));
                }
            }
            out.push((DssType::Object, Object::data(obj::QRYDTA, d.out)));
            if batch.len() < block {
                let _ = self.sql.close(&sec);
                let mut end = Sqlca::no_data();
                end.rdbname.clone_from(&self.cfg.rdb);
                out.push((DssType::Reply, reply::endqryrm(&self.cfg.rdb)));
                out.push(self.sqlcard(Some(&end)));
            } else {
                self.queries.insert(sec, Query { fields });
            }
        }
        out
    }

    #[allow(clippy::too_many_lines)] // one arm per command
    pub(super) fn sql_command(&mut self, c: &Object, data: &[Object]) -> Out {
        let rpy = |o: Object| (DssType::Reply, o);
        let pkg = Pkgnamcsn::from_command(c);
        match c.cp {
            cmd::BGNBND | cmd::BNDSQLSTT | cmd::ENDBND => self.bind_command(c, data),
            // The SET statements of the connection's environment, each run.
            cmd::EXCSQLSET => {
                let texts: Vec<String> = data
                    .iter()
                    .filter(|o| o.cp == obj::SQLSTT)
                    .filter_map(|o| sqlstt_bytes(o.bytes()).ok())
                    .map(|(b, _)| ccsid::conv::to_string(self.their_repr.ccsid, &b).unwrap_or_else(|_| crate::wire::text(&b)))
                    .collect();
                for t in texts {
                    if let Err(ca) = self.sql.execute_immediate(&t) {
                        return vec![self.sqlcard(Some(&ca))];
                    }
                }
                vec![self.sqlcard(None)]
            }
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
                let outexp = c.get(param::OUTEXP).and_then(|x| x.bytes().first().copied()) == Some(0xF1);
                let max = c.get_u16(param::MAXRSLCNT).unwrap_or(0);
                let sec = section_of(&pkg);
                match self.sql.execute(&sec, &params) {
                    Ok(ca) if outexp => {
                        let card = match self.sql.call_outputs(&sec) {
                            Ok(outs) => self.sqldtard(&ca, &outs),
                            Err(e) => return vec![self.sqlcard(Some(&e))],
                        };
                        if ca.sqlcode == 466 && max > 0 {
                            let mut out = self.result_sets(&pkg, &ca, max);
                            // The output values stand in for the SQLCARD.
                            out[1] = card;
                            out
                        } else {
                            vec![card]
                        }
                    }
                    Ok(ca) if ca.sqlcode == 466 && max > 0 => self.result_sets(&pkg, &ca, max),
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
