// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A member's database in memory: a store with `QGPL`, the files its
//! `dds:` steps create and the rows its `sql:` steps insert (`{lib}`
//! is `QGPL`), the external descriptions the compiler reads from the
//! `*FILE` objects, and the job's `StoreIo` over the same store. The
//! display files of its `dds:` steps are compiled by `dspf` and kept
//! beside the store, for the compiler and the kit's display.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::file::CreateFileOpts;
use dds::{CompileOpts, FileRef, MapRefs, Source, compile_lf, compile_pf};
use mi::io::store::StoreIo;
use qsname::{LibSpec, Name};
use rpg_oracle::case::{DbStep, DdsKind};
use rpgc::files::{ExtFile, ExtFiles};
use rpgextdesc::{database_desc, display_desc, printer_desc};
use sql::{Naming, RunOpts, SqlContext};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, ObjectId, types};
use store::{FormatParams, Store, StoreConfig};

use crate::qir::CLOCK;

/// The library every member's files live in.
pub const LIB: &str = "QGPL";

/// The kit job's qualified name.
pub const KIT_JOB: &str = "000001/QUSER/KIT";

fn s(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn name(n: &str) -> Result<Name, String> {
    Name::parse(n).map_err(|_| format!("{n}: not a name"))
}

/// A member's store.
#[derive(Clone)]
pub struct KitDb {
    /// The store.
    pub store: Rc<RefCell<Store>>,
    /// `QSYS`.
    pub qsys: ObjectId,
    /// `QGPL`.
    pub lib: ObjectId,
    /// The display files, by name.
    pub displays: Vec<(String, Rc<dspf::DisplayFile>)>,
    /// The printer files, by name.
    pub printers: Vec<(String, Rc<dds::prtf::PrtfDefinition>)>,
    /// The record locks the job's files and its SQL share.
    pub locks: db::odp::SharedLocks,
}

impl KitDb {
    /// Build the store a member's steps describe.
    pub fn build(steps: &[DbStep]) -> Result<Self, String> {
        let (mut store, qsys, lib) = empty_store()?;
        let mut ctx = SqlContext::new(
            qsys,
            JobHandle(1),
            NameContext {
                curlib: Some(lib),
                libl: vec![lib],
            },
        );
        ctx.now = Some(CLOCK);
        let mut refs = MapRefs::default();
        let mut made: Vec<(String, ObjectId)> = Vec::new();
        let mut displays = Vec::new();
        let mut printers = Vec::new();
        for step in steps {
            match step {
                DbStep::Dds {
                    name: n,
                    kind,
                    source,
                } => {
                    let (src, _) = Source::from_text(&source.join("\n"));
                    match kind {
                        DdsKind::Dspf => {
                            displays.push((n.clone(), Rc::new(display_file(&src, &refs, n)?)));
                            continue;
                        }
                        DdsKind::Prtf => {
                            printers.push((n.clone(), Rc::new(printer_file(&src, &refs, n)?)));
                            continue;
                        }
                        _ => {}
                    }
                    let opts = CreateFileOpts {
                        now: CLOCK,
                        ..CreateFileOpts::default()
                    };
                    let compile = CompileOpts {
                        file: Some(n.clone()),
                        ..CompileOpts::default()
                    };
                    let mut t = store.begin().map_err(s)?;
                    if *kind == DdsKind::Lf {
                        let c = compile_lf(&src, &refs, &compile);
                        let def = c
                            .definition
                            .ok_or_else(|| format!("{n}.lf:\n{}", c.listing))?;
                        let mut resolve = |fr: &FileRef| -> db::Result<ObjectId> {
                            made.iter()
                                .find(|(m, _)| *m == fr.file.to_string())
                                .map(|(_, id)| *id)
                                .ok_or_else(|| db::DbError::Definition {
                                    what: format!("based-on file {fr} not found"),
                                })
                        };
                        let id =
                            db::file::create_lf(&mut t, lib, name(n)?, &def, &mut resolve, &opts)
                                .map_err(s)?;
                        made.push((n.clone(), id));
                    } else {
                        let c = compile_pf(&src, &refs, &compile);
                        let def = c
                            .definition
                            .ok_or_else(|| format!("{n}.pf:\n{}", c.listing))?;
                        let id =
                            db::file::create_pf(&mut t, lib, name(n)?, &def, &opts).map_err(s)?;
                        made.push((n.clone(), id));
                        refs.insert_pf(LIB, n, &def);
                    }
                    t.commit().map_err(s)?;
                }
                DbStep::Sql(text) => {
                    let text = text.replace("{lib}", LIB);
                    let opts = RunOpts {
                        naming: Naming::Sql,
                        ..RunOpts::default()
                    };
                    let r = sql::runsqlstm(&mut store, &mut ctx, &text, &opts).map_err(s)?;
                    if r.errors > 0 {
                        return Err(format!("{text}\n{}", r.listing));
                    }
                }
                DbStep::SqlFile(f) => return Err(format!("sqlfile {f} was not loaded")),
            }
        }
        Ok(Self {
            store: Rc::new(RefCell::new(store)),
            qsys,
            lib,
            displays,
            printers,
            locks: db::odp::new_shared(),
        })
    }

    /// A member's `post:` commands that change the store: `CRTJRNRCV`,
    /// `CRTJRN` and `STRJRNPF` (both images); the rest (`CHGPF
    /// WAITRCD`, `STRCMTCTL`) have nothing to do here.
    pub fn post(&self, cmds: &[String]) -> Result<(), String> {
        use jrn::journal::{CrtJrnOpts, crtjrn};
        use jrn::rcv::{CrtJrnRcvOpts, crtjrnrcv};
        let obj = |q: &str| q.rsplit('/').next().unwrap_or(q).to_string();
        let mut store = self.store.borrow_mut();
        for cmd in cmds {
            let up = cmd.trim().to_ascii_uppercase();
            let verb = up.split_whitespace().next().unwrap_or("");
            let mut t = store.begin().map_err(s)?;
            let found = |t: &mut store::txn::Txn<'_>, n: &str, ty| {
                let ctx = NameContext {
                    curlib: Some(self.lib),
                    libl: vec![self.lib],
                };
                ops::resolve(
                    t,
                    self.qsys,
                    &ctx,
                    LibSpec::Libl,
                    Name::parse(n).map_err(|_| format!("{n}: not a name"))?,
                    ty,
                )
                .map(|f| f.id)
                .map_err(s)
            };
            match verb {
                "CRTJRNRCV" => {
                    let o = CrtJrnRcvOpts {
                        now: CLOCK,
                        ..CrtJrnRcvOpts::default()
                    };
                    crtjrnrcv(
                        &mut t,
                        self.lib,
                        name(&obj(&super::build::kwd(cmd, "JRNRCV")))?,
                        &o,
                    )
                    .map_err(s)?;
                }
                "CRTJRN" => {
                    let rcv = found(&mut t, &obj(&super::build::kwd(cmd, "JRNRCV")), jrn::JRNRCV)?;
                    let o = CrtJrnOpts {
                        now: CLOCK,
                        ..CrtJrnOpts::default()
                    };
                    crtjrn(
                        &mut t,
                        self.lib,
                        name(&obj(&super::build::kwd(cmd, "JRN")))?,
                        rcv,
                        &o,
                    )
                    .map_err(s)?;
                }
                "STRJRNPF" => {
                    let file = found(&mut t, &obj(&super::build::kwd(cmd, "FILE")), types::FILE)?;
                    let j = found(&mut t, &obj(&super::build::kwd(cmd, "JRN")), jrn::JRN)?;
                    db::journal::strjrnpf(&mut t, JobHandle(1), file, j, true, true, CLOCK)
                        .map_err(s)?;
                }
                _ => {}
            }
            t.commit().map_err(s)?;
        }
        Ok(())
    }

    /// The files of `QGPL`, by name.
    pub fn files(&self) -> Result<Vec<(Name, ObjectId)>, String> {
        let mut store = self.store.borrow_mut();
        let mut t = store.begin().map_err(s)?;
        let list = ops::list(&mut t, self.lib, Some(types::FILE)).map_err(s)?;
        t.abort();
        Ok(list.into_iter().map(|(k, id)| (k.name, id)).collect())
    }

    /// The job's database: a `StoreIo` over this store.
    pub fn io(&self) -> Result<StoreIo, String> {
        let resolve = db::odp::session::resolve_in(name(LIB)?, self.files()?);
        Ok(StoreIo::with_locks(
            Box::new(Rc::clone(&self.store)),
            JobHandle(1),
            Box::new(resolve),
            Arc::clone(&self.locks),
        ))
    }

    /// The job's embedded SQL over the same store, sharing the record
    /// locks.
    #[must_use]
    pub fn sql(&self) -> sql::embed::Session {
        let mut ctx = SqlContext::with_locks(
            self.qsys,
            JobHandle(1),
            NameContext {
                curlib: Some(self.lib),
                libl: vec![self.lib],
            },
            Arc::clone(&self.locks),
        );
        ctx.now = Some(CLOCK);
        ctx.registers.job_name = Some(KIT_JOB.into());
        sql::embed::Session::with_ctx(Box::new(Rc::clone(&self.store)), ctx)
    }

    /// The SQL of a `QSQSRVR` job serving one of the job's server-mode
    /// CLI connections: a job of its own over the same store.
    #[must_use]
    pub fn sql_server(&self) -> sql::embed::Session {
        let mut ctx = SqlContext::with_locks(
            self.qsys,
            JobHandle(2),
            NameContext {
                curlib: Some(self.lib),
                libl: vec![self.lib],
            },
            Arc::clone(&self.locks),
        );
        ctx.now = Some(CLOCK);
        ctx.registers.job_name = Some("000002/QUSER/QSQSRVR".into());
        ctx.registers.server_mode_job_name = Some(KIT_JOB.into());
        sql::embed::Session::with_ctx(Box::new(Rc::clone(&self.store)), ctx)
    }

    /// The external descriptions the compiler reads.
    #[must_use]
    pub fn ext(&self) -> KitExt {
        KitExt(self.clone())
    }
}

/// A store with `QSYS` and `QGPL`.
fn empty_store() -> Result<(Store, ObjectId, ObjectId), String> {
    let d = Arc::new(MemBlockDev::with_seed(32768, 13));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
        now: CLOCK,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .map_err(s)?;
    let mut t = store.begin().map_err(s)?;
    let qsys = ops::bootstrap(&mut t, CLOCK, "QS400").map_err(s)?;
    let lib = ops::create(
        &mut t,
        &CreateParams {
            library: qsys,
            name: name(LIB)?,
            type_code: types::LIB,
            attribute: String::new(),
            text: String::new(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: CLOCK,
            by_system: false,
        },
    )
    .map_err(s)?;
    t.commit().map_err(s)?;
    Ok((store, qsys, lib))
}

/// A display file from DDS, or its listing.
fn display_file(src: &Source, refs: &MapRefs, n: &str) -> Result<dspf::DisplayFile, String> {
    let opts = CompileOpts {
        file: Some(n.to_string()),
        ..CompileOpts::default()
    };
    let c = dspf::compile_dspf(src, refs, &opts);
    c.definition
        .ok_or_else(|| format!("{n}.dspf:\n{}", c.listing))
}

/// A printer file from DDS, or its listing.
fn printer_file(
    src: &Source,
    refs: &MapRefs,
    n: &str,
) -> Result<dds::prtf::PrtfDefinition, String> {
    let opts = CompileOpts {
        file: Some(n.to_string()),
        ..CompileOpts::default()
    };
    let c = dds::compile_prtf(src, refs, &opts);
    c.definition
        .ok_or_else(|| format!("{n}.prtf:\n{}", c.listing))
}

/// External descriptions from the store's `*FILE` objects.
pub struct KitExt(KitDb);

impl ExtFiles for KitExt {
    fn describe(&mut self, file: &str, lib: &str) -> Option<ExtFile> {
        let db = &self.0;
        if let Some((_, df)) = db
            .displays
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(file))
        {
            return Some(display_desc(df, LIB));
        }
        if let Some((_, def)) = db
            .printers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(file))
        {
            return Some(printer_desc(def, LIB));
        }
        let mut store = db.store.borrow_mut();
        let mut t = store.begin().ok()?;
        let libspec = match lib {
            "*LIBL" | "*CURLIB" | "" => LibSpec::Libl,
            l => LibSpec::Name(Name::parse(l).ok()?),
        };
        let ctx = NameContext {
            curlib: Some(db.lib),
            libl: vec![db.lib],
        };
        let found = ops::resolve(
            &mut t,
            db.qsys,
            &ctx,
            libspec,
            Name::parse(file).ok()?,
            types::FILE,
        )
        .ok();
        let desc = found.and_then(|f| db::file::describe(&mut t, f.id).ok());
        t.abort();
        desc.as_ref().map(database_desc)
    }
}
