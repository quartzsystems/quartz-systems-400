// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The kit run in memory: `QSTEST` built from the DDS through a source
//! file, loaded from the CSVs, the script and the queries run, and every
//! output written as the machine would write it.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::file::{self, CreateFileOpts, MemberOpt};
use db::util::{ImpfOpts, ImpfReport, cpyfrmimpf, cpytoimpf};
use dds::{CompileOpts, DateFmt, FileRef, MapRefs, TimeFmt, compile_lf, compile_pf};
use qsname::{MemberSpec, Name};
use sql::ast::ObjectName;
use sql::{Naming, Outcome, ResultSet, SqlContext, Value, prepare};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, ObjectId, types};
use store::txn::Txn;
use store::{FormatParams, Store, StoreConfig};

use super::kit::{self, Kit};

/// A fixed query and what it produced.
pub type QueryResult = (String, Result<ResultSet, String>);

/// The clock every run uses: 2026-01-01 00:00:00 UTC.
pub const NOW: u64 = 1_767_225_600;
/// The job.
pub const JOB: JobHandle = JobHandle(1);

/// A built library.
pub struct Local {
    /// The store.
    pub store: Store,
    /// The SQL session over it.
    pub ctx: SqlContext,
    /// `QSYS`.
    pub qsys: ObjectId,
    /// `QSTEST`.
    pub lib: ObjectId,
    /// The journal, once started.
    pub journal: Option<ObjectId>,
    files: HashMap<String, ObjectId>,
}

fn s(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Import and export options: ISO dates and times whatever the field's
/// format, nulls as empty fields.
#[must_use]
pub fn impf_opts() -> ImpfOpts {
    ImpfOpts {
        datfmt: Some(DateFmt::Iso),
        timfmt: Some(TimeFmt::Iso),
        nulls: true,
        ..ImpfOpts::default()
    }
}

fn create_opts(name: &str) -> CreateFileOpts {
    CreateFileOpts {
        now: NOW,
        reusedlt: kit::reuse_deleted(name),
        ..CreateFileOpts::default()
    }
}

impl Local {
    /// Format a store, create `QSTEST` and `QDDSSRC`, put every DDS
    /// member into it and create the files from there.
    pub fn build(kit: &Kit) -> Result<Self, String> {
        let dev = Arc::new(MemBlockDev::with_seed(32768, 7));
        let params = FormatParams {
            asp_number: 1,
            asp_uuid: [4; 16],
            unit_uuids: vec![[5; 16]],
            unit_serials: Vec::new(),
            wal_pages: 256,
            now: NOW,
        };
        let mut store = Store::format(
            vec![Box::new(dev) as Box<dyn BlockDev>],
            &params,
            StoreConfig::default(),
        )
        .map_err(s)?;
        let mut txn = store.begin().map_err(s)?;
        let qsys = ops::bootstrap(&mut txn, NOW, "QS400").map_err(s)?;
        let lib = ops::create(
            &mut txn,
            &CreateParams {
                library: qsys,
                name: Name::parse(kit::LIBRARY).expect("valid"),
                type_code: types::LIB,
                attribute: String::new(),
                text: "Database conformance kit".into(),
                owner: "QSECOFR".into(),
                system: "QS400".into(),
                now: NOW,
                by_system: false,
            },
        )
        .map_err(s)?;
        txn.commit().map_err(s)?;
        let mut ctx = SqlContext::new(
            qsys,
            JOB,
            NameContext {
                curlib: Some(lib),
                libl: vec![lib],
            },
        );
        ctx.now = Some(NOW);
        let mut local = Self {
            store,
            ctx,
            qsys,
            lib,
            journal: None,
            files: HashMap::new(),
        };
        local.create_files(kit)?;
        Ok(local)
    }

    /// Run `f` in one transaction, committed on success.
    pub fn txn<T>(
        &mut self,
        f: impl FnOnce(&mut Txn<'_>, &mut SqlContext) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut txn = self.store.begin().map_err(s)?;
        match f(&mut txn, &mut self.ctx) {
            Ok(v) => {
                txn.commit().map_err(s)?;
                Ok(v)
            }
            Err(e) => {
                txn.abort();
                Err(e)
            }
        }
    }

    /// The object of a kit file.
    #[must_use]
    pub fn file(&self, name: &str) -> Option<ObjectId> {
        self.files.get(name).copied()
    }

    fn create_files(&mut self, kit: &Kit) -> Result<(), String> {
        let members = kit.dds_members().map_err(s)?;
        let lib = self.lib;
        let src = self.txn(|txn, _| {
            let opts = CreateFileOpts {
                now: NOW,
                max_members: 0,
                member: MemberOpt::None,
                text: "DDS source".into(),
                ..CreateFileOpts::default()
            };
            let src = file::create_srcpf(
                txn,
                lib,
                Name::parse(kit::SRCFILE).expect("valid"),
                112,
                &opts,
            )
            .map_err(s)?;
            for (name, kind, text) in &members {
                let n = Name::parse(name).expect("valid");
                let ty = Name::parse(kind).expect("valid");
                file::add_member(txn, src, n, "", Some(ty), NOW).map_err(s)?;
                db::source::srcput(txn, JOB, src, MemberSpec::Name(n), text, Some(ty), NOW)
                    .map_err(s)?;
            }
            Ok(src)
        })?;
        self.files.insert(kit::SRCFILE.to_string(), src);
        let mut refs = MapRefs::default();
        for (name, kind, _) in &members {
            let n = Name::parse(name).expect("valid");
            let opts = create_opts(name);
            let compile = CompileOpts {
                file: Some(name.clone()),
                ..CompileOpts::default()
            };
            let ids = self.files.clone();
            let id = self.txn(|txn, _| {
                let source = db::source::source_of(txn, src, MemberSpec::Name(n)).map_err(s)?;
                if kind == "PF" {
                    let comp = compile_pf(&source, &refs, &compile);
                    let def = comp
                        .definition
                        .ok_or_else(|| format!("{name}.PF:\n{}", comp.listing))?;
                    let id = file::create_pf(txn, lib, n, &def, &opts).map_err(s)?;
                    refs.insert_pf(kit::LIBRARY, name, &def);
                    Ok(id)
                } else {
                    let comp = compile_lf(&source, &refs, &compile);
                    let def = comp
                        .definition
                        .ok_or_else(|| format!("{name}.LF:\n{}", comp.listing))?;
                    let mut resolve = |fr: &FileRef| -> db::Result<ObjectId> {
                        ids.get(&fr.file.to_string()).copied().ok_or_else(|| {
                            db::DbError::Definition {
                                what: format!("based-on file {fr} not found"),
                            }
                        })
                    };
                    file::create_lf(txn, lib, n, &def, &mut resolve, &opts).map_err(s)
                }
            })?;
            self.files.insert(name.clone(), id);
        }
        Ok(())
    }

    /// Load every CSV. Each import must take every row.
    pub fn load(&mut self, kit: &Kit) -> Result<Vec<(String, ImpfReport)>, String> {
        let mut out = Vec::new();
        for name in kit::PHYSICAL {
            let Some(csv) = kit.csv(name) else { continue };
            let csv = csv.map_err(s)?;
            let id = self
                .file(name)
                .ok_or_else(|| format!("{name} not created"))?;
            let r = self.txn(|txn, _| {
                cpyfrmimpf(txn, JOB, id, MemberSpec::First, &csv, &impf_opts()).map_err(s)
            })?;
            if r.errors > 0 {
                return Err(format!(
                    "{name}.csv: {} row(s) rejected: {:?}",
                    r.errors, r.skipped
                ));
            }
            out.push(((*name).to_string(), r));
        }
        Ok(out)
    }

    /// Create the journal and journal `CUSTOMER` with both images.
    pub fn start_journal(&mut self) -> Result<ObjectId, String> {
        let lib = self.lib;
        let customer = self.file("CUSTOMER").ok_or("CUSTOMER not created")?;
        let jrn = self.txn(|txn, _| {
            let rcv = jrn::rcv::crtjrnrcv(
                txn,
                lib,
                Name::parse(kit::RECEIVER).expect("valid"),
                &jrn::rcv::CrtJrnRcvOpts {
                    now: NOW,
                    ..jrn::rcv::CrtJrnRcvOpts::default()
                },
            )
            .map_err(s)?;
            let jrn = jrn::journal::crtjrn(
                txn,
                lib,
                Name::parse(kit::JOURNAL).expect("valid"),
                rcv,
                &jrn::CrtJrnOpts {
                    now: NOW,
                    ..jrn::CrtJrnOpts::default()
                },
            )
            .map_err(s)?;
            db::journal::strjrnpf(txn, JOB, customer, jrn, true, false, NOW).map_err(s)?;
            Ok(jrn)
        })?;
        self.journal = Some(jrn);
        Ok(jrn)
    }

    /// Run the RLA script in one transaction.
    pub fn run_rla(&mut self, kit: &Kit) -> Result<sql::rla::Report, String> {
        let lines = kit.rla_lines().map_err(s)?;
        self.txn(|txn, ctx| Ok(sql::rla::run_script(txn, ctx, &lines)))
    }

    /// Run every fixed query; each its own transaction.
    pub fn run_queries(&mut self, kit: &Kit) -> Result<Vec<QueryResult>, String> {
        let mut out = Vec::new();
        for q in kit.queries().map_err(s)? {
            let r = self.txn(|txn, ctx| {
                let p = prepare(&q, Naming::Sys).map_err(s)?;
                match p.execute(txn, ctx, &[]).map_err(s)? {
                    Outcome::Rows(rs) => Ok(rs),
                    _ => Err("not a query".into()),
                }
            });
            out.push((q, r));
        }
        Ok(out)
    }

    /// The export of a file: every record in key order when keyed.
    pub fn export(&mut self, name: &str) -> Result<String, String> {
        let id = self
            .file(name)
            .ok_or_else(|| format!("{name} not created"))?;
        self.txn(|txn, _| {
            cpytoimpf(txn, JOB, id, MemberSpec::First, &impf_opts())
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .map_err(s)
        })
    }

    /// `ffd.csv` rows for every file: one per field.
    pub fn ffd_csv(&mut self, kit_names: &[&str]) -> Result<String, String> {
        let mut out = String::from("FILE,FORMAT,FIELD,TYPE,DIGITS,DECIMALS,BYTES,POSITION,NULL\n");
        for name in kit_names {
            let id = self
                .file(name)
                .ok_or_else(|| format!("{name} not created"))?;
            let d = self.txn(|txn, _| db::dsp::dspffd(txn, id).map_err(s))?;
            for f in &d.formats {
                for x in &f.fields {
                    let _ = writeln!(
                        out,
                        "{name},{},{},{},{},{},{},{},{}",
                        f.name,
                        x.name,
                        x.data_type,
                        // `WHFLDD`: digits, 0 for a character field.
                        if matches!(x.data_type, 'A' | 'H' | 'G' | 'O' | 'L' | 'T' | 'Z' | 'J' | 'E') {
                            0
                        } else {
                            x.length
                        },
                        x.decimals,
                        x.byte_len,
                        x.position,
                        if x.allow_null { "Y" } else { "N" }
                    );
                }
            }
        }
        Ok(out)
    }

    /// `lvlid.csv`: one row per record format.
    pub fn lvlid_csv(&mut self, kit_names: &[&str]) -> Result<String, String> {
        let mut out = String::from("FILE,FORMAT,LEVELID\n");
        for name in kit_names {
            let id = self
                .file(name)
                .ok_or_else(|| format!("{name} not created"))?;
            let d = self.txn(|txn, _| db::dsp::dspffd(txn, id).map_err(s))?;
            for f in &d.formats {
                let _ = writeln!(out, "{name},{},{}", f.name, f.level_id);
            }
        }
        Ok(out)
    }

    /// `dspjrn.csv`: every entry of the journal without its time or job.
    pub fn dspjrn_csv(&mut self) -> Result<String, String> {
        let Some(jrn) = self.journal else {
            return Ok(String::new());
        };
        let entries =
            self.txn(|txn, _| jrn::dspjrn(txn, jrn, &jrn::Filter::default()).map_err(s))?;
        let mut out = String::from("SEQ,CODE,TYPE,OBJECT,MEMBER,COUNT,CYCLE\n");
        for e in &entries {
            let (obj, mbr) = e
                .object
                .as_ref()
                .map_or((String::new(), String::new()), |o| {
                    (
                        o.object.to_string(),
                        o.member.map(|m| m.to_string()).unwrap_or_default(),
                    )
                });
            let _ = writeln!(
                out,
                "{},{},{},{},{},{},{}",
                e.seq,
                e.code as char,
                String::from_utf8_lossy(&e.kind),
                obj,
                mbr,
                e.count,
                e.commit_cycle
            );
        }
        Ok(out)
    }

    /// Write every output under `dir`; the files written.
    pub fn dump(
        &mut self,
        kit: &Kit,
        rla: &sql::rla::Report,
        queries: &[QueryResult],
        dir: &Path,
    ) -> Result<Vec<PathBuf>, String> {
        let mut written = Vec::new();
        let mut put = |rel: &str, text: &str| -> Result<(), String> {
            let p = dir.join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).map_err(s)?;
            }
            fs::write(&p, text).map_err(s)?;
            written.push(p);
            Ok(())
        };
        let exported: Vec<&str> = kit::exported_physical()
            .chain(kit::EXPORTED_LOGICAL.iter().copied())
            .collect();
        for name in &exported {
            let text = self.export(name)?;
            put(&format!("data/{name}.csv"), &text)?;
        }
        let all: Vec<&str> = kit::PHYSICAL
            .iter()
            .chain(kit::LOGICAL.iter())
            .copied()
            .collect();
        let ffd = self.ffd_csv(&all)?;
        put("ffd.csv", &ffd)?;
        let lvl = self.lvlid_csv(&all)?;
        put("lvlid.csv", &lvl)?;
        put("rla.log", &rla.text)?;
        put("rla.csv", &rla_csv(rla))?;
        let jrn = self.dspjrn_csv()?;
        if !jrn.is_empty() {
            put("dspjrn.csv", &jrn)?;
        }
        for (i, (q, r)) in queries.iter().enumerate() {
            let rs = r
                .as_ref()
                .map_err(|e| format!("query {}: {e}\n  {q}", i + 1))?;
            put(&format!("sql/Q{:02}.csv", i + 1), &result_csv(rs))?;
        }
        let _ = kit;
        Ok(written)
    }
}

/// The per-line log the oracle's `RLALOG` also holds.
#[must_use]
pub fn rla_csv(r: &sql::rla::Report) -> String {
    let mut out = String::from("LINE,STATUS,FLAGS,DETAIL\n");
    for l in &r.lines {
        let flags = format!(
            "{}{}{}",
            if l.found { 'F' } else { '-' },
            if l.equal { 'E' } else { '-' },
            if l.eof { 'O' } else { '-' }
        );
        let detail = l
            .fields
            .iter()
            .map(|(f, v)| format!("{f}={v}"))
            .collect::<Vec<_>>()
            .join(" ");
        let _ = writeln!(
            out,
            "{},{},{},{}",
            l.line,
            l.status,
            flags,
            csv_quote(&detail)
        );
    }
    out
}

/// A result set as CSV without a header.
#[must_use]
pub fn result_csv(rs: &ResultSet) -> String {
    let mut out = String::new();
    for row in &rs.rows {
        let cells: Vec<String> = row
            .iter()
            .map(|v| match v {
                Value::Null => String::new(),
                Value::Str(t) => csv_quote(t.trim_end()),
                other => other.to_string(),
            })
            .collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

fn csv_quote(t: &str) -> String {
    format!("\"{}\"", t.replace('"', "\"\""))
}

/// The qualified kit name of a file.
#[must_use]
pub fn qualified(name: &str) -> ObjectName {
    ObjectName::qualified(kit::LIBRARY, name)
}
