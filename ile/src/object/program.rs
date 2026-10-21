// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*PGM` and `*SRVPGM` payload: a header, then sections for the
//! bound module copies, the service programs, the import resolution,
//! the exports, the signatures and the binder source.

use store::spec::{Layout, field};

use super::module::{ModuleObject, SECTION_LEN, read_sections};
use super::{
    ActGrpAttr, Export, ExportKind, ImportRes, ImportTarget, PgmAttr, PgmKind, SrvPgmRef,
    UserProfile,
};
use crate::bin::{R, W};
use crate::binder::signature::Signature;
use crate::error::Result;

/// Size of `PgmHdr`.
pub const PGM_HDR_LEN: usize = 128;
/// Size of `SrvPgmRef`.
pub const SRVPGM_REF_LEN: usize = 36;
const MAGIC: &[u8; 8] = b"QS400PGM";
const VERSION: u32 = 1;

const SEC_MODULES: u32 = 1;
const SEC_SRVPGMS: u32 = 2;
const SEC_IMPORTS: u32 = 3;
const SEC_EXPORTS: u32 = 4;
const SEC_SIGNATURES: u32 = 5;
const SEC_BNDSRC: u32 = 6;

/// Layout of `PgmHdr`, for the spec test.
pub static HDR_LAYOUT: Layout = Layout {
    name: "PgmHdr",
    size: Some(PGM_HDR_LEN),
    fields: &[
        field("magic", 0, 8),
        field("version", 8, 4),
        field("kind", 12, 1),
        field("actgrp_kind", 13, 1),
        field("usrprf", 14, 1),
        field("useadpaut", 15, 1),
        field("user_state", 16, 1),
        field("user_domain", 17, 1),
        field("alwupd", 18, 1),
        field("alwlibupd", 19, 1),
        field("observable", 20, 1),
        field("lvlchk", 21, 1),
        field("reserved", 22, 2),
        field("actgrp_name", 24, 10),
        field("tgtrls", 34, 6),
        field("created", 40, 8),
        field("codegen_version", 48, 4),
        field("module_count", 52, 4),
        field("srvpgm_count", 56, 4),
        field("import_count", 60, 4),
        field("export_count", 64, 4),
        field("signature_count", 68, 4),
        field("static_size", 72, 8),
        field("entry_module", 80, 4),
        field("entry_proc", 84, 4),
        field("section_count", 88, 4),
        field("reserved2", 92, 36),
    ],
};

/// Layout of `SrvPgmRef`, for the spec test.
pub static SRVPGM_REF_LAYOUT: Layout = Layout {
    name: "SrvPgmRef",
    size: Some(SRVPGM_REF_LEN),
    fields: &[
        field("name", 0, 10),
        field("library", 10, 10),
        field("signature", 20, 16),
    ],
};

/// A module copied into a program.
#[derive(Clone, Debug, PartialEq)]
pub struct BoundModule {
    /// The copy.
    pub module: ModuleObject,
    /// Library the module came from.
    pub lib: String,
    /// Offset of its static frame in the program's static area.
    pub frame_offset: u64,
}

/// A `*PGM` or `*SRVPGM` object's payload.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgramObject {
    /// Name.
    pub name: String,
    /// Attributes.
    pub attr: PgmAttr,
    /// Entry module and procedure, for a program.
    pub entry: Option<(u32, u32)>,
    /// Bound modules in order.
    pub modules: Vec<BoundModule>,
    /// Every import of every module and where it went.
    pub imports: Vec<ImportRes>,
    /// Service programs bound to.
    pub srvpgms: Vec<SrvPgmRef>,
    /// Bytes of the static area, a multiple of 16.
    pub static_size: u64,
    /// Exports, for a service program.
    pub exports: Vec<Export>,
    /// Signatures, current first.
    pub signatures: Vec<Signature>,
    /// The binder source the exports came from.
    pub binder_source: Option<String>,
}

impl ProgramObject {
    /// The current signature of a service program.
    #[must_use]
    pub fn signature(&self) -> Option<Signature> {
        self.signatures.first().copied()
    }

    /// Whether a signature is one the service program answers to.
    #[must_use]
    pub fn accepts(&self, s: Signature) -> bool {
        !self.attr.lvlchk || self.signatures.contains(&s)
    }

    /// The bound module and procedure an export names.
    #[must_use]
    pub fn export_named(&self, name: &str) -> Option<&Export> {
        self.exports.iter().find(|e| e.name == name)
    }

    /// The payload bytes.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.raw(MAGIC);
        w.u32(VERSION);
        w.u8(match self.attr.kind {
            PgmKind::Pgm => 0,
            PgmKind::SrvPgm => 1,
        });
        w.u8(match &self.attr.actgrp {
            ActGrpAttr::New => 0,
            ActGrpAttr::Caller => 1,
            ActGrpAttr::Named(_) => 2,
            ActGrpAttr::Default => 3,
        });
        w.u8(match self.attr.usrprf {
            UserProfile::User => 0,
            UserProfile::Owner => 1,
        });
        w.bool(self.attr.useadpaut);
        w.bool(self.attr.user_state);
        w.bool(self.attr.user_domain);
        w.bool(self.attr.alwupd);
        w.bool(self.attr.alwlibupd);
        w.bool(self.attr.observable);
        w.bool(self.attr.lvlchk);
        w.zeros(2);
        w.name(match &self.attr.actgrp {
            ActGrpAttr::Named(n) => n,
            _ => "",
        });
        w.text::<6>(&self.attr.tgtrls);
        w.u64(self.attr.created);
        w.u32(self.attr.codegen_version);
        w.u32(crate::bin::len32(self.modules.len()));
        w.u32(crate::bin::len32(self.srvpgms.len()));
        w.u32(crate::bin::len32(self.imports.len()));
        w.u32(crate::bin::len32(self.exports.len()));
        w.u32(crate::bin::len32(self.signatures.len()));
        w.u64(self.static_size);
        let (em, ep) = self.entry.unwrap_or((u32::MAX, u32::MAX));
        w.u32(em);
        w.u32(ep);
        let mut sections: Vec<(u32, Vec<u8>)> = Vec::new();
        sections.push((SEC_MODULES, {
            let mut s = W::default();
            s.list(&self.modules, |w, m| {
                w.name(&m.lib);
                w.u64(m.frame_offset);
                w.bytes(&m.module.encode());
            });
            s.out
        }));
        sections.push((SEC_SRVPGMS, {
            let mut s = W::default();
            for r in &self.srvpgms {
                s.name(&r.name);
                s.name(&r.lib);
                s.raw(&r.signature.0);
            }
            s.out
        }));
        sections.push((SEC_IMPORTS, {
            let mut s = W::default();
            s.list(&self.imports, |w, i| {
                w.u32(i.module);
                w.str(&i.name);
                w.u8(kind_code(i.kind));
                match &i.target {
                    ImportTarget::Proc { module, proc } => {
                        w.u8(0);
                        w.u32(*module);
                        w.u32(*proc);
                    }
                    ImportTarget::Data { offset } => {
                        w.u8(1);
                        w.u64(*offset);
                    }
                    ImportTarget::SrvPgm { srvpgm, export } => {
                        w.u8(2);
                        w.u32(*srvpgm);
                        w.str(export);
                    }
                    ImportTarget::Unresolved => w.u8(3),
                }
            });
            s.out
        }));
        sections.push((SEC_EXPORTS, {
            let mut s = W::default();
            s.list(&self.exports, |w, e| {
                w.str(&e.name);
                w.u8(kind_code(e.kind));
                w.u32(e.module);
                w.u32(e.index);
                w.u64(e.offset);
            });
            s.out
        }));
        sections.push((SEC_SIGNATURES, {
            let mut s = W::default();
            for sig in &self.signatures {
                s.raw(&sig.0);
            }
            s.out
        }));
        if let Some(src) = &self.binder_source {
            let mut s = W::default();
            s.str(src);
            sections.push((SEC_BNDSRC, s.out));
        }
        w.u32(crate::bin::len32(sections.len()));
        w.zeros(PGM_HDR_LEN - 92);
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
    #[allow(clippy::too_many_lines)]
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = R::new(bytes, "*PGM");
        if r.take(8)? != MAGIC {
            return Err(r.err("wrong magic"));
        }
        let version = r.u32()?;
        if version != VERSION {
            return Err(r.err(format!("payload version {version}")));
        }
        let kind = match r.u8()? {
            0 => PgmKind::Pgm,
            1 => PgmKind::SrvPgm,
            v => return Err(r.err(format!("{v} is not a program kind"))),
        };
        let actgrp_kind = r.u8()?;
        let usrprf = match r.u8()? {
            0 => UserProfile::User,
            _ => UserProfile::Owner,
        };
        let useadpaut = r.bool()?;
        let user_state = r.bool()?;
        let user_domain = r.bool()?;
        let alwupd = r.bool()?;
        let alwlibupd = r.bool()?;
        let observable = r.bool()?;
        let lvlchk = r.bool()?;
        r.take(2)?;
        let actgrp_name = r.name()?;
        let tgtrls = r.text(6)?;
        let created = r.u64()?;
        let codegen_version = r.u32()?;
        let module_count = r.u32()? as usize;
        let srvpgm_count = r.u32()? as usize;
        let _import_count = r.u32()?;
        let _export_count = r.u32()?;
        let signature_count = r.u32()? as usize;
        let static_size = r.u64()?;
        let em = r.u32()?;
        let ep = r.u32()?;
        let section_count = r.u32()? as usize;
        let actgrp = match actgrp_kind {
            0 => ActGrpAttr::New,
            1 => ActGrpAttr::Caller,
            2 => ActGrpAttr::Named(actgrp_name),
            3 => ActGrpAttr::Default,
            v => return Err(r.err(format!("{v} is not an activation group kind"))),
        };
        let sections = read_sections(&mut r, PGM_HDR_LEN, section_count)?;
        let find = |tag: u32| -> Option<&[u8]> {
            sections.iter().find(|(t, _)| *t == tag).map(|(_, s)| *s)
        };
        let modules = {
            let mut s = R::new(
                find(SEC_MODULES).ok_or_else(|| r.err("no modules section"))?,
                "*PGM",
            );
            let v = s.list(|r| {
                let lib = r.name()?;
                let frame_offset = r.u64()?;
                let module = ModuleObject::decode(&r.bytes()?)?;
                Ok(BoundModule {
                    module,
                    lib,
                    frame_offset,
                })
            })?;
            if v.len() != module_count {
                return Err(r.err("module count"));
            }
            v
        };
        let srvpgms = {
            let mut s = R::new(find(SEC_SRVPGMS).unwrap_or(&[]), "*PGM");
            let mut v = Vec::with_capacity(srvpgm_count);
            for _ in 0..srvpgm_count {
                let name = s.name()?;
                let lib = s.name()?;
                let sig = Signature(s.take(16)?.try_into().expect("16 bytes"));
                v.push(SrvPgmRef {
                    name,
                    lib,
                    signature: sig,
                });
            }
            v
        };
        let imports = {
            let mut s = R::new(find(SEC_IMPORTS).unwrap_or(&[0, 0, 0, 0]), "*PGM");
            s.list(|r| {
                let module = r.u32()?;
                let name = r.str()?;
                let kind = kind_of(r.u8()?).ok_or_else(|| r.err("symbol kind"))?;
                let target = match r.u8()? {
                    0 => ImportTarget::Proc {
                        module: r.u32()?,
                        proc: r.u32()?,
                    },
                    1 => ImportTarget::Data { offset: r.u64()? },
                    2 => ImportTarget::SrvPgm {
                        srvpgm: r.u32()?,
                        export: r.str()?,
                    },
                    3 => ImportTarget::Unresolved,
                    v => return Err(r.err(format!("{v} is not an import target"))),
                };
                Ok(ImportRes {
                    module,
                    name,
                    kind,
                    target,
                })
            })?
        };
        let exports = {
            let mut s = R::new(find(SEC_EXPORTS).unwrap_or(&[0, 0, 0, 0]), "*PGM");
            s.list(|r| {
                Ok(Export {
                    name: r.str()?,
                    kind: kind_of(r.u8()?).ok_or_else(|| r.err("symbol kind"))?,
                    module: r.u32()?,
                    index: r.u32()?,
                    offset: r.u64()?,
                })
            })?
        };
        let signatures = {
            let mut s = R::new(find(SEC_SIGNATURES).unwrap_or(&[]), "*PGM");
            let mut v = Vec::with_capacity(signature_count);
            for _ in 0..signature_count {
                v.push(Signature(s.take(16)?.try_into().expect("16 bytes")));
            }
            v
        };
        let binder_source = match find(SEC_BNDSRC) {
            Some(b) => Some(R::new(b, "*PGM").str()?),
            None => None,
        };
        Ok(Self {
            name: String::new(),
            attr: PgmAttr {
                kind,
                actgrp,
                usrprf,
                useadpaut,
                user_state,
                user_domain,
                alwupd,
                alwlibupd,
                observable,
                lvlchk,
                tgtrls,
                created,
                codegen_version,
            },
            entry: if em == u32::MAX { None } else { Some((em, ep)) },
            modules,
            imports,
            srvpgms,
            static_size,
            exports,
            signatures,
            binder_source,
        })
    }
}

fn kind_code(k: ExportKind) -> u8 {
    match k {
        ExportKind::Proc => 0,
        ExportKind::Data => 1,
    }
}

fn kind_of(b: u8) -> Option<ExportKind> {
    match b {
        0 => Some(ExportKind::Proc),
        1 => Some(ExportKind::Data),
        _ => None,
    }
}
