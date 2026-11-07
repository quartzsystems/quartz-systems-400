// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Display and printer files: each format's named fields and its
//! buffers' layouts.

use rpgc::files::{Dev, DspLayout, ExtField, ExtFile, ExtFormat};
use rpgc::types::{DatFmt, TimFmt, Ty};

/// A display file as the compiler sees it: each record format's named
/// fields and both buffers' layouts.
#[must_use]
pub fn display_desc(df: &dspf::DisplayFile, lib: &str) -> ExtFile {
    let formats = df
        .records
        .iter()
        .map(|r| {
            let named: Vec<(usize, &dspf::Field)> = r
                .fields
                .iter()
                .enumerate()
                .filter(|(_, f)| f.name.is_some())
                .collect();
            let out_of = |i: usize| r.layout.output.field(i).map(|b| b.offset);
            let in_of = |i: usize| r.layout.input.field(i).map(|b| b.offset);
            ExtFormat {
                name: r.name.clone(),
                level_id: r.layout.level_id.clone(),
                len: r.layout.output.len.max(r.layout.input.len),
                fields: named
                    .iter()
                    .map(|(i, f)| ExtField {
                        name: f.name.clone().unwrap_or_default(),
                        ty: display_type(f),
                        offset: out_of(*i).or_else(|| in_of(*i)).unwrap_or(0),
                        usage: f.usage.letter(),
                        alias: String::new(),
                        nullable: false,
                        default: None,
                    })
                    .collect(),
                keys: Vec::new(),
                dsp: Some(DspLayout {
                    out_len: r.layout.output.len,
                    in_len: r.layout.input.len,
                    option_inds: r.layout.output.indicators.clone(),
                    response_inds: r.layout.input.indicators.clone(),
                    ind_texts: df.response_texts(r),
                    offsets: named.iter().map(|(i, _)| (out_of(*i), in_of(*i))).collect(),
                    subfile: r.kind == dspf::RecordKind::Subfile,
                    sflctl: r.subfile.as_ref().map(|s| s.record.clone()),
                    slno_var: r.slno == Some(dspf::Slno::Var),
                }),
            }
        })
        .collect();
    ExtFile {
        lib: lib.to_string(),
        dev: Dev::Workstn,
        formats,
        indara: df.file.indara,
    }
}

/// A printer file as the compiler sees it: each format's fields in the
/// record buffer after its option indicators (none under `INDARA`),
/// as the job's printer reads them (`spool::ddsprt`).
#[must_use]
pub fn printer_desc(def: &dds::prtf::PrtfDefinition, lib: &str) -> ExtFile {
    use dds::prtf::PrtKind;
    let formats = def
        .formats
        .iter()
        .map(|f| {
            let inds = if def.indara {
                Vec::new()
            } else {
                f.indicators()
            };
            let mut at = u32::try_from(inds.len()).unwrap_or(0);
            let mut fields = Vec::new();
            for x in &f.fields {
                let ty = match x.kind {
                    PrtKind::Char(n) => Ty::Char(u32::from(n)),
                    PrtKind::Zoned(d, s) => Ty::Zoned(d, s),
                    _ => continue,
                };
                if x.name.is_empty() {
                    continue;
                }
                fields.push(ExtField {
                    name: x.name.clone(),
                    ty,
                    offset: at,
                    usage: 'O',
                    alias: String::new(),
                    nullable: false,
                    default: None,
                });
                at += u32::try_from(x.buffer_len()).unwrap_or(0);
            }
            ExtFormat {
                name: f.name.clone(),
                level_id: String::new(),
                len: at,
                dsp: Some(DspLayout {
                    out_len: at,
                    in_len: 0,
                    option_inds: inds,
                    response_inds: Vec::new(),
                    ind_texts: Vec::new(),
                    offsets: fields.iter().map(|x| (Some(x.offset), None)).collect(),
                    subfile: false,
                    sflctl: None,
                    slno_var: false,
                }),
                fields,
                keys: Vec::new(),
            }
        })
        .collect();
    ExtFile {
        lib: lib.to_string(),
        dev: Dev::Printer,
        formats,
        indara: def.indara,
    }
}

/// A display field's RPG type.
fn display_type(f: &dspf::Field) -> Ty {
    use dspf::TypeClass;
    let digits = u8::try_from(f.length).unwrap_or(63);
    match f.ty.class {
        TypeClass::Char => Ty::Char(f.length),
        TypeClass::Graphic => Ty::Graph(f.length),
        TypeClass::Zoned => Ty::Zoned(digits, f.decimals),
        TypeClass::Packed => Ty::Packed(digits, f.decimals),
        TypeClass::Binary => Ty::Bin(digits, f.decimals),
        TypeClass::Float => Ty::Float(if f.bytes == 4 { 4 } else { 8 }),
        TypeClass::Date => Ty::Date(DatFmt::Iso, '-'),
        TypeClass::Time => Ty::Time(TimFmt::Iso, '.'),
        TypeClass::Timestamp => Ty::Timestamp(6),
    }
}
