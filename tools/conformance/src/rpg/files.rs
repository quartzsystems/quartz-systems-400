// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A member's database in memory: a store with `QGPL`, the files its
//! `dds:` steps create and the rows its `sql:` steps insert (`{lib}`
//! is `QGPL`), the external descriptions the compiler reads from the
//! `*FILE` objects, and the job's `StoreIo` over the same store.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::file::CreateFileOpts;
use dds::{CompileOpts, DataType, FileRef, MapRefs, Source, compile_lf, compile_pf};
use mi::io::store::StoreIo;
use qsname::{LibSpec, Name};
use rpg_oracle::case::DbStep;
use rpgc::files::{Dev, ExtField, ExtFile, ExtFiles, ExtFormat};
use rpgc::types::{DatFmt, TimFmt, Ty};
use sql::{Naming, RunOpts, SqlContext};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, ObjectId, types};
use store::{FormatParams, Store, StoreConfig};

use crate::qir::CLOCK;

/// The library every member's files live in.
pub const LIB: &str = "QGPL";

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
}

impl KitDb {
    /// Build the store a member's steps describe.
    pub fn build(steps: &[DbStep]) -> Result<Self, String> {
        let d = Arc::new(MemBlockDev::with_seed(32768, 13));
        let params = FormatParams {
            asp_number: 1,
            asp_uuid: [4; 16],
            unit_uuids: vec![[5; 16]],
            unit_serials: Vec::new(),
            wal_pages: 256,
            now: CLOCK,
        };
        let mut store = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &params, StoreConfig::default()).map_err(s)?;
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
        for step in steps {
            match step {
                DbStep::Dds { name: n, logical, source } => {
                    let (src, _) = Source::from_text(&source.join("\n"));
                    let opts = CreateFileOpts {
                        now: CLOCK,
                        ..CreateFileOpts::default()
                    };
                    let compile = CompileOpts {
                        file: Some(n.clone()),
                        ..CompileOpts::default()
                    };
                    let mut t = store.begin().map_err(s)?;
                    if *logical {
                        let c = compile_lf(&src, &refs, &compile);
                        let def = c.definition.ok_or_else(|| format!("{n}.lf:\n{}", c.listing))?;
                        let mut resolve = |fr: &FileRef| -> db::Result<ObjectId> {
                            made.iter().find(|(m, _)| *m == fr.file.to_string()).map(|(_, id)| *id).ok_or_else(|| {
                                db::DbError::Definition {
                                    what: format!("based-on file {fr} not found"),
                                }
                            })
                        };
                        let id = db::file::create_lf(&mut t, lib, name(n)?, &def, &mut resolve, &opts).map_err(s)?;
                        made.push((n.clone(), id));
                    } else {
                        let c = compile_pf(&src, &refs, &compile);
                        let def = c.definition.ok_or_else(|| format!("{n}.pf:\n{}", c.listing))?;
                        let id = db::file::create_pf(&mut t, lib, name(n)?, &def, &opts).map_err(s)?;
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
        })
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
        Ok(StoreIo::new(Box::new(Rc::clone(&self.store)), JobHandle(1), Box::new(resolve)))
    }

    /// The external descriptions the compiler reads.
    #[must_use]
    pub fn ext(&self) -> KitExt {
        KitExt(self.clone())
    }
}

/// External descriptions from the store's `*FILE` objects.
pub struct KitExt(KitDb);

impl ExtFiles for KitExt {
    fn describe(&mut self, file: &str, lib: &str) -> Option<ExtFile> {
        let db = &self.0;
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
        let found = ops::resolve(&mut t, db.qsys, &ctx, libspec, Name::parse(file).ok()?, types::FILE).ok();
        let desc = found.and_then(|f| db::file::describe(&mut t, f.id).ok());
        t.abort();
        let desc = desc?;
        Some(ExtFile {
            lib: desc.library_name.to_string(),
            dev: Dev::Disk,
            formats: desc.formats.iter().map(format_of).collect(),
        })
    }
}

/// One record format as the compiler sees it.
#[must_use]
pub fn format_of(f: &db::FormatDef) -> ExtFormat {
    let r = &f.format;
    ExtFormat {
        name: r.name.to_string(),
        level_id: f.level_id.clone(),
        len: r.record_len,
        fields: r
            .fields
            .iter()
            .map(|d| ExtField {
                name: d.name.to_string(),
                ty: rpg_type(d),
                offset: d.offset,
                usage: match d.usage {
                    Some(dds::Usage::Input) => 'I',
                    Some(dds::Usage::Output) => 'O',
                    Some(dds::Usage::Neither) => 'N',
                    _ => 'B',
                },
                alias: d.alias.clone().unwrap_or_default(),
                nullable: d.allow_null,
            })
            .collect(),
        keys: r.keys.iter().map(|k| k.name.to_string()).collect(),
    }
}

/// A database field's RPG type.
fn rpg_type(d: &dds::FieldDef) -> Ty {
    let digits = u8::try_from(d.length).unwrap_or(63);
    match d.data_type {
        DataType::Char | DataType::Hex | DataType::DbcsOpen => {
            if d.varlen.is_some() {
                Ty::VarChar(d.length, 2)
            } else {
                Ty::Char(d.length)
            }
        }
        DataType::Graphic => Ty::Graph(d.length),
        DataType::Zoned => Ty::Zoned(digits, d.decimals),
        DataType::Packed => Ty::Packed(digits, d.decimals),
        DataType::Binary => Ty::Bin(digits, d.decimals),
        DataType::Float => Ty::Float(if d.byte_len == 4 { 4 } else { 8 }),
        DataType::Date => {
            let f = match d.date_fmt.unwrap_or_default() {
                dds::DateFmt::Iso => DatFmt::Iso,
                dds::DateFmt::Usa => DatFmt::Usa,
                dds::DateFmt::Eur => DatFmt::Eur,
                dds::DateFmt::Jis => DatFmt::Jis,
                dds::DateFmt::Mdy => DatFmt::Mdy,
                dds::DateFmt::Dmy => DatFmt::Dmy,
                dds::DateFmt::Ymd => DatFmt::Ymd,
                dds::DateFmt::Jul => DatFmt::Jul,
            };
            Ty::Date(f, d.date_sep.unwrap_or(f.sep()))
        }
        DataType::Time => {
            let f = match d.time_fmt.unwrap_or_default() {
                dds::TimeFmt::Hms => TimFmt::Hms,
                dds::TimeFmt::Iso => TimFmt::Iso,
                dds::TimeFmt::Usa => TimFmt::Usa,
                dds::TimeFmt::Eur => TimFmt::Eur,
                dds::TimeFmt::Jis => TimFmt::Jis,
            };
            Ty::Time(f, d.time_sep.unwrap_or(f.sep()))
        }
        DataType::Timestamp => Ty::Timestamp(6),
    }
}
