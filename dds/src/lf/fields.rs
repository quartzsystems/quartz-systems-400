// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Logical file fields: a based-on field as is or renamed, `CONCAT`,
//! `SST`, with the shape and keyword handling of the PF compiler.

use qsname::Name;

use super::{Base, Current};
use crate::compile::field::{self, apply_overrides, collect, field_name, finish};
use crate::compile::{self, FileKind, count, number_parm, parse_name};
use crate::diag::{Diagnostics, ids};
use crate::lex::{Keyword, Spec};
use crate::model::{DataType, FieldDef, FieldSource, FileRef, RefSource, Reference};
use crate::refs::RefResolver;

/// Compile a field line of the current format and add it.
pub(crate) fn add(spec: &Spec, cur: &mut Current, diags: &mut Diagnostics) {
    let Some((f, source)) = build(spec, &cur.bases, cur.lf.join, diags) else {
        return;
    };
    if cur.lf.format.field(&f.name).is_some() {
        diags.push(ids::duplicate_field(spec.line, &f.name.to_string()));
        return;
    }
    cur.lf.sources.push(source);
    cur.lf.format.fields.push(f);
}

fn build(
    spec: &Spec,
    bases: &[Base],
    join: bool,
    diags: &mut Diagnostics,
) -> Option<(FieldDef, FieldSource)> {
    let name = field_name(spec, diags)?;
    let kws = collect(spec, FileKind::Logical, diags);
    let derive: Vec<&Keyword> = [kws.rename, kws.concat, kws.sst]
        .into_iter()
        .flatten()
        .collect();
    if derive.len() > 1 {
        diags.push(ids::conflict(
            derive[1].line,
            &derive[1].name,
            &derive[0].name,
        ));
        return None;
    }
    let file = jref(kws.jref, bases, join, diags).ok()?;
    let ctx = Lookup {
        bases,
        file,
        join,
        line: spec.line,
    };
    let text = name.to_string();
    let (mut base, source) = if let Some(kw) = kws.concat {
        concat(kw, &text, &ctx, diags)?
    } else if let Some(kw) = kws.sst {
        substring(kw, &text, &ctx, diags)?
    } else {
        let target = match kws.rename {
            Some(kw) => rename_target(kw, diags)?,
            None => name,
        };
        let (fi, f) = ctx.find(&target, diags)?;
        let mut f = f.clone();
        f.reference = Some(Reference {
            source: RefSource::File(bases[fi].file.clone()),
            field: target,
            format: Some(bases[fi].format.name),
        });
        (
            f,
            FieldSource::Base {
                file: fi,
                field: target,
            },
        )
    };
    apply_overrides(spec, &mut base, &name, diags);
    let f = finish(spec, base, name, &kws, diags)?;
    Some((f, source))
}

fn rename_target(kw: &Keyword, diags: &mut Diagnostics) -> Option<Name> {
    if !count(kw, 1, 1, diags) {
        return None;
    }
    let Ok(n) = kw.word(0).ok_or_else(String::new).and_then(parse_name) else {
        diags.push(ids::bad_parm(
            kw.line,
            &kw.name,
            &kw.parms[0],
            "must be a field name",
        ));
        return None;
    };
    Some(n)
}

/// `JREF(n)` or `JREF(file)`: which based-on file, when given. `Err`
/// when the keyword is unusable.
fn jref(
    kw: Option<&Keyword>,
    bases: &[Base],
    join: bool,
    diags: &mut Diagnostics,
) -> Result<Option<usize>, ()> {
    let Some(kw) = kw else {
        return Ok(None);
    };
    if !join {
        compile::level_error(kw, "field", FileKind::Logical, diags);
        return Ok(None);
    }
    if !count(kw, 1, 1, diags) {
        return Err(());
    }
    let Some(i) = file_index(kw, 0, bases) else {
        diags.push(ids::jref_unknown(kw.line, &kw.parms[0]));
        return Err(());
    };
    Ok(Some(i))
}

/// Parameter `i` as a 1-based file number or a file name of the list.
pub(crate) fn file_index(kw: &Keyword, i: usize, bases: &[Base]) -> Option<usize> {
    let w = kw.word(i)?;
    if let Ok(n) = w.parse::<usize>() {
        if (1..=bases.len()).contains(&n) {
            return Some(n - 1);
        }
        return None;
    }
    let name = parse_name(w).ok()?;
    bases.iter().position(|b| b.file.file == name)
}

/// Where a field is looked for.
struct Lookup<'a> {
    bases: &'a [Base],
    file: Option<usize>,
    join: bool,
    line: usize,
}

impl<'a> Lookup<'a> {
    /// The field in the chosen file, the one file that has it (join),
    /// or the first file, which every other `PFILE` file must have too.
    fn find(&self, target: &Name, diags: &mut Diagnostics) -> Option<(usize, &'a FieldDef)> {
        let text = target.to_string();
        if let Some(i) = self.file {
            let b = &self.bases[i];
            let Some((_, f)) = b.format.field(target) else {
                diags.push(ids::base_field_not_found(self.line, &text, &b.file));
                return None;
            };
            return Some((i, f));
        }
        if self.join {
            let hits: Vec<(usize, &FieldDef)> = self
                .bases
                .iter()
                .enumerate()
                .filter_map(|(i, b)| b.format.field(target).map(|(_, f)| (i, f)))
                .collect();
            return match hits.as_slice() {
                [] => {
                    diags.push(ids::base_field_not_found(
                        self.line,
                        &text,
                        "the JFILE files",
                    ));
                    None
                }
                [one] => Some(*one),
                _ => {
                    diags.push(ids::ambiguous_field(self.line, &text));
                    None
                }
            };
        }
        let first = &self.bases[0];
        let Some((_, f)) = first.format.field(target) else {
            diags.push(ids::base_field_not_found(self.line, &text, &first.file));
            return None;
        };
        let mut ok = true;
        for b in &self.bases[1..] {
            if b.format.field(target).is_none() {
                diags.push(ids::base_field_not_found(self.line, &text, &b.file));
                ok = false;
            }
        }
        ok.then_some((0, f))
    }
}

/// `CONCAT(f1 f2 ...)`: zoned when every part is zoned, else character
/// over the parts' bytes.
fn concat(
    kw: &Keyword,
    name: &str,
    ctx: &Lookup<'_>,
    diags: &mut Diagnostics,
) -> Option<(FieldDef, FieldSource)> {
    if !count(kw, 2, 100, diags) {
        return None;
    }
    let mut parts = Vec::new();
    let mut file = None;
    let mut all_zoned = true;
    let mut digits = 0;
    let mut bytes = 0;
    for p in &kw.parms {
        let Ok(part) = p.as_word().ok_or_else(String::new).and_then(parse_name) else {
            diags.push(ids::bad_parm(kw.line, &kw.name, p, "must be a field name"));
            return None;
        };
        let (fi, f) = ctx.find(&part, diags)?;
        if *file.get_or_insert(fi) != fi {
            diags.push(ids::bad_parm(
                kw.line,
                &kw.name,
                p,
                "every CONCAT field must come from one file",
            ));
            return None;
        }
        let t = f.data_type;
        let allowed = matches!(
            t,
            DataType::Char | DataType::Hex | DataType::Zoned | DataType::Packed | DataType::Binary
        );
        if !allowed {
            diags.push(ids::concat_type(
                kw.line,
                name,
                &part.to_string(),
                t.letter(),
            ));
            return None;
        }
        all_zoned &= t == DataType::Zoned;
        digits += f.length;
        bytes += f.data_len();
        parts.push(part);
    }
    let placeholder = Name::parse("X").expect("name");
    let f = if all_zoned {
        FieldDef::new(placeholder, DataType::Zoned, digits, 0)
    } else {
        FieldDef::new(placeholder, DataType::Char, bytes, 0)
    };
    let source = FieldSource::Concat {
        file: file.unwrap_or(0),
        fields: parts,
    };
    Some((f, source))
}

/// `SST(field start [length])`: a character field over part of another.
fn substring(
    kw: &Keyword,
    name: &str,
    ctx: &Lookup<'_>,
    diags: &mut Diagnostics,
) -> Option<(FieldDef, FieldSource)> {
    if !count(kw, 2, 3, diags) {
        return None;
    }
    let Ok(target) = kw.word(0).ok_or_else(String::new).and_then(parse_name) else {
        diags.push(ids::bad_parm(
            kw.line,
            &kw.name,
            &kw.parms[0],
            "must be a field name",
        ));
        return None;
    };
    let start = number_parm(kw, 1, diags)?;
    let (fi, f) = ctx.find(&target, diags)?;
    let t = f.data_type;
    if !matches!(
        t,
        DataType::Char | DataType::Hex | DataType::Zoned | DataType::Graphic
    ) {
        diags.push(ids::sst_source_type(
            kw.line,
            name,
            &target.to_string(),
            t.letter(),
        ));
        return None;
    }
    let length = match kw.parms.get(2) {
        Some(_) => number_parm(kw, 2, diags)?,
        None => f.length.saturating_sub(start.saturating_sub(1)),
    };
    if start == 0 || length == 0 || start - 1 + length > f.length {
        diags.push(ids::sst_range(kw.line, name, &target.to_string(), f.length));
        return None;
    }
    let result_type = if t == DataType::Graphic {
        DataType::Graphic
    } else {
        DataType::Char
    };
    let placeholder = Name::parse("X").expect("name");
    let def = FieldDef::new(placeholder, result_type, length, 0);
    let source = FieldSource::Substring {
        file: fi,
        field: target,
        start,
        length,
    };
    Some((def, source))
}

/// `FORMAT(file)`: the fields of that file's format of the same name,
/// each of which the first based-on file must have.
pub(crate) fn copy_format(
    cur: &mut Current,
    fr: &FileRef,
    resolver: &dyn RefResolver,
    line: usize,
    diags: &mut Diagnostics,
) {
    cur.lf.format.from = Some(fr.clone());
    let Some(formats) = field::resolve(line, fr, resolver, diags) else {
        return;
    };
    let name = cur.lf.format.name;
    let Some(r) = formats.into_iter().find(|r| r.name == name) else {
        diags.push(ids::ref_format_not_found(line, fr, &name.to_string()));
        return;
    };
    let base = &cur.bases[0];
    for mut f in r.fields {
        if base.format.field(&f.name).is_none() {
            diags.push(ids::base_field_not_found(
                line,
                &f.name.to_string(),
                &base.file,
            ));
            continue;
        }
        f.line = line;
        cur.lf.sources.push(FieldSource::Base {
            file: 0,
            field: f.name,
        });
        cur.lf.format.fields.push(f);
    }
}
