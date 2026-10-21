// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*MODULE` payload: a header, a section table, then the IR, the
//! native output, the creation data and the source view.

use codegen::ModuleCode;
use qir::{Linkage, Module, StorageClass};
use store::spec::{Layout, field};

use super::ExportKind;
use super::code;
use crate::bin::{R, W};
use crate::error::Result;

/// Size of `ModuleHdr`.
pub const MODULE_HDR_LEN: usize = 64;
/// Size of `Section`.
pub const SECTION_LEN: usize = 24;
const MAGIC: &[u8; 8] = b"QS400MOD";
const VERSION: u32 = 1;

const SEC_IR: u32 = 1;
const SEC_CODE: u32 = 2;
const SEC_CREATE: u32 = 3;
const SEC_SOURCE: u32 = 4;

/// Layout of `ModuleHdr`, for the spec test.
pub static HDR_LAYOUT: Layout = Layout {
    name: "ModuleHdr",
    size: Some(MODULE_HDR_LEN),
    fields: &[
        field("magic", 0, 8),
        field("version", 8, 4),
        field("codegen_version", 12, 4),
        field("ir_version", 16, 2),
        field("lang", 18, 1),
        field("dbgview", 19, 1),
        field("created", 20, 8),
        field("section_count", 28, 4),
        field("flags", 32, 4),
        field("reserved", 36, 28),
    ],
};

/// Layout of `Section`, for the spec test.
pub static SECTION_LAYOUT: Layout = Layout {
    name: "Section",
    size: Some(SECTION_LEN),
    fields: &[
        field("tag", 0, 4),
        field("reserved", 4, 4),
        field("offset", 8, 8),
        field("len", 16, 8),
    ],
};

/// How a module was made.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CreateData {
    /// The compiler, such as `QIRC` or `RPGLE`.
    pub compiler: String,
    /// Options as given.
    pub options: String,
    /// Source file.
    pub src_file: String,
    /// Source library.
    pub src_lib: String,
    /// Source member.
    pub src_mbr: String,
}

/// A `*MODULE` object's payload.
#[derive(Clone, Debug, PartialEq)]
pub struct ModuleObject {
    /// Module name.
    pub name: String,
    /// The IR.
    pub ir: Module,
    /// The native output.
    pub code: ModuleCode,
    /// Creation data.
    pub create: CreateData,
    /// Unix seconds of creation.
    pub created: u64,
    /// The source view, when the compiler supplied it.
    pub source: Option<Vec<String>>,
}

impl ModuleObject {
    /// Compile a module's IR into a module object.
    pub fn compile(ir: Module, create: CreateData, created: u64) -> Result<Self> {
        let code = codegen::compile(&ir)?;
        Ok(Self {
            name: ir.name.clone(),
            ir,
            code,
            create,
            created,
            source: None,
        })
    }

    /// The symbols the module exports: exported procedures and statics.
    #[must_use]
    pub fn exports(&self) -> Vec<(String, ExportKind)> {
        let mut out: Vec<(String, ExportKind)> = self
            .ir
            .procs
            .iter()
            .filter(|p| p.export)
            .map(|p| (p.name.clone(), ExportKind::Proc))
            .collect();
        out.extend(
            self.ir
                .vars
                .iter()
                .filter(|v| v.export)
                .map(|v| (v.name.clone(), ExportKind::Data)),
        );
        out
    }

    /// The symbols the module imports: bound procedures and statics.
    #[must_use]
    pub fn imports(&self) -> Vec<(String, ExportKind)> {
        let mut out: Vec<(String, ExportKind)> = self
            .ir
            .protos
            .iter()
            .filter(|p| p.linkage == Linkage::Import)
            .map(|p| (p.name.clone(), ExportKind::Proc))
            .collect();
        out.extend(
            self.ir
                .vars
                .iter()
                .filter(|v| v.class == StorageClass::Import)
                .map(|v| (v.name.clone(), ExportKind::Data)),
        );
        out
    }

    /// The payload bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.raw(MAGIC);
        w.u32(VERSION);
        w.u32(self.code.version);
        w.u16(qir::FORMAT_VERSION);
        w.u8(match self.ir.lang {
            qir::Lang::Qir => 0,
            qir::Lang::Rpgle => 1,
            qir::Lang::Clle => 2,
        });
        w.u8(match self.ir.options.dbgview {
            qir::DbgView::None => 0,
            qir::DbgView::Stmt => 1,
            qir::DbgView::Source => 2,
            qir::DbgView::List => 3,
            qir::DbgView::All => 4,
        });
        w.u64(self.created);
        let mut sections: Vec<(u32, Vec<u8>)> = vec![
            (SEC_IR, qir::binary::write(&self.ir)),
            (SEC_CODE, {
                let mut cw = W::default();
                code::encode(&mut cw, &self.code);
                cw.out
            }),
            (SEC_CREATE, {
                let mut cw = W::default();
                cw.str(&self.create.compiler);
                cw.str(&self.create.options);
                cw.str(&self.create.src_file);
                cw.str(&self.create.src_lib);
                cw.str(&self.create.src_mbr);
                cw.out
            }),
        ];
        if let Some(lines) = &self.source {
            let mut cw = W::default();
            cw.list(lines, |w, l| {
                let b = ccsid::to_ebcdic(l).unwrap_or_else(|_| l.bytes().collect());
                w.bytes(&b);
            });
            sections.push((SEC_SOURCE, cw.out));
        }
        w.u32(crate::bin::len32(sections.len()));
        let mut flags = 0u32;
        if self.ir.options.srcstmt {
            flags |= 1;
        }
        if self.ir.options.nodebugio {
            flags |= 2;
        }
        if self.ir.nomain {
            flags |= 4;
        }
        w.u32(flags);
        w.zeros(MODULE_HDR_LEN - 36);
        let table_at = w.out.len();
        w.zeros(sections.len() * SECTION_LEN);
        for (i, (tag, data)) in sections.iter().enumerate() {
            let at = w.out.len();
            w.raw(data);
            let e = table_at + i * SECTION_LEN;
            w.patch_u32(e, *tag);
            w.patch_u64(e + 8, at as u64);
            w.patch_u64(e + 16, data.len() as u64);
        }
        w.out
    }

    /// Read a payload.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = R::new(bytes, "*MODULE");
        if r.take(8)? != MAGIC {
            return Err(r.err("wrong magic"));
        }
        let version = r.u32()?;
        if version != VERSION {
            return Err(r.err(format!("payload version {version}")));
        }
        let _codegen_version = r.u32()?;
        let _ir_version = r.u16()?;
        let _lang = r.u8()?;
        let _dbgview = r.u8()?;
        let created = r.u64()?;
        let count = r.u32()? as usize;
        let _flags = r.u32()?;
        let sections = read_sections(&mut r, MODULE_HDR_LEN, count)?;
        let find = |tag: u32| -> Option<&[u8]> {
            sections.iter().find(|(t, _)| *t == tag).map(|(_, s)| *s)
        };
        let ir = qir::binary::read(find(SEC_IR).ok_or_else(|| r.err("no IR section"))?)?;
        let code = {
            let mut cr = R::new(
                find(SEC_CODE).ok_or_else(|| r.err("no code section"))?,
                "*MODULE",
            );
            code::decode(&mut cr)?
        };
        let create = match find(SEC_CREATE) {
            Some(b) => {
                let mut cr = R::new(b, "*MODULE");
                CreateData {
                    compiler: cr.str()?,
                    options: cr.str()?,
                    src_file: cr.str()?,
                    src_lib: cr.str()?,
                    src_mbr: cr.str()?,
                }
            }
            None => CreateData::default(),
        };
        let source = match find(SEC_SOURCE) {
            Some(b) => {
                let mut cr = R::new(b, "*MODULE");
                Some(cr.list(|r| Ok(ccsid::from_ebcdic(&r.bytes()?)))?)
            }
            None => None,
        };
        Ok(Self {
            name: ir.name.clone(),
            ir,
            code,
            create,
            created,
            source,
        })
    }
}

/// Read a section table at `table_at` and return each section's bytes.
pub(crate) fn read_sections<'a>(
    r: &mut R<'a>,
    table_at: usize,
    count: usize,
) -> Result<Vec<(u32, &'a [u8])>> {
    r.seek(table_at)?;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let tag = r.u32()?;
        let _ = r.u32()?;
        let offset = usize::try_from(r.u64()?).map_err(|_| r.err("section offset"))?;
        let len = usize::try_from(r.u64()?).map_err(|_| r.err("section length"))?;
        entries.push((tag, offset, len));
    }
    let mut out = Vec::with_capacity(count);
    for (tag, offset, len) in entries {
        r.seek(offset)?;
        let data = r.take(len)?;
        out.push((tag, data));
    }
    Ok(out)
}

impl std::fmt::Display for ModuleObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "*MODULE {} ({} procedures)",
            self.name,
            self.ir.procs.len()
        )
    }
}
